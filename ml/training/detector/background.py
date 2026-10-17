"""Background images for the leaf detector: COCO train2017 photos with no plant.

    python -m ml.training.detector.background [--count 400]

The plant check showed the detector passes 51.9% of photos without a plant at score 0.35
(reports/plant-check-val-2026-10-16): it was trained only on PlantDoc's leaves, never on an
image with nothing to find. This picks photos with no `potted plant` or `broccoli` from COCO
**train2017**, so the val2017 photos that score the plant check stay unseen, and only from the
`indoor` and `people_pets` groups of `ml.training.data.negatives.select`: outdoor scenes show
trees and grass, and labelling those "no leaf" would teach the detector to ignore real foliage.

The pick is deterministic (the lowest SHA-256 of `coco_bg|<file name>`). Each image is downloaded
from COCO's server to `ml/training/data/raw/coco_train2017_bg/` (git-ignored) and pinned by its
SHA-256 in `ml/training/data/splits/background_coco_train.tsv`; a rerun checks the pins.
`python -m ml.training.detector.prepare --background` adds them to the training split with no
boxes. Licence: COCO annotations CC BY 4.0; each image keeps its Flickr licence; never committed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

from ml.training.data.negatives import select

DATA = Path(__file__).resolve().parents[1] / "data"
ANNOTATIONS = DATA / "raw/coco_annotations_2017/annotations/instances_train2017.json"
IMAGES = DATA / "raw/coco_train2017_bg"
TSV = DATA / "splits/background_coco_train.tsv"
URL = "http://images.cocodataset.org/train2017/{}"
GROUPS = ("indoor", "people_pets")


def pick(rows: list[tuple[str, str]], count: int) -> list[tuple[str, str]]:
    """`count` of the (file, group) rows in GROUPS, by the lowest hash of their file name."""
    eligible = [r for r in rows if r[1] in GROUPS]
    key = lambda r: hashlib.sha256(f"coco_bg|{r[0]}".encode()).hexdigest()
    return sorted(sorted(eligible, key=key)[:count])


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--count", type=int, default=400)
    args = ap.parse_args(argv)
    pinned = {}
    if TSV.exists():
        for line in TSV.read_text().splitlines()[1:]:
            _, path, _, digest = line.split("\t")
            pinned[path] = digest
    rows = pick(select(json.loads(ANNOTATIONS.read_text())), args.count)
    IMAGES.mkdir(parents=True, exist_ok=True)
    out = ["dataset\tpath\tgroup\tsha256"]
    for name, group in rows:
        dest = IMAGES / name
        if not dest.exists():
            tmp = dest.with_suffix(".partial")
            urllib.request.urlretrieve(URL.format(name), tmp)
            tmp.rename(dest)
        path = f"coco_train2017_bg/{name}"
        digest = sha256(dest)
        if path in pinned and pinned[path] != digest:
            raise SystemExit(f"{path}: SHA-256 {digest} differs from its pin {pinned[path]}")
        out.append(f"coco_train2017\t{path}\t{group}\t{digest}")
    TSV.write_text("\n".join(out) + "\n")
    groups = {g: sum(1 for _, x in rows if x == g) for g in GROUPS}
    print(f"{len(rows)} background images {groups} -> {TSV}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
