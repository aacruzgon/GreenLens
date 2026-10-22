"""Photos of people near plants from Open Images V7, to check the upload's face blur (task M7).

    python -m ml.training.data.fetch openimages_v7_faces    # the tables, pinned in sources.yaml
    python -m ml.training.data.faces [--per-bucket 20] [--workers 4] [--cached-only]

Evaluation only: the photos are never training data, never committed and never put in the
model repository. They stay under training/eval/faces/ (git-ignored).

Candidates, from the validation and test splits only (no train tables, to keep downloads small):
1. a `Human face` box (/m/0dzct) and a human-verified positive label for Plant, Houseplant,
   Flower, Tree or Flowerpot;
2. no face box marked as a group (`IsGroupOf`) or a depiction (`IsDepiction`), so every face in
   the photo has its own box;
3. a CC BY 2.0 licence in the image-information table, and a Flickr original of at least
   MIN_BYTES (smaller files are under 2,000 px).

Open Images' own copy of each candidate (the `open-images-dataset` bucket, about 1,024 px) gives
its aspect ratio, so the smallest face's width in the stored image (1,600 px long side, the size
`api` keeps) is known before the original is fetched. Candidates are taken in a fixed order (a
hash of the image ID) within each bucket of that width (BUCKETS), until each bucket has
--per-bucket photos. For each one:
4. the Flickr original through `OriginalURL`, checked against `OriginalMD5`; a dead link or a
   changed file is dropped;
5. made upright: its EXIF orientation applied, then the quarter turn that best matches Open
   Images' copy, to which the boxes are normalised (correlation of 64 x 64 grey thumbnails);
   dropped when the best is under MIN_MATCH or less than MARGIN ahead of the other turn of the
   same shape;
6. dropped when its long side is under MIN_LONG_SIDE;
7. saved as JPEG quality 95 without metadata to training/eval/faces/photos/<id>.jpg.

Writes splits/faces_openimages.tsv (COLUMNS): the image, its split and file, the upright size,
the original's bytes, licence, author and Flickr page, the number of faces, the smallest face's
width in the stored image and at full resolution, the turn applied after EXIF and the match.
Downloads are cached under training/eval/faces/cache/, so a rerun fetches nothing twice.
"""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import html
import io
import json
import sys
import threading
import time
import urllib.error
import urllib.request
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parents[3]
TABLES = ROOT / "ml/training/data/raw/openimages_v7_faces"
FACES = ROOT / "ml/training/eval/faces"
PHOTOS = FACES / "photos"
CACHE = FACES / "cache"
INDEX = ROOT / "ml/training/data/splits/faces_openimages.tsv"

FACE = "/m/0dzct"
PLANTS = {"/m/05s2s": "Plant", "/m/03fp41": "Houseplant", "/m/0c9ph5": "Flower", "/m/07j7r": "Tree",
          "/m/0fm3zh": "Flowerpot"}
SPLITS = {
    "validation": ("validation-annotations-bbox.csv", "oidv7-val-annotations-human-imagelabels.csv",
                   "validation-images-with-rotation.csv"),
    "test": ("test-annotations-bbox.csv", "oidv7-test-annotations-human-imagelabels.csv",
             "test-images-with-rotation.csv"),
}
LICENCES = {"https://creativecommons.org/licenses/by/2.0/": "CC BY 2.0"}
S3 = "https://s3.amazonaws.com/open-images-dataset/{split}/{id}.jpg"
HEADERS = {"User-Agent": "PDM-GreenLens-eval/0.1 (+https://github.com/aacruzgon/GreenLens)"}
CACHED_ONLY = False  # --cached-only: use what earlier runs downloaded, fetch nothing new

STORED_LONG_SIDE = 1600   # UploadConfig::default_long_side
MIN_LONG_SIDE = 2000
MIN_BYTES = 400_000
MIN_MATCH = 0.90
MARGIN = 0.05
# The smallest face's width in the stored image, px: [low, high).
BUCKETS = ((0, 20), (20, 40), (40, 80), (80, 160), (160, 100_000))
COLUMNS = ("image_id", "split", "file", "width", "height", "bytes", "licence", "author", "landing_url",
           "faces", "smallest_face_stored", "smallest_face_full", "exif", "turn", "match")

# Clockwise quarter turns as PIL transposes (PIL's ROTATE_90 turns counter-clockwise).
TURNS = {90: Image.Transpose.ROTATE_270, 180: Image.Transpose.ROTATE_180, 270: Image.Transpose.ROTATE_90}


# --- tables ---------------------------------------------------------------------------------------

