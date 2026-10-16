"""Deduplicate every dataset across sources and splits, then write the committed splits
(training.md §4, steps 2 and 4).

    python -m ml.training.data.dedupe [--threshold 0.98] [--datasets plantdoc tom2024 ...]

1. Embed every image with BioCLIP 2 (cached).
2. Pairs at cosine ≥ threshold are duplicates; groups are their connected components.
3. Each group keeps one image: the one in a published test split if any, otherwise the largest
   file. A group whose images disagree on (species_id, problem_id) is dropped entirely, unless
   a teammate relabels it.
4. Splits: a published test split stays test. Validation is 15% of PlantDoc's training images
   and, for field sets without their own split, 15% held out the same way (seed 0); the rest is
   train.

Writes data/splits/{train,val,test}.tsv (dataset, path relative to data/raw, species, problem)
and data/splits/dedupe_report.md with a sample of pairs to check by eye.
"""

from __future__ import annotations

import argparse
import collections
import random
from pathlib import Path

import numpy as np
import torch

from ml.pdm_vision.features import Embedder
from ml.training.data import index

HERE = Path(__file__).resolve().parent
SPLITS = HERE / "splits"


def duplicate_pairs(emb: np.ndarray, threshold: float, chunk: int = 4096) -> list[tuple[int, int, float]]:
    e = torch.from_numpy(emb).cuda().half()
    pairs = []
    for s in range(0, len(e), chunk):
        sims = e[s : s + chunk] @ e.T
        rows, cols = torch.nonzero(sims >= threshold, as_tuple=True)
        for r, c in zip(rows.tolist(), cols.tolist()):
            i = s + r
            if i < c:
                pairs.append((i, c, float(sims[r, c])))
    return pairs


def components(n: int, pairs: list[tuple[int, int, float]]) -> list[list[int]]:
    parent = list(range(n))

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for i, j, _ in pairs:
        parent[find(i)] = find(j)
    groups = collections.defaultdict(list)
    for i in range(n):
        groups[find(i)].append(i)
    return list(groups.values())


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--threshold", type=float, default=0.98)
    ap.add_argument("--datasets", nargs="*")
    ap.add_argument("--val-share", type=float, default=0.15)
    args = ap.parse_args()

    recs = index.load_all(args.datasets)
    emb = Embedder().files([r.path for r in recs])
    pairs = duplicate_pairs(emb, args.threshold)
    groups = components(len(recs), pairs)

    keep, dropped_conflict, removed_dup = [], [], 0
    for g in groups:
        targets = {recs[i].target for i in g}
        if len(targets) > 1:
            dropped_conflict.append(g)
            continue
        best = max(g, key=lambda i: (recs[i].split == "test", recs[i].path.stat().st_size))
        keep.append(best)
        removed_dup += len(g) - 1

    rng = random.Random(0)
    out = {"train": [], "val": [], "test": []}
    for i in sorted(keep, key=lambda i: str(recs[i].path)):
        r = recs[i]
        if r.split == "test":
            out["test"].append(r)
        elif r.split in ("validation", "val"):
            out["val"].append(r)
        else:
            out["val" if rng.random() < args.val_share else "train"].append(r)

    SPLITS.mkdir(exist_ok=True)
    raw = index.RAW.resolve()
    for split, rs in out.items():
        lines = [f"{r.dataset}\t{r.path.resolve().relative_to(raw)}\t{r.species_id}\t{r.problem_id}" for r in rs]
        (SPLITS / f"{split}.tsv").write_text("dataset\tpath\tspecies_id\tproblem_id\n" + "\n".join(lines) + "\n")

    cross = [(i, j, s) for i, j, s in pairs if recs[i].split != recs[j].split]
    conflict_pairs = [(i, j, s) for i, j, s in pairs if recs[i].target != recs[j].target]
    sample = random.Random(1).sample(pairs, min(30, len(pairs)))
    lines = [
        "# Deduplication report",
        "",
        f"Threshold: cosine ≥ {args.threshold} on BioCLIP 2 embeddings ({Embedder().model_sha}).",
        "",
        f"- Images: {len(recs)}; duplicate pairs: {len(pairs)} ({len(cross)} across published splits, {len(conflict_pairs)} with different labels)",
        f"- Groups dropped for conflicting labels: {len(dropped_conflict)} ({sum(len(g) for g in dropped_conflict)} images)",
        f"- Duplicates removed from kept groups: {removed_dup}",
        f"- Kept: train {len(out['train'])}, val {len(out['val'])}, test {len(out['test'])}",
        "",
        "## Per dataset and split (kept)",
        "",
        "| Dataset | train | val | test |",
        "|---|---|---|---|",
    ]
    per = collections.Counter((r.dataset, s) for s, rs in out.items() for r in rs)
    for d in sorted({r.dataset for r in recs}):
        lines.append(f"| {d} | {per[(d, 'train')]} | {per[(d, 'val')]} | {per[(d, 'test')]} |")
    lines += ["", "## Sample pairs to check by eye", "", "| cosine | a | b | labels |", "|---|---|---|---|"]
    for i, j, s in sorted(sample, key=lambda t: t[2]):
        a, b = recs[i], recs[j]
        lines.append(f"| {s:.4f} | {a.dataset}:{a.split}:{a.path.name} | {b.dataset}:{b.split}:{b.path.name} | {a.target} / {b.target} |")
    (SPLITS / "dedupe_report.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines[:12]))


if __name__ == "__main__":
    main()
