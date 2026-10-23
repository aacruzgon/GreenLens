"""Fit the spot segmenter's colour thresholds on PlantSeg's lesion masks (evaluation 6, M15).

    python -m ml.training.eval.spot_fit masks      # leaf masks from the worker's own models (CPU, ~25 min)
    python -m ml.training.eval.spot_fit fit        # fit on the fit half, score both halves, write the report
    python -m ml.training.eval.spot_fit clean-masks --photos DIR   # leaf masks of the healthy and diseased check photos
    python -m ml.training.eval.spot_fit clean      # the speck filter's share, and how often a photo reads clean
    python -m ml.training.eval.spot_fit parity     # the Rust segmenter against this copy, on hold-out leaves

`api`'s measurement stage runs `ColourSpotSegmenter` (backend/crates/vision/src/measure/spots.rs)
inside each leaf's SAM mask: necrotic pixels become the leaf's spots, and necrotic and halo pixels
together its `affected_pct`. The thresholds were starting values. This script:

1. **masks:** runs the perception worker's segmentation stage (the leaf detector, then SAM, from
   `manifest.cpu.toml`'s fp32 files) on every photo of data/splits/spots_plantseg.tsv through
   the `vision` crate's `perceive` example, and caches the leaf regions (COCO RLE masks).
2. **fit:** classifies each leaf's pixels with a NumPy copy of `ColourSpotSegmenter::classify`
   (the same HSV formula, the leaf's median value, the same comparisons in the same order) and
   scores them against PlantSeg's lesion mask inside the leaf: per photo, the lesion IoU of the
   affected pixels (necrotic or halo), the healthy leaf's IoU, and their mean, the two-class IoU.
   Coordinate descent over the ten `SpotThresholds` fields maximises the mean two-class IoU on
   the fit half's `spots` photos (PlantSeg's training split, diseases whose masks outline
   discrete spots); the hold-out half (its validation and test splits) and the `areas` photos
   only score the defaults and the fitted values. Brown stays below the halo's yellow, never
   overlapping it, so a spot's size doesn't take in its halo.
3. **clean-masks, clean:** the measurement's `clean_images` rule ([contracts 7]) counts a photo as
   evidence of health only when no leaf has a spot, so a speck of glare breaks it. Necrotic patches
   under `min_spot_share` of their leaf become leaf. The share is the largest on the grid that
   leaves PlantSeg's fit-half `spots` photos almost never clean (at most one point more often
   than with no filter); its hold-out, PlantSeg's `areas`, and the check photos at a photo with
   depth's stored size (data/splits/spots_clean.tsv: healthy dev cases and PlantDoc leaves, and the dev cases
   with spot diseases) only score it.
4. **parity:** runs the real segmenter (the `spot_mask` example) with the fitted values on
   hold-out leaves and requires the same class on at least 99.99% of their pixels.

PlantSeg labels lesions without telling a halo apart, so the halo fields are fitted only through
the affected pixels, never on their own.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import random
import subprocess
import sys
import time
from datetime import date
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "ml/training/data"
SPLIT = DATA / "splits/spots_plantseg.tsv"
CLEAN = DATA / "splits/spots_clean.tsv"
RAW = DATA / "raw"
CACHE = DATA / "cache/spots"
REPORTS = ROOT / "ml/training/eval/reports"
RUNS = ROOT / "ml/training/runs/spots"
BACKEND = ROOT / "backend"
MANIFEST = Path.home() / ".cache/pdm-models-cpu/manifest.toml"

# SpotThresholds::default() in spots.rs, in its field order.
DEFAULTS = {
    "dark_value": 0.45,
    "brown_hue_min": 10.0,
    "brown_hue_max": 45.0,
    "brown_saturation": 0.25,
    "brown_value": 0.85,
    "pale_saturation": 0.15,
    "pale_value": 1.25,
    "halo_hue_min": 45.0,
    "halo_hue_max": 70.0,
    "halo_saturation": 0.35,
    "min_spot_share": 0.0,  # colour-1 kept every patch
}
OUTSIDE, LEAF, NECROTIC, HALO = 0, 1, 2, 3


# --- the segmenter, as spots.rs writes it ---------------------------------------------------
def hsv(rgb: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """`spots::hsv` for an (..., 3) uint8 array: hue in degrees, saturation and value 0-1, f64."""
    c = rgb.astype(np.float64) / 255.0
    r, g, b = c[..., 0], c[..., 1], c[..., 2]
    mx = np.maximum(np.maximum(r, g), b)
    mn = np.minimum(np.minimum(r, g), b)
    delta = mx - mn
    safe = np.where(delta == 0.0, 1.0, delta)
    hue = np.where(
        delta == 0.0, 0.0,
        np.where(mx == r, 60.0 * np.mod((g - b) / safe, 6.0),
                 np.where(mx == g, 60.0 * ((b - r) / safe + 2.0), 60.0 * ((r - g) / safe + 4.0))))
    sat = np.where(mx == 0.0, 0.0, delta / np.where(mx == 0.0, 1.0, mx))
    return hue, sat, mx


def leaf_value(values: np.ndarray) -> float:
    """The leaf's median value as `segment` takes it: the sorted values' element at len / 2."""
    if values.size == 0:
        return 0.5
    return float(np.partition(values, values.size // 2)[values.size // 2])


def classify(h: np.ndarray, s: np.ndarray, v: np.ndarray, lv: float | np.ndarray, t: dict) -> np.ndarray:
    """`ColourSpotSegmenter::classify` per pixel: LEAF, NECROTIC or HALO. `lv` is the leaf's
    median value (a scalar, or one per pixel when several leaves are stacked)."""
    dark = v < lv * t["dark_value"]
    brown = (h >= t["brown_hue_min"]) & (h < t["brown_hue_max"]) & (s >= t["brown_saturation"]) & (v < lv * t["brown_value"])
    pale = (s < t["pale_saturation"]) & (v > lv * t["pale_value"])
    halo = (h >= t["halo_hue_min"]) & (h < t["halo_hue_max"]) & (s >= t["halo_saturation"])
    return np.where(dark | brown | pale, NECROTIC, np.where(halo, HALO, LEAF)).astype(np.uint8)


def segment(rgb: np.ndarray, leaf: np.ndarray, t: dict, hsv_: tuple | None = None) -> np.ndarray:
    """`ColourSpotSegmenter::segment`: a class per pixel, OUTSIDE where `leaf` is false, with
    necrotic patches under `min_spot_share` of the leaf turned to leaf. `hsv_` saves recomputing
    the photo's HSV for each of its leaves."""
    h, s, v = hsv_ if hsv_ is not None else hsv(rgb)
    out = np.full(leaf.shape, OUTSIDE, dtype=np.uint8)
    inside = leaf.astype(bool)
    lv = leaf_value(v[inside])
    out[inside] = classify(h[inside], s[inside], v[inside], lv, t)
    return drop_specks(out, int(inside.sum()), t.get("min_spot_share", 0.0))