def read_faces(path: Path, ids: set[str] | None = None) -> dict[str, list[dict]]:
    """Every `Human face` box per image: x0, x1, y0, y1 (normalised) and the attributes."""
    out: dict[str, list[dict]] = defaultdict(list)
    with path.open(newline="") as f:
        for r in csv.DictReader(f):
            if r["LabelName"] != FACE or (ids is not None and r["ImageID"] not in ids):
                continue
            out[r["ImageID"]].append({
                "x0": float(r["XMin"]), "x1": float(r["XMax"]), "y0": float(r["YMin"]), "y1": float(r["YMax"]),
                "occluded": int(r["IsOccluded"]), "truncated": int(r["IsTruncated"]),
                "group": int(r["IsGroupOf"]), "depiction": int(r["IsDepiction"]),
            })
    return dict(out)


def read_plant_labels(path: Path) -> dict[str, set[str]]:
    """The plant labels a person verified as present, per image."""
    out: dict[str, set[str]] = defaultdict(set)
    with path.open(newline="") as f:
        for r in csv.DictReader(f):
            if r["LabelName"] in PLANTS and float(r["Confidence"]) == 1.0:
                out[r["ImageID"]].add(PLANTS[r["LabelName"]])
    return dict(out)


def read_info(path: Path, ids: set[str]) -> dict[str, dict]:
    with path.open(newline="") as f:
        return {r["ImageID"]: r for r in csv.DictReader(f) if r["ImageID"] in ids}


def select(faces: dict[str, list[dict]], plants: dict[str, set[str]], info: dict[str, dict]) -> tuple[list[str], Counter]:
    """Candidate image IDs (steps 1-3), in a fixed order, and why the others were left out."""
    why: Counter = Counter()
    keep = []
    for image_id, boxes in faces.items():
        if image_id not in plants:
            why["no verified plant label"] += 1
        elif any(b["group"] == 1 or b["depiction"] == 1 for b in boxes):
            why["a group or depiction face box"] += 1
        elif image_id not in info:
            why["no image information"] += 1
        elif info[image_id]["License"] not in LICENCES:
            why["licence not CC BY 2.0"] += 1
        elif int(info[image_id]["OriginalSize"] or 0) < MIN_BYTES:
            why[f"original under {MIN_BYTES:,} bytes"] += 1
        else:
            keep.append(image_id)
    return sorted(keep, key=order), why


def order(image_id: str) -> str:
    return hashlib.sha256(image_id.encode()).hexdigest()


# --- sizes ----------------------------------------------------------------------------------------

def fit_long_side(size: tuple[int, int], long_side: int = STORED_LONG_SIDE) -> tuple[int, int]:
    """`imaging::fit_long_side`: the size with the long side at most `long_side`, never upscaled."""
    w, h = size
    if max(w, h) <= long_side:
        return w, h
    s = long_side / max(w, h)
    return max(1, round(w * s)), max(1, round(h * s))


def stored_shape(size: tuple[int, int], long_side: int = STORED_LONG_SIDE) -> tuple[int, int]:
    """The stored size of a photo of this shape whose original is larger than `long_side`, as
    every kept original is: unlike `fit_long_side`, a small copy is scaled up to it."""
    w, h = size
    s = long_side / max(w, h)
    return max(1, round(w * s)), max(1, round(h * s))


def face_width(box: dict, size: tuple[int, int]) -> float:
    """A face's width in pixels of an image of `size` (its box's width: "across")."""
    return (box["x1"] - box["x0"]) * size[0]


def bucket_of(width_px: float) -> int:
    return next(i for i, (lo, hi) in enumerate(BUCKETS) if lo <= width_px < hi)


def bucket_name(i: int) -> str:
    lo, hi = BUCKETS[i]
    return f"{lo}+ px" if hi >= 100_000 else f"{lo}-{hi} px"


# --- upright ------------------------------------------------------------------------------------

def grey(im: Image.Image, side: int = 64) -> np.ndarray:
    a = np.asarray(im.convert("L").resize((side, side), Image.Resampling.BILINEAR), dtype=np.float64)
    return (a - a.mean()) / (a.std() + 1e-9)


def turn_scores(upright: Image.Image, reference: Image.Image, side: int = 64, aspect_tol: float = 0.03) -> dict[int, float]:
    """The correlation with `reference` of `upright` turned clockwise by 0, 90, 180 and 270
    degrees, for the turns whose aspect ratio matches the reference's within `aspect_tol`."""
    rw, rh = reference.size
    ref = grey(reference, side)
    small = upright.copy()
    small.thumbnail((4 * side, 4 * side))
    out = {}
    for turn in (0, 90, 180, 270):
        im = small if turn == 0 else small.transpose(TURNS[turn])
        w, h = upright.size if turn in (0, 180) else upright.size[::-1]
        if abs((w / h) / (rw / rh) - 1) > aspect_tol:
            continue
        out[turn] = float((grey(im, side) * ref).mean())
    return out


