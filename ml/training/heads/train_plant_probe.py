"""The trained plant probe (models.md 2; M13): logistic regression on the BioCLIP 2 embedding of a
whole shot, shipped in the zero-shot probe's format (`pdm-linear-head/1`, kind `plant_probe`), so
the worker's `PlantProbe` reads it unchanged: p(plant) = σ(w · e + b).

    python -m ml.training.heads.train_plant_probe --version plant-probe-v1-2026-10-20

- Plant photos: every photo of the train split (all species, mostly leaves) and every
  iNaturalist training photo (`data/splits/inat_species.tsv`: the five crops as whole plants,
  flowers and fruit, and cacti, stonecrops, aroids, mints and gourds), the two sources weighing
  the same.
- Photos without a plant: half of COCO val2017's (`negatives_coco.tsv`), picked by a hash of the
  path. The other half is written as `negatives_tune.tsv` beside the probe, for
  `ml.training.eval.plant_check --negatives`, so the pair (τ_leaf, τ_plant) is chosen on photos the
  probe never saw; the COCO train2017 hold-out (`negatives_coco_train_holdout.tsv`) stays the
  final check.
- Plant photos and photos without one weigh the same in total. C is chosen on the val split's
  plant photos and iNaturalist's val photos against the tune half: the fewest negatives passing
  at the τ_plant where the probe alone fails at most 1% of plant photos.

Writes the probe and negatives_tune.tsv under runs/heads/<version>/, and the report under
heads/reports/.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
from sklearn.linear_model import LogisticRegression

from ml.pdm_vision import headfile
from ml.pdm_vision.features import Embedder
from ml.training.heads.train_disease import BIOCLIP, REPORTS, RUNS, read_split

ROOT = Path(__file__).resolve().parents[3]
RAW = ROOT / "ml/training/data/raw"
SPLITS = ROOT / "ml/training/data/splits"
MAX_FALSE_NO_PLANT = 0.01


def read_tsv(path: Path) -> list[dict]:
    lines = path.read_text().splitlines()
    head = lines[0].split("\t")
    return [dict(zip(head, line.split("\t"))) for line in lines[1:]]


def half(path: str) -> str:
    return "train" if int(hashlib.sha256(f"probe|{path}".encode()).hexdigest()[:8], 16) % 2 == 0 else "tune"


def tau_at(p_pos: np.ndarray, max_fn: float = MAX_FALSE_NO_PLANT) -> float:
    """The highest τ at which at most `max_fn` of the plant photos fall below it."""
    return float(np.quantile(p_pos, max_fn, method="lower"))


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--version", required=True)
    args = ap.parse_args(argv)

    inat = read_tsv(SPLITS / "inat_species.tsv")
    negatives = read_tsv(SPLITS / "negatives_coco.tsv")
    pos = {
        "train": [("leaf datasets", r["path"]) for r in read_split("train")]
                 + [("inaturalist", RAW / r["path"]) for r in inat if r["split"] == "train"],
        "val": [("leaf datasets", r["path"]) for r in read_split("val")]
               + [("inaturalist", RAW / r["path"]) for r in inat if r["split"] == "val"],
    }
    neg = {h: [r for r in negatives if half(r["path"]) == h] for h in ("train", "tune")}
    emb = Embedder()
    Xp = {n: emb.files([p for _, p in rs]) for n, rs in pos.items()}
    Xn = {h: emb.files([RAW / r["path"] for r in rs]) for h, rs in neg.items()}
    src = {n: np.asarray([s for s, _ in rs]) for n, rs in pos.items()}
    print({n: {s: int((src[n] == s).sum()) for s in set(src[n])} for n in pos}, {h: len(r) for h, r in neg.items()})

    # Plant and no-plant weigh the same; within plant, each source the same.
    w_pos = np.asarray([0.5 / (src["train"] == s).sum() for s in src["train"]])
    w_neg = np.full(len(Xn["train"]), 1.0 / len(Xn["train"]))
    X = np.concatenate([Xp["train"], Xn["train"]])
    y = np.concatenate([np.ones(len(w_pos)), np.zeros(len(w_neg))])
    w = np.concatenate([w_pos, w_neg])
    w *= len(w) / w.sum()

    def operating(clf) -> dict:
        p_pos = clf.predict_proba(Xp["val"])[:, 1]
        p_neg = clf.predict_proba(Xn["tune"])[:, 1]
        tau = tau_at(p_pos)
        return {"tau_plant_probe_alone": tau, "negatives_pass": float((p_neg >= tau).mean()),
                "false_no_plant": float((p_pos < tau).mean()),
                "false_no_plant_by_source": {s: float((p_pos[src["val"] == s] < tau).mean()) for s in sorted(set(src["val"]))}}

    best = None
    for C in (0.05, 0.2, 1, 5, 20, 100):
        clf = LogisticRegression(C=C, max_iter=5000)
        clf.fit(X, y, sample_weight=w)
        op = operating(clf)
        print(f"C={C}: {op}")
        if best is None or op["negatives_pass"] < best[0]["negatives_pass"]:
            best = (op, C, clf)
    op, C, clf = best

    out = RUNS / args.version
    out.mkdir(parents=True, exist_ok=True)
    headfile.write_linear_head(
        out / f"{args.version}.json", kind="plant_probe", version=args.version,
        features=f"{BIOCLIP}+{emb.model_sha}", weight=clf.coef_.astype(np.float32),
        bias=clf.intercept_.astype(np.float32), temperature=1.0, labels=[{"index": 0, "label": "plant"}],
    )
    (out / "negatives_tune.tsv").write_text(
        "dataset\tpath\tgroup\n" + "".join(f"{r['dataset']}\t{r['path']}\t{r['group']}\n" for r in neg["tune"]))
    report = {
        "version": args.version, "C": C, "probe_alone_on_val": op,
        "counts": {"plant_train": {s: int((src["train"] == s).sum()) for s in set(src["train"])},
                   "plant_val": {s: int((src["val"] == s).sum()) for s in set(src["val"])},
                   "negatives_train": len(neg["train"]), "negatives_tune": len(neg["tune"])},
        "note": ("The probe alone, at the τ_plant where it fails 1% of val plant photos, against the tune half of "
                 "COCO val2017's negatives. The plant check is leaf score >= τ_leaf OR p(plant) >= τ_plant; "
                 "plant_check --probe chooses the pair."),
    }
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{args.version}.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
