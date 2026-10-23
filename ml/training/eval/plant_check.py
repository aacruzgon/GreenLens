"""The whole plant check on plant photos and on photos without a plant (models.md 2).

    python -m ml.training.eval.plant_check [--split val] [--label plant-check-val-2026-10-16]

`plant_present` is true when a plant shot has `leaf score ≥ τ_leaf` OR `plant probability ≥
τ_plant`. This scores both signals per photo, as the perception worker does:

- the leaf score with `leaf_scores.LeafScorer` (fp32 detector on the CPU, `manifest.cpu.toml`'s
  `score_threshold`, `min_side_px` and `tau_leaf`);
- the plant probability with each prompt set of `ml.training.heads.plant_probe` on the fp16
  BioCLIP 2 embedding the GPU worker computes (`ml.pdm_vision.features.Embedder`, cached).

Positives are the split's plant photos; "no leaf" are those whose leaf score is below τ_leaf,
which only the probe can pass. Negatives are COCO photos without a potted plant or broccoli, by
group (`outdoor` photos may show trees or grass): by default COCO val2017
(`ml/training/data/splits/negatives_coco.tsv`), or with `--negatives` another list, such as the
half of it no trained probe learns from (`negatives_coco_tune.tsv`, the same list
`train_plant_probe` writes beside each probe) or the COCO train2017 hold-out
(`negatives_coco_train_holdout.tsv`, disjoint from the detector's background photos). For every τ_plant it reports the false `no_plant_found` rate on positives,
the share of no-leaf photos rescued, and the share of negatives that pass, by the probe alone
and by the whole check. False `no_plant_found` is also reported per dataset, because the pooled
rate can meet the target while the field photos (TOM2024) miss it.

Choosing. The rule fixed before measuring kept τ_leaf and set each prompt set's τ_plant to the
lowest value at which the probe alone passes at most 5% of the negatives. The first run showed
the leaf score alone passes about half of the negatives at τ_leaf 0.35, so that rule can't reach
models.md 2's target, and the report also searches both thresholds together: for each prompt
set, the pair with the fewest negatives passing among those with false `no_plant_found` at most
1% (models.md 2), once on the pooled positives and once on every dataset separately. Ties go to
the lower τ_leaf, so the leaf detector passes every plant photo it can without letting more
negatives through, then to the lower τ_plant. (Ties went to the higher τ_leaf until M17; for
probe v2 that picked 0.99, above every leaf score measured, which switched the detector off.)
`REFERENCE_PAIR`, the zero-shot pair the manifest shipped until M17, is reported with its own
numbers.

Whole plants. The split's photos are mostly leaves, so on the val split the check also scores
iNaturalist's val photos (`inat_species.tsv`: the five crops as whole plants, flowers and fruit,
and cacti, stonecrops, aroids, mints and gourds), reported as their own column and kept out of
the pooled and per-dataset rates. The joint searches hold them to the same target: they skip any
pair that fails more than 1% of them. Without that, probe v2's search chose τ_plant 0.85 to stop
one more tune photo and missed 4.2% of whole plants; with it, 0.31 and 0.99% (M17, 2026-10-23).
These photos choose the pair and there is no test split of them, so their rate is in-sample; the
dev cases (below), whose iNaturalist observations aren't in `inat_species.tsv`, are the check on
photos the search never saw. The trained probes learn from iNaturalist's train photos only.

A pair chosen on a split does a little better there than on new photos. To report a chosen pair
on photos it wasn't chosen on, score the test split against the hold-out negatives with the
pair fixed:

    python -m ml.training.eval.plant_check --split test \\
        --negatives ml/training/data/splits/negatives_coco_train_holdout.tsv --pair A:0.48:0.96

    python -m ml.training.eval.plant_check --rescore reports/<label>.json   # re-analyse, no models

On the evaluation's cases (evaluation 6, the plant check score: the highest threshold that keeps
99% recall on the dev cases with a plant), every plant shot of each case is scored and a case
passes when any of its shots does, as a scan does. For each `--pair`, the case and shot recall,
the highest τ_plant (τ_leaf fixed) that keeps 99% of cases passing, and at that τ_plant the
hold-out negatives that pass and the worst dataset's false `no_plant_found` on val, both from
committed reports' per-photo scores:

    python -m ml.training.eval.plant_check --cases ml/training/eval/manifest.jsonl \
        --probe ml/training/runs/heads/plant-probe-v2-2026-10-23/plant-probe-v2-2026-10-23.json \
        --pair A:0.40:0.70 --pair plant-probe-v2-2026-10-23:0.94:0.31 \
        --holdout-report ml/training/eval/reports/plant-check-test-holdout-2026-10-23-probe-v2.json \
        --val-report ml/training/eval/reports/plant-check-val-2026-10-23-probe-v2.json

Writes `<out>/<label>.json` (every photo's scores) and `<out>/<label>.md`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import shlex
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

from ml.training.eval.leaf_scores import LeafScorer
from ml.training.heads import plant_probe

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "ml/training/data"
NEGATIVES = DATA / "splits/negatives_coco.tsv"
INAT = DATA / "splits/inat_species.tsv"
MAX_NEGATIVE_PASS = 0.05
MAX_FALSE_NO_PLANT = 0.01
GRID = np.round(np.arange(0.01, 1.0, 0.01), 2)
# The zero-shot pair manifest.toml shipped until M17, with leaf_detector
# rfdetr-s-leaf-2026-10-19-bglic: prompt set A, chosen on the val split with at most 1% false
# no_plant_found on every dataset (reports/plant-check-val-2026-10-19-detector-bglic). It needs no
# file, so every run scores it. From M17 the manifest ships plant-probe-v2-2026-10-23 at
# 0.94 / 0.31, reported with --probe and --pair.
REFERENCE_PAIR = {"set": "A", "tau_leaf": 0.40, "tau_plant": 0.70}
REFERENCE_NAME = "zero-shot pair A 0.40 / 0.70 (shipped until M17)"
LEAF_ONLY_SWEEP = (0.35, 0.5, 0.7, 0.9)


def read_tsv(path: Path) -> list[dict]:
    lines = path.read_text().splitlines()
    head = lines[0].split("\t")
    return [dict(zip(head, line.split("\t"))) for line in lines[1:]]


def parse_pair(text: str) -> dict:
    """`A:0.48:0.96` -> prompt set A, τ_leaf 0.48, τ_plant 0.96. The name may also be the version
    of a trained probe given with --probe."""
    name, tl, tp = text.rsplit(":", 2)
    return {"set": name, "tau_leaf": float(tl), "tau_plant": float(tp)}


def choose_tau(p_neg: np.ndarray, grid=GRID, max_pass: float = MAX_NEGATIVE_PASS) -> float:
    """The lowest τ_plant at which at most `max_pass` of the negatives reach it."""
    for t in grid:
        if np.mean(p_neg >= t) <= max_pass:
            return float(t)
    return float(grid[-1])


def false_no_plant(passed: np.ndarray, datasets: np.ndarray | None) -> float:
    """The share of plant photos failing the check: pooled, or the worst dataset's."""
    if datasets is None:
        return float(np.mean(~passed))
    return max(float(np.mean(~passed[datasets == d])) for d in np.unique(datasets))


