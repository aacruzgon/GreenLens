"""Licence-clean photos without a plant for training the plant probe (models.md 2; M17).

    python -m ml.training.data.negatives_probe

The trained probe's first version (`plant-probe-v1-2026-10-20`) took the photos it learned "no
plant" from in COCO val2017, whose images keep their Flickr licences: 1,770 of its 2,372 were
non-commercial or no-derivatives, so it can't ship under the rule for shipped weights
(docs/vision/training.md §3), as the detector's `bgveg` couldn't. This list keeps v1's recipe and
replaces those photos:

- **v1's train half of `negatives_coco.tsv`** (`train_plant_probe.half`), only the photos whose
  licence allows commercial use and adaptation (`REUSABLE`): 602.
- **COCO train2017 photos without a potted plant or broccoli** (`negatives.select`), with a
  `REUSABLE` licence, not pinned as the plant check's hold-out negatives, as many per group as the
  train half drops, by the lowest SHA-256 of `probe_neg|<file name>`.

The tune half stays v1's: it only chooses thresholds, and evaluation photos go into no weights.
The new photos are downloaded from COCO's server to `ml/training/data/raw/coco_train2017_probe/`
(git-ignored) and the list is written to `ml/training/data/splits/negatives_probe_train.tsv` with
each photo's SHA-256 and licence; a rerun checks the pins.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from ml.training.data.negatives import select
from ml.training.data.negatives_holdout import TSV as HOLDOUT
from ml.training.data.negatives_holdout import pinned_names
from ml.training.detector.background import REUSABLE, URL, licences, sha256

DATA = Path(__file__).resolve().parent
RAW = DATA / "raw"
ANNOTATIONS = RAW / "coco_annotations_2017/annotations"
NEGATIVES = DATA / "splits/negatives_coco.tsv"
IMAGES = RAW / "coco_train2017_probe"
TSV = DATA / "splits/negatives_probe_train.tsv"
COLUMNS = "dataset\tpath\tgroup\tsha256\tlicence"


def read_tsv(path: Path) -> list[dict]:
    lines = path.read_text().splitlines()
    head = lines[0].split("\t")
    return [dict(zip(head, line.split("\t"))) for line in lines[1:]]


def split_train_half(rows: list[dict], licence: dict[str, str]) -> tuple[list[dict], Counter]:
    """v1's train half: the photos with a REUSABLE licence, and per group how many it drops."""
    from ml.training.heads.train_plant_probe import half

    train = [r for r in rows if half(r["path"]) == "train"]
    keep = [r for r in train if licence[r["path"].rsplit("/", 1)[-1]] in REUSABLE]
    dropped = Counter(r["group"] for r in train if licence[r["path"].rsplit("/", 1)[-1]] not in REUSABLE)
    return keep, dropped


def pick(rows: list[tuple[str, str]], per_group: dict[str, int], licence: dict[str, str],
         exclude: set[str]) -> list[tuple[str, str]]:
    """For each group, `per_group[group]` of the (file, group) rows with a REUSABLE licence and not in
    `exclude`, by the lowest hash of their file name."""
    key = lambda r: hashlib.sha256(f"probe_neg|{r[0]}".encode()).hexdigest()
    out = []
    for group, n in sorted(per_group.items()):
        eligible = [r for r in rows if r[1] == group and licence.get(r[0]) in REUSABLE and r[0] not in exclude]
        if len(eligible) < n:
            raise SystemExit(f"{group}: {len(eligible)} eligible photos, {n} wanted")
        out += sorted(eligible, key=key)[:n]
    return sorted(out)


def fetch(name: str) -> Path:
    import urllib.request

    dest = IMAGES / name
    if not dest.exists():
        tmp = dest.with_suffix(".partial")
        urllib.request.urlretrieve(URL.format(name), tmp)
        tmp.rename(dest)
    return dest


def main(argv=None) -> int:
    argparse.ArgumentParser(description=__doc__.split("\n\n")[0]).parse_args(argv)
    pinned = {r["path"]: r["sha256"] for r in read_tsv(TSV)} if TSV.exists() else {}
    val_licence = licences(json.loads((ANNOTATIONS / "instances_val2017.json").read_text()))
    keep, dropped = split_train_half(read_tsv(NEGATIVES), val_licence)
    train_annotations = json.loads((ANNOTATIONS / "instances_train2017.json").read_text())
    train_licence = licences(train_annotations)
    rows = pick(select(train_annotations), dict(dropped), train_licence, pinned_names(HOLDOUT))
    IMAGES.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(8) as pool:
        files = list(pool.map(fetch, [n for n, _ in rows]))

    out = [COLUMNS]
    for r in keep:
        name = r["path"].rsplit("/", 1)[-1]
        digest = sha256(RAW / r["path"])
        out.append(f"coco_val2017\t{r['path']}\t{r['group']}\t{digest}\t{val_licence[name]}")
    for (name, group), dest in zip(rows, files):
        out.append(f"coco_train2017\tcoco_train2017_probe/{name}\t{group}\t{sha256(dest)}\t{train_licence[name]}")
    for line in out[1:]:
        _, path, _, digest, _ = line.split("\t")
        if path in pinned and pinned[path] != digest:
            raise SystemExit(f"{path}: SHA-256 {digest} differs from its pin {pinned[path]}")
    TSV.write_text("\n".join(out) + "\n")
    print(f"{len(keep)} val2017 photos kept, {len(rows)} train2017 photos added "
          f"({dict(Counter(g for _, g in rows))}) -> {TSV}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
