"""Deduplicate every dataset across sources and splits, then write the committed splits
(training.md §4, steps 2 and 4).

    python -m ml.training.data.dedupe [--threshold 0.98] [--datasets plantdoc tom2024 ...]

1. Embed every image with BioCLIP 2 (cached).
2. Pairs at cosine ≥ threshold are duplicates, and so are files with the same original photo
   (`originals` in a mapping, index.py); groups are their connected components.
3. Each group keeps one image: the one in a published test split if any, then a published
   validation split, then the file closest to the original (the shortest name, for sets with
   `originals`), then the largest file. A group whose images disagree on
   (species_id, problem_id) is dropped entirely, unless a teammate relabels it.
4. Splits, by "same plant" group: near-copies at cosine ≥ --split-threshold (default 0.97,
   below the duplicate threshold) across all datasets, and the photos of one field visit
   (`sessions` in a mapping). At 0.95 the chained groups span every class of a dataset, so
   0.97 is the default. Within a dataset the near-copies are searched among every image, each
   standing for the image its duplicate group keeps: a set like PlantCity ships each photo
   flipped and turned, so two shots of one leaf can be near-copies only in orientations the
   groups didn't keep. Across datasets only kept images link, since a photo's many copies would
   chain other sets' look-alike leaves (healthy ones above all) into its group.
   - A group holding a published test image: that image stays test, and the group's other
     images are dropped, since they are near-copies of a test image.
   - Otherwise a group holding a published validation image goes to val.
   - The other groups, in the order of a hash of each, fill every (dataset, class)'s quotas:
     --test-share to test, only for datasets with no published test split (the field sets),
     so each of those has a held-out test of its own; --val-share to val; the rest to train.
     The hash order keeps the split stable when other datasets are added, and the quotas keep
     every class in val and test even though groups (whole field visits) vary in size.
   The field sets carry no site or plant ids, so these groups stand in for training.md §4's
   "held out by source".
5. --keep-splits, for adding datasets: every image already in the committed splits keeps its
   split (and wins its duplicate group), since refilling the quotas moves whole field visits and
   would put photos a shipped head trained on into test. A new image joins a group's committed
   images in the strictest split among them (test, then val, then train), or is dropped as a
   near-copy of a published test image; groups of new images only fill the quotas as above.

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

from ml.pdm_vision.features import Embedder
from ml.training.data import index

HERE = Path(__file__).resolve().parent
SPLITS = HERE / "splits"


def duplicate_pairs(emb: np.ndarray, threshold: float, chunk: int = 4096) -> list[tuple[int, int, float]]:
    """Every pair at cosine ≥ threshold, in float32 (half precision rounds by about 0.0005
    near 0.98 and can exceed 1, review of PR #6 item 6)."""
    import torch   # here, so the rest of the module imports without it (CI's ml job)

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


def assign_splits(plant_groups: list[list], committed: dict[str, str], relpath, has_test: set[str],
                  val_share: float, test_share: float) -> tuple[dict[str, list], list, int]:
    """Step 4 and --keep-splits (the module's docstring) for the "same plant" groups, each a list
    of records: the records per split, those dropped as near-copies of a published test image,
    and how many committed images kept their split. `committed` maps a path relative to data/raw
    (`relpath(record)`) to its committed split."""
    out = {"train": [], "val": [], "test": []}
    dropped_near_test, free = [], []
    kept_where = 0
    for members in plant_groups:
        fixed = [r for r in members if relpath(r) in committed]
        if fixed:
            for r in fixed:
                out[committed[relpath(r)]].append(r)
            new = [r for r in members if relpath(r) not in committed]
            if any(r.split == "test" for r in members):
                out["test"] += [r for r in new if r.split == "test"]
                dropped_near_test += [r for r in new if r.split != "test"]
            else:
                out[next(s for s in ("test", "val", "train") if any(committed[relpath(r)] == s for r in fixed))] += new
            kept_where += len(fixed)
        elif any(r.split == "test" for r in members):
            out["test"] += [r for r in members if r.split == "test"]
            dropped_near_test += [r for r in members if r.split != "test"]
        elif any(r.split in ("validation", "val") for r in members):
            out["val"] += members
        else:
            free.append(members)

    # The free groups, in the order of a hash of each, fill each (dataset, class)'s val and
    # test quotas: a group goes to test while that class's test share is short by at least
    # half the group, then to val the same way, else to train. Groups are whole field visits
    # or same-plant sets of very different sizes, so plain hash thresholds leave some classes
    # with no val or test at all.
    def label(members) -> tuple[str, str]:
        return members[0].dataset, collections.Counter(r.target for r in members).most_common(1)[0][0]

    def order(members) -> bytes:
        key = min(relpath(r) for r in members)
        return hashlib.sha256(f"{members[0].dataset}|{key}".encode()).digest()

    total = collections.Counter()
    for members in free:
        total[label(members)] += len(members)
    filled = {"val": collections.Counter(), "test": collections.Counter()}
    for members in sorted(free, key=order):
        lab, n = label(members), len(members)
        dest = "train"
        for split, frac in (("test", test_share if lab[0] not in has_test else 0.0), ("val", val_share)):
            if filled[split][lab] + n / 2 <= frac * total[lab]:
                dest = split
                break
        if dest != "train":
            filled[dest][lab] += n
        out[dest] += members
    for rs in out.values():
        rs.sort(key=lambda r: str(r.path))
    return out, dropped_near_test, kept_where


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--threshold", type=float, default=0.98)
    ap.add_argument("--datasets", nargs="*")
    ap.add_argument("--val-share", type=float, default=0.15)
    ap.add_argument("--test-share", type=float, default=0.15,
                    help="test share for datasets with no published test split")
    ap.add_argument("--keep-splits", action="store_true",
                    help="keep every image of the committed splits where it is; place only new images")
    ap.add_argument("--split-threshold", type=float, default=0.97,
                    help="near-copies at or above this cosine share a split")
    args = ap.parse_args()

    dropped_at_load: dict = {}
    recs = index.load_all(args.datasets, dropped_at_load)
    copies = {d: sum(n for k, n in c.items() if "(copy of" in k) for d, c in dropped_at_load.items()}
    embedder = Embedder()
    emb = embedder.files([r.path for r in recs])
    pairs = duplicate_pairs(emb, args.threshold)
    by_original = collections.defaultdict(list)
    for i, r in enumerate(recs):
        if r.original:
            by_original[r.original].append(i)
    same_photo = [(g[0], j, 1.0) for g in by_original.values() for j in g[1:]]
    groups = components(len(recs), pairs + same_photo)

    raw = index.RAW.resolve()
    relpath = lambda r: str(r.path.resolve().relative_to(raw))
    committed: dict[str, str] = {}
    committed_ds: set[str] = set()
    if args.keep_splits:
        for split in ("train", "val", "test"):
            for line in (SPLITS / f"{split}.tsv").read_text().splitlines()[1:]:
                committed[line.split("\t")[1]] = split
                committed_ds.add(line.split("\t")[0])

    def rank(i: int):
        r = recs[i]
        return (relpath(r) in committed, r.split == "test", r.split in ("validation", "val"),
                -len(r.path.name) if r.original else 0, r.path.stat().st_size)

    keep, dropped_conflict, removed_dup = [], [], 0
    kept_of: dict[int, int] = {}   # every image of a kept group -> the image the group keeps
    for g in groups:
        targets = {recs[i].target for i in g}
        if len(targets) > 1:
            dropped_conflict.append(g)
            continue
        k = max(g, key=rank)
        keep.append(k)
        kept_of.update((i, k) for i in g)
        removed_dup += len(g) - 1

    # "Same plant" groups for the splits. Within a dataset, near-copies among every image, each
    # mapped to the image its group keeps. Across datasets, only between kept images: each kept
    # PlantCity photo has about 13 copies, and letting all of them link chains other sets'
    # healthy leaves into PlantCity's groups (187 healthy tomato photos from four sets in one).
    kept = sorted(keep, key=lambda i: str(recs[i].path))
    pos = {k: a for a, k in enumerate(kept)}
    best: dict[tuple[int, int], float] = {}
    for i, j, c in duplicate_pairs(emb, args.split_threshold):
        if recs[i].dataset != recs[j].dataset and (kept_of.get(i) != i or kept_of.get(j) != j):
            continue
        if i in kept_of and j in kept_of and kept_of[i] != kept_of[j]:
            a, b = sorted((pos[kept_of[i]], pos[kept_of[j]]))
            best[(a, b)] = max(c, best.get((a, b), 0.0))
    # In a dataset with `sessions`, the field visit is the unit: cosine links between its own
    # images would chain its visits into one group (TOM2024's 640 x 480 photos sit at 0.97
    # across days), so only its links to other datasets count.
    by_visit = {r.dataset for r in recs if r.session}
    near_all = [(a, b, c) for (a, b), c in best.items()]
    near = [(a, b, c) for a, b, c in near_all
            if not (recs[kept[a]].dataset == recs[kept[b]].dataset and recs[kept[a]].dataset in by_visit)]
    by_session = collections.defaultdict(list)
    for a, i in enumerate(kept):
        if recs[i].session:
            by_session[recs[i].session].append(a)
    near += [(g[0], b, 1.0) for g in by_session.values() for b in g[1:]]
    has_test = {r.dataset for r in recs if r.split == "test"}

    plant = [[recs[kept[a]] for a in g] for g in components(len(kept), near)]
    plant_groups = [len(g) for g in plant]
    out, dropped_near_test, kept_where = assign_splits(plant, committed, relpath, has_test,
                                                       args.val_share, args.test_share)
    split_of = {id(r): name for name, rs in out.items() for r in rs}
    # A committed dataset's image that wasn't in the splits before: a group whose label conflict
    # went away when a class was dropped in place (training.md section 4, step 3) comes back here.
    new_in_committed = collections.Counter(r.dataset for rs in out.values() for r in rs
                                           if r.dataset in committed_ds and relpath(r) not in committed)
    straddle = collections.Counter(
        recs[kept[a]].dataset for a, b, _ in near_all
        if id(recs[kept[a]]) in split_of and id(recs[kept[b]]) in split_of
        and split_of[id(recs[kept[a]])] != split_of[id(recs[kept[b]])]
    )

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
        f"Splits by \"same plant\" group: near-copies at cosine ≥ {args.split_threshold} across datasets, and one field visit (`sessions`), share a split; val {args.val_share}, and test {args.test_share} for datasets without a published test split.",
        "",
        f"- Images: {len(recs)}; duplicate pairs: {len(pairs)} ({len(cross)} across published splits, {len(conflict_pairs)} with different labels)",
        f"- Groups dropped for conflicting labels: {len(dropped_conflict)} ({sum(len(g) for g in dropped_conflict)} images)",
        f"- Duplicates removed from kept groups: {removed_dup} (including {len(same_photo)} augmented copies of the same original photo)",
        *[f"- {d}: {n} files dropped as copies of another source's photos (`originals.drop_if_name_in`)" for d, n in copies.items() if n],
        f"- Dropped as near-copies (≥ {args.split_threshold}) of a test image: {len(dropped_near_test)}",
        f"- Near-copies (≥ {args.split_threshold}) still in different splits: {dict(straddle) or 0} (within datasets grouped by field visit instead)",
        *([f"- `--keep-splits`: {kept_where} of the {len(committed)} committed images kept their split; "
           f"{len(committed) - kept_where} are no longer kept; new from datasets already in the splits: "
           f"{dict(new_in_committed) or 0}"] if args.keep_splits else []),
        f"- Kept: train {len(out['train'])}, val {len(out['val'])}, test {len(out['test'])}",
        f"- Same-plant groups: {len(plant_groups)}; the largest holds {max(plant_groups)} images",
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