def whole_false(whole: tuple | None, tau_leaf: float, tau_plant: float) -> float | None:
    """The share of whole-plant photos failing the check; `whole` is (leaf scores, probabilities)."""
    if whole is None:
        return None
    leaf, p = whole
    return float(np.mean(~((leaf >= tau_leaf) | (p >= tau_plant))))


def joint(pos_leaf, pos_p, neg_leaf, neg_p, max_fn: float = MAX_FALSE_NO_PLANT,
          datasets: np.ndarray | None = None, whole: tuple | None = None,
          whole_max: float | None = None) -> dict | None:
    """The (τ_leaf, τ_plant) pair with the fewest negatives passing among those failing at most
    `max_fn` of the plant photos (of each dataset, when `datasets` is given) and, when `whole` is
    given, at most `whole_max` of the whole-plant photos; ties go to the lower τ_leaf, then the
    lower τ_plant."""
    best = None
    for tl in np.round(np.arange(0.35, 1.0, 0.01), 2):
        for tp in GRID:
            fn = false_no_plant((pos_leaf >= tl) | (pos_p >= tp), datasets)
            if fn > max_fn:
                continue
            wf = whole_false(whole, tl, tp)
            if wf is not None and whole_max is not None and wf > whole_max:
                continue
            fp = float(np.mean((neg_leaf >= tl) | (neg_p >= tp)))
            key = (fp, tl, tp)
            if best is None or key < best[0]:
                best = (key, {"tau_leaf": float(tl), "tau_plant": float(tp),
                              "positives_false_no_plant": float(np.mean(~((pos_leaf >= tl) | (pos_p >= tp)))),
                              "whole_plants_false_no_plant": wf, "negatives_pass_check": fp})
    return best[1] if best else None


def by_dataset(passed: np.ndarray, datasets: np.ndarray) -> dict:
    return {d: {"n": int((datasets == d).sum()), "false_no_plant": float(np.mean(~passed[datasets == d]))}
            for d in sorted(np.unique(datasets))}


