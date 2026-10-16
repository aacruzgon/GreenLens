"""Prepare the leaf detector's COCO dataset from PlantDoc's detection data.

Collapses every class to `leaf`, cleans the boxes, holds out 15% of train as
`valid` (seed 0, by image file name) and writes RF-DETR's COCO layout.
Idempotent: rebuilds ml/training/data/cache/leaf-coco from scratch.
"""
import csv
import json
import random
import re
import shutil
import sys
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path

from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parents[1]
RAW = next((ROOT / "data/raw/plantdoc_det").glob("PlantDoc-*"))
CACHE = ROOT / "data/cache"
OUT = CACHE / "leaf-coco"
MIN_SIDE = 2.0
VALID_FRACTION = 0.15
SEED = 0


csv_size = {}


def load_csv(split):
    rows = {}
    for r in csv.DictReader(open(RAW / f"{split}_labels.csv")):
        csv_size[r["filename"]] = (int(r["width"]), int(r["height"]))
        rows.setdefault(r["filename"], []).append(
            (float(r["xmin"]), float(r["ymin"]), float(r["xmax"]), float(r["ymax"]))
        )
    return rows


def resolve_image(folder, name):
    """CSV names come from the XML <filename>; some files on disk were saved under another name."""
    p = folder / name
    if p.is_file():
        return p
    for x in folder.glob("*.xml"):
        try:
            fn = ET.parse(x).findtext("filename")
        except ET.ParseError:
            continue
        if fn == name:
            cands = [c for c in folder.iterdir() if c.name != x.name and c.name.startswith(x.stem + ".") and c.suffix != ".xml"]
            if len(cands) == 1:
                return cands[0]
    return None


def clean(split, folder, rows, stats):
    out = []
    for name in sorted(rows):
        path = resolve_image(folder, name)
        if path is None:
            stats["images_unresolved"].append(f"{split}/{name}")
            continue
        with Image.open(path) as im:
            orient = im.getexif().get(274, 1)
            raw_size = im.size
            t_size = ImageOps.exif_transpose(im).size
            fmt = im.format
        # The boxes follow whichever frame the CSV's width/height match. Mostly that is the raw
        # pixel grid (EXIF ignored); for a few images it is the EXIF-transposed one.
        frame = "transposed" if (csv_size[name] == t_size and t_size != raw_size) else "raw"
        w, h = t_size if frame == "transposed" else raw_size
        if orient in (3, 6, 8):
            stats["exif_nontrivial"].append((f"{split}/{name}", orient, frame))
        boxes = []
        for x0, y0, x1, y1 in rows[name]:
            stats["boxes_raw"] += 1
            if x1 - x0 <= 0 or y1 - y0 <= 0:
                stats["dropped_zero_size"] += 1
                continue
            if x0 < 0 or y0 < 0 or x1 > w or y1 > h:
                stats["clipped_oob"] += 1
                x0, y0, x1, y1 = max(0, x0), max(0, y0), min(w, x1), min(h, y1)
            if x1 - x0 <= 0 or y1 - y0 <= 0 or min(x1 - x0, y1 - y0) < MIN_SIDE:
                stats["dropped_small"] += 1
                continue
            boxes.append([x0, y0, x1 - x0, y1 - y0])
        if not boxes:
            stats["images_no_boxes"].append(f"{split}/{name}")
            continue
        out.append({"name": name, "path": path, "w": w, "h": h, "fmt": fmt, "frame": frame, "rotated": orient in (3, 6, 8), "boxes": boxes})
    return out


def write_split(split, items):
    d = OUT / split
    d.mkdir(parents=True, exist_ok=True)
    images, anns = [], []
    for i, it in enumerate(items, 1):
        ext = {"JPEG": ".jpg", "PNG": ".png", "WEBP": ".webp", "MPO": ".jpg"}.get(it["fmt"], ".jpg")
        fn = f"{split}_{i:05d}{ext}"
        dst = d / fn
        if it["rotated"]:
            # Bake the frame the boxes use into the pixels and drop the EXIF tag, so every
            # loader sees the same orientation.
            with Image.open(it["path"]) as im:
                im = ImageOps.exif_transpose(im) if it["frame"] == "transposed" else im
                im.convert("RGB").save(dst, quality=95)
        else:
            dst.symlink_to(it["path"].resolve())
        images.append({"id": i, "file_name": fn, "width": it["w"], "height": it["h"], "orig_name": it["name"]})
        for b in it["boxes"]:
            anns.append({"id": len(anns) + 1, "image_id": i, "category_id": 1,
                         "bbox": [round(v, 2) for v in b], "area": round(b[2] * b[3], 2), "iscrowd": 0})
    coco = {"images": images, "annotations": anns,
            "categories": [{"id": 1, "name": "leaf", "supercategory": "none"}]}
    json.dump(coco, open(d / "_annotations.coco.json", "w"))
    return len(images), len(anns)


def main():
    stats = {"boxes_raw": 0, "clipped_oob": 0, "dropped_zero_size": 0, "dropped_small": 0,
             "images_unresolved": [], "images_no_boxes": [], "exif_nontrivial": []}
    tr_rows, te_rows = load_csv("train"), load_csv("test")
    cross = sorted(set(tr_rows) & set(te_rows))
    train = clean("train", RAW / "TRAIN", tr_rows, stats)
    test = clean("test", RAW / "TEST", te_rows, stats)

    names = sorted(it["name"] for it in train)
    rng = random.Random(SEED)
    rng.shuffle(names)
    valid_names = set(names[: round(len(names) * VALID_FRACTION)])
    valid = [it for it in train if it["name"] in valid_names]
    train = [it for it in train if it["name"] not in valid_names]

    if OUT.exists():
        shutil.rmtree(OUT)
    res = {s: write_split(s, items) for s, items in (("train", train), ("valid", valid), ("test", test))}
    CACHE.mkdir(parents=True, exist_ok=True)
    json.dump(cross, open(CACHE / "plantdoc_det_cross_split_names.json", "w"), indent=1)

    print("splits (images, boxes):", res)
    print({k: (v if not isinstance(v, list) else len(v)) for k, v in stats.items()})
    print("cross-split names:", len(cross))
    print("images unresolved:", stats["images_unresolved"])
    print("images with no boxes left:", stats["images_no_boxes"])
    print("EXIF orientation 3/6/8 images:", len(stats["exif_nontrivial"]),
          Counter((e[2]) for e in stats["exif_nontrivial"]), "(box frame: raw vs transposed)")


if __name__ == "__main__":
    sys.exit(main())
