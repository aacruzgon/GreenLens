"""Calibration on the dev cases, the part that runs offline on the model files (evaluation 5; M16).

    python -m ml.training.eval.calibrate [--heads disease-head-v2-2026-10-20 disease-head-v2c-2026-10-20] [--out calibration-<date>]

Each dev case runs through the worker's perception steps on the same model files:

- **Species (`T_species`, the `bioclip` entry's `species_temperature`).** Each shot is stored with
  its long side at most 1,600 px and embedded by BioCLIP 2; its species probabilities are
  `softmax(100 / T × e · rows)` over the label matrix's species rows, and the shots are averaged
  with the worker's weights (whole plant and flower 2, leaf close-up and underside 1), as
  `vision::perception::heads::combine_species` does. `T` is fitted by the negative log-likelihood
  of the true species, and the report checks evaluation 5's agreement rule: the top-1 accuracy of
  cases scored 0.8 or more, and the lowest threshold at which it reaches 95%.
- **Disease head (`T_disease`, the head file's `temperature`).** Every leaf crop the shipped
  detector keeps on a leaf shot (`ml.training.heads.crops`, cut as the worker cuts them), or the
  whole close-up when it keeps none; a one-photo case's photo is a leaf on its own, so it counts as
  a close-up. The head's raw logits are restricted to the case's species, as the worker restricts
  them. A crop carries its case's label, which is wrong for a healthy leaf on a diseased plant, so
  `T` is fitted and scored on the photos with exactly one crop, as training fits it; only species
  with two or more classes count. The fit uses the head's own validation crops plus the dev cases
  (evaluation 5), and the dev cases alone are reported beside it.

Not here, since they need the cases through the API (J5): the VLM certainty words, the diagnosis
margin and the indicator weights.

Writes `reports/<out>.md` and `.json`, and a reliability diagram per head and crop as
`reports/<out>-*.png`. The values aren't written into any model file: the chosen head and the real
label matrix (M12) aren't final yet, and a rerun takes a minute.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
import tomllib
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
from scipy.optimize import minimize_scalar

from ml.pdm_vision import preprocess as pp
from ml.pdm_vision.headfile import read_label_matrix, read_linear_head

ROOT = Path(__file__).resolve().parents[3]
EVAL = ROOT / "ml/training/eval"
MODELS = ROOT / "ml/models"
HEADS = ROOT / "ml/training/runs/heads"
REPORTS = EVAL / "reports"
STORED_LONG_SIDE = 1600
SHOT_WEIGHT = {"whole_plant": 2.0, "flower_fruit": 2.0, "leaf_closeup": 1.0, "leaf_underside": 1.0}
LEAFY = ("leaf_closeup", "leaf_underside")
AGREEMENT = 0.8          # architecture 6.2: BioCLIP's top-1 in the high band
AGREEMENT_ACCURACY = 0.95
BINS = 15


# --- shared -------------------------------------------------------------------------------------

def softmax(z: np.ndarray) -> np.ndarray:
    z = z - z.max(axis=-1, keepdims=True)
    e = np.exp(z)
    return e / e.sum(axis=-1, keepdims=True)


def ece(conf: np.ndarray, correct: np.ndarray, bins: int = BINS) -> float:
    """Expected calibration error of top-1 confidences, `bins` equal-width bins."""
    edges = np.linspace(0, 1, bins + 1)
    total = 0.0
    for lo, hi in zip(edges[:-1], edges[1:]):
        m = (conf > lo) & (conf <= hi)
        if m.any():
            total += m.mean() * abs(correct[m].mean() - conf[m].mean())
    return float(total)


def reliability(conf: np.ndarray, correct: np.ndarray, bins: int = 10) -> list[dict]:
    edges = np.linspace(0, 1, bins + 1)
    out = []
    for lo, hi in zip(edges[:-1], edges[1:]):
        m = (conf > lo) & (conf <= hi)
        if m.any():
            out.append({"lo": round(float(lo), 2), "hi": round(float(hi), 2), "n": int(m.sum()),
                        "confidence": float(conf[m].mean()), "accuracy": float(correct[m].mean())})
    return out


def fit_scalar(nll) -> float:
    return float(minimize_scalar(nll, bounds=(0.05, 10.0), method="bounded").x)


def dev_cases(manifest: Path, split: str = "dev") -> list[dict]:
    return [c for c in map(json.loads, filter(str.strip, manifest.read_text().splitlines())) if c["split"] == split]


def stored(path: Path):
    im = pp.load_rgb(path)
    return pp.resize(im, *pp.fit_long_side(im.size, STORED_LONG_SIDE))


# --- species ------------------------------------------------------------------------------------

def species_probs(cos: list[np.ndarray], weights: list[float], t: float) -> np.ndarray:
    """`combine_species` before its top 3: the weighted mean of each shot's softmax."""
    p = sum(w * softmax(100.0 / t * c) for c, w in zip(cos, weights))
    return p / sum(weights)


