"""Disease head on frozen BioCLIP 2 features (training.md §6; models.md §7).

    python -m ml.training.heads.train_disease --version disease-head-v0-2026-10-16

- Classes: every (species, problem | healthy) with at least --min-train training images.
- Multinomial logistic regression; C chosen on validation by restricted top-1, from a grid
  wide enough that the choice isn't at its edge (the report says if it is).
- Class weights by inverse frequency, capped at 10 times the median weight.
- Restricted softmax: at serving time the worker keeps only the working species' classes, so
  every accuracy and the temperature are computed on that restricted softmax.
- The headline numbers leave out --not-supported species (bean, until issue #4 decides), and
  every number is also reported per dataset, so the field sets' own test splits show apart
  from PlantDoc's.
- --views N adds N augmented views per training image (random resized crop 0.7–1.0, flip,
  colour jitter 0.2, mild blur).

Writes the head JSON (ml/models/FORMATS.md), classifier_labels.yaml and a report under
ml/training/runs/heads/<version>/.
"""

from __future__ import annotations

import argparse
import collections
import json
from pathlib import Path

import numpy as np
import yaml
from PIL import Image, ImageFilter
from scipy.optimize import minimize_scalar
from sklearn.linear_model import LogisticRegression

from ml.pdm_vision import headfile
from ml.pdm_vision import preprocess as pp
from ml.pdm_vision.features import Embedder

ROOT = Path(__file__).resolve().parents[3]
SPLITS = ROOT / "ml/training/data/splits"
RAW = ROOT / "ml/training/data/raw"
RUNS = ROOT / "ml/training/runs/heads"
REPORTS = ROOT / "ml/training/heads/reports"   # committed (training.md §6, Report)
BIOCLIP = "bioclip-2@2957b322"


def read_split(name: str) -> list[dict]:
    lines = (SPLITS / f"{name}.tsv").read_text().splitlines()[1:]
    out = []
    for line in lines:
        dataset, path, species, problem = line.split("\t")
        out.append({"dataset": dataset, "path": RAW / path, "species_id": species, "problem_id": problem})
    return out


def augment(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    w, h = im.size
    s = rng.uniform(0.7, 1.0)
    cw, ch = max(int(w * np.sqrt(s)), 8), max(int(h * np.sqrt(s)), 8)
    x, y = int(rng.integers(0, w - cw + 1)), int(rng.integers(0, h - ch + 1))
    im = im.crop((x, y, x + cw, y + ch))
    if rng.random() < 0.5:
        im = im.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    arr = np.asarray(im, np.float32)
    arr = arr * rng.uniform(0.8, 1.2) + rng.uniform(-0.2, 0.2) * 255 * 0.25
    mean = arr.mean(axis=2, keepdims=True)
    arr = mean + (arr - mean) * rng.uniform(0.8, 1.2)
    im = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))
    if rng.random() < 0.3:
        im = im.filter(ImageFilter.GaussianBlur(rng.uniform(0.3, 1.0)))
    return im


def restricted_logits(logits: np.ndarray, species_of_class: np.ndarray, species: list[str]) -> np.ndarray:
    """Logits with every class of another species set to -inf."""
    mask = species_of_class[None, :] == np.asarray(species)[:, None]
    return np.where(mask, logits, -np.inf)


def softmax(z: np.ndarray) -> np.ndarray:
    z = z - z.max(axis=1, keepdims=True)
    e = np.exp(z)
    return e / e.sum(axis=1, keepdims=True)


def topk_acc(probs: np.ndarray, y: np.ndarray, k: int) -> float:
    top = np.argsort(-probs, axis=1)[:, :k]
    return float((top == y[:, None]).any(axis=1).mean())


def ece(probs: np.ndarray, y: np.ndarray, bins: int = 15) -> float:
    conf = probs.max(1)
    correct = probs.argmax(1) == y
    edges = np.linspace(0, 1, bins + 1)
    total = 0.0
    for lo, hi in zip(edges[:-1], edges[1:]):
        m = (conf > lo) & (conf <= hi)
        if m.any():
            total += m.mean() * abs(correct[m].mean() - conf[m].mean())
    return float(total)


