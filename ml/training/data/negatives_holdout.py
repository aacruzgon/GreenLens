"""Hold-out photos with no plant, from COCO train2017, to report a chosen plant-check pair.

    python -m ml.training.data.negatives_holdout [--count 1000]

The plant check's thresholds are chosen on the val split and the COCO val2017 negatives
(`negatives.py`), so the numbers measured there flatter them. This draws a second, disjoint set of
photos without a plant from COCO **train2017**, for `plant_check --negatives` to report the chosen
pair on photos it wasn't chosen on (with the test split as the plant photos).

Same rule and groups as `negatives.select` (no `potted plant` or `broccoli`; indoor, people_pets,
outdoor, unlabelled), so the mix matches val2017's. The pick is deterministic (the lowest SHA-256
of `coco_holdout|<file name>`) and leaves out every photo pinned as a detector background
(`splits/background_coco_train.tsv`); `detector.background` refuses to pin a photo listed here.
Each image is downloaded to `ml/training/data/raw/coco_train2017_holdout/` (git-ignored) and pinned
by SHA-256 in `splits/negatives_coco_train_holdout.tsv`; a rerun checks the pins. Evaluation only:
never training data, never committed. Licence: COCO annotations CC BY 4.0; each image keeps its
Flickr licence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from ml.training.data.negatives import select

HERE = Path(__file__).resolve().parent
ANNOTATIONS = HERE / "raw/coco_annotations_2017/annotations/instances_train2017.json"
IMAGES = HERE / "raw/coco_train2017_holdout"
TSV = HERE / "splits/negatives_coco_train_holdout.tsv"
BACKGROUND = HERE / "splits/background_coco_train.tsv"
URL = "http://images.cocodataset.org/train2017/{}"


def pinned_names(tsv: Path) -> set[str]:
    """The image file names a pinned TSV lists (its `path` column's last part)."""
    if not tsv.exists():
        return set()
    return {line.split("\t")[1].rsplit("/", 1)[-1] for line in tsv.read_text().splitlines()[1:]}


def pick(rows: list[tuple[str, str]], count: int, exclude: set[str]) -> list[tuple[str, str]]:
    """`count` of the (file, group) rows not in `exclude`, by the lowest hash of their file name."""
    key = lambda r: hashlib.sha256(f"coco_holdout|{r[0]}".encode()).hexdigest()
    return sorted(sorted((r for r in rows if r[0] not in exclude), key=key)[:count])


def fetch(name: str) -> Path:
    dest = IMAGES / name
    if not dest.exists():
        tmp = dest.with_suffix(".partial")
        urllib.request.urlretrieve(URL.format(name), tmp)
        tmp.rename(dest)
    return dest


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--count", type=int, default=1000)
    args = ap.parse_args(argv)
    pinned = {}
    if TSV.exists():
        for line in TSV.read_text().splitlines()[1:]:
            _, path, _, digest = line.split("\t")
            pinned[path] = digest
    rows = pick(select(json.loads(ANNOTATIONS.read_text())), args.count, pinned_names(BACKGROUND))
    IMAGES.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(8) as pool:
        files = list(pool.map(fetch, [n for n, _ in rows]))
    out = ["dataset\tpath\tgroup\tsha256"]
    for (name, group), dest in zip(rows, files):
        path = f"coco_train2017_holdout/{name}"
        digest = hashlib.sha256(dest.read_bytes()).hexdigest()
        if path in pinned and pinned[path] != digest:
            raise SystemExit(f"{path}: SHA-256 {digest} differs from its pin {pinned[path]}")
        out.append(f"coco_train2017\t{path}\t{group}\t{digest}")
    TSV.write_text("\n".join(out) + "\n")
    print(f"{len(rows)} hold-out negatives {dict(sorted(Counter(g for _, g in rows).items()))} -> {TSV}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
