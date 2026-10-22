"""Augmented copies of one photo, found by perceptual hash when the file names don't say which
photo a file copies (index.py's `originals: {phash: N}`; PlantCity).

    python -m ml.training.data.copies plantcity [--bits 2]   # the groups, counted, and a sheet to check

PlantCity ships each photo with copies flipped, turned a quarter, mirrored into tiles, or turned
by any angle on the same canvas with the corners filled black, numbered in sequence, so nothing in
a name links a copy to its photo. Two files are copies when:

- **flipped or turned a quarter:** the 64-bit DCT hash of one (check_leakage's, of the whole image
  at 32 x 32) is within `bits` of the hash of any of the other's 8 rotations and flips;
- **turned by any angle:** a file with its four corners black is turned back by the angle its
  top-left black triangle gives (the four candidate signs and legs are tried), and the hash of its
  central 40 % is within `bits` of the hash of the central 40 % of another file, or of that
  file's rotations and flips;
- **turned by any angle, found by search:** the corner gives no usable angle on some files (JPEG
  noise along the edge), and a file turned one way is never compared with one turned another. So
  every file with black corners is also turned in 1° steps, and its central 40 % at each angle is
  compared with the central 40 % (rotations and flips too) of every file of the `NEAREST` groups
  closest to it by BioCLIP 2 cosine; within `ANGLE_BITS` is a copy. The central 40 % of a turned
  copy is all photo at any angle, since the canvas keeps the photo's size.

The groups are the connected components of these pairs, across every class of the dataset, so a
photo shipped under two labels becomes one group whose labels disagree, which dedupe.py drops. A
group's original is its smallest path. Cached under data/cache/copies/ by the files' paths,
sizes and modification times, `bits` and VERSION.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import multiprocessing as mp
import os
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
CACHE = HERE / "cache" / "copies"
VERSION = 2
BLACK = 12          # a grey level at or below this is fill, not photo
NEAREST = 3         # groups by BioCLIP 2 cosine that a file with black corners is turned against
ANGLE_BITS = 8      # a centre turned by the search within this of a neighbour's is a copy
TURN_SIDE = 96      # the long side, in pixels, of the central 40 % when the search turns it
CORNER = 6          # the side of the corner patches that must be black, in pixels
CENTRE = 0.4        # the central share of the side that a turned copy keeps in full


def _gray(path: Path) -> np.ndarray:
    from PIL import ImageOps

    with Image.open(path) as im:
        return np.asarray(ImageOps.exif_transpose(im).convert("L"))


def _small(a: np.ndarray) -> np.ndarray:
    return np.asarray(Image.fromarray(a).resize((32, 32), Image.Resampling.LANCZOS), np.float32)


def _hashes(a: np.ndarray) -> list[int]:
    """The hash of `a` and of its 7 other rotations and flips (of the 32 x 32 grey image)."""
    from ml.training.eval.check_leakage import dihedral, phash

    return [phash(v) for v in dihedral(_small(a))]


def _centre(a: np.ndarray) -> np.ndarray:
    h, w = a.shape
    ch, cw = max(int(h * CENTRE), 8), max(int(w * CENTRE), 8)
    return a[(h - ch) // 2 : (h - ch) // 2 + ch, (w - cw) // 2 : (w - cw) // 2 + cw]


def _black_corners(a: np.ndarray) -> bool:
    k = CORNER
    return all(c.mean() <= BLACK for c in (a[:k, :k], a[:k, -k:], a[-k:, :k], a[-k:, -k:]))


def _turned_back(a: np.ndarray) -> list[np.ndarray]:
    """The file turned back by each angle its top-left black triangle allows."""
    import cv2

    m = a <= BLACK
    leg_x = int(np.argmin(m[0, :])) if not m[0, :].all() else 0
    leg_y = int(np.argmin(m[:, 0])) if not m[:, 0].all() else 0
    if not (leg_x and leg_y):
        return []
    h, w = a.shape
    t1, t2 = math.degrees(math.atan2(leg_y, leg_x)), math.degrees(math.atan2(leg_x, leg_y))
    return [cv2.warpAffine(a, cv2.getRotationMatrix2D((w / 2, h / 2), t, 1.0), (w, h)) for t in (t1, -t1, t2, -t2)]


def turn_hashes(path: Path) -> list[int]:
    """The hash of the central 40 % of the file turned by every whole degree, at reduced size."""
    import cv2

    from ml.training.eval.check_leakage import phash

    a = _gray(path)
    h, w = a.shape
    f = max(1.0, CENTRE * max(h, w) / TURN_SIDE)
    a = cv2.resize(a, (max(8, round(w / f)), max(8, round(h / f))), interpolation=cv2.INTER_AREA)
    h, w = a.shape
    return [phash(_small(_centre(cv2.warpAffine(a, cv2.getRotationMatrix2D((w / 2, h / 2), t, 1.0), (w, h),
                                                    flags=cv2.INTER_LINEAR))))
            for t in range(360)]


def searched_pairs(black: list[int], turns: np.ndarray, centre: np.ndarray, emb: np.ndarray,
                   root: list[int]) -> list[tuple[int, int]]:
    """Pairs (i, j): file i of `black`, turned by some whole degree, within ANGLE_BITS of a
    rotation or flip of the centre of file j, one of the NEAREST groups other than i's own by
    BioCLIP 2 cosine (`emb` rows are unit length; `root` is each file's group so far)."""
    root = np.asarray(root)
    members: dict[int, np.ndarray] = {}
    for r in np.unique(root):
        members[int(r)] = np.nonzero(root == r)[0]
    pairs = []
    for s in range(0, len(black), 256):
        rows = black[s : s + 256]
        sims = emb[rows] @ emb.T
        for k, i in enumerate(rows):
            sims[k, root == root[i]] = -np.inf
            near, seen = [], set()
            for j in np.argsort(-sims[k])[: 64 * NEAREST]:
                if root[j] not in seen:
                    seen.add(int(root[j]))
                    near.append(int(root[j]))
                    if len(near) == NEAREST:
                        break
            cand = np.concatenate([members[r] for r in near]) if near else np.zeros(0, int)
            if not len(cand):
                continue
            t = turns[s + k]                                            # [360]
            d = np.bitwise_count(t[:, None, None] ^ centre[cand][None, :, :]).min(axis=(0, 2))
            pairs += [(i, int(j)) for j in cand[d <= ANGLE_BITS]]
    return pairs


def file_hashes(path: Path) -> dict:
    """The whole image's 8 rotation and flip hashes, its centre's 8, whether its corners are
    black, and, if they are, the centre hashes of it turned back."""
    a = _gray(path)
    black = _black_corners(a)
    back = [_hashes(_centre(b))[0] for b in _turned_back(a)] if black else []
    return {"whole": _hashes(a), "centre": _hashes(_centre(a)), "black": black, "back": back}


def _within(a: np.ndarray, b: np.ndarray, bits: int, chunk: int = 2048) -> list[tuple[int, int]]:
    """Pairs (i, j) with any hash of row i of `a` [n, k] within `bits` of hash 0 of row j of
    `b` [m, ...]."""
    out = []
    for s in range(0, len(a), chunk):
        d = np.full((min(chunk, len(a) - s), len(b)), 64, np.int16)
        for k in range(a.shape[1]):
            d = np.minimum(d, np.bitwise_count(a[s : s + chunk, k][:, None] ^ b[None, :, 0]).astype(np.int16))
        i, j = np.nonzero(d <= bits)
        out += list(zip((i + s).tolist(), j.tolist()))
    return out


def groups(paths: list[Path], bits: int, workers: int | None = None, embed=None) -> dict[Path, Path]:
    """Each file's original: the smallest path of its group of copies. `embed(paths)` gives
    BioCLIP 2 embeddings for the search (default: the training Embedder, cached per file)."""
    key = hashlib.sha256(json.dumps(
        [VERSION, bits, [(str(p), p.stat().st_size, p.stat().st_mtime_ns) for p in sorted(paths)]]).encode()).hexdigest()[:16]
    cached = CACHE / f"{key}.json"
    if cached.exists():
        table = json.loads(cached.read_text())
        return {Path(p): Path(table[p]) for p in map(str, paths)}
    paths = sorted(paths)
    with ProcessPoolExecutor(workers or max(1, (os.cpu_count() or 2) - 2), mp_context=mp.get_context("forkserver")) as pool:
        hs = list(pool.map(file_hashes, paths, chunksize=32))
        black = [i for i, h in enumerate(hs) if h["black"]]
        turns = np.array(list(pool.map(turn_hashes, [paths[i] for i in black], chunksize=8)), np.uint64)
    whole = np.array([h["whole"] for h in hs], np.uint64)
    centre = np.array([h["centre"] for h in hs], np.uint64)
    pairs = [(i, j) for i, j in _within(whole, whole, bits) if i != j]
    turned = [i for i, h in enumerate(hs) if len(h["back"]) == 4]
    if turned:
        back = np.array([hs[i]["back"] for i in turned], np.uint64)
        # Each turned-back centre against every file's centre, flipped or turned a quarter too
        # (the copy may be flipped as well as turned).
        for k in range(back.shape[1]):
            pairs += [(j, turned[i]) for j, i in _within(centre, back[:, k : k + 1], bits) if turned[i] != j]
    parent = list(range(len(paths)))

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(pairs: list[tuple[int, int]]) -> None:
        for i, j in pairs:
            a, b = find(i), find(j)
            if a != b:
                parent[max(a, b)] = min(a, b)   # the root is the smallest index: the smallest path

    union(pairs)
    if black:
        if embed is None:
            from ml.pdm_vision.features import Embedder

            embed = Embedder().files
        emb = np.asarray(embed(paths), np.float32)
        union(searched_pairs(black, turns, centre, emb, [find(i) for i in range(len(paths))]))
    table = {str(p): str(paths[find(i)]) for i, p in enumerate(paths)}
    CACHE.mkdir(parents=True, exist_ok=True)
    cached.write_text(json.dumps(table))
    return {Path(p): Path(o) for p, o in table.items()}


def main(argv=None) -> int:
    import collections

    import yaml

    from ml.training.data import index

    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("dataset", help="a mapping file's name, e.g. plantcity")
    ap.add_argument("--bits", type=int, help="default: the mapping's originals.phash")
    ap.add_argument("--sheet", type=Path, help="write a contact sheet of 12 random groups of 2 or more here")
    args = ap.parse_args(argv)
    m = yaml.safe_load((HERE / "mappings" / f"{args.dataset}.yaml").read_text())
    bits = args.bits if args.bits is not None else m["originals"]["phash"]
    root = (index.RAW / m["source"] / m.get("root", ".")).resolve()   # the paths index.load passes
    files = [p for f in m["splits"].values() for p in sorted((root / f).resolve().rglob("*"))
             if p.is_file() and p.suffix.lower() in index.IMAGE_EXT]
    orig = groups(files, bits)
    members = collections.defaultdict(list)
    for p, o in orig.items():
        members[o].append(p)
    sizes = collections.Counter(len(v) for v in members.values())
    print(f"{len(files)} files, {len(members)} groups at {bits} bits; group sizes {dict(sorted(sizes.items()))}")
    per_class = collections.Counter()
    for o, ps in members.items():
        for c in {p.parent.relative_to(root).as_posix() for p in ps}:
            per_class[c] += 1
    for c, n in sorted(per_class.items()):
        print(f"   {c:40s} {n:5d} groups")
    mixed = [ps for ps in members.values() if len({p.parent.name for p in ps}) > 1]
    print(f"groups spanning class folders: {len(mixed)}")
    if args.sheet:
        import random

        from PIL import ImageDraw

        rng = random.Random(1)
        big = [sorted(v) for v in members.values() if len(v) > 1]
        rows = rng.sample(big, min(12, len(big)))
        side, cols = 150, 10
        sheet = Image.new("RGB", (cols * side, len(rows) * (side + 14)), "white")
        draw = ImageDraw.Draw(sheet)
        for r, ps in enumerate(rows):
            for c, p in enumerate(ps[:cols]):
                with Image.open(p) as im:
                    im = im.convert("RGB")
                    im.thumbnail((side, side))
                    sheet.paste(im, (c * side, r * (side + 14)))
                draw.text((c * side + 2, r * (side + 14) + side), p.relative_to(root).as_posix()[-24:], fill="black")
        args.sheet.parent.mkdir(parents=True, exist_ok=True)
        sheet.save(args.sheet, quality=85)
        print(f"sheet: {args.sheet}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