def species_calibration(cases: list[dict], photos: Path, matrix: dict, embed) -> dict:
    ids = [l["id"] for l in matrix["labels"] if l["table"] == "species"]
    rows = matrix["rows"][[i for i, l in enumerate(matrix["labels"]) if l["table"] == "species"]]
    usable, left_out = [], Counter()
    for c in cases:
        sp = c["truth"].get("species_id")
        shots = [im for im in c["images"] if im["kind"] in SHOT_WEIGHT]
        if sp not in ids:
            left_out["species not in the label matrix"] += 1
        elif not shots:
            left_out["no scored shot"] += 1
        else:
            usable.append((c, shots))
    files = [photos / im["file"] for _, shots in usable for im in shots]
    emb = dict(zip(files, embed([stored(f) for f in files])))
    data = [([emb[photos / im["file"]] @ rows.T for im in shots], [SHOT_WEIGHT[im["kind"]] for im in shots], ids.index(c["truth"]["species_id"]), c)
            for c, shots in usable]
    y = np.array([d[2] for d in data])

    def probs(t):
        return np.stack([species_probs(cos, w, t) for cos, w, _, _ in data])

    def nll(t):
        return float(-np.log(probs(t)[np.arange(len(y)), y] + 1e-12).mean())

    fitted = fit_scalar(nll)
    out = {"cases": len(data), "left_out": dict(left_out), "labels": len(ids), "fitted_temperature": round(fitted, 4)}
    for name, t in (("t_1", 1.0), ("fitted", fitted)):
        p = probs(t)
        conf, top = p.max(1), p.argmax(1)
        correct = top == y
        sweep = []
        for thr in np.round(np.arange(0.5, 1.0, 0.05), 2):
            m = conf >= thr
            sweep.append({"threshold": float(thr), "n": int(m.sum()), "accuracy": float(correct[m].mean()) if m.any() else None})
        ok = [s["threshold"] for s in sweep if s["n"] >= 10 and s["accuracy"] is not None and s["accuracy"] >= AGREEMENT_ACCURACY]
        hi = conf >= AGREEMENT
        out[name] = {
            "temperature": round(t, 4), "top1": float(correct.mean()), "nll": nll(t), "ece": ece(conf, correct),
            "at_agreement": {"n": int(hi.sum()), "accuracy": float(correct[hi].mean()) if hi.any() else None},
            "lowest_threshold_at_95": min(ok) if ok else None, "sweep": sweep,
            "reliability": reliability(conf, correct),
            "per_species": {ids[s]: {"n": int((y == s).sum()), "top1": float(correct[y == s].mean())} for s in sorted(set(y))},
            "wrong": [{"case": d[3]["case_id"], "truth": ids[d[2]], "top": ids[k], "score": round(float(cf), 3)}
                      for d, k, cf, ok_ in zip(data, top, conf, correct) if not ok_],
        }
    return out


# --- disease head -------------------------------------------------------------------------------

def class_index(head: dict) -> tuple[dict, np.ndarray]:
    index = {(l["species_id"], l["problem_id"]): l["index"] for l in head["labels"]}
    species_of_class = np.array([l["species_id"] for l in sorted(head["labels"], key=lambda l: l["index"])])
    return index, species_of_class