def best_turn(scores: dict[int, float]) -> tuple[int | None, float, str]:
    """The turn to apply, its correlation, and why none was chosen (empty when one was)."""
    if not scores:
        return None, 0.0, "shape matches no turn"
    ranked = sorted(scores.items(), key=lambda kv: -kv[1])
    turn, score = ranked[0]
    if score < MIN_MATCH:
        return None, score, "poor match"
    if len(ranked) > 1 and score - ranked[1][1] < MARGIN:
        return None, score, "ambiguous turn"
    return turn, score, ""


def turned(im: Image.Image, turn: int) -> Image.Image:
    return im if turn == 0 else im.transpose(TURNS[turn])


# --- downloads ----------------------------------------------------------------------------------

def get(url: str, timeout: int = 120) -> bytes:
    """The body at `url`. Flickr answers 429 when fetched too fast: wait and try again, 3 times."""
    for wait in (30, 60, 120, None):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=timeout) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code != 429 or wait is None:
                raise
            time.sleep(wait)
    raise AssertionError("unreachable")


def cached(path: Path, url: str) -> bytes | None:
    """The file at `url`, from the cache when there; None when the server says it's gone (that
    answer is cached too, as an empty `<name>.gone`)."""
    gone = path.with_suffix(".gone")
    if path.exists():
        return path.read_bytes()
    if gone.exists():
        return None
    if CACHED_ONLY:
        raise FileNotFoundError(f"not downloaded yet (--cached-only): {path.name}")
    try:
        data = get(url)
    except urllib.error.HTTPError as e:
        if e.code in (403, 404, 410):
            gone.parent.mkdir(parents=True, exist_ok=True)
            gone.touch()
            return None
        raise
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".part")
    tmp.write_bytes(data)
    tmp.rename(path)
    return data


def reference(image_id: str, split: str) -> Image.Image | None:
    """Open Images' copy, or None when it can't be had."""
    try:
        data = cached(CACHE / "openimages" / f"{image_id}.jpg", S3.format(split=split, id=image_id))
        return None if data is None else Image.open(io.BytesIO(data))
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        print(f"  {image_id}: Open Images copy: {e}", file=sys.stderr)
        return None


def fetch_one(image_id: str, split: str, row: dict, ref: Image.Image) -> dict:
    """Steps 4-7 for one candidate: a status, and the index row when kept."""
    out = {"image_id": image_id, "status": ""}
    path = CACHE / "flickr" / f"{image_id}.jpg"
    try:
        data = cached(path, row["OriginalURL"])
    except FileNotFoundError:
        return out | {"status": "not downloaded"}
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        return out | {"status": f"download failed: {e}"}
    if data is None:
        return out | {"status": "original gone"}
    md5 = base64.b64encode(hashlib.md5(data).digest()).decode()
    if md5 != row["OriginalMD5"]:
        path.unlink(missing_ok=True)
        return out | {"status": "original changed (MD5)"}
    try:
        im = Image.open(io.BytesIO(data))
        exif = int(im.getexif().get(0x0112, 1) or 1)
        im = ImageOps.exif_transpose(im).convert("RGB")
    except (OSError, ValueError, SyntaxError) as e:
        return out | {"status": f"unreadable: {e}"}
    turn, match, why = best_turn(turn_scores(im, ref))
    if turn is None:
        return out | {"status": why, "match": round(match, 3)}
    im = turned(im, turn)
    if max(im.size) < MIN_LONG_SIDE:
        return out | {"status": f"long side under {MIN_LONG_SIDE} px", "size": im.size}
    PHOTOS.mkdir(parents=True, exist_ok=True)
    dest = PHOTOS / f"{image_id}.jpg"
    if not dest.exists():
        tmp = dest.with_suffix(".part")
        im.save(tmp, "JPEG", quality=95)   # no exif= argument: no metadata is written
        tmp.rename(dest)
    return out | {"status": "kept", "width": im.size[0], "height": im.size[1], "bytes": len(data),
                  "exif": exif, "turn": turn, "match": round(match, 3)}


def clean(text: str) -> str:
    """One line of plain text: Open Images' author names carry HTML entities (`&#x27;`)."""
    return " ".join(html.unescape(text).split())


# --- main ---------------------------------------------------------------------------------------

