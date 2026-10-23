"""Checks the evaluation photos against every training image (evaluation 2 and 9): a photo that
is a copy of a training image would measure memory, not generalisation.

    python -m ml.training.eval.check_leakage [--hamming 4] [--cosine 0.98] [--no-bioclip] [--calibrate 300] [--out leakage-<date>]

Every photo the manifest names is compared with every image file under `ml/training/data/raw`,
which holds every source any of our models trained or was tuned on, all splits. The run stops
first when a dataset in `sources.yaml`, or a COCO folder the detector or the plant check uses,
has no images there: the check would quietly cover less.

- **Perceptual hash:** a 64-bit DCT hash of the photo and of its 8 rotations and flips. A training
  image within `--hamming` bits of any of them is a copy: resized, recompressed, rotated or flipped.
  The hash sets the 32 coefficients above the median, so distances are even: 4 is the largest
  below 6, where the closest different photos sit (evaluation 2).
- **BioCLIP 2 cosine:** the embeddings the training code caches (`ml.pdm_vision.features`, on the
  GPU for files not cached yet). A pair at `--cosine` or more is a match. It backs the hash up but
  catches few crops: 24% of copies with 5% cut off each edge, 8% with 10% (`--calibrate`, evaluation
  2). The default is training 4's duplicate threshold. The field-set picks drop a
  candidate at 0.95 (`fieldsets.COPY`), which costs nothing there; here a match fails the run, and
  at 0.95 one tomato photo had 155 tomato training images above the bar (evaluation 2).

Either signal is a match. A match is looked at by eye: a copy drops the case, and a different photo
goes into `leakage_allowed.tsv` (evaluation file, training path, why) so it no longer fails.
The run exits 1 while any match isn't allowed.

`--calibrate N` remeasures both thresholds: N training images picked at random (seed 0) are edited
the ways a copy is (resized, recompressed, rotated, mirrored, brightened, cropped), and the report
gives the share of each edit that each signal still catches.

Training hashes are cached in `data/cache/phash/hashes.tsv`, keyed by path, size and mtime.
Writes `reports/<out>.md` and `.json`: every match, and per photo its nearest training image by
each signal; side-by-side sheets of the matches and the nearest pairs go to the git-ignored
`runs/leakage/<out>/`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import io
import json
import multiprocessing as mp
import os
import sys
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
import yaml
from PIL import Image, ImageDraw, ImageEnhance, ImageOps
from scipy.fft import dctn

from ml.pdm_vision import preprocess as pp

ROOT = Path(__file__).resolve().parents[3]
EVAL = ROOT / "ml/training/eval"
RAW = ROOT / "ml/training/data/raw"
SOURCES = ROOT / "ml/training/data/sources.yaml"
CACHE = ROOT / "ml/training/data/cache/phash"
REPORTS = EVAL / "reports"
RUNS = ROOT / "ml/training/runs/leakage"
ALLOWED = EVAL / "leakage_allowed.tsv"
COSINE = 0.98  # training 4's duplicate threshold
IMAGE_SUFFIXES = {".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff"}
NO_IMAGES = {"coco_annotations_2017"}  # datasets in sources.yaml that are annotations only
# COCO photos fetched outside sources.yaml: the detector's backgrounds (detector/background.py)
# and the plant check's hold-out negatives (data/negatives_holdout.py).
COCO_FOLDERS = ("coco_train2017_bg", "coco_train2017_holdout")


# --- perceptual hash ----------------------------------------------------------------------------

def gray32(src) -> np.ndarray:
    """The image (a path or a file object), upright by its EXIF orientation, in grey at 32 × 32."""
    with Image.open(src) as im:
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
    """The file's hash, or None when it can't be read. A blank image hashes to 0 and is readable."""
    try:
        return phash(gray32(path))
    except (OSError, ValueError, Image.DecompressionBombError):
        return None


def hamming(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Bit distances between every hash of `a` [n] and of `b` [m], as an [n, m] array."""
    return np.bitwise_count(a[:, None] ^ b[None, :]).astype(np.int16)


def nearest_variant(gray: np.ndarray, hashes: np.ndarray) -> np.ndarray:
    """Each hash's smallest distance to the 8 rotations and flips of `gray` [m]."""
    variants = np.array([phash(v) for v in dihedral(gray)], dtype=np.uint64)
    return hamming(variants, hashes).min(axis=0)