def case_label(c: dict, index: dict) -> int | None:
    """The head's class for a case, or None when it has none or more than one of the head's problems."""
    sp, t = c["truth"].get("species_id"), c["truth"]
    if t["outcome"] == "healthy":
        return index.get((sp, "healthy"))
    hits = {index[(sp, p)] for p in t.get("problems", []) if (sp, p) in index}
    return hits.pop() if len(hits) == 1 else None


def leaf_photos(c: dict) -> list[str]:
    """The case's leaf shots; a one-photo case's photo is a leaf on its own, taken as a close-up."""
    leafy = [im["file"] for im in c["images"] if im["kind"] in LEAFY]
    return leafy or ([c["images"][0]["file"]] if len(c["images"]) == 1 else [])


def source(file: str) -> str:
    """Where a photo comes from: `inat`, or `fieldsets/<set>`."""
    parts = file.split("/")
    return "/".join(parts[:2]) if parts[0] == "fieldsets" else parts[0]


def raw_logits(head: dict, emb: np.ndarray) -> np.ndarray:
    """The head's logits before its temperature: `weight · e + bias`, `e` L2-normalised."""
    e = emb / np.linalg.norm(emb, axis=-1, keepdims=True)
    return e @ head["weight"].T + head["bias"]


def restricted(logits: np.ndarray, species_of_class: np.ndarray, species: str) -> np.ndarray:
    return np.where(species_of_class == species, logits, -np.inf)


def disease_items(cases: list[dict], photos: Path, head: dict, crops_of, whole_of) -> tuple[list, Counter]:
    """(restricted raw logits, label, case id, species, source) for each single-crop leaf photo of a case
    the head can label, and the reasons the others are left out."""
    index, species_of_class = class_index(head)
    supported = set(species_of_class)
    items, left_out = [], Counter()
    for c in cases:
        sp = c["truth"].get("species_id")
        if sp not in supported:
            left_out["species without head classes"] += 1
            continue
        if (species_of_class == sp).sum() < 2:
            left_out["species with one class"] += 1
            continue
        y = case_label(c, index)
        if y is None:
            left_out["no single head class for the case's problems"] += 1
            continue
        files = leaf_photos(c)
        if not files:
            left_out["no leaf shot"] += 1
            continue
        for f in files:
            crops = crops_of(photos / f)
            if len(crops) > 1:
                left_out["leaf photos with several crops"] += 1
                continue
            emb = crops[0] if len(crops) == 1 else whole_of(photos / f)
            items.append((restricted(raw_logits(head, emb), species_of_class, sp), y, c["case_id"], sp, source(f)))
    return items, left_out


def val_items(head: dict, crop_loader) -> list:
    """The head's own validation crops, single-crop photos, restricted as in training."""
    from ml.training.heads.crops import read_split

    index, species_of_class = class_index(head)
    rows = [r for r in read_split("val") if (r["species_id"], r["problem_id"]) in index]
    loaded = crop_loader([r["path"] for r in rows])
    return [(restricted(raw_logits(head, z["emb"][0]), species_of_class, r["species_id"]), index[(r["species_id"], r["problem_id"])], None, r["species_id"], r["dataset"])
            for r, z in zip(rows, loaded) if len(z["emb"]) == 1 and (species_of_class == r["species_id"]).sum() >= 2]


def disease_scores(items: list, t: float) -> dict:
    if not items:
        return {"n": 0}
    p = softmax(np.stack([i[0] for i in items]) / t)
    y = np.array([i[1] for i in items])
    conf, correct = p.max(1), p.argmax(1) == y
    return {"n": len(items), "top1": float(correct.mean()), "ece": ece(conf, correct),
            "nll": float(-np.log(p[np.arange(len(y)), y] + 1e-12).mean()), "reliability": reliability(conf, correct)}


def fit_items(items: list) -> float:
    z = np.stack([i[0] for i in items])
    y = np.array([i[1] for i in items])
    return fit_scalar(lambda t: float(-np.log(softmax(z / t)[np.arange(len(y)), y] + 1e-12).mean()))