def summary(pos_leaf, pos_p, neg_leaf, neg_p, neg_groups, tau_leaf, tau, datasets=None, whole=None) -> dict:
    no_leaf = pos_leaf < tau_leaf
    passed = (pos_leaf >= tau_leaf) | (pos_p >= tau)
    out = {
        "tau_plant": tau,
        "positives_false_no_plant": float(np.mean(~passed)),
        "whole_plants_false_no_plant": whole_false(whole, tau_leaf, tau),
        "no_leaf_rescued": float(np.mean(pos_p[no_leaf] >= tau)) if no_leaf.any() else None,
        "negatives_pass_probe": float(np.mean(neg_p >= tau)),
        "negatives_pass_check": float(np.mean((neg_leaf >= tau_leaf) | (neg_p >= tau))),
        "by_group": {},
    }
    if datasets is not None:
        out["by_dataset"] = by_dataset(passed, datasets)
    for g in sorted(set(neg_groups)):
        m = np.asarray(neg_groups) == g
        out["by_group"][g] = {"n": int(m.sum()), "probe": float(np.mean(neg_p[m] >= tau)),
                              "check": float(np.mean((neg_leaf[m] >= tau_leaf) | (neg_p[m] >= tau)))}
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--split", default="val", choices=["train", "val", "test"])
    ap.add_argument("--label", default=None)
    ap.add_argument("--out", type=Path, default=ROOT / "ml/training/eval/reports")
    ap.add_argument("--rescore", type=Path, help="a previous report's JSON: re-analyse its per-photo scores")
    ap.add_argument("--detector", type=Path, help="a candidate fp32 detector file instead of the manifest's")
    ap.add_argument("--negatives", type=Path, default=None,
                    help="the no-plant photos (a negatives TSV); default negatives_coco.tsv, or with --probe the "
                         "negatives_tune.tsv beside the probe (the half of negatives_coco.tsv it didn't train on)")
    ap.add_argument("--pair", type=parse_pair, action="append", default=[],
                    help="a fixed pair to report, PROMPTS:TAU_LEAF:TAU_PLANT (repeatable)")
    ap.add_argument("--note", help="a line for the report's header, such as what the detector was trained on")
    ap.add_argument("--probe", type=Path, action="append", default=[],
                    help="a trained probe file (pdm-linear-head/1, kind plant_probe) scored beside the prompt sets; repeatable")
    ap.add_argument("--cases", type=Path, help="the evaluation manifest: score its cases instead of a split")
    ap.add_argument("--case-split", default="dev", help="with --cases: the cases' split")
    ap.add_argument("--photos", type=Path, help="with --cases: the cases' photos (default: photos/ beside the manifest)")
    ap.add_argument("--holdout-report", type=Path, help="with --cases: a report whose negatives are the hold-out photos")
    ap.add_argument("--val-report", type=Path, help="with --cases: a val report, for the per-dataset rule")
    argv = sys.argv[1:] if argv is None else list(argv)
    args = ap.parse_args(argv)
    args.made_with = made_with(argv)
    if args.cases:
        return cases_main(args)
    prev = json.loads(args.rescore.read_text()) if args.rescore else None
    known = set(prev["sets"]) if prev else set(plant_probe.PROMPT_SETS) | {headfile_version(f) for f in args.probe}
    for pair in args.pair:
        if pair["set"] not in known:
            ap.error(f"--pair: unknown prompt set or probe {pair['set']!r} (known: {', '.join(sorted(known))})")
    if not prev:
        try:
            args.negatives = negatives_for(args.probe, args.negatives)
        except ValueError as e:
            ap.error(str(e))
    if prev:
        pos, neg = prev["positives"], prev["negatives"]
        pos_leaf = np.array([r["leaf"] for r in pos], np.float32)
        neg_leaf = np.array([r["leaf"] for r in neg], np.float32)
        probs = {n: (np.array([r[f"p_{n}"] for r in pos]), np.array([r[f"p_{n}"] for r in neg]))
                 for n in prev["sets"]}
        whole = None
        if prev.get("whole_plants"):   # reports before M17 have none
            w = prev["whole_plants"]["photos"]
            whole = {"rows": [{k: r[k] for k in ("path", "slug")} for r in w],
                     "leaf": np.array([r["leaf"] for r in w], np.float32),
                     "p": {n: np.array([r[f"p_{n}"] for r in w]) for n in prev["sets"]}}
        tau_leaf, scale, embedder = prev["tau_leaf"], prev["logit_scale"], prev["embedder"]
        split = prev["split"]
        negatives = prev.get("negatives_source", NEGATIVES.name)
        detector = prev.get("detector", "manifest.cpu.toml's leaf_detector")
        note = args.note or prev.get("note")
    else:
        pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, embedder, whole = score(
            args.split, args.detector, args.negatives, args.probe)
        split, negatives = args.split, args.negatives.name
        detector = str(args.detector.resolve().relative_to(ROOT)) if args.detector else "manifest.cpu.toml's leaf_detector"
        note = args.note
    report = analyse(split, args.label, pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, embedder, args.pair, whole)
    report["detector"] = detector
    report["negatives_source"] = negatives
    report.update(args.made_with)
    if prev:
        report["rescored"] = {k: prev[k] for k in ("command", "date", "inputs") if k in prev}
    else:
        report["inputs"] = {"split": file_id(DATA / f"splits/{split}.tsv"), "negatives": file_id(args.negatives),
                            "probes": [file_id(f) for f in args.probe],
                            **({"whole_plants": file_id(INAT)} if whole else {})}
    if note:
        report["note"] = note
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / f"{report['label']}.json").write_text(json.dumps(report, indent=1) + "\n")
    (args.out / f"{report['label']}.md").write_text(markdown(report))
    print(markdown(report))
    return 0


def file_id(path: Path) -> dict:
    """A report's record of one input file: its path from the repository and its SHA-256 (16 hex)."""
    path = Path(path).resolve()
    return {"file": str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()[:16]}


def made_with(argv: list[str]) -> dict:
    """How a report was made: the command line and the day it ran (evaluation reports' convention,
    review of #156)."""
    return {"command": shlex.join(["python", "-m", "ml.training.eval.plant_check", *argv]),
            "date": dt.date.today().isoformat()}


def headfile_version(path: Path) -> str:
    return json.loads(path.read_text())["version"]


