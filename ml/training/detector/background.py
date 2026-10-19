"""Background images for the leaf detector: COCO train2017 photos with no plant.

    python -m ml.training.detector.background [--count 400]

The plant check showed the detector passes 51.9% of photos without a plant at score 0.35
(reports/plant-check-val-2026-10-16): it was trained only on PlantDoc's leaves, never on an
image with nothing to find. This picks photos with no `potted plant` or `broccoli` from COCO
**train2017**, so the val2017 photos that score the plant check stay unseen, and only from the
`indoor` and `people_pets` groups of `ml.training.data.negatives.select`, and only photos with no
vegetation at all: no pixel of COCO-Stuff's `plant` supercategory (tree, bush, grass, leaves,
flower, moss, branch, straw, plant-other). Labelling foliage "no leaf" would teach the detector to
ignore the hedges and shrubs users photograph. The groups alone don't ensure that: the first pick
(PR #33) used them alone, and about 8 of the first 20 people/pets photos were sports and street
scenes in front of trees, grass or shrubs. The whole-photo plant probe can't catch them either: it
scores those photos 0.04 to 0.12, because the people fill the frame.

The pick is deterministic (the lowest SHA-256 of `coco_bg|<file name>`). Each image is downloaded
from COCO's server to `ml/training/data/raw/coco_train2017_bg/` (git-ignored) and pinned by its
SHA-256 in `ml/training/data/splits/background_coco_train.tsv`; a rerun checks the pins.
`python -m ml.training.detector.prepare --background` adds them to the training split with no
boxes. Licence: COCO and COCO-Stuff annotations CC BY 4.0; each image keeps its Flickr licence;
never committed.

COCO-Stuff's annotations (`stuff_annotations_trainval2017.zip`, SHA-256 `64d200fe31602f44…`)
unpack to `ml/training/data/raw/coco_stuff_2017/`. The first run reduces `stuff_train2017.json`
(1.1 GB) to each image's plant fraction, cached beside it in `plant_fraction_train2017.json`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

from ml.training.data.negatives import select
from ml.training.data.negatives_holdout import TSV as HOLDOUT
from ml.training.data.negatives_holdout import pinned_names

DATA = Path(__file__).resolve().parents[1] / "data"
ANNOTATIONS = DATA / "raw/coco_annotations_2017/annotations/instances_train2017.json"
STUFF = DATA / "raw/coco_stuff_2017/annotations/stuff_train2017.json"
PLANT_FRACTION = STUFF.with_name("plant_fraction_train2017.json")
IMAGES = DATA / "raw/coco_train2017_bg"
TSV = DATA / "splits/background_coco_train.tsv"
URL = "http://images.cocodataset.org/train2017/{}"
GROUPS = ("indoor", "people_pets")


def plant_fraction(stuff: dict) -> dict[str, float]:
    """Each image's share of pixels in COCO-Stuff's `plant` supercategory, by file name."""
    plant = {c["id"] for c in stuff["categories"] if c["supercategory"] == "plant"}
    area: dict[int, float] = {}
    for a in stuff["annotations"]:
        if a["category_id"] in plant:
            area[a["image_id"]] = area.get(a["image_id"], 0.0) + a["area"]
    return {i["file_name"]: area.get(i["id"], 0.0) / (i["width"] * i["height"]) for i in stuff["images"]}


def load_plant_fraction() -> dict[str, float]:
    if not PLANT_FRACTION.exists():
        if not STUFF.exists():
            raise SystemExit(f"{STUFF} is missing: unpack COCO-Stuff's stuff_annotations_trainval2017.zip "
                             f"into {STUFF.parents[1]}")
        PLANT_FRACTION.write_text(json.dumps(plant_fraction(json.loads(STUFF.read_text()))))
    return json.loads(PLANT_FRACTION.read_text())


def pick(rows: list[tuple[str, str]], count: int, plant: dict[str, float]) -> list[tuple[str, str]]:
    """`count` of the (file, group) rows in GROUPS with no plant pixel, by the lowest hash of their
    file name. A photo COCO-Stuff doesn't cover counts as vegetated."""
    eligible = [r for r in rows if r[1] in GROUPS and plant.get(r[0], 1.0) == 0.0]
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
    rows = pick(select(json.loads(ANNOTATIONS.read_text())), args.count, load_plant_fraction())
    if clash := {n for n, _ in rows} & pinned_names(HOLDOUT):
        raise SystemExit(f"{len(clash)} picked photos are plant-check hold-out negatives, e.g. {min(clash)}")
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