def patches(nec: np.ndarray) -> tuple[np.ndarray, int]:
    """The 4-connected components of `nec`, as `mask::components` finds them: a label image and
    their count."""
    from scipy import ndimage
    labels, n = ndimage.label(nec)  # the default structure is the 4-neighbourhood
    return labels, int(n)


def drop_specks(cls: np.ndarray, leaf_pixels: int, share: float) -> np.ndarray:
    """Necrotic patches smaller than `share` x the leaf's pixels become LEAF."""
    min_area = share * leaf_pixels
    if min_area <= 0:
        return cls
    labels, n = patches(cls == NECROTIC)
    if n == 0:
        return cls
    small = np.bincount(labels.ravel(), minlength=n + 1) < min_area
    small[0] = False
    out = cls.copy()
    out[small[labels]] = LEAF
    return out


def hole_sizes(leaf: np.ndarray) -> np.ndarray:
    """`mask::holes`, as pixel counts: outside pixels inside the bounding box of the mask's
    largest component, in components that don't reach the box's edge."""
    labels, n = patches(leaf.astype(bool))
    if n == 0:
        return np.zeros(0, np.int64)
    big = 1 + int(np.argmax(np.bincount(labels.ravel())[1:]))
    ys, xs = np.nonzero(labels == big)
    box = ~leaf.astype(bool)[ys.min():ys.max() + 1, xs.min():xs.max() + 1]
    gaps, m = patches(box)
    edge = np.unique(np.concatenate([gaps[0], gaps[-1], gaps[:, 0], gaps[:, -1]]))
    sizes = np.bincount(gaps.ravel(), minlength=m + 1)
    keep = np.ones(m + 1, bool)
    keep[0] = False
    keep[edge] = False
    return sizes[keep]


def holes(leaf: np.ndarray) -> int:
    """`mask::holes`' count."""
    return int(hole_sizes(leaf).size)


# --- data -----------------------------------------------------------------------------------
def rows(split: Path = SPLIT) -> list[dict]:
    with split.open() as f:
        return list(csv.DictReader(f, delimiter="\t"))


def manifest_versions(manifest: Path) -> dict[str, str]:
    import tomllib
    m = tomllib.loads(manifest.read_text())
    return {e["name"]: e["version"] for e in m.get("model", []) if e["name"] in ("leaf_detector", "sam_encoder", "sam_decoder")}


def cache_dir(manifest: Path) -> Path:
    v = manifest_versions(manifest)
    key = hashlib.sha256(json.dumps(v, sort_keys=True).encode()).hexdigest()[:12]
    return CACHE / f"{v.get('leaf_detector', 'detector')}-{key}"


def ort_dylib() -> str:
    import onnxruntime
    capi = Path(onnxruntime.__file__).parent / "capi"
    libs = sorted(capi.glob("libonnxruntime.so*"))
    if not libs:
        raise SystemExit(f"no libonnxruntime under {capi}; set ORT_DYLIB_PATH")
    return str(libs[-1])


def cargo_example(name: str, stdin: str, env: dict | None = None) -> subprocess.Popen:
    """The `vision` crate's example, built in release, reading `stdin` and streaming JSON lines."""
    build = subprocess.run(["cargo", "build", "-q", "--release", "-p", "vision", "--example", name],
                           cwd=BACKEND, capture_output=True, text=True)
    if build.returncode != 0:
        raise SystemExit(f"cargo build --example {name} failed:\n{build.stderr}")
    target = Path(os.environ.get("CARGO_TARGET_DIR", BACKEND / "target"))
    p = subprocess.Popen([str(target / "release/examples" / name)], cwd=BACKEND, stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, text=True, env={**os.environ, **(env or {})})
    assert p.stdin is not None
    p.stdin.write(stdin)
    p.stdin.close()
    return p


def masks(args) -> int:
    out = cache_dir(args.manifest)
    out.mkdir(parents=True, exist_ok=True)
    regions = out / "regions.jsonl"
    done = set()
    if regions.exists():
        done = {json.loads(line)["name"] for line in regions.read_text().splitlines() if line.strip()}
    todo = [r for r in rows() if r["image"] not in done]
    print(f"{len(done)} photos cached in {out}, {len(todo)} to run", file=sys.stderr)
    if not todo:
        return 0
    job = {"manifest": str(args.manifest), "threads": args.threads,
           "images": [{"name": r["image"], "path": str(RAW / r["image"])} for r in todo]}
    t0 = time.time()
    p = cargo_example("perceive", json.dumps(job), {"ORT_DYLIB_PATH": os.environ.get("ORT_DYLIB_PATH") or ort_dylib()})
    assert p.stdout is not None
    n = 0
    with regions.open("a") as f:
        for line in p.stdout:
            f.write(line)
            f.flush()
            n += 1
            if n % 50 == 0:
                print(f"  {n}/{len(todo)} in {time.time() - t0:.0f} s", file=sys.stderr)
    code = p.wait()
    print(f"{n} photos in {time.time() - t0:.0f} s, exit {code}", file=sys.stderr)
    return code


# --- fitting ----------------------------------------------------------------------------------
GRID = {
    # From 0.2, not 0: a black pixel stays necrotic, as the segmenter means (spots.rs' unit test).
    # Free, the fit turns the rule off (shadows in field photos), and a black pixel with a
    # yellow tint becomes halo; keeping it at 0.2 cost 0.001 of the two-class IoU on 2026-10-22.
    "dark_value": np.arange(0.2, 0.801, 0.025),
    "brown_hue_min": np.arange(0.0, 30.1, 1.0),
    "brown_hue_max": np.arange(20.0, 90.1, 1.0),
    "brown_saturation": np.arange(0.0, 0.601, 0.025),
    "brown_value": np.arange(0.50, 2.501, 0.05),
    "pale_saturation": np.arange(0.0, 0.401, 0.02),
    "pale_value": np.arange(0.0, 2.001, 0.05),
    "halo_hue_min": np.arange(20.0, 90.1, 1.0),
    "halo_hue_max": np.arange(30.0, 120.1, 1.0),
    "halo_saturation": np.arange(0.0, 0.801, 0.025),
}
SAMPLE = 4000  # pixels per leaf while fitting; the scores use every pixel