def disease_calibration(name: str, cases: list[dict], photos: Path, crops_of, whole_of, crop_loader) -> dict:
    head = read_linear_head(HEADS / name / f"{name}.json")
    dev, left_out = disease_items(cases, photos, head, crops_of, whole_of)
    val = val_items(head, crop_loader)
    t_file = float(head["temperature"])
    t_dev = fit_items(dev) if len(dev) >= 10 else None
    t_both = fit_items(val + dev)
    out = {"head": name, "classes": len(head["labels"]), "dev_photos": len(dev), "val_photos": len(val),
           "left_out": dict(left_out), "t_file": round(t_file, 4), "t_dev": None if t_dev is None else round(t_dev, 4),
           "t_val_and_dev": round(t_both, 4),
           "dev": {k: disease_scores(dev, t) for k, t in (("t_1", 1.0), ("t_file", t_file), ("t_val_and_dev", t_both))},
           "val": {k: disease_scores(val, t) for k, t in (("t_file", t_file), ("t_val_and_dev", t_both))},
           "dev_per_species": {sp: disease_scores([i for i in dev if i[3] == sp], t_both)
                               for sp in sorted({i[3] for i in dev})},
           "dev_per_source": {src: disease_scores([i for i in dev if i[4] == src], t_both)
                              for src in sorted({i[4] for i in dev})}}
    return out


# --- report -------------------------------------------------------------------------------------

def plot(diagrams: dict[str, list[dict]], path: Path, title: str) -> None:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, len(diagrams), figsize=(3.2 * len(diagrams), 3.4), squeeze=False)
    for ax, (label, bins) in zip(axes[0], diagrams.items()):
        ax.plot([0, 1], [0, 1], color="0.6", lw=1, ls="--")
        if bins:
            ax.bar([b["lo"] for b in bins], [b["accuracy"] for b in bins], width=0.1, align="edge", alpha=0.6, edgecolor="k")
            ax.plot([b["confidence"] for b in bins], [b["accuracy"] for b in bins], "o", color="k", ms=3)
        n = sum(b["n"] for b in bins)
        ax.set(title=f"{label} (n={n})", xlim=(0, 1), ylim=(0, 1), xlabel="confidence", ylabel="accuracy")
    fig.suptitle(title, fontsize=10)
    fig.tight_layout()
    path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(path, dpi=90)
    plt.close(fig)


def pct(v) -> str:
    return "n/a" if v is None else f"{v:.0%}"


def num(v, d=3) -> str:
    return "n/a" if v is None else f"{v:.{d}f}"


