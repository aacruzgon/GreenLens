"""The species stage on the evaluation's dev cases, per label matrix and with the species head
(M12's zero-shot check; models.md §6, a trained species head).

    python -m ml.training.eval.species_check [--head species-head-v1-2026-10-20] [--out species-check-<date>]

Every plant shot of every dev case is embedded whole by BioCLIP 2, as the worker's pass 1 does
(`ml.pdm_vision.features`, cached). For each `bioclip_labels_*` entry in the manifest:

- **Zero-shot:** softmax of 100 / T · e · tᵀ over the matrix's species rows, T the `bioclip`
  entry's `species_temperature`, as the worker computes it. T doesn't change a shot's ranking,
  but it does change a case's (its shots are averaged) and the average with the head.
- **Head:** the species head alone, over its crops, at its own temperature.
- **Average:** models.md 6's rule. The head's and zero-shot's probabilities over the head's
  crops are averaged and scaled to zero-shot's share of those crops; with a matrix whose every
  species is one of the head's crops the share is 1, so this is the plain average.

Each is scored per shot and per case, the shots combined with the worker's weights (whole plant
and flower or fruit 2, leaf close-up and underside 1; `vision::perception::heads::combine_species`).
A species the matrix can't name counts as a miss for zero-shot; the head and the average are
scored on the head's crops only. Writes a markdown report under ml/training/eval/reports/.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import json
import tomllib
from pathlib import Path

import numpy as np

from ml.pdm_vision import headfile
from ml.pdm_vision.features import Embedder
from ml.training.eval.calibrate import SHOT_WEIGHT
from ml.training.heads.train_disease import softmax
from ml.training.heads.train_species import LOGIT_SCALE, combine, dev_rows, matrix_species

ROOT = Path(__file__).resolve().parents[3]
EVAL = ROOT / "ml/training/eval"
REPORTS = EVAL / "reports"
MODELS = ROOT / "ml/models"
RUNS = ROOT / "ml/training/runs/heads"


def manifest_entries() -> list[dict]:
    return tomllib.loads((MODELS / "manifest.toml").read_text())["model"]


def species_temperature(entries: list[dict]) -> float:
    """The `bioclip` entry's species_temperature, which the worker divides the logits by."""
    bioclip = next(e for e in entries if e["name"] == "bioclip")
    return float(bioclip.get("config", {}).get("species_temperature", 1.0))


def matrices(entries: list[dict]) -> list[dict]:
    out = []
    for e in entries:
        if e["name"].startswith("bioclip_labels"):
            rows, ids = matrix_species(e)
            out.append({"version": e["version"], "source": e["source"], "rows": rows, "ids": ids})
    return out


def by_case(rows: list[dict], p: np.ndarray) -> tuple[list[str], np.ndarray]:
    """Each case's shots averaged with the worker's weights: (truth per case, probabilities)."""
    groups: dict[str, list[int]] = collections.defaultdict(list)
    for i, r in enumerate(rows):
        groups[r["case"]].append(i)
    truth, out = [], []
    for case, ix in sorted(groups.items()):
        w = np.asarray([SHOT_WEIGHT[rows[i]["kind"]] for i in ix])
        out.append((p[ix] * w[:, None]).sum(0) / w.sum())
        truth.append(rows[ix[0]]["species_id"])
    return truth, np.stack(out)


def score(truth: list[str], p: np.ndarray, ids: list[str], keep: np.ndarray) -> dict:
    """Top-1 and top-3 over the rows in `keep`, and top-1 per species."""
    t = np.asarray(truth)[keep]
    top = np.argsort(-p[keep], 1)[:, :3]
    names = np.asarray(ids)[top]
    hit1 = names[:, 0] == t
    per = collections.defaultdict(lambda: [0, 0])
    for s, h in zip(t, hit1):
        per[s][0] += 1
        per[s][1] += int(h)
    return {"n": int(keep.sum()), "top1": float(hit1.mean()), "top3": float((names == t[:, None]).any(1).mean()),
            "per_species": {s: v for s, v in sorted(per.items())}}


