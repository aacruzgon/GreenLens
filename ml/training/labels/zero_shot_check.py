"""Zero-shot species check with a label matrix (evaluation.md §7, "BioCLIP zero-shot").

    python -m ml.training.labels.zero_shot_check <matrix.json> [--split test] [--T 1.0]

Scores every image of a split against the matrix's species rows, as the worker does:
softmax of (100 / T) · e · tᵀ. Reports top-1 and top-3 overall and per species. The day-1 pass
condition is top-3 containing the right species on 20 team photos; PlantDoc's test split is a
stand-in until those exist (and is harder: single leaves, the weakest view, models.md §6).
"""

from __future__ import annotations

import argparse
import collections
from pathlib import Path

import numpy as np

from ml.pdm_vision import headfile
from ml.pdm_vision.features import Embedder
from ml.training.heads.train_disease import read_split, softmax


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("matrix", type=Path)
    ap.add_argument("--split", default="test")
    ap.add_argument("--T", type=float, default=1.0)
    args = ap.parse_args()

    m = headfile.read_label_matrix(args.matrix)
    rows = [i for i, l in enumerate(m["labels"]) if l["table"] == "species"]
    ids = [m["labels"][i]["id"] for i in rows]
    text = m["rows"][rows]
    recs = [r for r in read_split(args.split) if r["species_id"] in ids]
    emb = Embedder().files([r["path"] for r in recs])
    probs = softmax((100.0 / args.T) * emb @ text.T)
    y = np.asarray([ids.index(r["species_id"]) for r in recs])
    top = np.argsort(-probs, 1)[:, :3]
    hit1, hit3 = top[:, 0] == y, (top == y[:, None]).any(1)
    print(f"{args.split}: n={len(y)} top-1 {hit1.mean():.3f} top-3 {hit3.mean():.3f} over {len(ids)} species labels")
    per = collections.defaultdict(list)
    for r, a, b in zip(recs, hit1, hit3):
        per[r["species_id"]].append((a, b))
    for s, v in sorted(per.items()):
        v = np.asarray(v)
        print(f"   {s:11s} n={len(v):3d} top-1 {v[:, 0].mean():.2f} top-3 {v[:, 1].mean():.2f}")
    conf = collections.Counter((ids[a], ids[b]) for a, b in zip(y, top[:, 0]) if a != b)
    print("most common confusions:", conf.most_common(6))


if __name__ == "__main__":
    main()