def pool(workers: int | None = None) -> ProcessPoolExecutor:
    return ProcessPoolExecutor(workers or max(1, (os.cpu_count() or 2) - 2), mp_context=mp.get_context("forkserver"))


# --- the training images ------------------------------------------------------------------------

def training_files(raw: Path = RAW) -> list[Path]:
    out = []
    for d, _, names in os.walk(raw, followlinks=True):
        out += [Path(d) / n for n in names if Path(n).suffix.lower() in IMAGE_SUFFIXES]
    return sorted(out)


def required_folders(sources: Path = SOURCES) -> list[str]:
    """The folders under `raw` that must hold images: every dataset in sources.yaml that has
    images, and the COCO photos the detector trains on and the plant check is tuned on."""
    names = [d["name"] for d in yaml.safe_load(sources.read_text())["datasets"]]
    return [n for n in names if n not in NO_IMAGES] + list(COCO_FOLDERS)


def per_folder(files: list[Path], raw: Path) -> dict[str, int]:
    """Images per top-level folder under `raw`."""
    return dict(sorted(Counter(f.relative_to(raw).parts[0] for f in files).items()))


def training_hashes(files: list[Path], cache: Path | None = None, workers: int | None = None) -> tuple[np.ndarray, np.ndarray]:
    """Each file's hash as uint64, and whether it could be read (an unreadable file never matches
    and is reported apart). Cached by path, size and mtime, so a rerun only hashes new or changed
    files. The cache defaults to CACHE as it is at call time, so a test's monkeypatch reaches it."""
    cache = CACHE if cache is None else cache
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
        with pool(workers) as ex:
            for i, h in zip(todo, ex.map(file_hash, [files[i] for i in todo], chunksize=64)):
                hashes[i] = "" if h is None else f"{h:016x}"
        rows = ["path\tsize\tmtime_ns\tphash"] + [f"{p}\t{s}\t{m}\t{h}" for (p, s, m), h in zip(keys, hashes)]
        table.write_text("\n".join(rows) + "\n")
    return (np.array([int(h, 16) if h else 0 for h in hashes], dtype=np.uint64),
            np.array([bool(h) for h in hashes], dtype=bool))


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


def nearest_by_hash(eval_arrays: list[np.ndarray], train: np.ndarray,
                    readable: np.ndarray | None = None) -> tuple[np.ndarray, np.ndarray]:
    """For each evaluation photo, every training image's smallest distance over its 8 variants
    [n, m], and the nearest training index [n]. Unreadable training files never match."""
    dist = np.empty((len(eval_arrays), len(train)), dtype=np.int16)
    for i, a in enumerate(eval_arrays):
        dist[i] = nearest_variant(a, train)
    if readable is not None:
        dist[:, ~readable] = 64
    return dist, dist.argmin(axis=1)


def embedder():
    from ml.pdm_vision.features import Embedder   # needs onnxruntime-gpu; imported only when used

    return Embedder()


def unit(v: np.ndarray) -> np.ndarray:
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def cosine_matrix(emb, eval_files: list[Path], train_files: list[Path]) -> np.ndarray:
    return unit(emb.files(eval_files)) @ unit(emb.files(train_files)).T


# --- calibration --------------------------------------------------------------------------------

def _crop(im: Image.Image, f: float) -> Image.Image:
    dx, dy = round(im.width * f), round(im.height * f)
    return im.crop((dx, dy, im.width - dx, im.height - dy))


