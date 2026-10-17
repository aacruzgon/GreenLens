"""The plant check's leaf score on labelled plant photos, to set a provisional τ_leaf (models.md 2).

    python -m ml.training.eval.leaf_scores [--split val] [--label tau-leaf-provisional-2026-10-16]

Every photo in the split shows a plant, so a photo whose leaf score is below τ_leaf would be a
false `no_plant_found` if it were the scan's only plant shot. The score is computed as the
perception worker computes it (backend vision `perception::segment` and `detector::decode`):

- the photo is stored with its long side at most 1,600 px (`UploadConfig::default_long_side`),
  squashed to the detector's input and normalised by `ml.pdm_vision.preprocess`;
- a query is kept when its `leaf` sigmoid is at least `score_threshold` and its box is at least
  `min_side_px` on each side in the stored image; NMS never removes the top box;
- the leaf score is the top kept score, or 0 when nothing is kept.

So any τ_leaf at or below `score_threshold` means "the detector kept a leaf". The thresholds and
the detector file come from the manifest (`manifest.cpu.toml` on the CPU, `manifest.toml` with
--cuda), and its files from `ml/models/files` or --files.

The split's photos are training-set photos (leaf close-ups and field photos), not the app's
shots, and there are no photos without a plant, so the report can't measure how often a no-plant
scan passes. The real τ_leaf is fitted on the evaluation set (evaluation 4, M16).

Writes `<out>/<label>.json` (every photo's score) and `<out>/<label>.md`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

import numpy as np

from ml.pdm_vision import preprocess as pp

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "ml/training/data"
MODELS = ROOT / "ml/models"
STORED_LONG_SIDE = 1600
THRESHOLDS = (0.35, 0.4, 0.45, 0.5)


def detector_entry(manifest: Path) -> dict:
    entries = tomllib.loads(manifest.read_text())["model"]
    return next(e for e in entries if e["name"] == "leaf_detector")


def leaf_score(dets: np.ndarray, logits: np.ndarray, stored: tuple[int, int], cfg: dict) -> float:
    """`detector::top_score(&detector::decode(..))` for one image: `dets` `[queries, 4]`
    (normalised cx, cy, w, h), `logits` `[queries, classes]` with `leaf` at index 0."""
    scores = 1.0 / (1.0 + np.exp(-logits[:, 0].astype(np.float32)))
    iw, ih = stored
    min_side = np.minimum(dets[:, 2] * iw, dets[:, 3] * ih)
    kept = scores[(scores >= cfg["score_threshold"]) & (min_side >= cfg["min_side_px"])]
    return float(kept.max()) if kept.size else 0.0


def rates(values, thresholds=THRESHOLDS) -> dict:
    v = np.asarray(values)
    return {f"{t:g}": round(float(np.mean(v < t)), 4) for t in thresholds}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--split", default="val", choices=["train", "val", "test"],
                    help="ml/training/data/splits/<split>.tsv (default val; test is scored once, Oct 24)")
    ap.add_argument("--files", type=Path, default=MODELS / "files")
    ap.add_argument("--cuda", action="store_true", help="the fp16 file on CUDA instead of fp32 on the CPU")
    ap.add_argument("--label", default=None)
    ap.add_argument("--out", type=Path, default=ROOT / "ml/training/eval/reports")
    args = ap.parse_args(argv)
    if args.split == "test":
        print("note: the test split is scored once (evaluation 2); use it only for the final run",
              file=sys.stderr)

    import onnxruntime as ort

    manifest = MODELS / ("manifest.toml" if args.cuda else "manifest.cpu.toml")
    entry = detector_entry(manifest)
    cfg = entry["config"]
    providers = ["CUDAExecutionProvider"] if args.cuda else ["CPUExecutionProvider"]
    sess = ort.InferenceSession(str(args.files / entry["file"]), providers=providers)
    in_name = entry["input"]["name"]
    out_names = [entry["outputs"]["boxes"], entry["outputs"]["logits"]]
    recipe = pp.Recipe(entry["input"]["shape"][-1], pp.Resize(entry["input"]["resize"]),
                       tuple(entry["input"]["mean"]), tuple(entry["input"]["std"]))

    rows = [line.split("\t") for line in (DATA / f"splits/{args.split}.tsv").read_text().splitlines()[1:]]
    photos = []
    for dataset, path, species, problem in rows:
        im = pp.load_rgb(DATA / "raw" / path)
        stored = pp.fit_long_side(im.size, STORED_LONG_SIDE)
        im = pp.resize(im, *stored)
        x = pp.batch([im], recipe)
        if args.cuda:
            x = x.astype(np.float16)
        dets, logits = sess.run(out_names, {in_name: x})
        photos.append({"dataset": dataset, "path": path, "species_id": species, "problem_id": problem,
                       "score": round(leaf_score(dets[0].astype(np.float32), logits[0], stored, cfg), 4)})

    groups = {"dataset": defaultdict(list), "species_id": defaultdict(list)}
    for p in photos:
        for g in groups:
            groups[g][p[g]].append(p["score"])
    allv = [p["score"] for p in photos]
    label = args.label or f"tau-leaf-{args.split}-{dt.date.today().isoformat()}"
    report = {
        "label": label, "split": args.split, "n": len(photos),
        "detector": {"file": entry["file"], "sha256": entry["sha256"], "manifest": manifest.name,
                     "provider": providers[0]},
        "config": {"score_threshold": cfg["score_threshold"], "min_side_px": cfg["min_side_px"],
                   "tau_leaf": cfg["tau_leaf"], "stored_long_side": STORED_LONG_SIDE},
        "below": {"all": rates(allv),
                  **{g: {k: {"n": len(v), **rates(v)} for k, v in sorted(d.items())}
                     for g, d in groups.items()}},
        "nothing_kept": round(float(np.mean(np.asarray(allv) == 0.0)), 4),
        "percentiles": {f"p{q}": round(float(np.percentile(allv, q)), 4) for q in (1, 5, 10, 50)},
        "lowest": sorted(photos, key=lambda p: p["score"])[:20],
        "photos": photos,
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / f"{label}.json").write_text(json.dumps(report, indent=1) + "\n")
    (args.out / f"{label}.md").write_text(markdown(report))
    print(markdown(report))
    return 0


def markdown(r: dict) -> str:
    ts = [f"{t:g}" for t in THRESHOLDS]
    head = "| | n | " + " | ".join(f"< {t}" for t in ts) + " |\n|---|---|" + "---|" * len(ts) + "\n"

    def row(name, d):
        return f"| {name} | {d['n']} | " + " | ".join(f"{d[t] * 100:.1f}%" for t in ts) + " |\n"

    c, d = r["config"], r["detector"]
    out = [f"# Leaf score on the {r['split']} split ({r['label']})\n\n",
           f"Detector `{d['file']}` ({d['manifest']}, {d['provider']}); `score_threshold` "
           f"{c['score_threshold']}, `min_side_px` {c['min_side_px']} on the stored image (long side "
           f"≤ {c['stored_long_side']} px); `tau_leaf` in the manifest when run: {c['tau_leaf']}.\n\n",
           f"Share of plant photos whose leaf score is below each τ_leaf (a false `no_plant_found` "
           f"if it were the scan's only plant shot). Nothing kept: {r['nothing_kept'] * 100:.1f}%. "
           f"Percentiles: " + ", ".join(f"{k} {v:.3f}" for k, v in r["percentiles"].items()) + ".\n\n",
           "## By dataset\n\n", head]
    out += [row(k, v) for k, v in r["below"]["dataset"].items()]
    out.append(row("**All**", {"n": r["n"], **r["below"]["all"]}))
    out += ["\n## By species\n\n", head]
    out += [row(k, v) for k, v in r["below"]["species_id"].items()]
    out += ["\n## The 20 lowest\n\n| score | dataset | species | problem | path |\n|---|---|---|---|---|\n"]
    out += [f"| {p['score']:.3f} | {p['dataset']} | {p['species_id']} | {p['problem_id']} | `{p['path']}` |\n"
            for p in r["lowest"]]
    return "".join(out)


if __name__ == "__main__":
    sys.exit(main())