def negatives_for(probes: list[Path], negatives: Path | None) -> Path:
    """The negatives to score: `negatives` or the default, refusing any photo a trained probe
    trained on. `train_plant_probe` trains on half of negatives_coco.tsv and writes the other
    half beside the probe as negatives_tune.tsv; a probe scored on its own training photos would
    get an optimistic τ_plant (review of #85)."""
    tunes = [f.parent / "negatives_tune.tsv" for f in probes]
    for f, t in zip(probes, tunes):
        if not t.exists():
            raise ValueError(f"--probe {f}: no negatives_tune.tsv beside it, so the photos it trained on are unknown")
    if negatives is None:
        if len({t.resolve() for t in tunes}) > 1:
            raise ValueError("--probe: probes with different tune halves; give --negatives")
        negatives = tunes[0] if tunes else NEGATIVES
    given = {r["path"] for r in read_tsv(negatives)}
    for f, t in zip(probes, tunes):
        trained = {r["path"] for r in read_tsv(NEGATIVES)} - {r["path"] for r in read_tsv(t)}
        if overlap := given & trained:
            raise ValueError(f"--negatives {negatives.name}: {len(overlap)} photos are ones {f.name} trained on")
    return negatives


def score(split: str, detector: Path | None = None, negatives: Path = NEGATIVES, probes: list[Path] = ()):
    from ml.pdm_vision import headfile
    from ml.pdm_vision.features import Embedder

    pos = read_tsv(DATA / f"splits/{split}.tsv")
    neg = read_tsv(negatives)
    scorer = LeafScorer(model_file=detector)
    tau_leaf = float(scorer.cfg["tau_leaf"])
    print(f"leaf scores: {len(pos)} positives, {len(neg)} negatives (CPU)", file=sys.stderr)
    pos_leaf = np.array([scorer.score(DATA / "raw" / r["path"]) for r in pos], np.float32)
    neg_leaf = np.array([scorer.score(DATA / "raw" / r["path"]) for r in neg], np.float32)
    print("embeddings (fp16 BioCLIP, cached)", file=sys.stderr)
    emb = Embedder()
    pos_e = emb.files([DATA / "raw" / r["path"] for r in pos])
    neg_e = emb.files([DATA / "raw" / r["path"] for r in neg])
    weights, scale = plant_probe.weights("cpu")
    heads = {n: (w, 0.0) for n, w in weights.items()}
    for f in probes:   # a trained probe: sigma(w . e + b), the worker's PlantProbe
        doc = headfile.read_linear_head(f)
        if doc["kind"] != "plant_probe":
            raise SystemExit(f"{f}: kind {doc['kind']}, not plant_probe")
        heads[doc["version"]] = (doc["weight"][0], float(doc["bias"][0]))
    probs = {n: (plant_probe.probability(pos_e, w, b), plant_probe.probability(neg_e, w, b)) for n, (w, b) in heads.items()}
    whole = None
    if split == "val":   # the trained probes learn from iNaturalist's train photos, never its val ones
        rows = [r for r in read_tsv(INAT) if r["split"] == "val"]
        print(f"whole plants: {len(rows)} iNaturalist val photos", file=sys.stderr)
        files = [DATA / "raw" / r["path"] for r in rows]
        whole_e = emb.files(files)
        whole = {"rows": [{"path": r["path"], "slug": r["slug"]} for r in rows],
                 "leaf": np.array([scorer.score(f) for f in files], np.float32),
                 "p": {n: plant_probe.probability(whole_e, w, b) for n, (w, b) in heads.items()}}
    return pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, emb.model_sha, whole


