"""The whole plant check on plant photos and on photos without a plant (models.md 2).

    python -m ml.training.eval.plant_check [--split val] [--label plant-check-val-2026-10-16]

`plant_present` is true when a plant shot has `leaf score ≥ τ_leaf` OR `plant probability ≥
τ_plant`. This scores both signals per photo, as the perception worker does:

- the leaf score with `leaf_scores.LeafScorer` (fp32 detector on the CPU, `manifest.cpu.toml`'s
  `score_threshold`, `min_side_px` and `tau_leaf`);
- the plant probability with each prompt set of `ml.training.heads.plant_probe` on the fp16
  BioCLIP 2 embedding the GPU worker computes (`ml.pdm_vision.features.Embedder`, cached).

Positives are the split's plant photos; "no leaf" are those whose leaf score is below τ_leaf,
which only the probe can pass. Negatives are COCO val2017 photos without a potted plant or
broccoli (`ml/training/data/splits/negatives_coco.tsv`, by group; `outdoor` photos may show
trees or grass). For every τ_plant it reports the false `no_plant_found` rate on positives, the
share of no-leaf photos rescued, and the share of negatives that pass, by the probe alone and by
the whole check.

Choosing. The rule fixed before measuring kept τ_leaf and set each set's τ_plant to the lowest
value at which the probe alone passes at most 5% of the negatives. The first run showed the leaf
score alone passes about half of the negatives at τ_leaf 0.35, so that rule can't reach models.md
2's target, and the report also searches both thresholds together: for each set, the pair with
the fewest negatives passing among those with false `no_plant_found` at most 1% (models.md 2).
`SHIPPED` is the pair in the manifest, reported with its own numbers.

    python -m ml.training.eval.plant_check --rescore reports/<label>.json   # re-analyse, no models

Writes `<out>/<label>.json` (every photo's scores) and `<out>/<label>.md`.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

from ml.training.eval.leaf_scores import LeafScorer
from ml.training.heads import plant_probe

ROOT = Path(__file__).resolve().parents[3]
DATA = ROOT / "ml/training/data"
NEGATIVES = DATA / "splits/negatives_coco.tsv"
MAX_NEGATIVE_PASS = 0.05
MAX_FALSE_NO_PLANT = 0.01
GRID = np.round(np.arange(0.01, 1.0, 0.01), 2)
# The pair in manifest.toml: the joint optimum for set A is τ_leaf 0.91, τ_plant 0.67; τ_leaf is
# taken one step lower for margin on plant photos (0.78% false no_plant_found instead of 0.95%).
SHIPPED = {"set": "A", "tau_leaf": 0.90, "tau_plant": 0.67}


def read_tsv(path: Path) -> list[dict]:
    lines = path.read_text().splitlines()
    head = lines[0].split("\t")
    return [dict(zip(head, line.split("\t"))) for line in lines[1:]]


def choose_tau(p_neg: np.ndarray, grid=GRID, max_pass: float = MAX_NEGATIVE_PASS) -> float:
    """The lowest τ_plant at which at most `max_pass` of the negatives reach it."""
    for t in grid:
        if np.mean(p_neg >= t) <= max_pass:
            return float(t)
    return float(grid[-1])


def joint(pos_leaf, pos_p, neg_leaf, neg_p, max_fn: float = MAX_FALSE_NO_PLANT) -> dict | None:
    """The (τ_leaf, τ_plant) pair with the fewest negatives passing among those failing at most
    `max_fn` of the plant photos; ties go to the higher τ_leaf, then the lower τ_plant."""
    best = None
    for tl in np.round(np.arange(0.35, 1.0, 0.01), 2):
        for tp in GRID:
            fn = float(np.mean(~((pos_leaf >= tl) | (pos_p >= tp))))
            if fn > max_fn:
                continue
            fp = float(np.mean((neg_leaf >= tl) | (neg_p >= tp)))
            key = (fp, -tl, tp)
            if best is None or key < best[0]:
                best = (key, {"tau_leaf": float(tl), "tau_plant": float(tp),
                              "positives_false_no_plant": fn, "negatives_pass_check": fp})
    return best[1] if best else None


def summary(pos_leaf, pos_p, neg_leaf, neg_p, neg_groups, tau_leaf, tau) -> dict:
    no_leaf = pos_leaf < tau_leaf
    out = {
        "tau_plant": tau,
        "positives_false_no_plant": float(np.mean(~((pos_leaf >= tau_leaf) | (pos_p >= tau)))),
        "no_leaf_rescued": float(np.mean(pos_p[no_leaf] >= tau)) if no_leaf.any() else None,
        "negatives_pass_probe": float(np.mean(neg_p >= tau)),
        "negatives_pass_check": float(np.mean((neg_leaf >= tau_leaf) | (neg_p >= tau))),
        "by_group": {},
    }
    for g in sorted(set(neg_groups)):
        m = np.asarray(neg_groups) == g
        out["by_group"][g] = {"n": int(m.sum()), "probe": float(np.mean(neg_p[m] >= tau)),
                              "check": float(np.mean((neg_leaf[m] >= tau_leaf) | (neg_p[m] >= tau)))}
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--split", default="val", choices=["train", "val"])
    ap.add_argument("--label", default=None)
    ap.add_argument("--out", type=Path, default=ROOT / "ml/training/eval/reports")
    ap.add_argument("--rescore", type=Path, help="a previous report's JSON: re-analyse its per-photo scores")
    ap.add_argument("--detector", type=Path, help="a candidate fp32 detector file instead of the manifest's")
    args = ap.parse_args(argv)
    if args.rescore:
        prev = json.loads(args.rescore.read_text())
        pos, neg = prev["positives"], prev["negatives"]
        pos_leaf = np.array([r["leaf"] for r in pos], np.float32)
        neg_leaf = np.array([r["leaf"] for r in neg], np.float32)
        probs = {n: (np.array([r[f"p_{n}"] for r in pos]), np.array([r[f"p_{n}"] for r in neg]))
                 for n in prev["sets"]}
        tau_leaf, scale, embedder = prev["tau_leaf"], prev["logit_scale"], prev["embedder"]
    else:
        pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, embedder = score(args.split, args.detector)
    report = analyse(args.split, args.label, pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, embedder)
    report["detector"] = (prev.get("detector", "manifest.cpu.toml's leaf_detector") if args.rescore
                          else str(args.detector.resolve().relative_to(ROOT)) if args.detector else "manifest.cpu.toml's leaf_detector")
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / f"{report['label']}.json").write_text(json.dumps(report, indent=1) + "\n")
    (args.out / f"{report['label']}.md").write_text(markdown(report))
    print(markdown(report))
    return 0


def score(split: str, detector: Path | None = None):
    from ml.pdm_vision.features import Embedder

    pos = read_tsv(DATA / f"splits/{split}.tsv")
    neg = read_tsv(NEGATIVES)
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
    probs = {n: (plant_probe.probability(pos_e, w), plant_probe.probability(neg_e, w)) for n, w in weights.items()}
    return pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, emb.model_sha


def analyse(split, label, pos, neg, pos_leaf, neg_leaf, probs, tau_leaf, scale, embedder) -> dict:
    groups = [r["group"] for r in neg]
    sets = {}
    for name, (pos_p, neg_p) in probs.items():
        tau = choose_tau(neg_p)
        sweep = [summary(pos_leaf, pos_p, neg_leaf, neg_p, groups, tau_leaf, float(t))
                 for t in (0.5, 0.6, 0.7, 0.8, 0.9, 0.95)]
        sets[name] = {"labels": plant_probe.PROMPT_SETS[name], "chosen": summary(
            pos_leaf, pos_p, neg_leaf, neg_p, groups, tau_leaf, tau), "sweep": sweep,
            "joint": joint(pos_leaf, pos_p, neg_leaf, neg_p), "pos_p": pos_p, "neg_p": neg_p}
    s = sets[SHIPPED["set"]]
    shipped = {**SHIPPED, **summary(pos_leaf, s["pos_p"], neg_leaf, s["neg_p"], groups,
                                    SHIPPED["tau_leaf"], SHIPPED["tau_plant"])}
    shipped["tau_leaf"] = SHIPPED["tau_leaf"]
    winner = max(sets, key=lambda n: (sets[n]["chosen"]["no_leaf_rescued"] or 0.0, n == "A"))

    no_leaf_by = defaultdict(lambda: [0, 0])
    for r, leaf, p in zip(pos, pos_leaf, sets[winner]["pos_p"]):
        if leaf < tau_leaf:
            no_leaf_by[r["problem_id"]][0] += 1
            no_leaf_by[r["problem_id"]][1] += int(p >= sets[winner]["chosen"]["tau_plant"])

    label = label or f"plant-check-{split}"
    report = {
        "label": label, "split": split, "n_positives": len(pos), "n_negatives": len(neg),
        "tau_leaf": tau_leaf, "logit_scale": round(scale, 4), "embedder": embedder,
        "rule": f"tau_plant = lowest value with the probe alone passing <= {MAX_NEGATIVE_PASS:.0%} of negatives",
        "leaf_only": {"positives_false_no_plant": float(np.mean(pos_leaf < tau_leaf)),
                      "no_leaf": int(np.sum(pos_leaf < tau_leaf)),
                      "negatives_pass": float(np.mean(neg_leaf >= tau_leaf))},
        "winner": winner,
        "shipped": shipped,
        "sets": {n: {k: v for k, v in s.items() if k not in ("pos_p", "neg_p")} for n, s in sets.items()},
        "no_leaf_by_problem": {k: {"no_leaf": v[0], "rescued": v[1]} for k, v in sorted(no_leaf_by.items())},
        "positives": [{"path": r["path"], "dataset": r["dataset"], "problem_id": r["problem_id"],
                       "leaf": round(float(l), 4), **{f"p_{n}": round(float(s["pos_p"][i]), 4) for n, s in sets.items()}}
                      for i, (r, l) in enumerate(zip(pos, pos_leaf))],
        "negatives": [{"path": r["path"], "group": r["group"], "leaf": round(float(l), 4),
                       **{f"p_{n}": round(float(s["neg_p"][i]), 4) for n, s in sets.items()}}
                      for i, (r, l) in enumerate(zip(neg, neg_leaf))],
    }
    return report


def shipped_md(r: dict) -> str:
    s = r["shipped"]
    head = (f"## Shipped: set {s['set']}, τ_leaf {s['tau_leaf']:.2f}, τ_plant {s['tau_plant']:.2f}\n\n"
            f"False `no_plant_found` on plant photos {pct(s['positives_false_no_plant'])}; photos without a "
            f"plant passing {pct(s['negatives_pass_check'])} (models.md 2's target: under 1% and at most 10%).\n\n"
            "| Group | n | pass: check |\n|---|---|---|\n")
    return head + "".join(f"| {g} | {v['n']} | {pct(v['check'])} |\n" for g, v in s["by_group"].items()) + "\n"


def pct(x) -> str:
    return "–" if x is None else f"{x * 100:.1f}%"


def markdown(r: dict) -> str:
    lo = r["leaf_only"]
    out = [f"# The plant check on the {r['split']} split ({r['label']})\n\n",
           f"{r['n_positives']} plant photos (the split) and {r['n_negatives']} photos without a plant "
           f"(COCO val2017 without potted plant or broccoli). `τ_leaf` {r['tau_leaf']}; probe = zero-shot "
           f"BioCLIP 2 at logit scale {r['logit_scale']:.0f} on the fp16 embedding ({r['embedder']}); "
           f"leaf detector: {r.get('detector', "manifest.cpu.toml's leaf_detector")}. "
           f"The manifest's pair comes from the joint search (below); the rule fixed before measuring, "
           f"{r['rule']}, is kept for the record.\n\n",
           f"**Leaf score alone:** false `no_plant_found` {pct(lo['positives_false_no_plant'])} "
           f"({lo['no_leaf']} no-leaf photos); negatives passing {pct(lo['negatives_pass'])}.\n\n",
           shipped_md(r),
           "## Joint search: both thresholds against models.md 2's target\n\n",
           "| Set | τ_leaf | τ_plant | false no_plant | negatives pass: check |\n|---|---|---|---|---|\n",
           *[f"| {n} | {j['tau_leaf']:.2f} | {j['tau_plant']:.2f} | {pct(j['positives_false_no_plant'])} | "
             f"{pct(j['negatives_pass_check'])} |\n" for n, s in r["sets"].items() if (j := s["joint"])],
           "\n## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives\n\n",
           "| Set | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |\n",
           "|---|---|---|---|---|---|\n"]
    for n, s in r["sets"].items():
        c = s["chosen"]
        mark = " (the rule's pick)" if n == r["winner"] else ""
        out.append(f"| {n}{mark} | {c['tau_plant']:.2f} | {pct(c['positives_false_no_plant'])} | "
                   f"{pct(c['no_leaf_rescued'])} | {pct(c['negatives_pass_probe'])} | {pct(c['negatives_pass_check'])} |\n")
    out.append("\nPrompt sets: " + "; ".join(
        f"{n}: plant = {', '.join(s['labels']['plant'])}; not = {', '.join(s['labels']['not'])}"
        for n, s in r["sets"].items()) + ".\n\n")
    w = r["sets"][r["winner"]]["chosen"]
    out += [f"## Negatives by group, set {r['winner']} at τ_plant {w['tau_plant']:.2f}\n\n",
            "| Group | n | pass: probe | pass: check |\n|---|---|---|---|\n"]
    out += [f"| {g} | {v['n']} | {pct(v['probe'])} | {pct(v['check'])} |\n" for g, v in w["by_group"].items()]
    out += [f"\n## No-leaf photos by problem, set {r['winner']}\n\n", "| Problem | no leaf | rescued |\n|---|---|---|\n"]
    out += [f"| {k} | {v['no_leaf']} | {v['rescued']} |\n" for k, v in r["no_leaf_by_problem"].items()]
    for n, s in r["sets"].items():
        out += [f"\n## Sweep, set {n}\n\n",
                "| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |\n|---|---|---|---|---|\n"]
        out += [f"| {x['tau_plant']:.2f} | {pct(x['positives_false_no_plant'])} | {pct(x['no_leaf_rescued'])} | "
                f"{pct(x['negatives_pass_probe'])} | {pct(x['negatives_pass_check'])} |\n" for x in s["sweep"]]
    return "".join(out)


if __name__ == "__main__":
    sys.exit(main())
