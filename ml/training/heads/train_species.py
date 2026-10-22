"""Species head for the supported crops on frozen BioCLIP 2 features (training.md §6; models.md §6).

    python -m ml.training.heads.train_species --version species-head-v1-2026-10-20

- Classes: --supported (tomato, apple, grape, pepper, corn: `classifier_supported`, issue #4).
- Data: the splits' photos of those species (mostly leaf photos) and iNaturalist observations of
  each (`data/splits/inat_species.tsv`, role `crop`: whole plants, flowers, fruit; observed
  before 2026, so never an evaluation case). Each photo is embedded whole, as the worker's
  species stage embeds a shot.
- Weights: every species weighs the same, and within a species both sources weigh the same, so
  whole-plant photos count as much as the far more numerous leaf photos.
- Multinomial logistic regression; C chosen on val by the mean of the two sources' top-1; one
  temperature fitted on val.
- Report on val, per source, and on the evaluation set's dev cases (each plant shot of each
  case), three ways: zero-shot over the label matrix in the manifest (the worker's species stage
  today; a stand-in until the seed's matrix exists), the head alone over the supported crops,
  and both combined as models.md 6 says the worker combines them. The head is turned on only if
  the combination beats zero-shot on the dev cases (`SPECIES_HEAD_ENABLED`).

Writes the head (`pdm-linear-head/1`, kind `species_head`) and report.json under
runs/heads/<version>/, and the report under heads/reports/.
"""

from __future__ import annotations

import argparse
import collections
import json
import tomllib
from pathlib import Path

import numpy as np
from sklearn.linear_model import LogisticRegression

from ml.pdm_vision import headfile
from ml.pdm_vision.features import Embedder
from ml.training.heads.train_disease import BIOCLIP, REPORTS, RUNS, fit_temperature, read_split, softmax
from ml.training.labels import current

ROOT = Path(__file__).resolve().parents[3]
RAW = ROOT / "ml/training/data/raw"
INAT = ROOT / "ml/training/data/splits/inat_species.tsv"
FILES = ROOT / "ml/models/files"
EVAL = ROOT / "ml/training/eval"
PLANT_KINDS = ("whole_plant", "leaf_closeup", "leaf_underside", "flower_fruit")
LOGIT_SCALE = 100.0   # BioCLIP 2's, as the worker's zero-shot uses it (models.md 6)


def inat_rows(split: str, supported: list[str]) -> list[dict]:
    lines = INAT.read_text().splitlines()
    cols = lines[0].split("\t")
    rows = [dict(zip(cols, line.split("\t"))) for line in lines[1:]]
    return [{"source": "inaturalist", "path": RAW / r["path"], "species_id": r["slug"]}
            for r in rows if r["role"] == "crop" and r["split"] == split and r["slug"] in supported]


def dataset_rows(split: str, supported: list[str]) -> list[dict]:
    return [{"source": "leaf datasets", "path": r["path"], "species_id": r["species_id"], "dataset": r["dataset"]}
            for r in read_split(split) if r["species_id"] in supported]


def dev_rows(manifest: Path) -> list[dict]:
    """Every plant shot of the dev cases; none when the manifest isn't there (it lands with #86)."""
    if not manifest.exists():
        print(f"no evaluation manifest at {manifest}: the dev comparison is skipped")
        return []
    rows = []
    for line in manifest.read_text().splitlines():
        if not line.strip():
            continue
        c = json.loads(line)
        if c.get("split") != "dev" or not c["truth"].get("species_id"):
            continue
        for im in c["images"]:
            if im["kind"] in PLANT_KINDS:
                rows.append({"source": "evaluation dev", "path": manifest.parent / "photos" / im["file"],
                             "species_id": c["truth"]["species_id"], "case": c["case_id"], "kind": im["kind"]})
    return rows


def label_matrix() -> tuple[np.ndarray, list[str], str]:
    """The species rows of the label matrix the API asks for, their ids and its version."""
    entry = current.matrix_entry(tomllib.loads((ROOT / "ml/models/manifest.toml").read_text())["model"])
    doc = headfile.read_label_matrix(FILES / entry["file"])
    idx = [i for i, label in enumerate(doc["labels"]) if label["table"] == "species"]
    return doc["rows"][idx], [doc["labels"][i]["id"] for i in idx], entry["version"]