def rle_decode(mask: dict) -> np.ndarray:
    from pycocotools import mask as cmask
    return cmask.decode({"size": mask["size"], "counts": mask["counts"].encode()}).astype(bool)


class Leaves:
    """Every leaf of a set of photos, flattened: HSV, the leaf's median value, the lesion label,
    which photo it belongs to, and a weight (1, or inside / sample when sampled)."""

    def __init__(self, parts: list[dict]):
        cat = lambda k, dt=None: np.concatenate([p[k] for p in parts]).astype(dt) if parts else np.zeros(0, dt)
        self.h, self.s, self.v, self.lv = cat("h"), cat("s"), cat("v"), cat("lv")
        self.gt = cat("gt", bool)
        self.photo = cat("photo", np.int64)
        self.leaf = cat("leaf", np.int64)
        self.w = cat("w", np.float64)

    def counts(self, t: dict, by: str = "photo") -> dict[str, np.ndarray]:
        """Weighted pixel counts per photo (or leaf): true and false positives, false negatives,
        lesion pixels, leaf pixels and necrotic-only true and false positives."""
        cls = classify(self.h, self.s, self.v, self.lv, t)
        aff = cls != LEAF
        nec = cls == NECROTIC
        key = getattr(self, by)
        n = int(key.max()) + 1 if key.size else 0
        b = lambda m: np.bincount(key, weights=self.w * m, minlength=n)
        return {"tp": b(aff & self.gt), "fp": b(aff & ~self.gt), "fn": b(~aff & self.gt),
                "gt": b(self.gt), "all": b(np.ones_like(self.gt)), "nec_tp": b(nec & self.gt), "nec_fp": b(nec & ~self.gt),
                "aff": b(aff)}


def load_photos(rows_: list[dict], regions: dict[str, dict], sample: int | None, seed: int = 0) -> tuple[Leaves, list[dict]]:
    """Leaves of the photos that have a leaf mask and a lesion pixel inside it. Returns the
    flattened pixels and one record per kept photo (its row, leaf count, lesion pixels)."""
    rng = np.random.default_rng(seed)
    parts, kept = [], []
    leaf_no = 0
    for r in rows_:
        res = regions.get(r["image"])
        if not res or "regions" not in res:
            continue
        leaves = [g for g in res["regions"] if g["class"] == "leaf" and g.get("mask")]
        if not leaves:
            continue
        rgb = np.asarray(Image.open(RAW / r["image"]).convert("RGB"))
        lesion = np.asarray(Image.open(RAW / r["mask"])) > 0
        if lesion.ndim == 3:
            lesion = lesion.any(axis=2)
        if lesion.shape != rgb.shape[:2]:
            continue
        hh, ss, vv = hsv(rgb)
        photo_parts = []
        for g in leaves:
            m = rle_decode(g["mask"])
            if m.shape != lesion.shape:
                continue
            idx = np.flatnonzero(m)
            if idx.size == 0:
                continue
            lv = leaf_value(vv.ravel()[idx])
            w = 1.0
            if sample and idx.size > sample:
                w = idx.size / sample
                idx = rng.choice(idx, sample, replace=False)
            photo_parts.append({"h": hh.ravel()[idx], "s": ss.ravel()[idx], "v": vv.ravel()[idx],
                                "lv": np.full(idx.size, lv), "gt": lesion.ravel()[idx],
                                "leaf": np.full(idx.size, leaf_no), "w": np.full(idx.size, w)})
            leaf_no += 1
        if not photo_parts or not any(p["gt"].any() for p in photo_parts):
            leaf_no -= len(photo_parts)
            continue
        for p in photo_parts:
            p["photo"] = np.full(p["h"].size, len(kept))
        parts += photo_parts
        kept.append({**r, "leaves": len(photo_parts)})
    return Leaves(parts), kept