def analyse(split, label, pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, embedder, pairs=(), whole=None) -> dict:
    """`whole`: the whole-plant photos, {"rows", "leaf", "p": {set: probabilities}}, or None."""
    groups = [r["group"] for r in neg]
    datasets = np.array([r["dataset"] for r in pos])
    w_of = (lambda name: (whole["leaf"], whole["p"][name])) if whole else (lambda name: None)
    whole_max = MAX_FALSE_NO_PLANT if whole else None
    sets = {}
    for name, (pos_p, neg_p) in probs.items():
        tau = choose_tau(neg_p)
        sweep = [summary(pos_leaf, pos_p, neg_leaf, neg_p, groups, tau_leaf, float(t), whole=w_of(name))
                 for t in (0.5, 0.6, 0.7, 0.8, 0.9, 0.95)]
        sets[name] = {"labels": plant_probe.PROMPT_SETS.get(name, f"trained probe {name}"), "chosen": summary(
            pos_leaf, pos_p, neg_leaf, neg_p, groups, tau_leaf, tau, whole=w_of(name)), "sweep": sweep,
            "joint": joint(pos_leaf, pos_p, neg_leaf, neg_p, whole=w_of(name), whole_max=whole_max),
            "joint_every_dataset": joint(pos_leaf, pos_p, neg_leaf, neg_p, datasets=datasets,
                                         whole=w_of(name), whole_max=whole_max),
            "pos_p": pos_p, "neg_p": neg_p}
    for j in ("joint", "joint_every_dataset"):
        for s in sets.values():
            if s[j]:
                passed = (pos_leaf >= s[j]["tau_leaf"]) | (s["pos_p"] >= s[j]["tau_plant"])
                s[j]["by_dataset"] = by_dataset(passed, datasets)

    def fixed(pair: dict, name: str) -> dict:
        s = sets[pair["set"]]
        return {"name": name, **pair, **summary(pos_leaf, s["pos_p"], neg_leaf, s["neg_p"], groups,
                                                pair["tau_leaf"], pair["tau_plant"], datasets, w_of(pair["set"]))}

    fixed_pairs = [fixed(REFERENCE_PAIR, REFERENCE_NAME)]
    fixed_pairs += [fixed(p, f"{p['set']} {p['tau_leaf']:.2f} / {p['tau_plant']:.2f}") for p in pairs]
    winner = max(sets, key=lambda n: (sets[n]["chosen"]["no_leaf_rescued"] or 0.0, n == "A"))

    no_leaf_by = defaultdict(lambda: [0, 0])
    for r, leaf, p in zip(pos, pos_leaf, sets[winner]["pos_p"]):
        if leaf < tau_leaf:
            no_leaf_by[r["problem_id"]][0] += 1
            no_leaf_by[r["problem_id"]][1] += int(p >= sets[winner]["chosen"]["tau_plant"])

    leaf_only = []
    for tl in sorted({*LEAF_ONLY_SWEEP, tau_leaf}):
        passed = pos_leaf >= tl
        leaf_only.append({"tau_leaf": tl, "positives_false_no_plant": float(np.mean(~passed)),
                          "negatives_pass": float(np.mean(neg_leaf >= tl)), "by_dataset": by_dataset(passed, datasets)})

    label = label or f"plant-check-{split}"
    report = {
        "label": label, "split": split, "n_positives": len(pos), "n_negatives": len(neg),
        "tau_leaf": tau_leaf, "logit_scale": round(scale, 4), "embedder": embedder,
        "rule": f"tau_plant = lowest value with the probe alone passing <= {MAX_NEGATIVE_PASS:.0%} of negatives",
        "leaf_only": {"positives_false_no_plant": float(np.mean(pos_leaf < tau_leaf)),
                      "no_leaf": int(np.sum(pos_leaf < tau_leaf)),
                      "negatives_pass": float(np.mean(neg_leaf >= tau_leaf))},
        "leaf_only_sweep": leaf_only,
        "winner": winner,
        "fixed_pairs": fixed_pairs,
        "sets": {n: {k: v for k, v in s.items() if k not in ("pos_p", "neg_p")} for n, s in sets.items()},
        "no_leaf_by_problem": {k: {"no_leaf": v[0], "rescued": v[1]} for k, v in sorted(no_leaf_by.items())},
        "positives": [{"path": r["path"], "dataset": r["dataset"], "problem_id": r["problem_id"],
                       "leaf": round(float(l), 4), **{f"p_{n}": round(float(s["pos_p"][i]), 4) for n, s in sets.items()}}
                      for i, (r, l) in enumerate(zip(pos, pos_leaf))],
        "negatives": [{"path": r["path"], "group": r["group"], "leaf": round(float(l), 4),
                       **{f"p_{n}": round(float(s["neg_p"][i]), 4) for n, s in sets.items()}}
                      for i, (r, l) in enumerate(zip(neg, neg_leaf))],
    }
    if whole:
        report["whole_plants"] = {
            "source": f"{INAT.name}, split val", "n": len(whole["rows"]), "cap": whole_max,
            "cap_rule": f"the joint searches skip pairs failing more than {MAX_FALSE_NO_PLANT:.0%} of the whole "
                        "plants, models.md 2's target",
            "in_sample": ("these photos choose the cap and the pair, and inat_species.tsv has no test split, so "
                          "their rate is in-sample; the dev cases, whose iNaturalist observations aren't in "
                          "inat_species.tsv, are the check on photos the search never saw"),
            "photos": [{**r, "leaf": round(float(whole["leaf"][i]), 4),
                        **{f"p_{n}": round(float(whole["p"][n][i]), 4) for n in sets}}
                       for i, r in enumerate(whole["rows"])]}
    return report


def pct(x, digits: int = 1) -> str:
    return "–" if x is None else f"{x * 100:.{digits}f}%"


def dataset_cells(by: dict, names: list[str]) -> str:
    return "".join(f" {pct(by[d]['false_no_plant'], 2)} |" if d in by else " – |" for d in names)


def whole_head(r: dict) -> tuple[str, str]:
    """The whole-plants column's header and rule cells, empty for a report without whole plants."""
    return (" whole plants |", "---|") if r.get("whole_plants") else ("", "")


def whole_cell(r: dict, x: dict) -> str:
    return f" {pct(x.get('whole_plants_false_no_plant'), 2)} |" if r.get("whole_plants") else ""


