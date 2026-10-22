"""Disease head on frozen BioCLIP 2 features (training.md §6; models.md §7).

    python -m ml.training.heads.train_disease --version disease-head-v0-2026-10-16

- Classes: every (species, problem | healthy) with at least --min-train training images.
- Multinomial logistic regression; C chosen on validation by restricted top-1, from a grid
  wide enough that the choice isn't at its edge (the report says if it is), or with
  --select-on dev-top3 by the top 3 the API keeps on the dev cases (below). With --features
  crops, --select-on crop (the default) scores what the worker returns: each leaf crop of the
  val photos with exactly one crop (a crop carries its photo's label, which is wrong for a
  healthy leaf in a diseased photo with several). --select-on photo scores each photo by the
  mean of its crops' probabilities, which the worker doesn't do. The report gives both.
- Class weights by inverse frequency, capped at 10 times the median weight.
- Restricted softmax: at serving time the worker keeps only the working species' classes, so
  every accuracy and the temperature are computed on that restricted softmax.
- --supported trains only the head's crops; D3 (issue #4) settled them as tomato, apple, grape,
  pepper and corn, as v3c was trained. Without it every species is trained, bean included.
- The headline numbers leave out --not-supported species (bean by default, a label_only species
  since D3), and every number is also reported per dataset, so the field sets' own test splits
  show apart from PlantDoc's.
- --views N adds N augmented views per training image (random resized crop 0.7–1.0, flip,
  colour jitter 0.2, mild blur); with --features crops, N views of each crop, embedded first by
  `ml.training.heads.crops --splits train --views N`. Each view of a photo weighs 1 in total.
- --drop-classes leaves the named classes out (their photos too), for a problem with no profile
  in the knowledge base.
- The evaluation set's dev cases judge the head by its job (evaluation 5): the API adds its top 3
  to the candidates the VLM's questions decide between (architecture 6.3), so every C in the grid
  is also scored by top-3 on the dev cases' single-crop leaf photos, scored as
  `ml.training.eval.calibrate` scores them. The temperature is fitted on those photos, the ones most
  like the app's (--temperature-on val keeps the val fit), and the report gives the dev top-3
  against guessing, the medium-confidence claims at both temperatures, and both per crop and per
  source. --no-dev skips it (the temperature is then fitted on val). The dev photos' crops must be
  cached first: `python -m ml.training.eval.calibrate` caches them.

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


def select_c(scores: list[dict], select_on: str) -> int:
    """The grid point to keep. With `dev-top3`, every C whose dev top 3 (`calibrate`'s
    `top3_sent`: the true problem among the 3 classes the worker sends, on the dev photos with a
    problem) is within one standard error of the best is as good on so few photos; of those, the
    best val top-1 per crop wins."""
    if select_on != "dev-top3":
        key = "val_top1_per_photo" if select_on == "photo" else "val_top1_per_crop"
        return max(range(len(scores)), key=lambda i: scores[i][key])
    best = max(s["dev_top3_sent"] for s in scores)
    se = (best * (1 - best) / scores[0]["dev_with_problem"]) ** 0.5
    near = [i for i, s in enumerate(scores) if s["dev_top3_sent"] >= best - se - 1e-12]
    return max(near, key=lambda i: scores[i]["val_top1_per_crop"])


def val_without(score: dict, dataset: str) -> float:
    """A grid point's val top-1 per crop with one dataset's crops left out."""
    rest = [v for d, v in score["val_by_dataset"].items() if d != dataset]
    return sum(v["top1"] * v["n"] for v in rest) / sum(v["n"] for v in rest)


def c_without_each_dataset(scores: list[dict], select_on: str) -> dict:
    """The C that `select_c` picks with each dataset's val crops left out in turn, so a dataset
    whose val looks like its train (Makerere's close-ups of one field) can be seen not to decide C."""
    if select_on == "photo":
        return {}
    return {d: scores[select_c([{**s, "val_top1_per_crop": val_without(s, d)} for s in scores], select_on)]["C"]
            for d in scores[0]["val_by_dataset"]}


def dev_set(manifest: Path, photos: Path, emb: Embedder, crop_source) -> dict:
    """The dev cases, and their leaf photos' embeddings from the caches, as calibrate reads them."""
    from ml.training.eval import calibrate as cb

    cases = cb.dev_cases(manifest)
    files = sorted({photos / f for c in cases for f, _ in cb.leaf_photos(c)})
    whole = dict(zip(files, emb.images([cb.stored(f) for f in files])))
    if crop_source is None:
        return {"cases": cases, "crops_of": lambda f: [], "whole_of": whole.__getitem__}
    try:
        loaded = dict(zip(files, crop_source.load(files)))
    except FileNotFoundError as e:
        raise SystemExit(f"a dev photo's crops aren't cached ({e.filename}): run `python -m ml.training.eval.calibrate` "
                         "first, or pass --no-dev") from None
    return {"cases": cases, "crops_of": lambda f: loaded[f]["emb"], "whole_of": whole.__getitem__}


def main(argv=None) -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--features", choices=("image", "crops"), default="image",
                    help="image: one embedding per photo (v0-v1d); crops: one per leaf our detector keeps "
                         "(ml.training.heads.crops), or the whole photo when it keeps none, as the worker does")
    ap.add_argument("--supported", nargs="*",
                    help="train only these species' classes (classifier_supported; D3 on issue #4: tomato apple "
                         "grape pepper corn); the others go to the VLM")
    ap.add_argument("--min-train", type=int, default=30, help="training photos a class needs")
    ap.add_argument("--views", type=int, default=0)
    ap.add_argument("--drop-classes", nargs="*", default=[], metavar="SPECIES/PROBLEM",
                    help="classes to leave out, e.g. tomato/tomato_mosaic_virus")
    ap.add_argument("--not-supported", nargs="*", default=["bean"],
                    help="species left out of the headline numbers (still trained and reported)")
    ap.add_argument("--balance-sources", action="store_true",
                    help="within each class, give every dataset the same total weight")
    ap.add_argument("--select-on", choices=("crop", "photo", "dev-top3"), default="crop",
                    help="with --features crops, choose C by per-crop top-1 on val photos with one crop (as the "
                         "worker returns each crop), or by per-photo top-1 (the mean of a photo's crops); "
                         "dev-top3 by the top 3 the API keeps on the dev photos with a problem, the head's job: "
                         "of the C within one standard error of the best, the best per-crop val top-1")
    ap.add_argument("--dev-manifest", type=Path, default=ROOT / "ml/training/eval/manifest.jsonl",
                    help="the evaluation set, whose dev cases score each C by top-3 and fit the temperature")
    ap.add_argument("--dev-photos", type=Path, default=ROOT / "ml/training/eval/photos")
    ap.add_argument("--no-dev", action="store_true", help="skip the dev cases; the temperature is fitted on val")
    ap.add_argument("--temperature-on", choices=("dev", "val"), default="dev",
                    help="fit the temperature on the dev cases' single-crop leaf photos (evaluation 5), or on "
                         "val's one-crop photos")
    args = ap.parse_args(argv)
    if args.no_dev:
        args.temperature_on = "val"
    if args.select_on == "dev-top3" and args.no_dev:
        ap.error("--select-on dev-top3 needs the dev cases")
    if args.features != "crops" and args.select_on == "crop":
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

    labels = [
        {"index": i, "label": c, "species_id": c.split("/")[0], "problem_id": c.split("/")[1]}
        for i, c in enumerate(classes)
    ]
    healthy = {i for i, c in enumerate(classes) if c.endswith("/healthy")}
    dev = None if args.no_dev else dev_set(args.dev_manifest, args.dev_photos, emb, crop_source)

    def dev_items(clf) -> tuple[list, dict]:
        from ml.training.eval import calibrate as cb

        head = {"weight": clf.coef_, "bias": clf.intercept_, "labels": labels}
        return cb.disease_items(dev["cases"], args.dev_photos, head, dev["crops_of"], dev["whole_of"])

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

    fits = []
    grid = (0.01, 0.03, 0.1, 0.3, 1, 5, 20, 50, 100, 200, 500, 1000, 2000, 5000)
    dv = data["val"]
    val_multi = np.asarray([(species_of_class == s).sum() > 1 for s in dv["sp_img"]])
    # The crops the worker returns one by one, from photos where the photo's label is the crop's.
    crop_multi = np.asarray([(species_of_class == s).sum() > 1 for s in dv["sp"]])
    crop_sel = dv["single"] & crop_multi
    if crop_sel.sum() < 200:
        crop_sel = crop_multi
    crop_ds = dv["ds_img"][dv["group"]][crop_sel]
    grid_scores = []
    for C in grid:
        clf = LogisticRegression(C=C, max_iter=3000)
        clf.fit(Xtr, ytr, sample_weight=sample_w)
        per_photo = topk_acc(photo_probs("val", clf)[val_multi], dv["y_img"][val_multi], 1)
        lv_c = restricted_logits(clf.decision_function(dv["X"][crop_sel]), species_of_class, dv["sp"][crop_sel])
        per_crop = topk_acc(softmax(lv_c), dv["y"][crop_sel], 1)
        right = softmax(lv_c).argmax(1) == dv["y"][crop_sel]
        by_ds = {d: {"top1": float(right[crop_ds == d].mean()), "n": int((crop_ds == d).sum())}
                 for d in sorted(set(crop_ds))}
        grid_scores.append({"C": C, "val_top1_per_photo": per_photo, "val_top1_per_crop": per_crop,
                            "val_by_dataset": by_ds})
        print(f"C={C}: restricted val top-1 per photo {per_photo:.3f}, per crop {per_crop:.3f}", end="")
        if dev:
            from ml.training.eval import calibrate as cb

            sd = cb.disease_scores(dev_items(clf)[0], 1.0, healthy)
            grid_scores[-1].update({"dev_top3_sent": sd["top3_sent"], "dev_with_problem": sd["with_problem"],
                                    "dev_top1": sd["top1"], "dev_n": sd["n"]})
            print(f"; dev top-3 {sd['top3_sent']:.3f} of {sd['with_problem']} with a problem, "
                  f"top-1 {sd['top1']:.3f} of {sd['n']}", end="")
        print()
        fits.append((C, clf))
    i = select_c(grid_scores, args.select_on)
    C, clf = fits[i]
    c_at_edge = C in (grid[0], grid[-1])
    if c_at_edge:
        print(f"WARNING: C={C} is at the edge of the grid {grid}")

    # The worker tempers each crop. A crop carries its photo's label, which is wrong for a healthy
    # leaf in a diseased photo, so T is fitted on the crops of photos with exactly one crop.
    lv = restricted_logits(clf.decision_function(dv["X"]), species_of_class, dv["sp"])
    fit_on = dv["single"] if dv["single"].sum() >= 200 else np.ones(len(dv["y"]), bool)
    T_val = fit_temperature(lv[fit_on], dv["y"][fit_on])
    # On the dev cases instead (evaluation 5): the val fit lets the head claim a problem at medium
    # confidence on photos unlike its training data far more often than it is right.
    dev_report, T_dev = None, None
    if dev:
        from ml.training.eval import calibrate as cb

        items, dev_left_out = dev_items(clf)
        T_dev = cb.fit_items(items) if len(items) >= 10 else None

        def brief(it, t):
            return {k: v for k, v in cb.disease_scores(it, t, healthy).items() if k != "reliability"}

        dev_report = {
            "photos": len(items), "left_out": dict(dev_left_out),
            "at_T_val": brief(items, T_val), **({"at_T_dev": brief(items, T_dev)} if T_dev else {}),
            "per_species": {sp: brief([i for i in items if i[3] == sp], T_dev or T_val) for sp in sorted({i[3] for i in items})},
            "per_source": {src: brief([i for i in items if i[4] == src], T_dev or T_val) for src in sorted({i[4] for i in items})},
        }
    if args.temperature_on == "dev" and T_dev is None:
        raise SystemExit("--temperature-on dev: fewer than 10 dev leaf photos the head can label")
    T = T_dev if args.temperature_on == "dev" else T_val

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
        "C_selected_on": ("best val top-1 per crop within one standard error of the best dev top 3 as sent, on the photos with a problem" if args.select_on == "dev-top3"
                          else "val top-1 per crop, photos with one crop" if args.select_on == "crop" and crop_sel is not crop_multi
                          else "val top-1 per crop, every crop" if args.select_on == "crop" else "val top-1 per photo"),
        "C_grid_scores": grid_scores,
        "C_with_dataset_left_out_of_val": c_without_each_dataset(grid_scores, args.select_on),
        "temperature": T,
        "temperature_fitted_on": ("dev cases' single-crop leaf photos" if args.temperature_on == "dev"
                                  else "val crops of photos with one crop" if fit_on is dv["single"] else "all val crops"),
        "temperature_val": T_val,
        "temperature_dev": T_dev,
        "views": args.views,
        "dropped_classes": list(args.drop_classes),
        "balance_sources": args.balance_sources,
        "train_rows": int(len(ytr)),
        "train_photos": int(len(tr["y_img"])),
        "val": report("val"),
        "test": report("test"),
        "dev": dev_report,
    }
    (out / "report.json").write_text(json.dumps(summary, indent=2) + "\n")
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{args.version}.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k not in ("val", "test", "dev")}, indent=2))
    if dev_report:
        def line(r):
            m = r["medium"]
            return (f"top-3 {r['top3_sent']:.2f} of {r['with_problem']} with a problem (guessing "
                    f"{r['chance_top3_sent']:.2f}), top-1 {r['top1']:.2f}, medium claims {m['n']} ({m['right']} right), "
                    f"healthy blocked {r['healthy']['blocked']} of {r['healthy']['n']}, ECE {r['ece']:.3f}")
        print(f"dev: {dev_report['photos']} single-crop leaf photos | at T_val {T_val:.3f}: {line(dev_report['at_T_val'])}")
        if "at_T_dev" in dev_report:
            print(f"     at T_dev {T_dev:.3f}: {line(dev_report['at_T_dev'])}")
        for key in ("per_species", "per_source"):
            for k, v in dev_report[key].items():
                t3 = "n/a" if v.get("top3_sent") is None else f"{v['top3_sent']:.2f} (guessing {v['chance_top3_sent']:.2f})"
                print(f"   {k:32s} n={v['n']:3d} top-3 {t3} top-1 {v['top1']:.2f}")
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
