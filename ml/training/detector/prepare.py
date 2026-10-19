"""Prepare the leaf detector's COCO dataset from PlantDoc's detection data.

    python -m ml.training.detector.prepare [--background]

Collapses every class to `leaf`, cleans the boxes, drops training images that duplicate a
test image, holds out 15% of train as `valid` by "same plant" group (near-copies at cosine ≥
0.97 share a split, as in data/dedupe.py), and writes RF-DETR's COCO layout. Idempotent: rebuilds ml/training/data/cache/leaf-coco from scratch.
With --background, the photos of splits/background_coco_train.tsv (detector/background.py: COCO
train2017, indoor and people or pets, no plant) join the training split with no boxes, so the
detector also learns images with nothing to find; valid and test are unchanged.
"""
import csv
import hashlib
import json
import re
import shutil
import sys
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path

import torch
from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "data/cache"
OUT = CACHE / "leaf-coco"
MIN_SIDE = 2.0
VALID_FRACTION = 0.15


SPLIT_THRESHOLD = 0.97  # near-copies at or above it never straddle train, valid and test (data/dedupe.py)


def raw_dir() -> Path:
    found = sorted((ROOT / "data/raw/plantdoc_det").glob("PlantDoc-*"))
    if not found:
        raise SystemExit("PlantDoc detection data is missing: run python -m ml.training.data.fetch plantdoc_det")
    return found[0]


def load_csv(split):
    """Boxes and the CSV's image size per file name, for one split. The sizes are per split:
    a name in both splits can have different sizes in each (review of PR #6, item 4)."""
    rows, sizes = {}, {}
    with open(raw_dir() / f"{split}_labels.csv", newline="") as f:
        table = list(csv.DictReader(f))
    for r in table:
        sizes[r["filename"]] = (int(r["width"]), int(r["height"]))
        rows.setdefault(r["filename"], []).append(
            (float(r["xmin"]), float(r["ymin"]), float(r["xmax"]), float(r["ymax"]))
        )
    return rows, sizes


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


def clean(split, folder, rows, csv_size, stats):
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
    with open(d / "_annotations.coco.json", "w") as f:
        json.dump(coco, f)
    return len(images), len(anns)


def background_items() -> list[dict]:
    """The pinned background photos as training items with no boxes."""
    tsv = ROOT / "data/splits/background_coco_train.tsv"
    if not tsv.exists():
        raise SystemExit(f"{tsv} is missing: run python -m ml.training.detector.background")
    items = []
    for line in tsv.read_text().splitlines()[1:]:
        _, rel, _, digest = line.split("\t")
        path = ROOT / "data/raw" / rel
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise SystemExit(f"{path} doesn't match its pinned SHA-256")
        with Image.open(path) as im:
            w, h, fmt = im.width, im.height, im.format
        items.append({"path": path, "name": path.name, "w": w, "h": h, "fmt": fmt,
                      "rotated": False, "frame": "raw", "boxes": []})
    return items


def main():
    import argparse

    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--background", action="store_true", help="add the pinned COCO background photos to train")
    args = ap.parse_args()
    stats = {"boxes_raw": 0, "clipped_oob": 0, "dropped_zero_size": 0, "dropped_small": 0,
             "images_unresolved": [], "images_no_boxes": [], "exif_nontrivial": []}
    (tr_rows, tr_size), (te_rows, te_size) = load_csv("train"), load_csv("test")
    cross = sorted(set(tr_rows) & set(te_rows))
    train = clean("train", raw_dir() / "TRAIN", tr_rows, tr_size, stats)
    test = clean("test", raw_dir() / "TEST", te_rows, te_size, stats)

    # Deduplicate train against test (training.md §5, step 2): the test copy stays, and a
    # training image goes when it shares a file name with a test image or is a near-copy of
    # one (BioCLIP 2 cosine >= SPLIT_THRESHOLD, float32, the same rule as data/dedupe.py), so
    # test mAP is never measured on training images.
    from ml.pdm_vision.features import Embedder

    emb = Embedder()
    e_tr = torch.from_numpy(emb.files([it["path"] for it in train])).cuda().float()
    e_te = torch.from_numpy(emb.files([it["path"] for it in test])).cuda().float()
    near = (e_tr @ e_te.T).max(dim=1).values.cpu().numpy() >= SPLIT_THRESHOLD
    drop = {i for i, it in enumerate(train) if it["name"] in te_rows or near[i]}
    stats["train_dropped_as_test_duplicates"] = len(drop)
    kept = [i for i in range(len(train)) if i not in drop]
    train = [train[i] for i in kept]

    # Validation by a hash of each "same plant" group (review of PR #6): near-copies inside
    # PlantDoc's train never straddle train and valid, since valid picks the checkpoint. The
    # group's key is its smallest file name, so the split doesn't move when unrelated images
    # are added or dropped.
    from ml.training.data.dedupe import components, duplicate_pairs

    near = duplicate_pairs(e_tr[kept].cpu().numpy(), SPLIT_THRESHOLD)
    groups = components(len(train), near)
    stats["valid_groups"] = len(groups)
    stats["valid_largest_group"] = max(len(g) for g in groups)
    valid_idx = set()
    for g in groups:
        key = min(train[i]["name"] for i in g)
        if int.from_bytes(hashlib.sha256(f"plantdoc_det|{key}".encode()).digest()[:8], "big") / 2**64 < VALID_FRACTION:
            valid_idx.update(g)
    valid = [it for i, it in enumerate(train) if i in valid_idx]
    train = [it for i, it in enumerate(train) if i not in valid_idx]

    if args.background:
        bg = background_items()
        stats["train_background"] = len(bg)
        train = train + bg

    if OUT.exists():
        shutil.rmtree(OUT)
    res = {s: write_split(s, items) for s, items in (("train", train), ("valid", valid), ("test", test))}
    CACHE.mkdir(parents=True, exist_ok=True)
    with open(CACHE / "plantdoc_det_cross_split_names.json", "w") as f:
        json.dump(cross, f, indent=1)

    print("splits (images, boxes):", res)
    print({k: (v if not isinstance(v, list) else len(v)) for k, v in stats.items()})
    print("cross-split names:", len(cross))
    print("images unresolved:", stats["images_unresolved"])
    print("images with no boxes left:", stats["images_no_boxes"])
    print("EXIF orientation 3/6/8 images:", len(stats["exif_nontrivial"]),
          Counter((e[2]) for e in stats["exif_nontrivial"]), "(box frame: raw vs transposed)")


if __name__ == "__main__":
    sys.exit(main())