def fixed_pairs_md(r: dict, names: list[str]) -> str:
    wh, wr = whole_head(r)
    out = ["## Fixed pairs\n\n",
           "False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant "
           "passing (models.md 2's target: under 1% and at most 10%). The first row is the zero-shot pair the "
           "manifest shipped until M17, chosen on the val split with the current detector; on another detector "
           "or split it is only a reference.\n\n",
           "| Pair | Prompts | τ_leaf | τ_plant | false no_plant |" + "".join(f" {d} |" for d in names)
           + wh + " negatives pass |\n", "|---|---|---|---|---|" + "---|" * len(names) + wr + "---|\n"]
    for p in r["fixed_pairs"]:
        out.append(f"| {p['name']} | {p['set']} | {p['tau_leaf']:.2f} | {p['tau_plant']:.2f} | "
                   f"{pct(p['positives_false_no_plant'], 2)} |{dataset_cells(p['by_dataset'], names)}{whole_cell(r, p)} "
                   f"{pct(p['negatives_pass_check'])} |\n")
    groups = sorted({g for p in r["fixed_pairs"] for g in p["by_group"]})
    out += ["\nPhotos without a plant passing, by group:\n\n",
            "| Pair |" + "".join(f" {g} (n {r['fixed_pairs'][0]['by_group'].get(g, {}).get('n', 0)}) |" for g in groups) + "\n",
            "|---|" + "---|" * len(groups) + "\n"]
    for p in r["fixed_pairs"]:
        out.append(f"| {p['name']} |" + "".join(f" {pct(p['by_group'][g]['check'])} |" if g in p["by_group"] else " – |" for g in groups) + "\n")
    return "".join(out) + "\n"


def markdown(r: dict) -> str:
    lo = r["leaf_only"]
    names = sorted({p["dataset"] for p in r["positives"]})
    negatives = r.get("negatives_source", NEGATIVES.name)
    out = [f"# The plant check on the {r['split']} split ({r['label']})\n\n",
           *([f"> {r['note']}\n\n"] if r.get("note") else []),
           *([f"Made with `{r['command']}` on {r['date']}" + (
               ", inputs " + "; ".join(f"`{i['file']}` (SHA-256 `{i['sha256']}`)" for i in
                                       [r["inputs"]["split"], r["inputs"]["negatives"], *r["inputs"]["probes"],
                                        *([r["inputs"]["whole_plants"]] if "whole_plants" in r["inputs"] else [])])
               if "inputs" in r else f", from the scores of a report made with `{r.get('rescored', {}).get('command', 'an earlier run')}`")
              + ".\n\n"] if r.get("command") else []),
           f"{r['n_positives']} plant photos (the split) and {r['n_negatives']} photos without a plant "
           f"(`{negatives}`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit "
           f"scale {r['logit_scale']:.0f} on the fp16 embedding ({r['embedder']}); leaf detector: "
           f"{r.get('detector', "manifest.cpu.toml's leaf_detector")}. The manifest's `τ_leaf` ({r['tau_leaf']}) "
           f"is used by the leaf-score-alone line and by the rule fixed before measuring, "
           f"{r['rule']}, kept for the record. The pairs to choose from are the joint searches (below). "
           f"\"Prompts\" names the probe's prompt set (A or B, listed below).",
           *([f" Whole plants: the {r['whole_plants']['n']} photos of {r['whole_plants']['source']}, a column of their "
              "own, outside the pooled and per-dataset rates."] if r.get("whole_plants") else []), "\n\n",
           f"**Leaf score alone at the manifest's `τ_leaf` {r['tau_leaf']}:** false `no_plant_found` "
           f"{pct(lo['positives_false_no_plant'])} ({lo['no_leaf']} no-leaf photos); negatives passing "
           f"{pct(lo['negatives_pass'])}.\n\n",
           "## Leaf score alone\n\n",
           "| τ_leaf | false no_plant |" + "".join(f" {d} |" for d in names) + " negatives pass |\n",
           "|---|---|" + "---|" * len(names) + "---|\n",
           *[f"| {x['tau_leaf']:.2f} | {pct(x['positives_false_no_plant'], 2)} |{dataset_cells(x['by_dataset'], names)} "
             f"{pct(x['negatives_pass'])} |\n" for x in r["leaf_only_sweep"]],
           "\n", fixed_pairs_md(r, names),
           "## Joint search: both thresholds against models.md 2's target\n\n",
           "Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on "
           "each dataset, so the field photos can't hide behind the web photos.",
           *([f" Both also skip any pair failing more than {pct(r['whole_plants']['cap'], 0)} of the "
              f"{r['whole_plants']['n']} whole-plant photos ({r['whole_plants']['source']}). Those photos "
              "choose the cap and the pair and have no test split, so their rate is in-sample; the dev cases "
              "are the check on photos the search never saw."] if r.get("whole_plants") else []), "\n\n",
           "| Constraint | Prompts | τ_leaf | τ_plant | false no_plant |" + "".join(f" {d} |" for d in names)
           + whole_head(r)[0] + " negatives pass: check |\n",
           "|---|---|---|---|---|" + "---|" * len(names) + whole_head(r)[1] + "---|\n"]
    for key, what in (("joint", "pooled"), ("joint_every_dataset", "every dataset")):
        for n, s in r["sets"].items():
            j = s.get(key)
            if j is None:
                if key in s:
                    out.append(f"| {what} | {n} | – | – | no pair meets it |" + " – |" * len(names)
                               + (" – |" if r.get("whole_plants") else "") + " – |\n")
                continue
            out.append(f"| {what} | {n} | {j['tau_leaf']:.2f} | {j['tau_plant']:.2f} | "
                       f"{pct(j['positives_false_no_plant'], 2)} |{dataset_cells(j.get('by_dataset', {}), names)}"
                       f"{whole_cell(r, j)} {pct(j['negatives_pass_check'])} |\n")
    out += ["\n## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives\n\n",
            "| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |\n",
            "|---|---|---|---|---|---|\n"]
    for n, s in r["sets"].items():
        c = s["chosen"]
        mark = " (the rule's pick)" if n == r["winner"] else ""
        out.append(f"| {n}{mark} | {c['tau_plant']:.2f} | {pct(c['positives_false_no_plant'])} | "
                   f"{pct(c['no_leaf_rescued'])} | {pct(c['negatives_pass_probe'])} | {pct(c['negatives_pass_check'])} |\n")
    out.append("\nPrompt sets: " + "; ".join(
        f"{n}: plant = {', '.join(s['labels']['plant'])}; not = {', '.join(s['labels']['not'])}"
        if isinstance(s["labels"], dict) else f"{n}: {s['labels']}"   # a trained probe has no prompts
        for n, s in r["sets"].items()) + ".\n\n")
    w = r["sets"][r["winner"]]["chosen"]
    out += [f"## Negatives by group, prompts {r['winner']} at τ_plant {w['tau_plant']:.2f}\n\n",
            "| Group | n | pass: probe | pass: check |\n|---|---|---|---|\n"]
    out += [f"| {g} | {v['n']} | {pct(v['probe'])} | {pct(v['check'])} |\n" for g, v in w["by_group"].items()]
    out += [f"\n## No-leaf photos by problem, prompts {r['winner']}\n\n", "| Problem | no leaf | rescued |\n|---|---|---|\n"]
    out += [f"| {k} | {v['no_leaf']} | {v['rescued']} |\n" for k, v in r["no_leaf_by_problem"].items()]
    for n, s in r["sets"].items():
        out += [f"\n## Sweep, prompts {n}\n\n",
                "| τ_plant | false no_plant | no-leaf rescued |" + whole_head(r)[0] + " negatives: probe | negatives: check |\n"
                "|---|---|---|" + whole_head(r)[1] + "---|---|\n"]
        out += [f"| {x['tau_plant']:.2f} | {pct(x['positives_false_no_plant'])} | {pct(x['no_leaf_rescued'])} |"
                f"{whole_cell(r, x)} {pct(x['negatives_pass_probe'])} | {pct(x['negatives_pass_check'])} |\n" for x in s["sweep"]]
    return "".join(out)


