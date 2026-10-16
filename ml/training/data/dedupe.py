"""Deduplicate every dataset across sources and splits, then write the committed splits
(training.md §4, steps 2 and 4).

    python -m ml.training.data.dedupe [--threshold 0.98] [--datasets plantdoc tom2024 ...]

1. Embed every image with BioCLIP 2 (cached).
2. Pairs at cosine ≥ threshold are duplicates; groups are their connected components.
3. Each group keeps one image: the one in a published test split if any, otherwise the largest
   file. A group whose images disagree on (species_id, problem_id) is dropped entirely, unless
   a teammate relabels it.
4. Splits: a published test split stays test, a published validation split stays val.
   Everything else goes to val or train by a hash of its "same plant" group, so the split is
   stable when datasets are added or duplicates change, and near-copies (cosine ≥
   --split-threshold, default 0.97, below the duplicate threshold) never straddle train and
   val. At 0.95 the chained groups span every class of a dataset, so 0.97 is the default. The field
   sets carry no site or plant ids, so this grouping stands in for training.md §4's "held
   out by source".

Writes data/splits/{train,val,test}.tsv (dataset, path relative to data/raw, species, problem)
and data/splits/dedupe_report.md with a sample of pairs to check by eye.
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import random
from pathlib import Path

import numpy as np
import torch

from ml.pdm_vision.features import Embedder
from ml.training.data import index

HERE = Path(__file__).resolve().parent
SPLITS = HERE / "splits"


def duplicate_pairs(emb: np.ndarray, threshold: float, chunk: int = 4096) -> list[tuple[int, int, float]]:
    """Every pair at cosine ≥ threshold, in float32 (half precision rounds by about 0.0005
    near 0.98 and can exceed 1, review of PR #6 item 6)."""
    e = torch.from_numpy(emb).cuda().float()
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
    ap.add_argument("--split-threshold", type=float, default=0.97,
                    help="near-copies at or above this cosine share a split")
    args = ap.parse_args()

    recs = index.load_all(args.datasets)
    embedder = Embedder()
    emb = embedder.files([r.path for r in recs])
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

    # "Same plant" groups among the kept images, within each dataset, for the train/val split.
    raw = index.RAW.resolve()
    kept = sorted(keep, key=lambda i: str(recs[i].path))
    near = duplicate_pairs(emb[kept], args.split_threshold)
    near = [(a, b, s) for a, b, s in near if recs[kept[a]].dataset == recs[kept[b]].dataset]
    plant_of = {}
    for g in components(len(kept), near):
        key = min(str(recs[kept[a]].path.resolve().relative_to(raw)) for a in g)
        for a in g:
            plant_of[kept[a]] = key

    def to_val(i: int) -> bool:
        h = hashlib.sha256(f"{recs[i].dataset}|{plant_of[i]}".encode()).digest()
        return int.from_bytes(h[:8], "big") / 2**64 < args.val_share

    out = {"train": [], "val": [], "test": []}
    for i in kept:
        r = recs[i]
        if r.split == "test":
            out["test"].append(r)
        elif r.split in ("validation", "val"):
            out["val"].append(r)
        else:
            out["val" if to_val(i) else "train"].append(r)

    SPLITS.mkdir(exist_ok=True)
    for split, rs in out.items():
        lines = [f"{r.dataset}\t{r.path.resolve().relative_to(raw)}\t{r.species_id}\t{r.problem_id}" for r in rs]
        (SPLITS / f"{split}.tsv").write_text("dataset\tpath\tspecies_id\tproblem_id\n" + "\n".join(lines) + "\n")

    cross = [(i, j, s) for i, j, s in pairs if recs[i].split != recs[j].split]
    conflict_pairs = [(i, j, s) for i, j, s in pairs if recs[i].target != recs[j].target]
    band = [p for p in pairs if p[2] < 0.99]
    rest = [p for p in pairs if p[2] >= 0.99]
    rng = random.Random(1)
    sample = rng.sample(band, min(20, len(band))) + rng.sample(rest, min(10, len(rest)))
    lines = [
        "# Deduplication report",
        "",
        f"Threshold: cosine ≥ {args.threshold} (float32) on BioCLIP 2 embeddings ({embedder.model_sha}).",
        f"Train/val by a hash of each \"same plant\" group: near-copies at cosine ≥ {args.split_threshold} within a dataset share a split.",
        "",
        f"- Images: {len(recs)}; duplicate pairs: {len(pairs)} ({len(cross)} across published splits, {len(conflict_pairs)} with different labels)",
        f"- Groups dropped for conflicting labels: {len(dropped_conflict)} ({sum(len(g) for g in dropped_conflict)} images)",
        f"- Duplicates removed from kept groups: {removed_dup}",
        f"- Kept: train {len(out['train'])}, val {len(out['val'])}, test {len(out['test'])}",
        f"- Same-plant groups: {len(set(plant_of.values()))}; the largest holds {max(collections.Counter(plant_of.values()).values())} images",
        "",
        "## Per dataset and split (kept)",
        "",
        "| Dataset | train | val | test |",
        "|---|---|---|---|",
    ]
    per = collections.Counter((r.dataset, s) for s, rs in out.items() for r in rs)
    for d in sorted({r.dataset for r in recs}):
        lines.append(f"| {d} | {per[(d, 'train')]} | {per[(d, 'val')]} | {per[(d, 'test')]} |")
    lines += [
        "",
        "## Sample pairs to check by eye",
        "",
        f"20 pairs from the {args.threshold}–0.99 band (the ones issue #5 asks about) and 10 above it. Paths are under `ml/training/data/raw/`.",
        "",
        "| cosine | a | b | labels |",
        "|---|---|---|---|",
    ]
    rel = lambda r: f"`{r.path.resolve().relative_to(raw)}` ({r.split})"
    for i, j, s in sorted(sample, key=lambda t: t[2]):
        a, b = recs[i], recs[j]
        lines.append(f"| {s:.4f} | {rel(a)} | {rel(b)} | {a.target} / {b.target} |")
    (SPLITS / "dedupe_report.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines[:12]))


if __name__ == "__main__":
    main()