def write_report(out: str, species: dict, disease: list[dict], args) -> None:
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{out}.json").write_text(json.dumps({"date": args.date, "species": species, "disease": disease}, indent=2) + "\n")
    plot({"T = 1": species["t_1"]["reliability"], f"T = {species['fitted_temperature']}": species["fitted"]["reliability"]},
         REPORTS / f"{out}-species.png", "BioCLIP 2 species, dev cases")
    for d in disease:
        plot({sp: v.get("reliability", []) for sp, v in d["dev_per_species"].items()}, REPORTS / f"{out}-{d['head']}.png",
             f"{d['head']}, dev leaf photos, T = {d['t_val_and_dev']}")

    s1, sf = species["t_1"], species["fitted"]
    lines = [f"# Calibration on the dev cases: {out}", "",
             "The offline part of evaluation 5 ([calibrate.py](../calibrate.py)). The values are proposals: the "
             "label matrix is the 20-species stand-in until M12, and the disease head isn't chosen yet (#85).", "",
             "## Species: BioCLIP 2 zero-shot (`species_temperature`)", "",
             f"{species['cases']} dev cases over the label matrix's {species['labels']} species"
             + (f"; left out: {', '.join(f'{v} {k}' for k, v in species['left_out'].items())}" if species["left_out"] else "") + ".", "",
             "| `species_temperature` | Top-1 | NLL | ECE (15 bins) | Cases at 0.8 or more | Their top-1 | Lowest threshold with 95% top-1 |",
             "|---|---|---|---|---|---|---|"]
    for r in (s1, sf):
        lines.append(f"| {r['temperature']} | {pct(r['top1'])} | {num(r['nll'])} | {num(r['ece'])} | {r['at_agreement']['n']} | "
                     f"{pct(r['at_agreement']['accuracy'])} | {num(r['lowest_threshold_at_95'], 2)} |")
    lines += ["", "Per species at the fitted temperature: " + ", ".join(f"{k} {pct(v['top1'])} of {v['n']}" for k, v in sf["per_species"].items()) + ".",
              "", "Wrong top-1 at the fitted temperature: " + ("; ".join(f"`{w['case']}` → {w['top']} ({w['score']})" for w in sf["wrong"]) or "none") + ".",
              "", f"![species reliability]({out}-species.png)", "",
              "## Disease head (`temperature`)", "",
              "Leaf photos of the dev cases with exactly one crop (or none, so the whole photo), restricted to the case's species.", "",
              "| Head | Classes | T in the file (val) | T fitted on dev | T fitted on val and dev | Dev photos | Dev top-1 | Dev ECE: T = 1 / file / val and dev | Val photos | Val ECE: file / val and dev |",
              "|---|---|---|---|---|---|---|---|---|---|"]
    for d in disease:
        dv, vl = d["dev"], d["val"]
        lines.append(f"| `{d['head']}` | {d['classes']} | {d['t_file']} | {num(d['t_dev'], 4)} | {d['t_val_and_dev']} | {d['dev_photos']} | "
                     f"{pct(dv['t_val_and_dev'].get('top1'))} | {num(dv['t_1'].get('ece'))} / {num(dv['t_file'].get('ece'))} / {num(dv['t_val_and_dev'].get('ece'))} | "
                     f"{d['val_photos']} | {num(vl['t_file'].get('ece'))} / {num(vl['t_val_and_dev'].get('ece'))} |")
    for d in disease:
        lines += ["", f"`{d['head']}` per crop on dev (val and dev T): "
                  + ", ".join(f"{sp} {pct(v.get('top1'))} of {v['n']}, ECE {num(v.get('ece'))}" for sp, v in d["dev_per_species"].items()) + ".",
                  f"`{d['head']}` per source on dev: "
                  + ", ".join(f"{src} {pct(v.get('top1'))} of {v['n']}" for src, v in d["dev_per_source"].items()) + ".",
                  "Left out: " + (", ".join(f"{v} {k}" for k, v in d["left_out"].items()) or "none") + ".",
                  "", f"![{d['head']} reliability]({out}-{d['head']}.png)"]
    (REPORTS / f"{out}.md").write_text("\n".join(lines) + "\n")


def main(argv=None) -> int:
    today = dt.date.today().isoformat()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl")
    ap.add_argument("--photos", type=Path, default=EVAL / "photos")
    ap.add_argument("--heads", nargs="*", default=["disease-head-v2-2026-10-20", "disease-head-v2c-2026-10-20"],
                    help="disease heads under ml/training/runs/heads/<version>/")
    ap.add_argument("--out", default=f"calibration-{today}")
    args = ap.parse_args(argv)
    args.date = today

    from ml.pdm_vision.features import Embedder
    from ml.training.heads.crops import CropEmbedder

    cases = dev_cases(args.manifest)
    entries = tomllib.loads((MODELS / "manifest.toml").read_text())["model"]
    matrix_entry = next(e for e in entries if e["name"].startswith("bioclip_labels"))
    matrix = read_label_matrix(MODELS / "files" / matrix_entry["file"])
    embedder = Embedder()
    species = species_calibration(cases, args.photos, matrix, embedder.images)

    crops = CropEmbedder()
    leaf_files = sorted({args.photos / f for c in cases for f in leaf_photos(c)})
    crops.run(leaf_files)
    loaded = dict(zip(leaf_files, crops.load(leaf_files)))
    whole = dict(zip(leaf_files, embedder.images([stored(f) for f in leaf_files])))
    disease = [disease_calibration(h, cases, args.photos, lambda f: loaded[f]["emb"], lambda f: whole[f], CropEmbedder(gpu=False).load)
               for h in args.heads]
    write_report(args.out, species, disease, args)
    print((REPORTS / f"{args.out}.md").read_text())
    return 0


if __name__ == "__main__":
    sys.exit(main())
