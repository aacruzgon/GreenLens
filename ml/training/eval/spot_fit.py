"""Fit the spot segmenter's colour thresholds on PlantSeg's lesion masks (evaluation 6, M15).

    python -m ml.training.eval.spot_fit masks      # leaf masks from the worker's own models (CPU, ~25 min)
    python -m ml.training.eval.spot_fit fit        # fit on the fit half, score both halves, write the report
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
3. **parity:** runs the real segmenter (the `spot_mask` example) with the fitted values on
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


def segment(rgb: np.ndarray, leaf: np.ndarray, t: dict) -> np.ndarray:
    """`ColourSpotSegmenter::segment`: a class per pixel, OUTSIDE where `leaf` is false."""
    h, s, v = hsv(rgb)
    out = np.full(leaf.shape, OUTSIDE, dtype=np.uint8)
    inside = leaf.astype(bool)
    lv = leaf_value(v[inside])
    out[inside] = classify(h[inside], s[inside], v[inside], lv, t)
    return out


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


def parity(args) -> int:
    """The Rust segmenter against `segment`, with the report's fitted values, on hold-out leaves."""
    regions = read_regions(args.manifest)
    t = json.loads((REPORTS / f"spots-{args.date}.json").read_text())["fitted"] if not args.defaults else DEFAULTS
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
    q = sub.add_parser("parity", help="the Rust segmenter against this copy, on hold-out leaves")
    q.add_argument("--manifest", type=Path, default=MANIFEST)
    q.add_argument("--date", default=date.today().isoformat(), help="the report whose fitted values to use")
    q.add_argument("--defaults", action="store_true", help="use the defaults instead")
    q.add_argument("--leaves", type=int, default=20)
    args = ap.parse_args(argv)
    return {"masks": masks, "fit": fit, "parity": parity}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