def _scale(im: Image.Image, f: int) -> Image.Image:
    return im.resize((max(1, im.width // f), max(1, im.height // f)), Image.Resampling.LANCZOS)


# The ways a copy is edited: (edit, JPEG quality the copy is saved at).
EDITS = {
    "half size": (lambda im: _scale(im, 2), 90),
    "quarter size": (lambda im: _scale(im, 4), 90),
    "JPEG quality 40": (lambda im: im, 40),
    "rotated 90°": (lambda im: im.transpose(Image.Transpose.ROTATE_90), 90),
    "mirrored": (lambda im: im.transpose(Image.Transpose.FLIP_LEFT_RIGHT), 90),
    "15% brighter": (lambda im: ImageEnhance.Brightness(im).enhance(1.15), 90),
    "5% off each edge": (lambda im: _crop(im, 0.05), 90),
    "10% off each edge": (lambda im: _crop(im, 0.10), 90),
}


def edited_copies(task: tuple[Path, int, bool]) -> tuple[list[int], np.ndarray | None]:
    """One training image's copies, each saved as a JPEG: their hash distances to the original's
    hash, and their BioCLIP input tensors when asked for."""
    path, original, tensors = task
    im = pp.load_rgb(path)
    bits, out = [], []
    for edit, quality in EDITS.values():
        data = io.BytesIO()
        edit(im).save(data, "JPEG", quality=quality)
        data.seek(0)
        bits.append(int(nearest_variant(gray32(data), np.array([original], dtype=np.uint64))[0]))
        if tensors:
            data.seek(0)
            out.append(pp.to_tensor(pp.load_rgb(data), pp.BIOCLIP))
    return bits, (np.stack(out) if tensors else None)


def calibrate(files: list[Path], train: np.ndarray, readable: np.ndarray, n: int, hamming_max: int,
              cosine_min: float, emb=None, workers: int | None = None, seed: int = 0) -> dict:
    """The share of each edit's copies that each signal catches, over `n` training images picked
    at random from the readable ones."""
    rng = np.random.default_rng(seed)
    pick = sorted(rng.choice(np.flatnonzero(readable), size=min(n, int(readable.sum())), replace=False).tolist())
    bits = np.zeros((len(pick), len(EDITS)), dtype=np.int16)
    cos = np.full((len(pick), len(EDITS)), np.nan, dtype=np.float32)
    originals = None if emb is None else unit(emb.files([files[i] for i in pick]))
    with pool(workers) as ex:
        tasks = [(files[i], int(train[i]), emb is not None) for i in pick]
        for r, (b, t) in enumerate(ex.map(edited_copies, tasks, chunksize=4)):
            bits[r] = b
            if t is not None:
                cos[r] = unit(emb.run(t)) @ originals[r]
    edits = {}
    for k, name in enumerate(EDITS):
        b, c = bits[:, k], cos[:, k]
        row = {"hash_caught": round(float(np.mean(b <= hamming_max)), 3),
               "bits_median": float(np.median(b)), "bits_max": int(b.max())}
        if emb is not None:
            row |= {"cosine_caught": round(float(np.mean(c >= cosine_min)), 3),
                    "cosine_median": round(float(np.median(c)), 4), "cosine_min": round(float(c.min()), 4),
                    "either_caught": round(float(np.mean((b <= hamming_max) | (c >= cosine_min))), 3)}
        edits[name] = row
    return {"images": len(pick), "seed": seed, "edits": edits}


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


def shown(path: Path) -> str:
    """`path` relative to the repository when it is inside it."""
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def calibration_lines(cal: dict, hamming_max: int, cosine_min: float | None) -> list[str]:
    head = f"| Edit | Hash ≤ {hamming_max} bits | Median bits | Most bits |"
    if cosine_min is not None:
        head += f" Cosine ≥ {cosine_min} | Median cosine | Lowest cosine | Either |"
    lines = ["## Calibration", "",
             f"Edited copies of {cal['images']} training images picked at random (seed {cal['seed']}), each saved as a JPEG "
             "at quality 90 unless the edit says otherwise, against their originals. The share of copies each signal catches:", "",
             head, "|" + "---|" * (head.count("|") - 1)]
    for name, r in cal["edits"].items():
        row = f"| {name} | {r['hash_caught']:.0%} | {r['bits_median']:g} | {r['bits_max']} |"
        if cosine_min is not None:
            row += f" {r['cosine_caught']:.0%} | {r['cosine_median']} | {r['cosine_min']} | {r['either_caught']:.0%} |"
        lines.append(row)
    return lines + [""]


def run(args) -> int:
    photos = eval_photos(args.manifest, args.photos)
    files = training_files(args.raw)
    folders = per_folder(files, args.raw)
    missing = [n for n in required_folders(args.sources) if not folders.get(n)]
    if missing:
        raise SystemExit(f"no images under {args.raw} for {', '.join(missing)}: fetch them first (ml/README.md), "
                         "or the check covers less than every source")
    print(f"{len(photos)} evaluation photos against {len(files)} training images", file=sys.stderr)
    train, readable = training_hashes(files)
    unreadable = [str(f.relative_to(args.raw)) for f, ok in zip(files, readable) if not ok]
    dist, near_h = nearest_by_hash([gray32(p) for _, p in photos], train, readable)
    emb = None if args.no_bioclip else embedder()
    cos = near_c = None
    if emb is not None:
        cols = np.flatnonzero(readable)   # an unreadable file would stop the embedder; it never matches
        cos = np.full((len(photos), len(files)), -1.0, dtype=np.float32)
        cos[:, cols] = cosine_matrix(emb, [p for _, p in photos], [files[j] for j in cols])
        near_c = cos.argmax(axis=1)
    cal = calibrate(files, train, readable, args.calibrate, args.hamming, args.cosine, emb) if args.calibrate else None
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
    manifest_sha = hashlib.sha256(args.manifest.read_bytes()).hexdigest()
    summary = {"date": args.date, "manifest": shown(args.manifest), "manifest_sha256": manifest_sha,
               "photos": len(photos), "training_images": len(files), "training_images_by_folder": folders,
               "unreadable_training_images": unreadable, "hamming_max": args.hamming,
               "cosine_min": None if cos is None else args.cosine, "matches": matches, "per_photo": per_photo,
               "calibration": cal}
    (REPORTS / f"{args.out}.json").write_text(json.dumps(summary, indent=2) + "\n")

    hist = np.bincount(np.minimum(dist.min(axis=1), 32), minlength=33)
    closest = min(per_photo, key=lambda r: r["hamming"])
    lines = [f"# Leakage check: {args.out}", "",
             f"{len(photos)} evaluation photos (`{shown(args.manifest)}`) against {len(files):,} training images "
             f"under `ml/training/data/raw`, {len(unreadable)} of them unreadable. The manifest's SHA-256 is `{manifest_sha}`.",
             f"A match is a perceptual-hash distance of {args.hamming} bits or less over the photo's 8 rotations and flips"
             + ("" if cos is None else f", or a BioCLIP 2 cosine of {args.cosine} or more") + ".", "",
             f"**{len(matches)} match{'' if len(matches) == 1 else 'es'}, {len(failing)} not allowed.**", ""]
    if matches:
        lines += ["| Evaluation photo | Training image | Hamming | Cosine | Allowed |", "|---|---|---|---|---|"]
        lines += [f"| `{m['photo']}` | `{m['training']}` | {m['hamming']} | {m['cosine']} | {'yes' if m['allowed'] else 'no'} |" for m in matches]
        lines.append("")
    lines += ["Training images per folder:", "", "| Folder | Images |", "|---|---|"]
    lines += [f"| `{k}` | {v:,} |" for k, v in folders.items()] + [""]
    lines += ["## Nearest training image per photo", "",
              "Smallest hash distance per photo, in bits (64 is as far as two hashes can be):", "",
              "| Bits | " + " | ".join(str(b) for b in range(0, 33, 4)) + " |",
              "|---|" + "---|" * 9,
              "| Photos at or under | " + " | ".join(str(int(hist[: b + 1].sum())) for b in range(0, 33, 4)) + " |", "",
              f"The closest pair by hash is at {closest['hamming']} bits: `{closest['photo']}` and `{closest['nearest_hash']}`.", ""]
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
    if cal is not None:
        lines += calibration_lines(cal, args.hamming, None if cos is None else args.cosine)
    (REPORTS / f"{args.out}.md").write_text("\n".join(lines))
    print("\n".join(lines[:8]))
    return 1 if failing else 0


def parse_args(argv=None) -> argparse.Namespace:
    today = dt.date.today().isoformat()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl")
    ap.add_argument("--photos", type=Path, default=EVAL / "photos")
    ap.add_argument("--raw", type=Path, default=RAW)
    ap.add_argument("--sources", type=Path, default=SOURCES, help="the datasets whose folders must hold images")
    ap.add_argument("--allowed", type=Path, default=ALLOWED)
    ap.add_argument("--hamming", type=int, default=4, help="largest hash distance that is a copy (bits; distances are even)")
    ap.add_argument("--cosine", type=float, default=COSINE, help="smallest BioCLIP cosine that is a match (training 4)")
    ap.add_argument("--no-bioclip", action="store_true", help="the hash only, without a GPU")
    ap.add_argument("--calibrate", type=int, default=0, metavar="N",
                    help="also measure both thresholds on edited copies of N training images")
    ap.add_argument("--out", default=f"leakage-{today}")
    args = ap.parse_args(argv)
    args.date = today
    return args


def main(argv=None) -> int:
    return run(parse_args(argv))


if __name__ == "__main__":
    sys.exit(main())