def main(argv=None) -> int:
    today = dt.date.today().isoformat()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--head", default="species-head-v1-2026-10-20", help="under ml/training/runs/heads/<version>/")
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl", help="the evaluation manifest")
    ap.add_argument("--out", default=f"species-check-{today}")
    args = ap.parse_args(argv)

    rows = dev_rows(args.manifest)
    if not rows:
        print("no dev cases: nothing to score")
        return 0
    head = headfile.read_linear_head(RUNS / args.head / f"{args.head}.json")
    crops = [l["species_id"] for l in head["labels"]]
    entries = manifest_entries()
    t_species = species_temperature(entries)
    X = Embedder().files([r["path"] for r in rows])
    truth = [r["species_id"] for r in rows]
    p_head = softmax((X @ head["weight"].T + head["bias"]) / head["temperature"])
    on_crop = np.asarray([t in crops for t in truth])
    case_truth, p_head_case = by_case(rows, p_head)
    case_on_crop = np.asarray([t in crops for t in case_truth])
    everything = np.ones(len(truth), bool)

    results = []
    for m in matrices(entries):
        p_zs = softmax(LOGIT_SCALE / t_species * X @ m["rows"].T)
        missing = [c for c in crops if c not in m["ids"]]
        r = {"matrix": m["version"], "source": m["source"], "species": len(m["ids"]), "missing_crops": missing}
        _, p_zs_case = by_case(rows, p_zs)
        r["shot"] = {"zero-shot, all shots": score(truth, p_zs, m["ids"], everything),
                     "zero-shot": score(truth, p_zs, m["ids"], on_crop),
                     "head": score(truth, p_head, crops, on_crop)}
        r["case"] = {"zero-shot, all cases": score(case_truth, p_zs_case, m["ids"], np.ones(len(case_truth), bool)),
                     "zero-shot": score(case_truth, p_zs_case, m["ids"], case_on_crop),
                     "head": score(case_truth, p_head_case, crops, case_on_crop)}
        if not missing:
            p_avg = combine(p_zs, m["ids"], p_head, crops)
            _, p_avg_case = by_case(rows, p_avg)
            r["shot"]["average"] = score(truth, p_avg, m["ids"], on_crop)
            r["case"]["average"] = score(case_truth, p_avg_case, m["ids"], case_on_crop)
            share = p_zs[:, [m["ids"].index(c) for c in crops]].sum(1)
            r["crop_share_min"] = float(share.min())
        results.append(r)

    kinds = collections.Counter(r["kind"] for r in rows)
    report = {"date": today, "head": args.head, "head_crops": crops, "species_temperature": t_species,
              "shots": len(rows),
              "cases": len(case_truth), "kinds": dict(kinds), "results": results}
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{args.out}.json").write_text(json.dumps(report, indent=2) + "\n")
    (REPORTS / f"{args.out}.md").write_text(markdown(report, args.out))
    print((REPORTS / f"{args.out}.md").read_text())
    return 0


def markdown(rep: dict, name: str) -> str:
    lines = [f"# Species stage on the dev cases, {rep['date']}", "",
             f"`python -m ml.training.eval.species_check --head {rep['head']} --out {name}` "
             f"({name}.json has every number). {rep['shots']} plant shots of {rep['cases']} dev cases "
             f"({', '.join(f'{k} {v}' for k, v in sorted(rep['kinds'].items()))}); the species head "
             f"`{rep['head']}` names {', '.join(rep['head_crops'])}.", "",
             f"Zero-shot is over the matrix's species rows at the manifest's `species_temperature` "
             f"{rep.get('species_temperature', 1.0):g}; \"head\" is the species head alone; "
             "\"average\" is models.md 6's rule (with every species of the matrix one of the head's crops, "
             "the plain average). Rows without \"all\" count only the head's crops, so the three compare.", ""]
    for r in rep["results"]:
        lines += [f"## Matrix `{r['matrix']}`: {r['species']} species", "", f"{r['source']}.", ""]
        if r["missing_crops"]:
            lines += [f"It has no row for {', '.join(r['missing_crops'])}, so there is no average.", ""]
        lines += ["| Scored | Method | n | Top-1 | Top-3 | Per species (right / n) |", "|---|---|---|---|---|---|"]
        for unit in ("shot", "case"):
            for method, s in r[unit].items():
                per = ", ".join(f"{k} {v[1]}/{v[0]}" for k, v in s["per_species"].items())
                lines.append(f"| per {unit} | {method} | {s['n']} | {s['top1']:.3f} | {s['top3']:.3f} | {per} |")
        lines.append("")
    return "\n".join(lines)


if __name__ == "__main__":
    raise SystemExit(main())