def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--per-bucket", type=int, default=20)
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--cached-only", action="store_true",
                    help="rebuild the index from earlier downloads; fetch nothing (Flickr answers 429 when pressed)")
    args = ap.parse_args(argv)
    global CACHED_ONLY
    CACHED_ONLY = args.cached_only

    candidates: list[tuple[str, str]] = []
    faces: dict[str, list[dict]] = {}
    info: dict[str, dict] = {}
    why: Counter = Counter()
    for split, (boxes_csv, labels_csv, info_csv) in SPLITS.items():
        f = read_faces(TABLES / boxes_csv)
        plants = read_plant_labels(TABLES / labels_csv)
        i = read_info(TABLES / info_csv, set(f))
        keep, w = select(f, plants, i)
        why += w
        candidates += [(image_id, split) for image_id in keep]
        faces |= {k: f[k] for k in keep}
        info |= {k: i[k] for k in keep}
        print(f"{split}: {len(f)} images with a face box, {len(keep)} candidates", file=sys.stderr)
    for reason, n in why.most_common():
        print(f"  left out: {reason}: {n}", file=sys.stderr)
    candidates.sort(key=lambda c: order(c[0]))

    # Open Images' copies: the aspect ratio, so the bucket, before any original is fetched.
    with ThreadPoolExecutor(args.workers) as pool:
        refs = dict(zip([c[0] for c in candidates],
                        pool.map(lambda c: reference(*c), candidates)))
    bucket: dict[str, int] = {}
    for image_id, _ in candidates:
        ref = refs[image_id]
        if ref is not None:
            stored = stored_shape(ref.size)
            bucket[image_id] = bucket_of(min(face_width(b, stored) for b in faces[image_id]))
    print(f"Open Images copies: {len(bucket)} of {len(candidates)}; per bucket "
          + ", ".join(f"{bucket_name(b)}: {n}" for b, n in sorted(Counter(bucket.values()).items())),
          file=sys.stderr)

    results: dict[str, dict] = {}
    lock = threading.Lock()
    for b in range(len(BUCKETS)):
        queue = [c for c in candidates if bucket.get(c[0]) == b]
        kept = 0
        while queue and kept < args.per_bucket:
            batch, queue = queue[: args.workers], queue[args.workers:]
            with ThreadPoolExecutor(args.workers) as pool:
                done = list(pool.map(lambda c: fetch_one(c[0], c[1], info[c[0]], refs[c[0]]), batch))
            for c, r in zip(batch, done):
                r["split"], r["bucket"] = c[1], b
                if r["status"] == "kept":
                    if kept >= args.per_bucket:
                        r["status"] = "bucket full"
                    else:
                        kept += 1
                with lock:
                    results[c[0]] = r
        print(f"bucket {bucket_name(b)}: {kept} kept", file=sys.stderr, flush=True)

    rows = []
    for image_id, r in sorted(results.items()):
        if r["status"] != "kept":
            continue
        row = info[image_id]
        size = (r["width"], r["height"])
        stored = fit_long_side(size)
        rows.append({
            "image_id": image_id, "split": r["split"], "file": f"photos/{image_id}.jpg",
            "width": r["width"], "height": r["height"], "bytes": r["bytes"],
            "licence": LICENCES[row["License"]], "author": clean(row["Author"]),
            "landing_url": row["OriginalLandingURL"], "faces": len(faces[image_id]),
            "smallest_face_stored": round(min(face_width(b, stored) for b in faces[image_id]), 1),
            "smallest_face_full": round(min(face_width(b, size) for b in faces[image_id]), 1),
            "exif": r["exif"], "turn": r["turn"], "match": r["match"],
        })
    INDEX.write_text("\t".join(COLUMNS) + "\n"
                     + "".join("\t".join(str(r[c]) for c in COLUMNS) + "\n" for r in rows))
    status = Counter(r["status"].split(":")[0] for r in results.values())
    (CACHE / "status.json").write_text(json.dumps(results, indent=1, default=str))
    summary = {
        "candidates": len(candidates), "left_out": dict(why.most_common()),
        "with_open_images_copy": len(bucket),
        "candidates_per_bucket": {bucket_name(b): n for b, n in sorted(Counter(bucket.values()).items())},
        "tried": len(results), "status": dict(status.most_common()),
        "tried_per_bucket": {bucket_name(b): dict(Counter(r["status"].split(":")[0] for r in results.values()
                                                          if r["bucket"] == b)) for b in range(len(BUCKETS))},
        "kept": len(rows), "faces": sum(r["faces"] for r in rows),
    }
    (FACES / "selection.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary, indent=1))
    print(f"{len(rows)} photos with {sum(r['faces'] for r in rows)} faces in {INDEX}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