# --- the evaluation's cases (evaluation 6) --------------------------------------------------------

PLANT_KINDS = ("whole_plant", "leaf_closeup", "leaf_underside", "flower_fruit")
CASE_RECALL = 0.99


def case_pass(shot_leaf: np.ndarray, shot_p: np.ndarray, case_of_shot: np.ndarray, n_cases: int,
              tau_leaf: float, tau_plant: float) -> np.ndarray:
    """A case passes when any of its plant shots passes (`plant_present` over a scan's shots)."""
    shot = (shot_leaf >= tau_leaf) | (shot_p >= tau_plant)
    out = np.zeros(n_cases, bool)
    np.logical_or.at(out, case_of_shot, shot)
    return out


def highest_tau_plant(shot_leaf, shot_p, case_of_shot, n_cases, tau_leaf, recall: float = CASE_RECALL) -> float | None:
    """The highest τ_plant on GRID at which at least `recall` of the cases pass."""
    ok = [float(t) for t in GRID
          if case_pass(shot_leaf, shot_p, case_of_shot, n_cases, tau_leaf, float(t)).mean() >= recall]
    return max(ok) if ok else None


def report_scores(path: Path, side: str, name: str) -> tuple[np.ndarray, np.ndarray, list[dict]]:
    doc = json.loads(path.read_text())
    rows = doc[side]
    if f"p_{name}" not in rows[0]:
        raise SystemExit(f"{path}: no p_{name} scores (it has {', '.join(doc['sets'])})")
    return (np.array([r["leaf"] for r in rows], np.float32), np.array([r[f"p_{name}"] for r in rows]), rows)


