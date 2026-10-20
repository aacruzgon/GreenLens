"""Checks the evaluation photos against every training image (evaluation 2 and 9): a photo that
is a copy of a training image would measure memory, not generalisation.

    python -m ml.training.eval.check_leakage [--hamming 5] [--cosine 0.98] [--no-bioclip] [--out leakage-<date>]

Every photo the manifest names is compared with every image file under `ml/training/data/raw`,
which holds every source any of our models trained or was tuned on, all splits:

- **Perceptual hash:** a 64-bit DCT hash of the photo and of its 8 rotations and flips. A training
  image within `--hamming` bits of any of them is a copy: resized, recompressed, rotated or flipped.
- **BioCLIP 2 cosine:** the embeddings the training code caches (`ml.pdm_vision.features`, on the
  GPU for files not cached yet). A pair at `--cosine` or more is a duplicate by training 4's rule,
  which also catches crops and colour edits the hash misses.

Either signal is a match. A match is looked at by eye: a copy drops the case, and a different photo
goes into `leakage_allowed.tsv` (evaluation file, training path, why) so it no longer fails.
The run exits 1 while any match isn't allowed.

Training hashes are cached in `data/cache/phash/hashes.tsv`, keyed by path, size and mtime.
Writes `reports/<out>.md` and `.json`: every match, and per photo its nearest training image by
each signal; side-by-side sheets of the matches and the nearest pairs go to the git-ignored
`runs/leakage/<out>/`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import multiprocessing as mp
import os
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageOps
from scipy.fft import dctn

ROOT = Path(__file__).resolve().parents[3]
EVAL = ROOT / "ml/training/eval"
RAW = ROOT / "ml/training/data/raw"
CACHE = ROOT / "ml/training/data/cache/phash"
REPORTS = EVAL / "reports"
RUNS = ROOT / "ml/training/runs/leakage"
ALLOWED = EVAL / "leakage_allowed.tsv"
IMAGE_SUFFIXES = {".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff"}


# --- perceptual hash ----------------------------------------------------------------------------

def gray32(path: Path) -> np.ndarray:
    """The image, upright by its EXIF orientation, in grey at 32 × 32."""
    with Image.open(path) as im:
        im.draft("L", (64, 64))          # JPEG: decode at reduced size, much faster
        im = ImageOps.exif_transpose(im).convert("L")
        return np.asarray(im.resize((32, 32), Image.Resampling.LANCZOS), dtype=np.float32)


def phash(a: np.ndarray) -> int:
    """The 64-bit pHash of a 32 × 32 grey array: the 8 × 8 lowest DCT frequencies above their median."""
    low = dctn(a, norm="ortho")[:8, :8]
    bits = (low > np.median(low)).flatten()
    return int(np.packbits(bits).view(">u8")[0])


def dihedral(a: np.ndarray) -> list[np.ndarray]:
    """The 4 rotations of `a` and of its mirror image."""
    return [np.rot90(b, k) for b in (a, np.fliplr(a)) for k in range(4)]


def file_hash(path: Path) -> int | None:
    try:
        return phash(gray32(path))
    except (OSError, ValueError, Image.DecompressionBombError):
        return None


def hamming(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Bit distances between every hash of `a` [n] and of `b` [m], as an [n, m] array."""
    return np.bitwise_count(a[:, None] ^ b[None, :]).astype(np.int16)


# --- the training images ------------------------------------------------------------------------

def training_files(raw: Path = RAW) -> list[Path]:
    out = []
    for d, _, names in os.walk(raw, followlinks=True):
        out += [Path(d) / n for n in names if Path(n).suffix.lower() in IMAGE_SUFFIXES]
    return sorted(out)


def training_hashes(files: list[Path], cache: Path = CACHE, workers: int | None = None) -> np.ndarray:
    """Each file's hash as uint64 (0 for an unreadable file, reported apart), cached by path, size
    and mtime so a rerun only hashes new or changed files."""
    cache.mkdir(parents=True, exist_ok=True)
    table = cache / "hashes.tsv"
    known: dict[str, tuple[int, int, str]] = {}
    if table.exists():
        for line in table.read_text().splitlines()[1:]:
            p, size, mtime, h = line.split("\t")
            known[p] = (int(size), int(mtime), h)
    keys, todo = [], []
    for i, f in enumerate(files):
        st = f.stat()
        keys.append((str(f), st.st_size, st.st_mtime_ns))
        k = known.get(str(f))
        if not k or k[:2] != (st.st_size, st.st_mtime_ns):
            todo.append(i)
    hashes = [known.get(str(f), (0, 0, ""))[2] for f in files]
    if todo:
        with ProcessPoolExecutor(workers or max(1, (os.cpu_count() or 2) - 2), mp_context=mp.get_context("forkserver")) as pool:
            for i, h in zip(todo, pool.map(file_hash, [files[i] for i in todo], chunksize=64)):
                hashes[i] = "" if h is None else f"{h:016x}"
        rows = ["path\tsize\tmtime_ns\tphash"] + [f"{p}\t{s}\t{m}\t{h}" for (p, s, m), h in zip(keys, hashes)]
        table.write_text("\n".join(rows) + "\n")
    return np.array([int(h, 16) if h else 0 for h in hashes], dtype=np.uint64)