def fit_temperature(logits: np.ndarray, y: np.ndarray) -> float:
    def nll(t: float) -> float:
        p = softmax(logits / t)
        return float(-np.log(p[np.arange(len(y)), y] + 1e-12).mean())

    return float(minimize_scalar(nll, bounds=(0.05, 10.0), method="bounded").x)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--min-train", type=int, default=30)
    ap.add_argument("--views", type=int, default=0)
    ap.add_argument("--not-supported", nargs="*", default=["bean"],
                    help="species left out of the headline numbers (still trained and reported)")
    ap.add_argument("--balance-sources", action="store_true",
                    help="within each class, give every dataset the same total weight")
    args = ap.parse_args()

    train, val, test = read_split("train"), read_split("val"), read_split("test")
    counts = collections.Counter(f"{r['species_id']}/{r['problem_id']}" for r in train)
    classes = sorted(c for c, n in counts.items() if n >= args.min_train)
    left_out = {c: n for c, n in counts.items() if n < args.min_train}
    cidx = {c: i for i, c in enumerate(classes)}
    species_of_class = np.asarray([c.split("/")[0] for c in classes])

    def keep(rs):
        return [r for r in rs if f"{r['species_id']}/{r['problem_id']}" in cidx]

    left_out_images = {n: len(rs) - len(keep(rs)) for n, rs in (("val", val), ("test", test))}
    train, val, test = keep(train), keep(val), keep(test)
    emb = Embedder()
    feats = {n: emb.files([r["path"] for r in rs]) for n, rs in (("train", train), ("val", val), ("test", test))}
    ys = {n: np.asarray([cidx[f"{r['species_id']}/{r['problem_id']}"] for r in rs]) for n, rs in (("train", train), ("val", val), ("test", test))}
    sp = {n: [r["species_id"] for r in rs] for n, rs in (("train", train), ("val", val), ("test", test))}
    dsets = {n: np.asarray([r["dataset"] for r in rs]) for n, rs in (("train", train), ("val", val), ("test", test))}

    Xtr, ytr = feats["train"], ys["train"]
    if args.views:
        rng = np.random.default_rng(0)
        extra, extra_y = [], []
        for r, y in zip(train, ytr):
            im = pp.load_rgb(r["path"])
            views = [augment(im, rng) for _ in range(args.views)]
            extra.append(emb.images(views))
            extra_y += [y] * args.views
        Xtr = np.concatenate([Xtr] + extra)
        ytr = np.concatenate([ytr, np.asarray(extra_y)])

    freq = np.bincount(ytr, minlength=len(classes)).astype(float)
    w = 1.0 / freq
    w = np.minimum(w / np.median(w), 10.0)
    sample_w = w[ytr]
    if args.balance_sources:
        src = np.asarray([r["dataset"] for r in train] + [r["dataset"] for r, y in zip(train, ys["train"]) for _ in range(args.views)])
        pair = collections.Counter(zip(src, ytr))
        n_src = collections.Counter(y for _, y in pair)
        sample_w = sample_w * np.asarray([freq[y] / (n_src[y] * pair[(s, y)]) for s, y in zip(src, ytr)])

    best = None
    grid = (1, 5, 20, 50, 100, 200, 500, 1000, 2000, 5000)
    for C in grid:
        clf = LogisticRegression(C=C, max_iter=3000)
        clf.fit(Xtr, ytr, sample_weight=sample_w)
        lv = restricted_logits(clf.decision_function(feats["val"]), species_of_class, sp["val"])
        multi = np.asarray([(species_of_class == s).sum() > 1 for s in sp["val"]])
        acc = topk_acc(softmax(lv[multi]), ys["val"][multi], 1)
        print(f"C={C}: restricted val top-1 {acc:.3f}")
        if best is None or acc > best[0]:
            best = (acc, C, clf)
    _, C, clf = best
    c_at_edge = C in (grid[0], grid[-1])
    if c_at_edge:
        print(f"WARNING: C={C} is at the edge of the grid {grid}")

    lv = restricted_logits(clf.decision_function(feats["val"]), species_of_class, sp["val"])
    T = fit_temperature(lv, ys["val"])

    def report(name: str) -> dict:
        raw = clf.decision_function(feats[name])
        full = softmax(raw)
        res = softmax(restricted_logits(raw, species_of_class, sp[name]) / T)
        res_untempered = softmax(restricted_logits(raw, species_of_class, sp[name]))
        y = ys[name]
        spn = np.asarray(sp[name])
        # Restricted to the true species, a crop with k or fewer classes is right at top-k by
        # construction (review of PR #6, item 1). Each top-k is reported only over crops with
        # more than k classes, and calibration only over crops with 2 or more.
        n_cls = {s: int((species_of_class == s).sum()) for s in set(spn)}
        k_of = np.asarray([n_cls[s] for s in spn])
        per_crop = {}
        for s in sorted(set(spn)):
            m = spn == s
            per_crop[s] = {"n": int(m.sum()), "classes": n_cls[s], "top1": topk_acc(res[m], y[m], 1), "top3": topk_acc(res[m], y[m], 3)}

        def over(mask, f):
            return {"value": f(mask), "n": int(mask.sum())} if mask.any() else {"value": None, "n": 0}

        supported = ~np.isin(spn, args.not_supported)
        multi = supported & (k_of > 1)
        per_dataset = {}
        for d in sorted(set(dsets[name])):
            m = (dsets[name] == d) & (k_of > 1)
            per_dataset[d] = {
                "restricted_top1_multiclass": over(m, lambda mm: topk_acc(res[mm], y[mm], 1)),
                "ece_after_T_multiclass": over(m, lambda mm: ece(res[mm], y[mm])),
            }

        return {
            "n": int(len(y)),
            "left_out_class_images": left_out_images.get(name, 0),
            "headline": {
                "excludes": list(args.not_supported),
                "restricted_top1_multiclass": over(multi, lambda m: topk_acc(res[m], y[m], 1)),
                "restricted_top3_over_3_classes": over(supported & (k_of > 3), lambda m: topk_acc(res[m], y[m], 3)),
                "ece_after_T_multiclass": over(multi, lambda m: ece(res[m], y[m])),
            },
            "per_dataset": per_dataset,
            "full_top1": topk_acc(full, y, 1),
            "full_top3": topk_acc(full, y, 3),
            "restricted_top1_multiclass": over(k_of > 1, lambda m: topk_acc(res[m], y[m], 1)),
            "restricted_top3_over_3_classes": over(k_of > 3, lambda m: topk_acc(res[m], y[m], 3)),
            "ece_before_T_multiclass": over(k_of > 1, lambda m: ece(res_untempered[m], y[m])),
            "ece_after_T_multiclass": over(k_of > 1, lambda m: ece(res[m], y[m])),
            "trivial_share": float((k_of == 1).mean()),
            "per_crop": per_crop,
            "note": "Restricted metrics assume the species is right. The head is trained and scored on whole images; the worker feeds it leaf crops.",
        }

    out = RUNS / args.version
    out.mkdir(parents=True, exist_ok=True)
    labels = [
        {"index": i, "label": c, "species_id": c.split("/")[0], "problem_id": c.split("/")[1]}
        for i, c in enumerate(classes)
    ]
    headfile.write_linear_head(
        out / f"{args.version}.json",
        kind="disease_head",
        version=args.version,
        features=f"{BIOCLIP}+{emb.model_sha}",
        weight=clf.coef_.astype(np.float32),
        bias=clf.intercept_.astype(np.float32),
        temperature=T,
        labels=labels,
    )
    (out / "classifier_labels.yaml").write_text(
        yaml.safe_dump(
            {"model_version": args.version, "labels": [{k: l[k] for k in ("index", "label", "species_id", "problem_id")} for l in labels]},
            sort_keys=False,
        )
    )
    summary = {
        "version": args.version,
        "classes": len(classes),
        "left_out_below_min": left_out,
        "C": C,
        "C_grid": list(grid),
        "C_at_grid_edge": c_at_edge,
        "temperature": T,
        "views": args.views,
        "balance_sources": args.balance_sources,
        "train_rows": int(len(ytr)),
        "val": report("val"),
        "test": report("test"),
    }
    (out / "report.json").write_text(json.dumps(summary, indent=2) + "\n")
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{args.version}.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k not in ("val", "test")}, indent=2))
    for n in ("val", "test"):
        r = summary[n]
        h = r["headline"]
        fmt = lambda d: "n/a" if d["value"] is None else f"{d['value']:.3f} (n={d['n']})"
        print(f"{n}: n={r['n']} (+{r['left_out_class_images']} of left-out classes) | headline without {h['excludes']}: "
              f"top-1 {fmt(h['restricted_top1_multiclass'])} | top-3 {fmt(h['restricted_top3_over_3_classes'])} | "
              f"ECE {fmt(h['ece_after_T_multiclass'])}")
        for d, v in r["per_dataset"].items():
            print(f"   {d:22s} top-1 {fmt(v['restricted_top1_multiclass'])} ECE {fmt(v['ece_after_T_multiclass'])}")
        for s, v in r["per_crop"].items():
            print(f"   {s:11s} classes={v['classes']} n={v['n']:4d} top1 {v['top1']:.2f} top3 {v['top3']:.2f}")


if __name__ == "__main__":
    main()
