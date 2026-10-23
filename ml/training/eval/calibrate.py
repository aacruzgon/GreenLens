"""Calibration on the dev cases, the part that runs offline on the model files (evaluation 5; M16).

    python -m ml.training.eval.calibrate [--heads <version> ...] [--out calibration-<date>]

`--heads` defaults to the head the seed's classifier_labels.yaml names, the API's.

Each dev case runs through the worker's perception steps on the same model files:

- **Species (`T_species`, the `bioclip` entry's `species_temperature`).** Each shot is stored with
  its long side at most 1,600 px and embedded by BioCLIP 2; its species probabilities are
  `softmax(100 / T × e · rows)` over the label matrix's species rows, and the shots are averaged
  with the worker's weights (whole plant and flower 2, leaf close-up and underside 1), as
  `vision::perception::heads::combine_species` does. `T` is fitted by the negative log-likelihood
  of the true species, and the report checks evaluation 5's agreement rule: the top-1 accuracy of
  cases scored 0.8 or more, a case whose species the matrix can't name counted as wrong. If it is
  under 95%, the threshold is raised to the lowest value at which it holds; the temperature stays
  at the fit. The JSON keeps each case's inputs (cosines and logits, no image data), so the fits
  can be recomputed without the photos (`species_data`, `disease_items_from`).
  The fit is also scored with every case at a temperature fitted without it (5 folds).
- **Disease head (`T_disease`, the head file's `temperature`).** Every leaf crop the shipped
  detector keeps on a leaf shot (`ml.training.heads.crops`, cut as the worker cuts them), or the
  whole close-up when it keeps none; an underside with no leaf box is dropped, as the worker drops
  it. A one-photo case's photo is sent as a close-up, as evaluation 8's bench sends it: as
  labelled (`whole_plant`), the worker wouldn't classify it. The head's raw logits are restricted
  to the case's true species; the worker restricts them to the working species, BioCLIP's top 1
  unless the user names one, so these numbers assume the species is right. A crop carries its
  case's label, which is wrong for a healthy leaf on a diseased plant, so `T` is fitted and scored
  on the photos with exactly one crop, as training fits it; only species with two or more classes
  count.

  The head's job is its top 3. The worker sends each leaf's top 3 classes, `healthy` among them,
  and the API keeps the problems among them as the scan's classifier candidates (contracts 5), so
  on a photo with a problem, a hit is the true problem in the head's top 3, against guessing 3 of
  the species' classes. The report also gives the top 3 problems once `healthy` is dropped, what
  the API would get if the worker sent 3 problems per leaf. The temperature doesn't change either
  ranking. What it changes is how often the head claims a problem at medium confidence, which
  shows a possible problem and keeps a scan from `healthy`, and how often its top class is
  `healthy` at medium, which counts as positive evidence of health (contracts 7). The report gives
  both, overall and per species, at the head file's temperature (fitted when the head was chosen), one fitted on
  validation and dev (evaluation 5), one fitted on dev, and one per species fitted on dev; and
  each dev fit again with every case scored at a temperature fitted without it (5 folds).

Not here, since they need the cases through the API (J5): the VLM certainty words, the diagnosis
margin and the indicator weights.

Writes `reports/<out>.md` and `.json`, and a reliability diagram per head and crop as
`reports/<out>-*.png`. The values aren't written into any model file: the final manifests (M17)
take them by hand.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import shlex
import sys
import tomllib
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
from scipy.optimize import minimize_scalar

from ml.pdm_vision import preprocess as pp
from ml.pdm_vision.headfile import read_label_matrix, read_linear_head
from ml.training.labels import current

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
MEDIUM = 0.5             # contracts 1: the medium band starts here
BINS = 15
FOLDS = 5
SENT = 3                 # vision::perception: each leaf's top 3 classes, `healthy` included


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


T_BOUNDS = (0.05, 10.0)


def fit_scalar(nll) -> float:
    return float(minimize_scalar(nll, bounds=T_BOUNDS, method="bounded").x)


def folds(cases: list[tuple[str, str]], k: int = FOLDS, seed: int = 0) -> dict[str, int]:
    """Each case's fold. Cases are shuffled with a fixed seed within each stratum (the species),
    then dealt out in turn, so every fold holds every species that has `k` cases or more."""
    rng = np.random.default_rng(seed)
    by = defaultdict(set)
    for case, stratum in cases:
        by[stratum].add(case)
    out, n = {}, 0
    for stratum in sorted(by):
        ids = sorted(by[stratum])
        rng.shuffle(ids)
        for c in ids:
            out[c] = n % k
            n += 1
    return out


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
    """`species_temperature` on the dev cases. A case whose species has no row in the matrix
    (squash, before its profile is in the seed) can't be named, so it is always wrong: it's left
    out of the likelihood fit, which needs the true row, but counted as wrong at the agreement
    threshold, since a confident wrong species is what the one-tap rule must not offer."""
    ids = [l["id"] for l in matrix["labels"] if l["table"] == "species"]
    rows = matrix["rows"][[i for i, l in enumerate(matrix["labels"]) if l["table"] == "species"]]
    usable, outside, left_out = [], [], Counter()
    for c in cases:
        sp = c["truth"].get("species_id")
        shots = [im for im in c["images"] if im["kind"] in SHOT_WEIGHT]
        if not shots:
            left_out["no scored shot"] += 1
        elif sp in ids:
            usable.append((c, shots))
        elif sp:
            outside.append((c, shots))
        else:
            left_out["no species"] += 1
    files = [photos / im["file"] for _, shots in usable + outside for im in shots]
    emb = dict(zip(files, embed([stored(f) for f in files])))

    def item(c, shots, y):
        return ([emb[photos / im["file"]] @ rows.T for im in shots], [SHOT_WEIGHT[im["kind"]] for im in shots], y, c)

    data = [item(c, shots, ids.index(c["truth"]["species_id"])) for c, shots in usable]
    out_data = [item(c, shots, None) for c, shots in outside]
    fitted = fit_species(data)
    out = {"cases": len(data), "outside": dict(Counter(c["truth"]["species_id"] for c, _ in outside)),
           "left_out": dict(left_out), "labels": len(ids), "fitted_temperature": round(fitted, 4)}
    for name, t in (("t_1", 1.0), ("fitted", fitted)):
        out[name] = species_summary(data, ids, np.full(len(data), t), out_data, np.full(len(out_data), t))
        out[name]["temperature"] = round(t, 4)
    # The fit scored without each case's own photos: fitted on the other folds. The cases the
    # matrix can't name are dealt into folds too, so the agreement check sees them in every fold.
    fold = folds([(d[3]["case_id"], d[3]["truth"]["species_id"]) for d in data + out_data])
    tf, tf_out, per_fold = np.empty(len(data)), np.empty(len(out_data)), []
    for f in range(FOLDS):
        t = fit_species([d for d in data if fold[d[3]["case_id"]] != f])
        per_fold.append(round(t, 4))
        tf[[n for n, d in enumerate(data) if fold[d[3]["case_id"]] == f]] = t
        tf_out[[n for n, d in enumerate(out_data) if fold[d[3]["case_id"]] == f]] = t
    out["fitted_out_of_fold"] = species_summary(data, ids, tf, out_data, tf_out)
    out["fitted_out_of_fold"]["temperature"] = per_fold
    out["species_ids"] = ids
    out["inputs"] = species_inputs(data + out_data)
    return out


def species_inputs(data: list) -> list[dict]:
    """What the fit and every species score are computed from, without image data: each case's
    shots as their weights and cosines with the matrix's species rows (6 decimals), and the true
    row (None for a species the matrix can't name). `species_data` reads them back."""
    return [{"case": c["case_id"], "species": c["truth"]["species_id"], "y": y, "weights": w,
             "cos": [[round(float(v), 6) for v in shot] for shot in cos]} for cos, w, y, c in data]


def species_data(inputs: list[dict]) -> tuple[list, list]:
    """A report's species inputs as `species_calibration`'s (named, outside) items."""
    items = [([np.asarray(s, float) for s in r["cos"]], r["weights"], r["y"],
              {"case_id": r["case"], "truth": {"species_id": r["species"]}}) for r in inputs]
    return [i for i in items if i[2] is not None], [i for i in items if i[2] is None]


THRESHOLD_STEP = 0.01


def agreement(data: list, tt: np.ndarray, out_data: list, tt_out: np.ndarray, thr: float = AGREEMENT) -> dict:
    """The cases scored `thr` or more, counting every case outside the matrix there as wrong."""
    conf_in, right_in = np.zeros(0), np.zeros(0, bool)
    if data:
        p = species_matrix(data, tt)
        y = np.array([d[2] for d in data])
        conf_in, right_in = p.max(1), p.argmax(1) == y
    conf_out = species_matrix(out_data, tt_out).max(1) if out_data else np.zeros(0)
    hi_in, hi_out = conf_in >= thr, conf_out >= thr
    n = int(hi_in.sum() + hi_out.sum())
    return {"n": n, "outside": int(hi_out.sum()), "accuracy": float(right_in[hi_in].sum() / n) if n else None}


def agreement_threshold(data: list, tt: np.ndarray, out_data: list, tt_out: np.ndarray) -> dict | None:
    """Evaluation 5's agreement threshold at these temperatures: 0.8 when the cases scored 0.8 or
    more, those outside the matrix counted as wrong, are 95% right; otherwise the lowest threshold
    above it, in steps of 0.01, at which they are; None if none below 1 holds. The temperature stays
    at the fit: raising it would change every species confidence, not only the one-tap cut."""
    for k in range(int(round(AGREEMENT / THRESHOLD_STEP)), int(round(1 / THRESHOLD_STEP))):
        thr = round(k * THRESHOLD_STEP, 2)
        a = agreement(data, tt, out_data, tt_out, thr)
        if a["n"] == 0 or a["accuracy"] >= AGREEMENT_ACCURACY:
            return {"threshold": thr, **a}
    return None


def species_matrix(data: list, tt: np.ndarray) -> np.ndarray:
    return np.stack([species_probs(cos, w, t) for (cos, w, _, _), t in zip(data, tt)])


def fit_species(data: list) -> float:
    y = np.array([d[2] for d in data])
    return fit_scalar(lambda t: float(-np.log(species_matrix(data, np.full(len(data), t))[np.arange(len(y)), y] + 1e-12).mean()))


def species_summary(data: list, ids: list[str], tt: np.ndarray, out_data: list = (), tt_out=None) -> dict:
    """The species scores with case `n` at temperature `tt[n]`; the agreement check also with the
    cases outside the matrix (`out_data`, at `tt_out`), counted as wrong."""
    y = np.array([d[2] for d in data])
    p = species_matrix(data, tt)
    conf, top = p.max(1), p.argmax(1)
    correct = top == y
    sweep = []
    for thr in np.round(np.arange(0.5, 1.0, 0.05), 2):
        m = conf >= thr
        sweep.append({"threshold": float(thr), "n": int(m.sum()), "accuracy": float(correct[m].mean()) if m.any() else None})
    ok = [s["threshold"] for s in sweep if s["n"] >= 10 and s["accuracy"] is not None and s["accuracy"] >= AGREEMENT_ACCURACY]
    hi = conf >= AGREEMENT
    tt_out = np.asarray(tt_out if tt_out is not None else [], float)
    return {
        "top1": float(correct.mean()), "nll": float(-np.log(p[np.arange(len(y)), y] + 1e-12).mean()), "ece": ece(conf, correct),
        "at_agreement": {"n": int(hi.sum()), "accuracy": float(correct[hi].mean()) if hi.any() else None},
        "at_agreement_with_outside": agreement(data, tt, list(out_data), tt_out),
        "agreement_threshold": agreement_threshold(data, tt, list(out_data), tt_out),
        "lowest_threshold_at_95": min(ok) if ok else None, "sweep": sweep,
        "reliability": reliability(conf, correct),
        "per_species": {ids[s]: {"n": int((y == s).sum()), "top1": float(correct[y == s].mean())} for s in sorted(set(y))},
        "wrong": [{"case": d[3]["case_id"], "truth": ids[d[2]], "top": ids[k], "score": round(float(cf), 3)}
                  for d, k, cf, ok_ in zip(data, top, conf, correct) if not ok_],
        "outside_top": [{"case": d[3]["case_id"], "truth": d[3]["truth"]["species_id"], "top": ids[int(q.argmax())],
                         "score": round(float(q.max()), 3)}
                        for d, q in zip(out_data, species_matrix(list(out_data), tt_out) if len(out_data) else [])],
        "per_case": [{"case": d[3]["case_id"], "top": ids[k], "confidence": round(float(cf), 4), "right": bool(r)}
                     for d, k, cf, r in zip(data, top, conf, correct)],
    }


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


def leaf_photos(c: dict) -> list[tuple[str, str]]:
    """The case's leaf shots as (file, kind). A one-photo case's photo is a leaf on its own, sent as a
    close-up as evaluation 8's bench sends it; as labelled (`whole_plant`), the worker skips it."""
    leafy = [(im["file"], im["kind"]) for im in c["images"] if im["kind"] in LEAFY]
    return leafy or ([(c["images"][0]["file"], "leaf_closeup")] if len(c["images"]) == 1 else [])


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
        for f, kind in files:
            crops = crops_of(photos / f)
            if len(crops) > 1:
                left_out["leaf photos with several crops"] += 1
                continue
            if len(crops) == 0 and kind != "leaf_closeup":     # vision::perception: whole photos only for close-ups
                left_out["undersides with no leaf box"] += 1
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


def temperatures(items: list, t) -> np.ndarray:
    """One temperature per item. `t` is a number, one value per item, or a {species: T} map whose
    `"*"` entry covers the species it doesn't name."""
    if isinstance(t, dict):
        return np.array([t.get(i[3], t["*"]) for i in items], float)
    return np.broadcast_to(np.asarray(t, float), (len(items),)).astype(float)


def _rate(hit: np.ndarray, mask: np.ndarray):
    return float(hit[mask].mean()) if mask.any() else None


def disease_scores(items: list, t, healthy: set[int] = frozenset()) -> dict:
    """What the head gives the API at temperature `t` (see `temperatures`):
    - `top3_sent`: on photos with a problem, the true problem among the head's top 3 classes, which
      the worker sends with `healthy` among them and the API keeps the problems of (contracts 5);
      `chance_top3_sent` is guessing 3 of the species' k classes;
    - `top3_problems`: the true problem among the top 3 once `healthy` is dropped, what the API would
      get if the worker sent 3 problems per leaf; `chance_top3_problems` is guessing 3 of k − 1;
    - `*_can_miss`: the same on the species where the top 3 can miss (more than 3 classes, or more
      than 3 problems);
    - `medium`: photos whose top class is a problem at `MEDIUM` or more, which the API shows as a
      possible problem when the indicators are unsure and which keeps a scan from `healthy`, and how
      many are right;
    - `healthy`: of the healthy leaves, how many have `healthy` on top at `MEDIUM` or more, positive
      evidence of health (contracts 7), and how many a problem blocks;
    - `called_healthy`: photos with a problem whose top class is `healthy` at `MEDIUM` or more;
    - `top1` (every photo), `ece`, `nll` and the reliability bins.
    The two rankings don't depend on the temperature."""
    if not items:
        return {"n": 0}
    z = np.stack([i[0] for i in items])
    tt = temperatures(items, t)
    p = softmax(z / tt[:, None])
    y = np.array([i[1] for i in items])
    order = np.argsort(-z, axis=1, kind="stable")
    k = np.isfinite(z).sum(1)
    h = sorted(healthy)
    kp = k - (np.isfinite(z[:, h]).sum(1) if h else 0)
    conf, top = p.max(1), order[:, 0]
    correct = top == y
    top_healthy = np.isin(top, h)
    is_healthy = np.isin(y, h)
    sick = ~is_healthy
    medium = (conf >= MEDIUM) & ~top_healthy
    sent = np.array([yy in o[:SENT] for yy, o in zip(y, order)])
    problems = np.array([yy in [c for c in o[:kk] if c not in healthy][:3] for yy, o, kk in zip(y, order, k)])
    chance_sent = np.minimum(SENT, k) / k
    chance_problems = np.minimum(3, kp) / np.maximum(kp, 1)

    def can_miss(hit, chance, m):
        return {"n": int(m.sum()), "top3": _rate(hit, m), "chance": _rate(chance, m)}

    return {"n": len(items), "with_problem": int(sick.sum()),
            "top3_sent": _rate(sent, sick), "chance_top3_sent": _rate(chance_sent, sick),
            "top3_sent_can_miss": can_miss(sent, chance_sent, sick & (k > SENT)),
            "top3_problems": _rate(problems, sick), "chance_top3_problems": _rate(chance_problems, sick),
            "top3_problems_can_miss": can_miss(problems, chance_problems, sick & (kp > 3)),
            "top1": float(correct.mean()), "ece": ece(conf, correct),
            "nll": float(-np.log(p[np.arange(len(y)), y] + 1e-12).mean()),
            "medium": {"n": int(medium.sum()), "right": int((medium & correct).sum())},
            "healthy": {"n": int(is_healthy.sum()), "evidence": int((is_healthy & top_healthy & (conf >= MEDIUM)).sum()),
                        "blocked": int((is_healthy & medium).sum())},
            "called_healthy": {"n": int(sick.sum()), "called": int((sick & top_healthy & (conf >= MEDIUM)).sum())},
            "reliability": reliability(conf, correct)}


def fit_items(items: list) -> float:
    z = np.stack([i[0] for i in items])
    y = np.array([i[1] for i in items])
    return fit_scalar(lambda t: float(-np.log(softmax(z / t)[np.arange(len(y)), y] + 1e-12).mean()))


def fit_per_species(items: list) -> dict:
    """A temperature per species, and the pooled one as `"*"` for the species without photos."""
    by = defaultdict(list)
    for i in items:
        by[i[3]].append(i)
    return {"*": fit_items(items), **{sp: fit_items(v) for sp, v in sorted(by.items())}}


def out_of_fold(items: list, fit) -> tuple[np.ndarray, list]:
    """Each item's temperature fitted without its own case: `fit(training items)` gives a number or a
    {species: T} map, fitted on the other folds. Returns the temperatures and each fold's fit."""
    fold = folds([(i[2], i[3]) for i in items])
    tt, fits = np.empty(len(items)), []
    for f in range(FOLDS):
        held = [n for n, i in enumerate(items) if fold[i[2]] == f]
        t = fit([i for i in items if fold[i[2]] != f])
        fits.append(t)
        tt[held] = temperatures([items[n] for n in held], t)
    return tt, fits


def confusion(items: list, head: dict) -> dict:
    """Per species, each true class's top-1 answers (they don't depend on the temperature)."""
    name = {l["index"]: l["problem_id"] for l in head["labels"]}
    out = defaultdict(lambda: defaultdict(Counter))
    for z, y, _, sp, _ in items:
        out[sp][name[y]][name[int(np.argmax(z))]] += 1
    return {sp: {t: dict(c.most_common()) for t, c in sorted(v.items())} for sp, v in sorted(out.items())}


def disease_inputs(items: list) -> list[dict]:
    """Each dev leaf photo's head logits over its species' classes (6 decimals), its label, case,
    species and source: what the dev fits and scores are computed from. `disease_items_from` reads them back."""
    out = []
    for z, y, case, sp, src in items:
        k = np.flatnonzero(np.isfinite(z))
        out.append({"case": case, "species": sp, "source": src, "label": int(y), "classes": k.tolist(),
                    "logits": [round(float(v), 6) for v in z[k]]})
    return out


def disease_items_from(inputs: list[dict], classes: int) -> list:
    """A report's disease inputs as `disease_items`' items, the other species' classes at -inf."""
    items = []
    for r in inputs:
        z = np.full(classes, -np.inf)
        z[r["classes"]] = r["logits"]
        items.append((z, r["label"], r["case"], r["species"], r["source"]))
    return items


def lean(r: dict) -> dict:
    """A subgroup's scores without the reliability bins, which only the whole set's plots use."""
    return {k: v for k, v in r.items() if k != "reliability"}


def rounded(t):
    return {k: round(v, 4) for k, v in t.items()} if isinstance(t, dict) else round(t, 4)


def disease_calibration(name: str, cases: list[dict], photos: Path, crops_of, whole_of, crop_loader) -> dict:
    head = read_linear_head(HEADS / name / f"{name}.json")
    dev, left_out = disease_items(cases, photos, head, crops_of, whole_of)
    val = val_items(head, crop_loader)
    healthy = {l["index"] for l in head["labels"] if l["problem_id"] == "healthy"}
    temps = {"t_file": float(head["temperature"]), "t_val_and_dev": fit_items(val + dev),
             "t_dev": fit_items(dev), "t_per_species_dev": fit_per_species(dev)}
    fits = {"t_val_and_dev": lambda tr: fit_items(val + tr), "t_dev": fit_items, "t_per_species_dev": fit_per_species}
    oof = {k: out_of_fold(dev, f) for k, f in fits.items()}
    species = sorted({i[3] for i in dev})
    one_photo = {c["case_id"] for c in cases if len(c["images"]) == 1 and c["images"][0]["kind"] not in LEAFY}
    return {"head": name, "classes": len(head["labels"]), "dev_photos": len(dev), "val_photos": len(val),
            "dev_one_photo": sum(i[2] in one_photo for i in dev),
            "dev_one_photo_healthy": sum(i[2] in one_photo and i[1] in healthy for i in dev),
            "classes_per_species": {sp: int(sum(l["species_id"] == sp for l in head["labels"])) for sp in species},
            "left_out": dict(left_out), "bounds": list(T_BOUNDS),
            "temperatures": {k: rounded(t) for k, t in temps.items()},
            "dev": {k: disease_scores(dev, t, healthy) for k, t in temps.items()},
            "val": {k: disease_scores(val, t, healthy) for k, t in temps.items()},
            "dev_out_of_fold": {k: {**disease_scores(dev, tt, healthy), "fold_temperatures": [rounded(t) for t in fs],
                                    "per_species": {sp: lean(disease_scores([i for i in dev if i[3] == sp], tt[[i[3] == sp for i in dev]], healthy))
                                                    for sp in species}}
                                for k, (tt, fs) in oof.items()},
            "dev_per_species": {k: {sp: lean(disease_scores([i for i in dev if i[3] == sp], t, healthy)) for sp in species}
                                for k, t in temps.items()},
            "dev_per_source_species": {f"{src} {sp}": lean(disease_scores([i for i in dev if i[4] == src and i[3] == sp], temps["t_file"], healthy))
                                       for src, sp in sorted({(i[4], i[3]) for i in dev})},
            "dev_confusion": confusion(dev, head), "inputs": disease_inputs(dev)}


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
    (REPORTS / f"{out}.json").write_text(json.dumps({"date": args.date, "command": args.command, "manifest": args.manifest_label,
                                                     "species": species, "disease": disease}, indent=2) + "\n")
    plot({"T = 1": species["t_1"]["reliability"], f"T = {species['fitted_temperature']}": species["fitted"]["reliability"]},
         REPORTS / f"{out}-species.png", "BioCLIP 2 species, dev cases")
    for d in disease:
        plot({TEMP_SHORT[k]: d["dev"][k]["reliability"] for k in TEMP_NAMES}, REPORTS / f"{out}-{d['head']}.png",
             f"{d['head']}, dev leaf photos")

    s1, sf, so = species["t_1"], species["fitted"], species["fitted_out_of_fold"]
    n_out = sum(species["outside"].values())
    outside = ", ".join(f"{v} {k}" for k, v in species["outside"].items()) or "none"
    m = args.manifest_label
    lines = [f"# Calibration on the dev cases: {out}", "",
             f"`{args.command}`, on {args.date}, over `{m['file']}` (SHA-256 `{m['sha256']}`). The JSON beside this report holds "
             "each case's inputs (shot cosines and leaf logits, no image data), so every fit and score here can be recomputed "
             "without the photos.", "",
             "The offline part of evaluation 5 ([calibrate.py](../calibrate.py)), on the label matrix the API asks for "
             f"(`{args.matrix}`) and the disease heads named. "
             f"Every fit on the dev cases is also scored **out of fold**: each case at a temperature fitted on the other "
             f"{FOLDS - 1} of {FOLDS} folds, the cases dealt into folds within each species (seed 0).", "",
             "## Species: BioCLIP 2 zero-shot (`species_temperature`)", "",
             f"{species['cases']} dev cases over the label matrix's {species['labels']} species"
             + (f"; left out: {', '.join(f'{v} {k}' for k, v in species['left_out'].items())}" if species["left_out"] else "")
             + f". Cases whose species the matrix can't name ({outside}) are left out of the fit, which needs the true row, "
             "and counted as wrong at the agreement threshold: a confident wrong species is what the one-tap rule must not offer. "
             "The **agreement threshold** is 0.8 while the cases at 0.8 or more, those included, are 95% right, and otherwise the "
             "lowest threshold above it at which they are (evaluation 5); the temperature stays at the fit.", "",
             f"| `species_temperature` | Top-1 | NLL | ECE (15 bins) | Cases at 0.8 or more | Their top-1 | With the {n_out} outside: at 0.8 or more | Their top-1 | Agreement threshold | Lowest threshold with 95% top-1 |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    for label, r in ((f"{s1['temperature']}", s1), (f"**{sf['temperature']}, fitted**", sf),
                     (f"fitted out of fold ({min(so['temperature'])}–{max(so['temperature'])})", so)):
        w, a = r["at_agreement_with_outside"], r["agreement_threshold"]
        thr = "none below 1" if a is None else f"{a['threshold']:.2f} ({a['n']} cases, {pct(a['accuracy'])})"
        lines.append(f"| {label} | {pct(r['top1'])} | {num(r['nll'])} | {num(r['ece'])} | {r['at_agreement']['n']} | "
                     f"{pct(r['at_agreement']['accuracy'])} | {w['n']} ({w['outside']} outside) | {pct(w['accuracy'])} | {thr} | "
                     f"{num(r['lowest_threshold_at_95'], 2)} |")
    lines += ["", f"Per species at {sf['temperature']}: " + ", ".join(f"{k} {pct(v['top1'])} of {v['n']}" for k, v in sf["per_species"].items()) + ".",
              "", f"Wrong top-1 at {sf['temperature']}: " + ("; ".join(f"`{w['case']}` → {w['top']} ({w['score']})" for w in sf["wrong"]) or "none") + ".",
              "", f"The cases outside the matrix at {sf['temperature']}: " + ("; ".join(f"`{w['case']}` ({w['truth']}) → {w['top']} ({w['score']})" for w in sf["outside_top"]) or "none") + ".",
              "", f"![species reliability]({out}-species.png)", "",
              "## Disease head (`temperature`)", "",
              "Leaf photos of the dev cases with exactly one crop, or none (then the whole close-up; an underside with no "
              "leaf box is dropped, as the worker drops it). Where this differs from a scan through the worker:", "",
              "- **A one-photo case's photo is sent as a `leaf_closeup`,** as evaluation 8's bench sends it. As labelled "
              "(`whole_plant`), the worker wouldn't classify it (`no_leaves_found`). "
              f"{disease[0]['dev_one_photo']} of the {disease[0]['dev_photos']} photos are such cases, "
              f"{disease[0]['dev_one_photo_healthy']} of the {disease[0]['dev']['t_file']['healthy']['n']} healthy leaves among them.",
              "- **The head is restricted to the case's true species.** The worker restricts it to the working species, "
              "BioCLIP's top 1 unless the user names one, so these numbers assume the species is right.",
              f"- **The healthy leaves are field-set photos,** {disease[0]['dev']['t_file']['healthy']['n']} of them and none from iNaturalist.", "",
              "### The top 3 (the temperature doesn't change it)", "",
              "The worker sends each leaf's top 3 classes, `healthy` among them, and the API keeps the problems among them as "
              "the scan's classifier candidates (contracts 5). So on a photo with a problem, a hit is the true problem among "
              "the head's top 3, against guessing 3 of the species' k classes. **Top 3 problems** drops `healthy` first, "
              "against guessing 3 of k − 1: what the API would get if the worker sent 3 problems per leaf. **Where it can "
              "miss** leaves out the species whose classes (or problems) are 3 or fewer, where both always score 100%.", "",
              "| Head | Classes | Photos with a problem | **Top 3 as sent** (guessing) | Where it can miss: photos, top 3 (guessing) "
              "| Top 3 problems (guessing) | Where it can miss | Top-1, every photo |",
              "|---|---|---|---|---|---|---|---|"]
    for d in disease:
        r = d["dev"]["t_file"]
        cs, cp = r["top3_sent_can_miss"], r["top3_problems_can_miss"]
        lines.append(f"| `{d['head']}` | {d['classes']} | {r['with_problem']} | **{pct(r['top3_sent'])}** ({pct(r['chance_top3_sent'])}) | "
                     f"{cs['n']}, {pct(cs['top3'])} ({pct(cs['chance'])}) | {pct(r['top3_problems'])} ({pct(r['chance_top3_problems'])}) | "
                     f"{cp['n']}, {pct(cp['top3'])} ({pct(cp['chance'])}) | {pct(r['top1'])} of {r['n']} |")
    for d in disease:
        per = d["dev_per_species"]["t_file"]
        lines += ["", f"`{d['head']}`, top 3 as sent (guessing) on the photos with a problem, and top-1 of every photo:", ""]
        lines.append("- Per species: " + "; ".join(
            f"{sp} ({d['classes_per_species'][sp]} classes) {count(v['top3_sent'], v['with_problem'])} (guessing {pct(v['chance_top3_sent'])}), "
            f"top-1 {count(v['top1'], v['n'])}" for sp, v in per.items()) + ".")
        lines.append("- Per source: " + "; ".join(
            f"{k} {count(v['top3_sent'], v['with_problem'])} (guessing {pct(v['chance_top3_sent'])}), top-1 {count(v['top1'], v['n'])}"
            for k, v in d["dev_per_source_species"].items()) + ".")
        lines.append("- Top-1 answers per true class: " + " ".join(
            f"**{sp}:** " + "; ".join(f"{short(t)} ({sum(c.values())}) → " + ", ".join(f"{short(p)} {n}" for p, n in c.items())
                                      for t, c in rows.items()) + "." for sp, rows in d["dev_confusion"].items()))
        lines.append("- Left out: " + (", ".join(f"{v} {k}" for k, v in d["left_out"].items()) or "none") + ".")
    lines += ["", "### What the temperature changes", "",
              "At medium confidence (0.5) or more, a problem on top is shown as a possible problem when the indicators are "
              "unsure, and keeps a scan from `healthy`; `healthy` on top counts as positive evidence of health (contracts 7). "
              "One crop can't have two classes at 0.5 or more.", "",
              "| Head | Temperature | Value | Problem at medium: photos (right) | Healthy leaves with `healthy` at medium | "
              "Healthy leaves blocked by a problem | Leaves with a problem given `healthy` at medium | Dev ECE | Val ECE |",
              "|---|---|---|---|---|---|---|---|---|"]
    for d in disease:
        for key, label in TEMP_NAMES.items():
            r, v = d["dev"][key], d["val"][key]
            lines.append(f"| `{d['head']}` | {label} | {tvalue(d['temperatures'][key])} | {claims(r)} | "
                         f"{r['healthy']['evidence']} of {r['healthy']['n']} | {r['healthy']['blocked']} of {r['healthy']['n']} | "
                         f"{r['called_healthy']['called']} of {r['called_healthy']['n']} | {num(r['ece'])} | {num(v.get('ece'))} |")
    sps = sorted({sp for d in disease for sp in d["classes_per_species"]})
    lines += ["", "Per species: problem at medium, photos (right), and the healthy leaves with `healthy` at medium.", "",
              "| Head | Temperature | " + " | ".join(f"{sp} ({disease[0]['dev_per_species']['t_file'][sp]['n']})" for sp in sps) + " |",
              "|---|---|" + "---|" * len(sps)]
    for d in disease:
        for key, label in TEMP_NAMES.items():
            per = d["dev_per_species"][key]
            lines.append(f"| `{d['head']}` | {label} | " + " | ".join(
                claims(per[sp]) + (f"; healthy {per[sp]['healthy']['evidence']} of {per[sp]['healthy']['n']}" if per[sp]["healthy"]["n"] else "")
                for sp in sps) + " |")
    lines += ["", "**Out of fold.** Each case at the temperature fitted without its fold; the head file's value isn't "
              "refitted here, so it has no out-of-fold row.", "",
              "| Head | Temperature | Values per fold | Problem at medium: photos (right) | " + " | ".join(sps) +
              " | Healthy leaves with `healthy` at medium | Healthy leaves blocked | Dev ECE |",
              "|---|---|---|---|" + "---|" * len(sps) + "---|---|---|"]
    for d in disease:
        for key, r in d["dev_out_of_fold"].items():
            lines.append(f"| `{d['head']}` | {TEMP_NAMES[key]} | {fold_values(r['fold_temperatures'])} | {claims(r)} | "
                         + " | ".join(claims(r["per_species"][sp]) for sp in sps)
                         + f" | {r['healthy']['evidence']} of {r['healthy']['n']} | {r['healthy']['blocked']} of {r['healthy']['n']} | {num(r['ece'])} |")
    lines += ["", f"A per-species fit runs towards the lower bound ({T_BOUNDS[0]}) for a species whose dev photos are all right, "
              "as grape's are: such a value says only that those few photos were easy."]
    for d in disease:
        lines += ["", f"![{d['head']} reliability at each temperature]({out}-{d['head']}.png)"]
    (REPORTS / f"{out}.md").write_text("\n".join(lines) + "\n")


TEMP_NAMES = {"t_file": "in the head file", "t_val_and_dev": "fitted on val and dev", "t_dev": "fitted on dev",
              "t_per_species_dev": "per species, fitted on dev"}
TEMP_SHORT = {"t_file": "file", "t_val_and_dev": "val + dev", "t_dev": "dev", "t_per_species_dev": "per species"}


def count(rate, n) -> str:
    return "n/a" if rate is None or not n else f"{round(rate * n)} of {n}"


def claims(r: dict) -> str:
    m = r["medium"]
    return f"{m['n']} ({m['right']})"


def short(problem: str) -> str:
    return problem.split("_", 1)[1].replace("_", " ") if "_" in problem and problem != "healthy" else problem


def tvalue(t) -> str:
    return ", ".join(f"{k} {v}" for k, v in t.items() if k != "*") if isinstance(t, dict) else f"{t}"


def fold_values(ts: list) -> str:
    if isinstance(ts[0], dict):
        return "; ".join(f"{sp} {min(t[sp] for t in ts)}–{max(t[sp] for t in ts)}" for sp in ts[0] if sp != "*")
    return f"{min(ts)}–{max(ts)}"


def main(argv=None) -> int:
    today = dt.date.today().isoformat()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl")
    ap.add_argument("--photos", type=Path, default=EVAL / "photos")
    ap.add_argument("--heads", nargs="*", help="disease heads under ml/training/runs/heads/<version>/ (default: the "
                    "one the seed's classifier_labels.yaml names, the head the API reads classes for)")
    ap.add_argument("--out", default=f"calibration-{today}")
    args = ap.parse_args(sys.argv[1:] if argv is None else argv)
    args.date = today
    args.command = shlex.join(["python", "-m", "ml.training.eval.calibrate", *(sys.argv[1:] if argv is None else argv)])
    if not args.heads:
        head = current.classifier_labels_version()
        if head is None:
            raise SystemExit("no --heads, and the seed has no classifier_labels.yaml to name the API's head")
        args.heads = [head]
    data = args.manifest.read_bytes()
    path = args.manifest.resolve()
    args.manifest_label = {"file": str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
                           "sha256": hashlib.sha256(data).hexdigest()[:16]}

    from ml.pdm_vision.features import Embedder
    from ml.training.heads.crops import CropEmbedder

    cases = dev_cases(args.manifest)
    entries = tomllib.loads((MODELS / "manifest.toml").read_text())["model"]
    matrix_entry = current.matrix_entry(entries)
    args.matrix = matrix_entry["version"]
    matrix = read_label_matrix(MODELS / "files" / matrix_entry["file"])
    embedder = Embedder()
    species = species_calibration(cases, args.photos, matrix, embedder.images)

    crops = CropEmbedder()
    leaf_files = sorted({args.photos / f for c in cases for f, _ in leaf_photos(c)})
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