# --- the evaluation photos ----------------------------------------------------------------------

def eval_photos(manifest: Path, photos: Path) -> list[tuple[str, Path]]:
    """Each photo file the manifest names, once: (manifest path, file)."""
    seen: dict[str, Path] = {}
    for line in manifest.read_text().splitlines():
        if line.strip():
            for im in json.loads(line)["images"]:
                seen.setdefault(im["file"], photos / im["file"])
    return sorted(seen.items())


def allowed(path: Path = ALLOWED) -> set[tuple[str, str]]:
    """(evaluation file, training path relative to data/raw) pairs looked at and found different."""
    if not path.exists():
        return set()
    rows = [l.split("\t") for l in path.read_text().splitlines()[1:] if l.strip() and not l.startswith("#")]
    return {(r[0], r[1]) for r in rows}


def nearest_by_hash(eval_arrays: list[np.ndarray], train: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """For each evaluation photo, every training image's smallest distance over its 8 variants
    [n, m], and the nearest training index [n]. Unreadable training files (hash 0) never match."""
    dist = np.empty((len(eval_arrays), len(train)), dtype=np.int16)
    for i, a in enumerate(eval_arrays):
        variants = np.array([phash(v) for v in dihedral(a)], dtype=np.uint64)
        dist[i] = hamming(variants, train).min(axis=0)
    dist[:, train == 0] = 64
    return dist, dist.argmin(axis=1)


def cosine_matrix(eval_files: list[Path], train_files: list[Path]) -> np.ndarray:
    from ml.pdm_vision.features import Embedder   # needs onnxruntime-gpu; imported only when used

    emb = Embedder()
    e = emb.files(eval_files)
    t = emb.files(train_files)
    e /= np.linalg.norm(e, axis=1, keepdims=True)
    t /= np.linalg.norm(t, axis=1, keepdims=True)
    return e @ t.T


# --- report -------------------------------------------------------------------------------------

def sheet(pairs: list[tuple[Path, Path, str]], out: Path, side: int = 256) -> None:
    """The pairs side by side, one row each, captioned."""
    if not pairs:
        return
    canvas = Image.new("RGB", (2 * side + 30, len(pairs) * (side + 24)), "white")
    draw = ImageDraw.Draw(canvas)
    for r, (a, b, caption) in enumerate(pairs):
        for c, p in enumerate((a, b)):
            with Image.open(p) as im:
                im = ImageOps.exif_transpose(im).convert("RGB")
                im.thumbnail((side, side))
                canvas.paste(im, (c * (side + 30), r * (side + 24) + 20))
        draw.text((4, r * (side + 24) + 4), caption, fill="black")
    out.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(out, quality=85)


def run(args) -> int:
    photos = eval_photos(args.manifest, args.photos)
    files = training_files(args.raw)
    print(f"{len(photos)} evaluation photos against {len(files)} training images", file=sys.stderr)
    train = training_hashes(files)
    unreadable = [str(f.relative_to(args.raw)) for f, h in zip(files, train) if h == 0]
    dist, near_h = nearest_by_hash([gray32(p) for _, p in photos], train)
    cos = None if args.no_bioclip else cosine_matrix([p for _, p in photos], files)
    near_c = None if cos is None else cos.argmax(axis=1)
    ok = allowed(args.allowed)

    rel = lambda i: str(files[i].relative_to(args.raw))
    matches, per_photo = [], []
    for i, (name, path) in enumerate(photos):
        hit = set(np.flatnonzero(dist[i] <= args.hamming).tolist())
        if cos is not None:
            hit |= set(np.flatnonzero(cos[i] >= args.cosine).tolist())
        for j in sorted(hit):
            matches.append({"photo": name, "training": rel(j), "hamming": int(dist[i, j]),
                            "cosine": None if cos is None else round(float(cos[i, j]), 4),
                            "allowed": (name, rel(j)) in ok})
        per_photo.append({"photo": name,
                          "nearest_hash": rel(int(near_h[i])), "hamming": int(dist[i, near_h[i]]),
                          "nearest_cosine": None if cos is None else rel(int(near_c[i])),
                          "cosine": None if cos is None else round(float(cos[i, near_c[i]]), 4)})
    failing = [m for m in matches if not m["allowed"]]

    out_dir = RUNS / args.out
    by_hash = sorted(per_photo, key=lambda r: r["hamming"])[:12]
    sheet([(args.photos / r["photo"], args.raw / r["nearest_hash"], f"{r['photo']}  hamming {r['hamming']}") for r in by_hash],
          out_dir / "nearest-by-hash.jpg")
    if cos is not None:
        by_cos = sorted(per_photo, key=lambda r: -r["cosine"])[:12]
        sheet([(args.photos / r["photo"], args.raw / r["nearest_cosine"], f"{r['photo']}  cosine {r['cosine']}") for r in by_cos],
              out_dir / "nearest-by-cosine.jpg")
    sheet([(args.photos / m["photo"], args.raw / m["training"], f"{m['photo']}  h {m['hamming']}  c {m['cosine']}") for m in matches[:40]],
          out_dir / "matches.jpg")

    REPORTS.mkdir(parents=True, exist_ok=True)
    summary = {"date": args.date, "photos": len(photos), "training_images": len(files),
               "unreadable_training_images": unreadable, "hamming_max": args.hamming,
               "cosine_min": None if cos is None else args.cosine, "matches": matches, "per_photo": per_photo}
    (REPORTS / f"{args.out}.json").write_text(json.dumps(summary, indent=2) + "\n")

    hist = np.bincount(np.minimum(dist.min(axis=1), 32), minlength=33)
    lines = [f"# Leakage check: {args.out}", "",
             f"{len(photos)} evaluation photos (`{args.manifest.relative_to(ROOT)}`) against {len(files):,} training images "
             f"under `ml/training/data/raw`, {len(unreadable)} of them unreadable.",
             f"A match is a perceptual-hash distance of {args.hamming} bits or less over the photo's 8 rotations and flips"
             + ("" if cos is None else f", or a BioCLIP 2 cosine of {args.cosine} or more") + ".", "",
             f"**{len(matches)} match{'' if len(matches) == 1 else 'es'}, {len(failing)} not allowed.**", ""]
    if matches:
        lines += ["| Evaluation photo | Training image | Hamming | Cosine | Allowed |", "|---|---|---|---|---|"]
        lines += [f"| `{m['photo']}` | `{m['training']}` | {m['hamming']} | {m['cosine']} | {'yes' if m['allowed'] else 'no'} |" for m in matches]
        lines.append("")
    lines += ["## Nearest training image per photo", "",
              "Smallest hash distance per photo, in bits (64 is as far as two hashes can be):", "",
              "| Bits | " + " | ".join(str(b) for b in range(0, 33, 4)) + " |",
              "|---|" + "---|" * 9,
              "| Photos at or under | " + " | ".join(str(int(hist[: b + 1].sum())) for b in range(0, 33, 4)) + " |", ""]
    if cos is not None:
        top = np.array([r["cosine"] for r in per_photo])
        lines += [f"Highest cosine per photo: median {np.median(top):.3f}, 95th percentile {np.percentile(top, 95):.3f}, "
                  f"maximum {top.max():.3f}.", ""]
        lines += ["The 10 closest pairs by cosine:", "", "| Evaluation photo | Training image | Cosine | Hamming |", "|---|---|---|---|"]
        idx = {r["photo"]: i for i, r in enumerate(per_photo)}
        for r in sorted(per_photo, key=lambda r: -r["cosine"])[:10]:
            j = files.index(args.raw / r["nearest_cosine"])
            lines.append(f"| `{r['photo']}` | `{r['nearest_cosine']}` | {r['cosine']} | {int(dist[idx[r['photo']], j])} |")
        lines.append("")
    (REPORTS / f"{args.out}.md").write_text("\n".join(lines))
    print("\n".join(lines[:8]))
    return 1 if failing else 0


def main(argv=None) -> int:
    today = dt.date.today().isoformat()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl")
    ap.add_argument("--photos", type=Path, default=EVAL / "photos")
    ap.add_argument("--raw", type=Path, default=RAW)
    ap.add_argument("--allowed", type=Path, default=ALLOWED)
    ap.add_argument("--hamming", type=int, default=5, help="largest hash distance that is a copy (bits)")
    ap.add_argument("--cosine", type=float, default=0.98, help="smallest BioCLIP cosine that is a copy (training 4)")
    ap.add_argument("--no-bioclip", action="store_true", help="the hash only, without a GPU")
    ap.add_argument("--out", default=f"leakage-{today}")
    args = ap.parse_args(argv)
    args.date = today
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
