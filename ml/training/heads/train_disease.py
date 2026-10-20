"""Disease head on frozen BioCLIP 2 features (training.md §6; models.md §7).

    python -m ml.training.heads.train_disease --version disease-head-v0-2026-10-16

- Classes: every (species, problem | healthy) with at least --min-train training images.
- Multinomial logistic regression; C chosen on validation by restricted top-1, from a grid
  wide enough that the choice isn't at its edge (the report says if it is). With --features
  crops, --select-on crop (the default) scores what the worker returns: each leaf crop of the
  val photos with exactly one crop (a crop carries its photo's label, which is wrong for a
  healthy leaf in a diseased photo with several). --select-on photo scores each photo by the
  mean of its crops' probabilities, which the worker doesn't do. The report gives both.
- Class weights by inverse frequency, capped at 10 times the median weight.
- Restricted softmax: at serving time the worker keeps only the working species' classes, so
  every accuracy and the temperature are computed on that restricted softmax.
- The headline numbers leave out --not-supported species (bean, until issue #4 decides), and
  every number is also reported per dataset, so the field sets' own test splits show apart
  from PlantDoc's.
- --views N adds N augmented views per training image (random resized crop 0.7–1.0, flip,
  colour jitter 0.2, mild blur); with --features crops, N views of each crop, embedded first by
  `ml.training.heads.crops --splits train --views N`. Each view of a photo weighs 1 in total.
- --drop-classes leaves the named classes out (their photos too), for a problem with no profile
  in the knowledge base.

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


def main(argv=None) -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--features", choices=("image", "crops"), default="image",
                    help="image: one embedding per photo (v0-v1d); crops: one per leaf our detector keeps "
                         "(ml.training.heads.crops), or the whole photo when it keeps none, as the worker does")
    ap.add_argument("--supported", nargs="*",
                    help="train only these species' classes (classifier_supported, issue #4); the others go to the VLM")
    ap.add_argument("--min-train", type=int, default=30, help="training photos a class needs")
    ap.add_argument("--views", type=int, default=0)
    ap.add_argument("--drop-classes", nargs="*", default=[], metavar="SPECIES/PROBLEM",
                    help="classes to leave out, e.g. tomato/tomato_mosaic_virus")
    ap.add_argument("--not-supported", nargs="*", default=["bean"],
                    help="species left out of the headline numbers (still trained and reported)")
    ap.add_argument("--balance-sources", action="store_true",
                    help="within each class, give every dataset the same total weight")
    ap.add_argument("--select-on", choices=("crop", "photo"), default="crop",
                    help="with --features crops, choose C by per-crop top-1 on val photos with one crop (as the "
                         "worker returns each crop), or by per-photo top-1 (the mean of a photo's crops)")
    args = ap.parse_args(argv)
    if args.features != "crops":
        args.select_on = "photo"   # one row per photo: the two are the same

    splits = {n: read_split(n) for n in ("train", "val", "test")}
    left_out_species = {}
    if args.supported:
        for n, rs in splits.items():
            left_out_species[n] = dict(collections.Counter(r["species_id"] for r in rs if r["species_id"] not in args.supported))
            splits[n] = [r for r in rs if r["species_id"] in args.supported]
    counts = collections.Counter(f"{r['species_id']}/{r['problem_id']}" for r in splits["train"])
    unknown = set(args.drop_classes) - set(counts)
    if unknown:
        ap.error(f"--drop-classes: no such class {sorted(unknown)}")
    classes = sorted(c for c, n in counts.items() if n >= args.min_train and c not in args.drop_classes)
    left_out = {c: n for c, n in counts.items() if n < args.min_train}
    cidx = {c: i for i, c in enumerate(classes)}
    species_of_class = np.asarray([c.split("/")[0] for c in classes])

    def keep(rs):
        return [r for r in rs if f"{r['species_id']}/{r['problem_id']}" in cidx]

    left_out_images = {n: len(splits[n]) - len(keep(splits[n])) for n in ("val", "test")}
    rows = {n: keep(rs) for n, rs in splits.items()}
    emb = Embedder()
    crop_source = None
    if args.features == "crops":
        from ml.training.heads.crops import CropEmbedder
        crop_source = CropEmbedder(gpu=False)   # the cache only: no GPU memory held while training

    # One feature row per crop (or per photo); `group` maps a row to its photo, `weight` makes
    # every photo count once in total, `single` marks rows of photos with exactly one crop.
    data = {}
    for n, rs in rows.items():
        whole = emb.files([r["path"] for r in rs])
        crops = crop_source.load([r["path"] for r in rs]) if crop_source else None
        X, group, weight, single, no_leaf = [], [], [], [], []
        for i in range(len(rs)):
            no_leaf.append(crops is not None and len(crops[i]["emb"]) == 0)
            e = crops[i]["emb"] if crops is not None and len(crops[i]["emb"]) else whole[i : i + 1]
            X.append(e)
            group += [i] * len(e)
            weight += [1.0 / len(e)] * len(e)
            single += [len(e) == 1] * len(e)
        group = np.asarray(group)
        y_img = np.asarray([cidx[f"{r['species_id']}/{r['problem_id']}"] for r in rs])
        data[n] = {
            "X": np.concatenate(X), "group": group, "weight": np.asarray(weight), "single": np.asarray(single),
            "y": y_img[group], "y_img": y_img,
            "sp": np.asarray([r["species_id"] for r in rs])[group], "sp_img": np.asarray([r["species_id"] for r in rs]),
            "ds_img": np.asarray([r["dataset"] for r in rs]),
            "crops_per_photo": np.bincount(group, minlength=len(rs)), "no_leaf": np.asarray(no_leaf),
        }

    tr = data["train"]
    Xtr, ytr, photo_w = tr["X"], tr["y"], tr["weight"]
    src_rows = tr["ds_img"][tr["group"]]
    if args.views and crop_source:
        # Every crop's views, crop by crop; a photo's n crops give n rows per view, 1/n each.
        views = crop_source.load_views([r["path"] for r in rows["train"]], args.views)
        extra_y = np.concatenate([np.full(len(v), y) for v, y in zip(views, tr["y_img"])])
        extra_w = np.concatenate([np.full(len(v), args.views / len(v)) for v in views])
        extra_src = np.concatenate([np.full(len(v), ds, dtype=object) for v, ds in zip(views, tr["ds_img"])])
        Xtr = np.concatenate([Xtr, *views])
        ytr = np.concatenate([ytr, extra_y])
        photo_w = np.concatenate([photo_w, extra_w])
        src_rows = np.concatenate([src_rows.astype(object), extra_src])
    elif args.views:
        rng = np.random.default_rng(0)
        extra, extra_y = [], []
        for r, y in zip(rows["train"], tr["y_img"]):
            im = pp.load_rgb(r["path"])
            views = [augment(im, rng) for _ in range(args.views)]
            extra.append(emb.images(views))
            extra_y += [y] * args.views
        Xtr = np.concatenate([Xtr] + extra)
        ytr = np.concatenate([ytr, np.asarray(extra_y)])
        photo_w = np.concatenate([photo_w, np.ones(len(extra_y))])
        src_rows = np.concatenate([src_rows, np.repeat(tr["ds_img"], args.views)])

    # Class weights by inverse frequency of photos, not crops, capped at 10 times the median.
    freq = np.bincount(tr["y_img"], minlength=len(classes)).astype(float) * (1 + args.views)
    w = 1.0 / freq
    w = np.minimum(w / np.median(w), 10.0)
    sample_w = w[ytr] * photo_w
    if args.balance_sources:
        pair = collections.Counter()
        for s_, y_, pw in zip(src_rows, ytr, photo_w):
            pair[(s_, y_)] += pw
        n_src = collections.Counter(y for _, y in pair)
        sample_w = sample_w * np.asarray([freq[y] / (n_src[y] * pair[(s_, y)]) for s_, y in zip(src_rows, ytr)])

    def photo_probs(n: str, clf, T: float = 1.0) -> np.ndarray:
        """Each photo's verdict: the mean of its crops' restricted, tempered probabilities."""
        d = data[n]
        p = softmax(restricted_logits(clf.decision_function(d["X"]), species_of_class, d["sp"]) / T)
        out = np.zeros((len(d["y_img"]), len(classes)))
        np.add.at(out, d["group"], p)
        return out / d["crops_per_photo"][:, None]

    best = None
    grid = (1, 5, 20, 50, 100, 200, 500, 1000, 2000, 5000)
    dv = data["val"]
    val_multi = np.asarray([(species_of_class == s).sum() > 1 for s in dv["sp_img"]])
    # The crops the worker returns one by one, from photos where the photo's label is the crop's.
    crop_multi = np.asarray([(species_of_class == s).sum() > 1 for s in dv["sp"]])
    crop_sel = dv["single"] & crop_multi
    if crop_sel.sum() < 200:
        crop_sel = crop_multi
    grid_scores = []
    for C in grid:
        clf = LogisticRegression(C=C, max_iter=3000)
        clf.fit(Xtr, ytr, sample_weight=sample_w)
        per_photo = topk_acc(photo_probs("val", clf)[val_multi], dv["y_img"][val_multi], 1)
        lv_c = restricted_logits(clf.decision_function(dv["X"][crop_sel]), species_of_class, dv["sp"][crop_sel])
        per_crop = topk_acc(softmax(lv_c), dv["y"][crop_sel], 1)
        grid_scores.append({"C": C, "val_top1_per_photo": per_photo, "val_top1_per_crop": per_crop})
        print(f"C={C}: restricted val top-1 per photo {per_photo:.3f}, per crop {per_crop:.3f}")
        acc = per_crop if args.select_on == "crop" else per_photo
        if best is None or acc > best[0]:
            best = (acc, C, clf)
    _, C, clf = best
    c_at_edge = C in (grid[0], grid[-1])
    if c_at_edge:
        print(f"WARNING: C={C} is at the edge of the grid {grid}")

    # The worker tempers each crop. A crop carries its photo's label, which is wrong for a healthy
    # leaf in a diseased photo, so T is fitted on the crops of photos with exactly one crop.
    lv = restricted_logits(clf.decision_function(dv["X"]), species_of_class, dv["sp"])
    fit_on = dv["single"] if dv["single"].sum() >= 200 else np.ones(len(dv["y"]), bool)
    T = fit_temperature(lv[fit_on], dv["y"][fit_on])

    def report(name: str) -> dict:
        d = data[name]
        raw = clf.decision_function(d["X"])
        res_crop_untempered = softmax(restricted_logits(raw, species_of_class, d["sp"]))
        res_crop = softmax(restricted_logits(raw, species_of_class, d["sp"]) / T)
        res = photo_probs(name, clf, T)
        res_untempered = photo_probs(name, clf)
        full = np.zeros((len(d["y_img"]), len(classes)))
        np.add.at(full, d["group"], softmax(raw))
        full /= d["crops_per_photo"][:, None]
        y, spn = d["y_img"], d["sp_img"]
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
        for ds in sorted(set(d["ds_img"])):
            m = (d["ds_img"] == ds) & (k_of > 1)
            per_dataset[ds] = {
                "restricted_top1_multiclass": over(m, lambda mm: topk_acc(res[mm], y[mm], 1)),
                "ece_after_T_multiclass": over(m, lambda mm: ece(res[mm], y[mm])),
            }
        pred = res.argmax(1)
        per_class = {}
        for ci, cname in enumerate(classes):
            sup, hit = y == ci, pred == ci
            per_class[cname] = {"n": int(sup.sum()),
                                "recall": float(hit[sup].mean()) if sup.any() else None,
                                "precision": float((y[hit] == ci).mean()) if hit.any() else None}
        k_crop = k_of[d["group"]]
        one = d["single"] & (k_crop > 1)
        out = {
            "n": int(len(y)),
            "left_out_class_images": left_out_images.get(name, 0),
            "left_out_species_images": left_out_species.get(name, {}),
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
            "per_class": per_class,
        }
        if args.features == "crops":
            yc = d["y"]
            out["per_leaf_crop"] = {
                "crops": int(len(yc)),
                "photos_with_no_leaf_kept": int(d["no_leaf"].sum()),
                "crops_per_photo_median": float(np.median(d["crops_per_photo"])),
                "top1_multiclass": over(k_crop > 1, lambda m: topk_acc(res_crop[m], yc[m], 1)),
                "top3_over_3_classes": over(k_crop > 3, lambda m: topk_acc(res_crop[m], yc[m], 3)),
                "single_crop_photos_top1_multiclass": over(one, lambda m: topk_acc(res_crop[m], yc[m], 1)),
                "single_crop_photos_ece_before_T": over(one, lambda m: ece(res_crop_untempered[m], yc[m])),
                "single_crop_photos_ece_after_T": over(one, lambda m: ece(res_crop[m], yc[m])),
            }
            out["note"] = ("Headline numbers are per photo: the mean of its crops' restricted, tempered probabilities; "
                           "the worker doesn't average, it returns each crop's top 3. per_leaf_crop scores each crop "
                           "against its photo's label, which is wrong for a healthy leaf in a diseased photo; photos "
                           "with one crop are the cleanest and the closest to what the worker returns. Restricted "
                           "metrics assume the species is right.")
        else:
            out["note"] = "Restricted metrics assume the species is right. The head is trained and scored on whole images; the worker feeds it leaf crops."
        return out

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
        "features": args.features,
        "detector": crop_source.det.entry["version"] if crop_source else None,
        "supported": args.supported,
        "classes": len(classes),
        "left_out_below_min": left_out,
        "C": C,
        "C_grid": list(grid),
        "C_at_grid_edge": c_at_edge,
        "C_selected_on": ("val top-1 per crop, photos with one crop" if args.select_on == "crop" and crop_sel is not crop_multi
                          else "val top-1 per crop, every crop" if args.select_on == "crop" else "val top-1 per photo"),
        "C_grid_scores": grid_scores,
        "temperature": T,
        "temperature_fitted_on": "val crops of photos with one crop" if fit_on is dv["single"] else "all val crops",
        "views": args.views,
        "dropped_classes": list(args.drop_classes),
        "balance_sources": args.balance_sources,
        "train_rows": int(len(ytr)),
        "train_photos": int(len(tr["y_img"])),
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
        for cname, v in r["per_class"].items():
            rc = "n/a" if v["recall"] is None else f"{v['recall']:.2f}"
            pc = "n/a" if v["precision"] is None else f"{v['precision']:.2f}"
            print(f"      {cname:42s} n={v['n']:4d} recall {rc} precision {pc}")
        if "per_leaf_crop" in r:
            c = r["per_leaf_crop"]
            print(f"   per leaf crop: {c['crops']} crops, top-1 {fmt(c['top1_multiclass'])}, "
                  f"one-crop photos top-1 {fmt(c['single_crop_photos_top1_multiclass'])} "
                  f"ECE {fmt(c['single_crop_photos_ece_before_T'])} -> {fmt(c['single_crop_photos_ece_after_T'])}")


if __name__ == "__main__":
    main()