def combine(p_zs: np.ndarray, ids: list[str], p_head: np.ndarray, supported: list[str]) -> np.ndarray:
    """models.md 6: the head's and zero-shot's probabilities over the supported crops are averaged
    and replace zero-shot's there, scaled so the supported crops keep their zero-shot share."""
    sup = [ids.index(s) for s in supported]
    share = p_zs[:, sup].sum(1, keepdims=True)
    out = p_zs.copy()
    out[:, sup] = share * (p_head + p_zs[:, sup] / np.maximum(share, 1e-12)) / 2
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--version", required=True)
    ap.add_argument("--supported", nargs="+", default=["tomato", "apple", "grape", "pepper", "corn"])
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl", help="the evaluation manifest")
    args = ap.parse_args(argv)
    sup = args.supported
    sidx = {s: i for i, s in enumerate(sup)}

    sets = {
        "train": dataset_rows("train", sup) + inat_rows("train", sup),
        "val": dataset_rows("val", sup) + inat_rows("val", sup),
        # C and T are chosen on val, so test (the leaf datasets' test split; iNaturalist has none)
        # and dev are the unbiased numbers (review of #85).
        "test": dataset_rows("test", sup),
        "dev": dev_rows(args.manifest),
    }
    emb = Embedder()
    X = {n: emb.files([r["path"] for r in rs]) if rs else np.zeros((0, 768), np.float32) for n, rs in sets.items()}
    y = {n: np.asarray([sidx.get(r["species_id"], -1) for r in rs]) for n, rs in sets.items()}
    src = {n: np.asarray([r["source"] for r in rs]) for n, rs in sets.items()}
    counts = {n: dict(collections.Counter(f"{r['species_id']}/{r['source']}" for r in rs)) for n, rs in sets.items()}
    print(json.dumps(counts, indent=1))

    # Every species the same total weight; within it, each source the same.
    pair = collections.Counter(zip(y["train"], src["train"]))
    n_src = collections.Counter(c for c, _ in pair)
    w = np.asarray([1.0 / (n_src[c] * pair[(c, s)]) for c, s in zip(y["train"], src["train"])])
    w *= len(w) / w.sum()

    def top1_by_source(p: np.ndarray, n: str) -> dict:
        hit = p.argmax(1) == y[n]
        return {s: float(hit[src[n] == s].mean()) for s in sorted(set(src[n]))}

    best = None
    for C in (0.1, 0.5, 1, 5, 20, 50, 200, 1000):
        clf = LogisticRegression(C=C, max_iter=5000)
        clf.fit(X["train"], y["train"], sample_weight=w)
        acc = top1_by_source(clf.predict_proba(X["val"]), "val")
        score = float(np.mean(list(acc.values())))
        print(f"C={C}: val top-1 {acc} mean {score:.3f}")
        if best is None or score > best[0]:
            best = (score, C, clf)
    _, C, clf = best
    T = fit_temperature(clf.decision_function(X["val"]), y["val"])

    rows_zs, ids, label_version = label_matrix()
    missing = [s for s in sup if s not in ids]
    if missing:
        raise SystemExit(f"the label matrix has no row for {missing}")

    def evaluate(n: str) -> dict:
        truth = [r["species_id"] for r in sets[n]]
        p_head = softmax(clf.decision_function(X[n]) / T)
        p_zs = softmax(LOGIT_SCALE * X[n] @ rows_zs.T)
        p_comb = combine(p_zs, ids, p_head, sup)
        on_sup = np.asarray([t in sidx for t in truth], bool)

        def scores(m: np.ndarray) -> dict:
            t = [truth[i] for i in np.flatnonzero(m)]
            zs = [ids[i] for i in p_zs[m].argmax(1)]
            head = [sup[i] for i in p_head[m].argmax(1)]
            comb = [ids[i] for i in p_comb[m].argmax(1)]
            return {
                "n": int(m.sum()),
                "zero_shot_top1": float(np.mean([a == b for a, b in zip(zs, t)])),
                "head_top1_over_supported": float(np.mean([a == b for a, b in zip(head, t)])),
                "combined_top1": float(np.mean([a == b for a, b in zip(comb, t)])),
                "zero_shot_top1_over_supported": float(np.mean(
                    [sup[i] == b for i, b in zip(p_zs[m][:, [ids.index(x) for x in sup]].argmax(1), t)])),
                "by_species": {sp: int(c) for sp, c in collections.Counter(t).items()},
            }

        out = {}
        for s in sorted(set(src[n])):
            m = src[n] == s
            out[s] = scores(m)
            if (m & ~on_sup).any() and (m & on_sup).any():
                # The head can only name a supported crop, so its fair score is on those photos.
                out[f"{s}, supported crops only"] = scores(m & on_sup)
        return out

    report = {
        "version": args.version, "supported": sup, "C": C, "temperature": T,
        "label_matrix": label_version, "logit_scale": LOGIT_SCALE,
        "counts": counts, "C_selected_on": "val: the mean of the two sources' top-1",
        "val": evaluate("val"), "test": evaluate("test"), "dev": evaluate("dev") if sets["dev"] else None,
        "note": ("Zero-shot is over every species row of the manifest's label matrix (a stand-in of 20 species "
                 "until the seed's matrix exists), with T_species 1. Combined is models.md 6's rule. Dev is every "
                 "plant shot of the evaluation set's dev cases; where a source holds photos of an unsupported species "
                 "(squash), the head can't name them, so a second row scores the supported crops only. Test is the leaf "
                 "datasets' test split: C and T are chosen on val."),
    }
    out = RUNS / args.version
    out.mkdir(parents=True, exist_ok=True)
    headfile.write_linear_head(
        out / f"{args.version}.json", kind="species_head", version=args.version,
        features=f"{BIOCLIP}+{emb.model_sha}", weight=clf.coef_.astype(np.float32),
        bias=clf.intercept_.astype(np.float32), temperature=T,
        labels=[{"index": i, "species_id": s} for i, s in enumerate(sup)],
    )
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{args.version}.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"C={C}, T={T:.3f}")
    for n in ("val", "test", "dev"):
        for s, v in (report[n] or {}).items():
            print(f"{n:4s} {s:15s} n={v['n']:5d} zero-shot {v['zero_shot_top1']:.3f} "
                  f"(over supported {v['zero_shot_top1_over_supported']:.3f}) | head {v['head_top1_over_supported']:.3f} "
                  f"| combined {v['combined_top1']:.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
