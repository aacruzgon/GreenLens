"""Top-1 per photo on each dataset's test split for a saved disease head, scored as train_disease
scores it: the mean of a photo's crops' restricted, tempered probabilities, on species with two
or more classes. It scores an older head on today's splits; on the splits it was trained with,
it gives the head's own report numbers (v2c: PlantDoc 0.801, TOM2024 0.946, Tomato-Pakistan 0.762).

    python -m ml.training.heads.score_test disease-head-v2c-2026-10-20 \
        --out ml/training/heads/reports/disease-head-v2c-2026-10-20-on-garden-test.json
"""

from __future__ import annotations

import argparse
import collections
import datetime
import json
from pathlib import Path

import numpy as np

from ml.pdm_vision.features import Embedder
from ml.training.eval.calibrate import HEADS, read_linear_head, softmax
from ml.training.heads.crops import CropEmbedder, read_split


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("head", help="a head under ml/training/runs/heads, such as disease-head-v2c-2026-10-20")
    ap.add_argument("--out", type=Path, help="write the scores here as JSON")
    args = ap.parse_args()

    head = read_linear_head(HEADS / args.head / f"{args.head}.json")
    W, b, T = np.asarray(head["weight"]), np.asarray(head["bias"]), float(head["temperature"])
    cidx = {(l["species_id"], l["problem_id"]): l["index"] for l in head["labels"]}
    sp = np.array([l["species_id"] for l in head["labels"]])
    rows = [r for r in read_split("test") if (r["species_id"], r["problem_id"]) in cidx]
    crops = CropEmbedder(gpu=False).load([r["path"] for r in rows])
    whole = Embedder().files([r["path"] for r in rows])
    hit = collections.defaultdict(list)
    for r, c, w in zip(rows, crops, whole):
        if (sp == r["species_id"]).sum() < 2:
            continue
        z = (c["emb"] if len(c["emb"]) else w[None]) @ W.T + b
        z[:, sp != r["species_id"]] = -np.inf
        hit[r["dataset"]].append(int(softmax(z / T).mean(0).argmax() == cidx[(r["species_id"], r["problem_id"])]))
    scores = {d: {"top1": round(float(np.mean(v)), 3), "n": len(v)} for d, v in sorted(hit.items())}
    print(json.dumps(scores))
    if args.out:
        args.out.write_text(json.dumps({
            "head": args.head, "date": datetime.date.today().isoformat(),
            "scored": "top-1 per photo on each dataset's test split in ml/training/data/splits, the mean of "
                      "each photo's crops' restricted probabilities at the head's temperature",
            "test_top1_per_photo": scores,
        }, indent=2) + "\n")


if __name__ == "__main__":
    main()
