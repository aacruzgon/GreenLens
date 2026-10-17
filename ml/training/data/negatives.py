"""Photos with no plant, from COCO val2017, to score the app's plant check.

    python -m ml.training.data.negatives

Reads instances_val2017.json and writes data/splits/negatives_coco.tsv (dataset, path relative
to data/raw, group). An image is kept unless it has an instance of `potted plant` or
`broccoli`. Evaluation only: these photos are never training data, and the images are never
committed.

Group, by the first rule that fits: `unlabelled` (no annotations at all); `indoor` (any
category whose COCO supercategory is furniture, appliance, electronic or kitchen);
`people_pets` (person, cat or dog); else `outdoor`.
"""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
RAW = HERE / "raw"
ANNOTATIONS = RAW / "coco_annotations_2017" / "annotations" / "instances_val2017.json"
IMAGES = "coco_val2017/val2017"
OUT = HERE / "splits" / "negatives_coco.tsv"

PLANTS = {"potted plant", "broccoli"}
INDOOR = {"furniture", "appliance", "electronic", "kitchen"}
PEOPLE_PETS = {"person", "cat", "dog"}


def select(instances: dict) -> list[tuple[str, str]]:
    """(file_name, group) for every image without a plant, sorted by file name."""
    cats = {c["id"]: c for c in instances["categories"]}
    by_image: dict[int, set[int]] = {}
    for a in instances["annotations"]:
        by_image.setdefault(a["image_id"], set()).add(a["category_id"])
    out = []
    for img in instances["images"]:
        ids = by_image.get(img["id"], set())
        names = {cats[i]["name"] for i in ids}
        if names & PLANTS:
            continue
        supers = {cats[i]["supercategory"] for i in ids}
        if not ids:
            group = "unlabelled"
        elif supers & INDOOR:
            group = "indoor"
        elif names & PEOPLE_PETS:
            group = "people_pets"
        else:
            group = "outdoor"
        out.append((img["file_name"], group))
    return sorted(out)


def main() -> None:
    instances = json.loads(ANNOTATIONS.read_text())
    rows = select(instances)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as f:
        f.write("dataset\tpath\tgroup\n")
        for name, group in rows:
            f.write(f"coco_val2017\t{IMAGES}/{name}\t{group}\n")
    kept = {n for n, _ in rows}
    cats = {c["name"]: c["id"] for c in instances["categories"]}
    dropped = {n: set() for n in PLANTS}
    file_of = {i["id"]: i["file_name"] for i in instances["images"]}
    for a in instances["annotations"]:
        for n in PLANTS:
            if a["category_id"] == cats[n]:
                dropped[n].add(a["image_id"])
    print(f"kept {len(rows)} of {len(instances['images'])} images -> {OUT}")
    for g, n in sorted(Counter(g for _, g in rows).items()):
        print(f"  {g}: {n}")
    for n, ids in sorted(dropped.items()):
        print(f"  images with {n}: {len(ids)}")
    print(f"  images with either: {len(set().union(*dropped.values()))}")
    assert not kept & {file_of[i] for s in dropped.values() for i in s}


if __name__ == "__main__":
    main()