def cases_main(args) -> int:
    import datetime as dt

    from ml.pdm_vision import headfile
    from ml.pdm_vision.features import Embedder

    cases = [c for c in map(json.loads, filter(str.strip, args.cases.read_text().splitlines()))
             if c["split"] == args.case_split]
    photos = args.photos or args.cases.parent / "photos"
    shots = [(n, photos / im["file"], im["kind"]) for n, c in enumerate(cases)
             for im in c["images"] if im["kind"] in PLANT_KINDS]
    case_of_shot = np.array([n for n, _, _ in shots])
    scorer = LeafScorer()
    print(f"leaf scores: {len(shots)} shots of {len(cases)} cases (CPU)", file=sys.stderr)
    leaf = np.array([scorer.score(f) for _, f, _ in shots], np.float32)
    emb = Embedder().files([f for _, f, _ in shots])
    weights, _ = plant_probe.weights("cpu")
    probs = {n: plant_probe.probability(emb, w) for n, w in weights.items()}
    for f in args.probe:
        doc = headfile.read_linear_head(f)
        probs[doc["version"]] = plant_probe.probability(emb, doc["weight"][0], float(doc["bias"][0]))
    pairs = args.pair or [REFERENCE_PAIR]
    out = {"label": args.label or f"plant-check-{args.case_split}-cases-{dt.date.today().isoformat()}",
           **args.made_with, "cases_file": file_id(args.cases), "probes": [file_id(f) for f in args.probe],
           "cases": len(cases), "shots": len(shots), "kinds": {},
           "holdout_report": str(args.holdout_report.resolve().relative_to(ROOT)) if args.holdout_report else None,
           "val_report": str(args.val_report.resolve().relative_to(ROOT)) if args.val_report else None, "pairs": []}
    for _, _, k in shots:
        out["kinds"][k] = out["kinds"].get(k, 0) + 1
    for pair in pairs:
        name, tl, tp = pair["set"], pair["tau_leaf"], pair["tau_plant"]
        if name not in probs:
            raise SystemExit(f"--pair {name}: unknown prompt set or probe (known: {', '.join(probs)})")
        p = probs[name]
        passed = case_pass(leaf, p, case_of_shot, len(cases), tl, tp)
        shot_ok = (leaf >= tl) | (p >= tp)
        best = highest_tau_plant(leaf, p, case_of_shot, len(cases), tl)
        r = {"set": name, "tau_leaf": tl, "tau_plant": tp, "case_recall": float(passed.mean()),
             "shot_recall": float(shot_ok.mean()), "highest_tau_plant_at_99": best,
             "failing_cases": [{"case": cases[n]["case_id"], "species": cases[n]["truth"].get("species_id"),
                                "best_leaf": float(leaf[case_of_shot == n].max()),
                                "best_p": float(p[case_of_shot == n].max())}
                               for n in np.flatnonzero(~passed)]}
        for t, key in ((tp, "at_pair"), (best, "at_highest")):
            if t is None:
                continue
            row = {"tau_plant": t, "case_recall": float(case_pass(leaf, p, case_of_shot, len(cases), tl, t).mean())}
            if args.holdout_report:
                nl, npp, _ = report_scores(args.holdout_report, "negatives", name)
                row["holdout_negatives_pass"] = float(np.mean((nl >= tl) | (npp >= t)))
            if args.val_report:
                pl, ppp, rows = report_scores(args.val_report, "positives", name)
                ds = np.array([x["dataset"] for x in rows])
                row["val_worst_dataset_false_no_plant"] = false_no_plant((pl >= tl) | (ppp >= t), ds)
                w = json.loads(args.val_report.read_text()).get("whole_plants")
                if w:
                    row["val_whole_plants_false_no_plant"] = whole_false(
                        (np.array([x["leaf"] for x in w["photos"]]), np.array([x[f"p_{name}"] for x in w["photos"]])), tl, t)
            r[key] = row
        out["pairs"].append(r)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / f"{out['label']}.json").write_text(json.dumps(out, indent=1) + "\n")
    (args.out / f"{out['label']}.md").write_text(cases_markdown(out))
    print(cases_markdown(out))
    return 0


def cases_markdown(r: dict) -> str:
    def pc(x):
        return "–" if x is None else f"{100 * x:.1f}%"
    lines = [f"# Plant check on the evaluation cases: {r['label']}", "",
             f"{r['cases']} cases, {r['shots']} plant shots ({', '.join(f'{k} {v}' for k, v in sorted(r['kinds'].items()))}). "
             "Every case has a plant. A case passes when any shot passes (leaf score ≥ τ_leaf or plant probability ≥ τ_plant). "
             f"Hold-out negatives: `{r['holdout_report']}`; val per dataset: `{r['val_report']}`.", "",
             *([f"Made with `{r['command']}` on {r['date']}, over `{r['cases_file']['file']}` (SHA-256 "
                f"`{r['cases_file']['sha256']}`).", ""] if r.get("command") else []),
             "| Pair | τ_plant | Cases passing | Shots passing | Hold-out no-plant photos passing | Val, worst dataset's false `no_plant_found` | Val, whole plants failing |",
             "|---|---|---|---|---|---|---|"]
    for p in r["pairs"]:
        a = p["at_pair"]
        lines.append(f"| {p['set']} {p['tau_leaf']} / {p['tau_plant']} | {p['tau_plant']} | {pc(a['case_recall'])} | "
                     f"{pc(p['shot_recall'])} | {pc(a.get('holdout_negatives_pass'))} | {pc(a.get('val_worst_dataset_false_no_plant'))} | "
                     f"{pc(a.get('val_whole_plants_false_no_plant'))} |")
        if "at_highest" in p:
            h = p["at_highest"]
            lines.append(f"| {p['set']}, highest τ_plant keeping 99% of cases | {h['tau_plant']} | {pc(h['case_recall'])} | | "
                         f"{pc(h.get('holdout_negatives_pass'))} | {pc(h.get('val_worst_dataset_false_no_plant'))} | "
                         f"{pc(h.get('val_whole_plants_false_no_plant'))} |")
    lines.append("")
    for p in r["pairs"]:
        f = p["failing_cases"]
        lines.append(f"Failing at {p['set']} {p['tau_leaf']} / {p['tau_plant']}: " + (
            "; ".join(f"`{x['case']}` ({x['species']}, best leaf {x['best_leaf']:.2f}, best p {x['best_p']:.2f})" for x in f) or "none") + ".")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    sys.exit(main())