def ious(c: dict[str, np.ndarray]) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Per photo: the lesion IoU, the healthy leaf's IoU, and their mean over the classes present
    (a class counts when it is labelled or predicted somewhere in the photo's leaves)."""
    tn = c["all"] - c["tp"] - c["fp"] - c["fn"]
    les_den = c["tp"] + c["fp"] + c["fn"]
    leaf_den = tn + c["fp"] + c["fn"]
    les = np.where(les_den > 0, c["tp"] / np.where(les_den > 0, les_den, 1), np.nan)
    leaf = np.where(leaf_den > 0, tn / np.where(leaf_den > 0, leaf_den, 1), np.nan)
    both = np.nanmean(np.stack([les, leaf]), axis=0)
    return les, leaf, both


def mean_iou(c: dict[str, np.ndarray]) -> float:
    """The fit's objective: the two-class IoU (lesion and healthy leaf) averaged over photos.
    Lesion IoU alone is maximised by calling the whole leaf affected wherever the masks are
    coarse (it reached 0.432 so, the hold-out's lesion share), so the healthy leaf counts too."""
    return float(np.nanmean(ious(c)[2])) if c["tp"].size else float("nan")


def half_counts(rs: list[dict], regions: dict[str, dict], ts: dict[str, dict]) -> tuple[list[dict], dict, dict]:
    """Every pixel of a half, a photo at a time (all at once doesn't fit in memory): per
    threshold set, the counts per photo and the affected, lesion and leaf pixels per leaf."""
    photos: list[dict] = []
    per_photo = {name: {} for name in ts}
    per_leaf = {name: {} for name in ts}
    for r in rs:
        leaves, kept = load_photos([r], regions, None)
        if not kept:
            continue
        photos += kept
        for name, t in ts.items():
            for k, v in leaves.counts(t).items():
                per_photo[name].setdefault(k, []).append(float(v[0]))
            for k, v in leaves.counts(t, by="leaf").items():
                per_leaf[name].setdefault(k, []).extend(v.tolist())
    arr = lambda d: {name: {k: np.array(v) for k, v in c.items()} for name, c in d.items()}
    return photos, arr(per_photo), arr(per_leaf)


def scores(photos: list[dict], c: dict[str, np.ndarray], lc: dict[str, np.ndarray]) -> dict:
    """The report's numbers for one set of thresholds on a half, from `half_counts`."""
    den = c["tp"] + c["fp"] + c["fn"]
    les, leaf, both = ious(c)
    iou = both
    nden = c["nec_fp"] + c["gt"]
    nec_iou = np.where(nden > 0, c["nec_tp"] / np.where(nden > 0, nden, 1), 1.0)
    big = lc["all"] > 0
    err = np.abs(100 * lc["aff"][big] / lc["all"][big] - 100 * lc["gt"][big] / lc["all"][big])
    by = lambda key: {k: round(float(np.nanmean(iou[[i for i, p in enumerate(photos) if p[key] == k]])), 3)
                      for k in sorted({p[key] for p in photos})}
    count = lambda key: {k: sum(p[key] == k for p in photos) for k in sorted({p[key] for p in photos})}
    return {"photos": len(photos), "leaves": int(big.sum()),
            "mean_iou": round(float(np.nanmean(both)), 3), "lesion_iou": round(float(np.nanmean(les)), 3),
            "leaf_iou": round(float(np.nanmean(leaf)), 3), "pooled_lesion_iou": round(float(c["tp"].sum() / den.sum()), 3),
            "mean_necrotic_iou": round(float(nec_iou.mean()), 3),
            "affected_pct_error": {"median": round(float(np.median(err)), 1), "p90": round(float(np.percentile(err, 90)), 1)},
            "lesion_share_pct": round(float(100 * c["gt"].sum() / c["all"].sum()), 1),
            "affected_share_pct": round(float(100 * c["aff"].sum() / c["all"].sum()), 1),
            "by_crop": by("crop"), "by_disease": by("disease"), "photos_by_crop": count("crop"),
            "photos_by_disease": count("disease")}


def coordinate_descent(leaves: Leaves, start: dict, rounds: int = 4) -> tuple[dict, list[dict]]:
    """Each field in turn set to its best grid value, all else fixed, until a round changes
    nothing. Ties keep the current value, so the defaults move only when it pays."""
    t = {k: max(v, float(GRID[k][0])) if k in GRID and v < GRID[k][0] else v for k, v in start.items()}
    best = mean_iou(leaves.counts(t))
    log = [{"round": 0, "mean_iou": round(best, 4)}]
    print(f"  start: mean IoU {best:.4f}", file=sys.stderr)
    for rnd in range(1, rounds + 1):
        changed = False
        for k, grid in GRID.items():
            for val in grid:
                trial = {**t, k: round(float(val), 4)}
                # The bands keep their meaning: brown below the halo's yellow, never overlapping it,
                # so a spot's size (necrotic pixels) doesn't take in its halo.
                if not (trial["brown_hue_min"] < trial["brown_hue_max"] <= trial["halo_hue_min"] < trial["halo_hue_max"]):
                    continue
                score = mean_iou(leaves.counts(trial))
                if score > best + 1e-6:
                    best, t, changed = score, trial, True
        log.append({"round": rnd, "mean_iou": round(best, 4), "thresholds": dict(t)})
        print(f"  round {rnd}: mean IoU {best:.4f}", file=sys.stderr)
        if not changed:
            break
    return t, log


def read_regions(manifest: Path) -> dict[str, dict]:
    path = cache_dir(manifest) / "regions.jsonl"
    if not path.exists():
        raise SystemExit(f"no leaf masks in {path}: run `spot_fit masks` first")
    return {d["name"]: d for d in map(json.loads, path.read_text().splitlines()) if d.get("name")}


def fit(args) -> int:
    regions = read_regions(args.manifest)
    all_rows = rows()
    halves = {f"{h} {k}": [r for r in all_rows if r["half"] == h and r["kind"] == k]
              for h in ("fit", "holdout") for k in ("spots", "areas")}
    errors = sum(1 for d in regions.values() if "error" in d)
    print(f"{len(regions)} photos with results ({errors} errors)", file=sys.stderr)
    sampled, fit_photos = load_photos(halves["fit spots"], regions, SAMPLE)
    print(f"fitting on {len(fit_photos)} photos, {sampled.h.size} sampled pixels", file=sys.stderr)
    fitted, log = coordinate_descent(sampled, DEFAULTS)
    result = {"date": args.date, "manifest_versions": manifest_versions(args.manifest), "defaults": DEFAULTS,
              "fitted": fitted, "descent": log, "halves": {}, "selected": {h: len(v) for h, v in halves.items()},
              "with_results": len(regions), "errors": errors}
    for half, rs in halves.items():
        photos, c, lc = half_counts(rs, regions, {"defaults": DEFAULTS, "fitted": fitted})
        no_leaf = sum(1 for r in rs if not [g for g in regions.get(r["image"], {}).get("regions", []) if g["class"] == "leaf"])
        result["halves"][half] = {"scored": len(photos), "no_leaf_found": no_leaf,
                                  **{name: scores(photos, c[name], lc[name]) for name in ("defaults", "fitted")}}
        print(f"  {half}: mean IoU {result['halves'][half]['defaults']['mean_iou']} -> {result['halves'][half]['fitted']['mean_iou']}", file=sys.stderr)
    REPORTS.mkdir(parents=True, exist_ok=True)
    stem = REPORTS / f"spots-{args.date}"
    stem.with_suffix(".json").write_text(json.dumps(result, indent=2) + "\n")
    stem.with_suffix(".md").write_text(report_md(result))
    for k in ("spots", "areas"):
        sheet(halves[f"holdout {k}"], regions, fitted, RUNS / f"holdout-{k}-{args.date}.jpg")
    print(f"wrote {stem}.md and .json", file=sys.stderr)
    return 0


def report_md(r: dict) -> str:
    h = r["halves"]
    groups = ("fit spots", "holdout spots", "fit areas", "holdout areas")
    rows_ = []
    for g in groups:
        for which in ("defaults", "fitted"):
            s = h[g][which]
            rows_.append(f"| {g} | {which} | {s['photos']} | **{s['mean_iou']:.3f}** | {s['lesion_iou']:.3f} | {s['leaf_iou']:.3f} | "
                         f"{s['mean_necrotic_iou']:.3f} | {s['affected_pct_error']['median']} / {s['affected_pct_error']['p90']} | "
                         f"{s['affected_share_pct']} (lesions {s['lesion_share_pct']}) |")
    th = "\n".join(f"| `{k}` | {r['defaults'][k]} | {r['fitted'][k]} |" for k in r["defaults"])

    def table(key: str) -> str:
        out = []
        for g in ("holdout spots", "holdout areas"):
            d, f, n = h[g]["defaults"][f"by_{key}"], h[g]["fitted"][f"by_{key}"], h[g]["fitted"][f"photos_by_{key}"]
            out += [f"| {g.split()[1]} | {k} | {n[k]} | {d[k]:.3f} | {f[k]:.3f} |" for k in sorted(f)]
        return "\n".join(out)

    sel = r["selected"]
    return head(r, h, groups, sel, rows_, th, table) + specks_md(r)


def specks_md(r: dict) -> str:
    """The speck filter's section, once `spot_fit clean` has run."""
    sp = r.get("specks")
    if not sp:
        return ""
    g = sp["groups"]
    cols = [("plantseg fit spots", "clean_pct"), ("plantseg holdout spots", "clean_pct"), ("plantseg holdout areas", "clean_pct"),
            ("dev spots", "clean_pct"), ("dev areas", "clean_pct"), ("healthy", "clean_pct"), ("healthy", "clean_with_holes_pct"),
            ("healthy", "median_spots_per_leaf"), ("plantseg holdout spots", "mean_iou")]
    fmt = lambda v: "–" if v is None else (f"{v:g}" if isinstance(v, float) else str(v))
    lines = []
    for s in sp["shares"]:
        cells = [fmt(g[name][str(s)][key]) if name in g else "–" for name, key in cols]
        mark = "**" if s == sp["chosen"] else ""
        lines.append(f"| {mark}{s:g}{mark} | " + " | ".join(cells) + " |")
    n = {k: v for k, v in sp["check_photos"]["by_group"].items()}
    ho = sp.get("healthy_holes")
    holes_line = "" if not ho else (
        f"The {ho['leaves']} healthy leaf masks have {ho['holes']} holes, median {ho['median_px']:g} px (90th percentile "
        f"{ho['p90_px']:g} px), {ho['up_to_20_px']} of them 20 px or less, which points to gaps in SAM's masks rather than holes in "
        f"the leaves (from their sizes; not inspected one by one). "
        f"Photos with no hole in any leaf: {ho['photos_with_no_hole_if_smaller_ignored']['0.0']} of {n.get('healthy', 0)}; "
        f"{ho['photos_with_no_hole_if_smaller_ignored']['0.0001']} if holes under 0.01% of their leaf were ignored, "
        f"{ho['photos_with_no_hole_if_smaller_ignored']['0.001']} under 0.1%. Holes count at any size in the contract, so "
        f"this is left to a change of the contract or of SAM's post-processing (filling small holes, as its own mask generator does).")
    src = "; ".join(f"{k} {v['0.0']['clean_pct']}% → {v[str(sp['chosen'])]['clean_pct']}% ({v['0.0']['photos']} photos)"
                    for k, v in sp["healthy_by_source"].items())
    return f"""
## Specks and clean photos (`min_spot_share`)

`python -m ml.training.eval.spot_fit clean-masks` then `clean`. The measurement counts a photo as evidence of health only
when no measured leaf has a spot or a hole ([contracts 7](../../../../docs/architecture/contracts.md#7-observations-measurements-and-records),
`clean_images`), and every necrotic patch counts as a spot, one pixel or more. So a speck of glare or grain on a healthy leaf
keeps a photo from reading clean. The segmenter now turns a necrotic patch (4-connected) smaller than `min_spot_share` of
its leaf's pixels into leaf; a share rather than a pixel count, so it holds at any resolution.

- **Choice:** the largest share on the grid at which PlantSeg's fit-half `spots` photos read clean at most
  {100 * sp['tolerance']:g} point more often than with no filter. Every other column only scores it.
- **Photos:** PlantSeg as above ({n.get('plantseg fit spots', 0)} fit and {n.get('plantseg holdout spots', 0)} hold-out
  `spots`, {n.get('plantseg holdout areas', 0)} hold-out `areas` photos with a lesion pixel in a leaf mask); and the check
  photos of `ml/training/data/splits/spots_clean.tsv` at the stored size of a photo with depth (long side 2,048 px), through the same leaf
  masks: {n.get('healthy', 0)} healthy (the dev cases' healthy photos and PlantDoc's healthy apple, pepper, grape and tomato
  leaves), {n.get('dev spots', 0)} dev photos with a spot disease and {n.get('dev areas', 0)} with an area disease.
- **Clean %** is the share of photos whose every leaf has no spot; for diseased photos it is wrong, for healthy ones right.
  *With holes* also needs no hole in any leaf, as the rule does. The IoU is the hold-out two-class IoU with the filter.
- Healthy photos clean, no filter → chosen share: {src}.
- **Holes now keep more healthy photos from reading clean than spots do.** {holes_line}

| `min_spot_share` | PlantSeg fit spots clean % | hold-out spots clean % | hold-out areas clean % | dev spots clean % | dev areas clean % | healthy clean % | healthy clean with holes % | healthy spots per leaf (median) | hold-out spots two-class IoU |
|---|---|---|---|---|---|---|---|---|---|
{chr(10).join(lines)}
"""


def head(r: dict, h: dict, groups: tuple, sel: dict, rows_: list, th: str, table) -> str:
    return f"""# Spot colour thresholds, {r['date']}

`python -m ml.training.eval.spot_fit fit` (M15; [evaluation 6](../../../../docs/vision/evaluation.md#6-thresholds-set-from-the-evaluation-set)).
The spot segmenter (`ColourSpotSegmenter`, `backend/crates/vision/src/measure/spots.rs`) inside the leaf masks the
perception worker's own models cut ({', '.join(f'{k} `{v}`' for k, v in r['manifest_versions'].items())}, fp32 on the CPU),
scored against PlantSeg's lesion masks. Fit: PlantSeg's training split; hold-out: its validation and test splits, scored once.

- **Photos** (`ml/training/data/splits/spots_plantseg.tsv`): {', '.join(f"{g} {n}" for g, n in sel.items())}. A photo is scored
  when one of its leaf masks holds a lesion pixel: {', '.join(f"{g} {h[g]['scored']}" for g in groups)}. No leaf was found on
  {', '.join(f"{g} {h[g]['no_leaf_found']}" for g in groups)}; {r['errors']} photos failed to run.
- **Spots and areas.** PlantSeg's masks outline discrete spots for some diseases (`spots`: rusts, scab, septoria, bacterial
  spot, gray leaf spot, frogeye, black rot, early blight) and paint whole regions for others (`areas`: mildews, late blight,
  leaf mold, northern leaf blight). The thresholds are fitted on `spots` only; `areas` is reported, not fitted.
- **The objective** is the two-class IoU per photo, the mean of the lesion IoU (affected pixels, necrotic or halo, against
  the lesion mask inside the leaf masks) and the healthy leaf's IoU, averaged over photos. A first fit on the lesion IoU
  alone called almost every leaf pixel affected: on these coarse masks that scores the lesion share (0.432 on the
  hold-out), so it rewarded painting the leaf.
- **Necrotic IoU** uses the necrotic pixels alone (a leaf's spots). **Affected-% error:** per leaf, the absolute
  difference between the predicted and the labelled share of the leaf, in points (median / 90th percentile).
- PlantSeg labels lesions without telling a halo apart, so the halo fields are fitted only through the affected pixels.
- **Constraints:** brown's hues stay below the halo's (`brown_hue_max` ≤ `halo_hue_min`), so a spot's size doesn't take
  in its halo, and `dark_value` is searched from 0.2, so a black pixel stays necrotic. Free, the fit turns the dark rule
  off (shadows in field photos) and a black pixel with a yellow tint becomes halo; the floor cost 0.001 of the
  two-class IoU.

## Results

| Photos | Thresholds | n | Two-class IoU | Lesion IoU | Leaf IoU | Necrotic IoU | Affected-% error | Affected share % |
|---|---|---|---|---|---|---|---|---|
{chr(10).join(rows_)}

## Thresholds

| Field | Default | Fitted |
|---|---|---|
{th}

## Hold-out by crop (two-class IoU)

| Kind | Crop | Photos | Defaults | Fitted |
|---|---|---|---|---|
{table("crop")}

## Hold-out by disease (two-class IoU)

| Kind | Disease | Photos | Defaults | Fitted |
|---|---|---|---|---|
{table("disease")}
"""


def sheet(rs: list[dict], regions: dict[str, dict], t: dict, out: Path, n: int = 24, side: int = 320) -> None:
    """Hold-out photos in pairs: the photo with the lesion mask (red) and the leaf outlines
    (white), then the fitted classes (necrotic red, halo yellow, leaf green, outside dimmed)."""
    from PIL import ImageDraw
    picks = [r for r in rs if regions.get(r["image"], {}).get("regions")][:: max(1, len(rs) // n)][:n]
    tiles = []
    for r in picks:
        rgb = np.asarray(Image.open(RAW / r["image"]).convert("RGB"))
        lesion = np.asarray(Image.open(RAW / r["mask"])) > 0
        if lesion.ndim == 3:
            lesion = lesion.any(axis=2)
        if lesion.shape != rgb.shape[:2]:
            continue
        a = rgb.copy()
        a[lesion] = (0.5 * a[lesion] + [127, 0, 0]).astype(np.uint8)
        b = (rgb * 0.3).astype(np.uint8)
        for g in regions[r["image"]]["regions"]:
            if g["class"] != "leaf" or not g.get("mask"):
                continue
            m = rle_decode(g["mask"])
            if m.shape != lesion.shape:
                continue
            edge = m & ~np.roll(m, 1, 0) | m & ~np.roll(m, 1, 1)
            a[edge] = 255
            cls = segment(rgb, m, t)
            for code, colour in ((LEAF, (40, 160, 40)), (NECROTIC, (220, 30, 30)), (HALO, (240, 220, 0))):
                b[cls == code] = colour
        pair = Image.new("RGB", (2 * side, side))
        for i, im in enumerate((a, b)):
            x = Image.fromarray(im)
            x.thumbnail((side, side))
            pair.paste(x, (i * side, 0))
        ImageDraw.Draw(pair).text((4, 4), f"{r['disease']}", fill=(255, 255, 255))
        tiles.append(pair)
    if not tiles:
        return
    cols = 2
    grid = Image.new("RGB", (cols * 2 * side, ((len(tiles) + cols - 1) // cols) * side))
    for i, tile in enumerate(tiles):
        grid.paste(tile, ((i % cols) * 2 * side, (i // cols) * side))
    out.parent.mkdir(parents=True, exist_ok=True)
    grid.save(out, quality=85)


# --- specks and clean photos ------------------------------------------------------------------
# The dev cases' spot and area diseases, as the PlantSeg selection splits them (plantseg.SPOTS).
SPOT_PROBLEMS = {"apple_scab", "apple_cedar_apple_rust", "corn_common_rust", "corn_gray_leaf_spot", "grape_black_rot",
                 "tomato_bacterial_spot", "tomato_early_blight", "tomato_septoria_leaf_spot"}
AREA_PROBLEMS = {"corn_northern_leaf_blight", "squash_powdery_mildew", "tomato_late_blight"}
PLANTDOC_HEALTHY = {"Apple leaf": "apple", "Bell_pepper leaf": "pepper", "grape leaf": "grape", "Tomato leaf": "tomato"}
PLANTDOC_CREDIT = "Singh et al., PlantDoc: A Dataset for Visual Plant Disease Detection, CoDS-COMAD 2020"
SHARES = [0.0, 1e-5, 2e-5, 5e-5, 1e-4, 2e-4, 5e-4, 1e-3, 2e-3, 5e-3, 1e-2]
TOLERANCE = 0.01  # the filter may leave fit-half spot photos clean at most this much more often
STORED = 2048     # api stores a photo with depth, the only kind it measures, at a long side of 2,048 px


def clean_rows() -> list[dict]:
    """The check photos: the dev cases that are healthy or have a spot or area disease (not
    their flower or fruit shots), PlantDoc's healthy apple, pepper, grape and tomato leaves (all
    of its test split, 15 a class from its train split). Deterministic."""
    out = []
    for line in (ROOT / "ml/training/eval/manifest.jsonl").read_text().splitlines():
        r = json.loads(line)
        if r["split"] != "dev":
            continue
        truth = r["truth"]
        spots, areas = sorted(set(truth["problems"]) & SPOT_PROBLEMS), sorted(set(truth["problems"]) & AREA_PROBLEMS)
        if truth["outcome"] == "healthy":
            group, kind, disease = "healthy", "-", "healthy"
        elif spots or areas:
            group, kind, disease = "diseased", "spots" if spots else "areas", (spots or areas)[0]
        else:
            continue
        for i in r["images"]:
            if i["kind"] != "flower_fruit":
                out.append({"image": f"eval/photos/{i['file']}", "set": group, "kind": kind, "source": f"dev:{r['case_id']}",
                            "crop": truth["species_id"], "disease": disease, "licence": r["licence"], "credit": r["credit"]})
    root = next((RAW / "plantdoc_cls").glob("PlantDoc-Dataset-*"))
    rng = random.Random(0)
    for cls, crop in PLANTDOC_HEALTHY.items():
        for split, cap in (("test", None), ("train", 15)):
            files = sorted(f.name for f in (root / split / cls).iterdir() if f.suffix.lower() in (".jpg", ".jpeg", ".png"))
            if cap:
                files = sorted(rng.sample(files, min(cap, len(files))))
            out += [{"image": f"data/raw/plantdoc_cls/{root.name}/{split}/{cls}/{f}", "set": "healthy", "kind": "-",
                     "source": f"plantdoc:{split}", "crop": crop, "disease": "healthy", "licence": "CC BY 4.0",
                     "credit": PLANTDOC_CREDIT} for f in files]
    return out


def clean_source(row: dict, photos: Path) -> Path:
    rel = row["image"]
    return photos / rel.removeprefix("eval/photos/") if rel.startswith("eval/photos/") else ROOT / "ml/training" / rel


def clean_png(row: dict, manifest: Path) -> Path:
    """The photo at the stored size of a photo with depth (`stored_long_side`: a long side of
    2,048 px, never enlarged), upright, as PNG so both sides read the same pixels."""
    key = hashlib.sha256(row["image"].encode()).hexdigest()[:16]
    return cache_dir(manifest) / f"clean-{STORED}/png" / f"{key}.png"


def clean_masks(args) -> int:
    from PIL import ImageOps
    rs = clean_rows()
    CLEAN.parent.mkdir(parents=True, exist_ok=True)
    with CLEAN.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rs[0]), delimiter="\t", lineterminator="\n")
        w.writeheader()
        w.writerows(rs)
    out = cache_dir(args.manifest) / f"clean-{STORED}"
    (out / "png").mkdir(parents=True, exist_ok=True)
    for r in rs:
        png = clean_png(r, args.manifest)
        if not png.exists():
            im = ImageOps.exif_transpose(Image.open(clean_source(r, args.photos))).convert("RGB")
            im.thumbnail((STORED, STORED), Image.LANCZOS)
            im.save(png)
    regions = out / "regions.jsonl"
    done = set()
    if regions.exists():
        done = {json.loads(line)["name"] for line in regions.read_text().splitlines() if line.strip()}
    todo = [r for r in rs if r["image"] not in done]
    print(f"{len(rs)} check photos in {CLEAN.name}, {len(todo)} to run", file=sys.stderr)
    if not todo:
        return 0
    job = {"manifest": str(args.manifest), "threads": args.threads,
           "images": [{"name": r["image"], "path": str(clean_png(r, args.manifest))} for r in todo]}
    p = cargo_example("perceive", json.dumps(job), {"ORT_DYLIB_PATH": os.environ.get("ORT_DYLIB_PATH") or ort_dylib()})
    assert p.stdout is not None
    with regions.open("a") as f:
        for line in p.stdout:
            f.write(line)
    return p.wait()


def leaf_stats(rgb: np.ndarray, regions: dict, t: dict, lesion: np.ndarray | None = None) -> list[dict]:
    """Per leaf mask, with `t` and no speck filter: its pixels, the affected pixels' counts
    against the lesion mask (when given), each necrotic patch's size and lesion pixels, and its
    holes."""
    hsv_ = hsv(rgb)
    out = []
    for g in regions.get("regions", []):
        if g["class"] != "leaf" or not g.get("mask"):
            continue
        m = rle_decode(g["mask"])
        if m.shape != rgb.shape[:2]:
            continue
        cls = segment(rgb, m, {**t, "min_spot_share": 0.0}, hsv_)
        labels, n = patches(cls == NECROTIC)
        aff = (cls == NECROTIC) | (cls == HALO)
        gt = lesion & m if lesion is not None else np.zeros_like(m)
        out.append({"px": int(m.sum()), "tp": int((aff & gt).sum()), "fp": int((aff & ~gt).sum()), "fn": int((~aff & gt & m).sum()),
                    "sizes": np.bincount(labels.ravel(), minlength=n + 1)[1:],
                    "gt": np.bincount(labels.ravel(), weights=gt.ravel(), minlength=n + 1)[1:], "hole_sizes": hole_sizes(m)})
    return out


def at_share(photos: list[list[dict]], share: float) -> dict:
    """The photos read at one `min_spot_share`: how many are clean (no leaf has a spot), clean
    with no hole either, the spots per leaf, and the two-class IoU per photo after the filter."""
    clean = clean_holes = 0
    spots, iou = [], []
    for leaves in photos:
        kept = [lf["sizes"] >= share * lf["px"] for lf in leaves]
        counts = [int(k.sum()) for k in kept]
        spots += counts
        clean += all(c == 0 for c in counts)
        clean_holes += all(c == 0 for c in counts) and all(lf["hole_sizes"].size == 0 for lf in leaves)
        gone = [(~k) for k in kept]
        tp = sum(lf["tp"] - lf["gt"][g].sum() for lf, g in zip(leaves, gone))
        fp = sum(lf["fp"] - (lf["sizes"][g] - lf["gt"][g]).sum() for lf, g in zip(leaves, gone))
        fn = sum(lf["fn"] + lf["gt"][g].sum() for lf, g in zip(leaves, gone))
        iou.append((tp, fp, fn, sum(lf["px"] for lf in leaves)))
    a = np.array(iou, dtype=np.float64).reshape(-1, 4)
    les, leaf, both = ious({"tp": a[:, 0], "fp": a[:, 1], "fn": a[:, 2], "all": a[:, 3]})
    n = len(photos)
    return {"photos": n, "clean_pct": round(100 * clean / n, 1) if n else None,
            "clean_with_holes_pct": round(100 * clean_holes / n, 1) if n else None,
            "median_spots_per_leaf": float(np.median(spots)) if spots else None,
            "mean_iou": round(float(np.nanmean(both)), 3) if n and np.isfinite(both).any() else None}


def choose_share(false_clean: dict[float, float], tolerance: float = TOLERANCE) -> float:
    """The largest share whose clean rate on diseased photos is at most `tolerance` above the
    rate with no filter (share 0)."""
    base = false_clean[0.0]
    return max(s for s, rate in false_clean.items() if rate <= base + 100 * tolerance)


def clean(args) -> int:
    path = REPORTS / f"spots-{args.date}.json"
    r = json.loads(path.read_text())
    fitted = {k: v for k, v in r["fitted"].items() if k != "min_spot_share"}
    regions = read_regions(args.manifest)
    groups: dict[str, list[list[dict]]] = {}
    for h in ("fit", "holdout"):
        for k in ("spots", "areas"):
            photos = []
            for row in rows():
                if row["half"] != h or row["kind"] != k or row["image"] not in regions:
                    continue
                rgb = np.asarray(Image.open(RAW / row["image"]).convert("RGB"))
                lesion = np.asarray(Image.open(RAW / row["mask"])) > 0
                lesion = lesion.any(axis=2) if lesion.ndim == 3 else lesion
                if lesion.shape != rgb.shape[:2]:
                    continue
                leaves = leaf_stats(rgb, regions[row["image"]], fitted, lesion)
                if any(lf["tp"] + lf["fn"] > 0 for lf in leaves):  # a lesion pixel in a leaf, as load_photos
                    photos.append(leaves)
            groups[f"plantseg {h} {k}"] = photos
            print(f"  plantseg {h} {k}: {len(photos)} photos", file=sys.stderr)
    cregions = {d["name"]: d for d in map(json.loads, (cache_dir(args.manifest) / f"clean-{STORED}/regions.jsonl").read_text().splitlines())}
    check = list(csv.DictReader(CLEAN.open(), delimiter="\t"))
    by_source: dict[str, list[list[dict]]] = {}
    for row in check:
        res = cregions.get(row["image"])
        if not res:
            continue
        leaves = leaf_stats(np.asarray(Image.open(clean_png(row, args.manifest)).convert("RGB")), res, fitted)
        if not leaves:
            continue
        g = "healthy" if row["set"] == "healthy" else f"dev {row['kind']}"
        groups.setdefault(g, []).append(leaves)
        if row["set"] == "healthy":
            by_source.setdefault(row["source"].split(":")[0] if row["source"].startswith("dev") else row["source"], []).append(leaves)
    table = {g: {str(s): at_share(ph, s) for s in SHARES} for g, ph in groups.items()}
    hs = np.concatenate([lf["hole_sizes"] for ph in groups["healthy"] for lf in ph])
    hole_note = {"leaves": sum(len(ph) for ph in groups["healthy"]), "holes": int(hs.size),
                 "median_px": float(np.median(hs)), "p90_px": float(np.percentile(hs, 90)), "up_to_20_px": int((hs <= 20).sum()),
                 "photos_with_no_hole_if_smaller_ignored": {
                     str(s): sum(all((lf["hole_sizes"] >= s * lf["px"]).sum() == 0 for lf in ph) for ph in groups["healthy"])
                     for s in (0.0, 1e-4, 1e-3)}}
    chosen = choose_share({s: table["plantseg fit spots"][str(s)]["clean_pct"] for s in SHARES})
    check_counts = {"rows": len(check), "with_leaf": sum(len(v) for g, v in groups.items() if not g.startswith("plantseg")),
                    "by_group": {g: len(v) for g, v in groups.items()}}
    r["defaults"]["min_spot_share"] = 0.0
    r["fitted"] = {**fitted, "min_spot_share": chosen}
    r["specks"] = {"shares": SHARES, "tolerance": TOLERANCE, "chosen": chosen, "groups": table, "check_photos": check_counts,
                   "healthy_holes": hole_note, "healthy_by_source": {src: {str(s): at_share(ph, s) for s in (0.0, chosen)} for src, ph in by_source.items()}}
    path.write_text(json.dumps(r, indent=2) + "\n")
    path.with_suffix(".md").write_text(report_md(r))
    print(f"min_spot_share {chosen}; wrote {path.with_suffix('.md')}", file=sys.stderr)
    return 0


def parity(args) -> int:
    """The Rust segmenter against `segment`, with the report's fitted values, on hold-out leaves."""
    regions = read_regions(args.manifest)
    t = json.loads((REPORTS / f"spots-{args.date}.json").read_text())["fitted"] if not args.defaults else DEFAULTS
    if args.share is not None:
        t = {**t, "min_spot_share": args.share}
    work = RUNS / "parity"
    work.mkdir(parents=True, exist_ok=True)
    jobs, expected = [], {}
    for r in [r for r in rows() if r["half"] == "holdout"]:
        for k, g in enumerate(regions.get(r["image"], {}).get("regions", [])):
            if g["class"] != "leaf" or not g.get("mask") or len(jobs) >= args.leaves:
                continue
            m = rle_decode(g["mask"])
            stem = work / f"{len(jobs):03d}"
            # The pixels go over as PNG: the two JPEG decoders differ by a level here and there,
            # and this checks the segmenter, not the decoders.
            rgb = np.asarray(Image.open(RAW / r["image"]).convert("RGB"))
            Image.fromarray(rgb).save(f"{stem}.rgb.png")
            Image.fromarray((m * 255).astype(np.uint8)).save(f"{stem}.leaf.png")
            jobs.append({"image": f"{stem}.rgb.png", "mask": f"{stem}.leaf.png", "out": f"{stem}.rust.png", "thresholds": t})
            expected[f"{stem}.rust.png"] = segment(rgb, m, t)
    p = cargo_example("spot_mask", "".join(json.dumps(j) + "\n" for j in jobs))
    assert p.stdout is not None
    same = total = 0
    for line in p.stdout:
        d = json.loads(line)
        if "error" in d:
            raise SystemExit(f"spot_mask: {d}")
        rust = np.asarray(Image.open(d["out"]))
        py = expected[d["out"]]
        same += int((rust == py).sum())
        total += py.size
    p.wait()
    share = same / total if total else 0.0
    print(f"{len(jobs)} leaves, {total} pixels: {total - same} with another class ({100 * share:.4f}% the same)", file=sys.stderr)
    return 0 if share >= 0.9999 else 1


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("masks", help="leaf masks from the worker's models, cached")
    m.add_argument("--manifest", type=Path, default=MANIFEST, help="manifest.cpu.toml's set, fetched with --for perception --cpu")
    m.add_argument("--threads", type=int, default=12)
    f = sub.add_parser("fit", help="fit on the fit half, score both halves, write the report")
    f.add_argument("--manifest", type=Path, default=MANIFEST)
    f.add_argument("--date", default=date.today().isoformat())
    c = sub.add_parser("clean-masks", help="leaf masks of the healthy and diseased check photos, at the stored size")
    c.add_argument("--manifest", type=Path, default=MANIFEST)
    c.add_argument("--photos", type=Path, default=ROOT / "ml/training/eval/photos", help="the evaluation photos (fetched from pdm-models)")
    c.add_argument("--threads", type=int, default=12)
    k = sub.add_parser("clean", help="choose min_spot_share and score the clean-photo rule; adds to the report")
    k.add_argument("--manifest", type=Path, default=MANIFEST)
    k.add_argument("--date", default=date.today().isoformat(), help="the report to add to")
    q = sub.add_parser("parity", help="the Rust segmenter against this copy, on hold-out leaves")
    q.add_argument("--manifest", type=Path, default=MANIFEST)
    q.add_argument("--date", default=date.today().isoformat(), help="the report whose fitted values to use")
    q.add_argument("--defaults", action="store_true", help="use the defaults instead")
    q.add_argument("--leaves", type=int, default=20)
    q.add_argument("--share", type=float, help="min_spot_share to check instead (a larger one exercises the speck filter)")
    args = ap.parse_args(argv)
    return {"masks": masks, "fit": fit, "clean-masks": clean_masks, "clean": clean, "parity": parity}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
