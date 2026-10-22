"""The face check (task M7) and the upload sizes of issue #41, on the shipped Rust face detector.

    python -m ml.training.data.faces                          # the Open Images photos (once)
    python -m ml.training.eval.face_check run --models-dir DIR [--thresholds 0.15 0.2 0.3 …] [--sets dev]
    python -m ml.training.eval.face_check run … --variants original --thresholds 0.3 --pass-a-long-side 640
    python -m ml.training.eval.face_check score [--out faces-2026-10-22] [--notes notes.md] [--crop-thresholds 0.3]

`run` builds `vision`'s `face_check` example in release (`backend/crates/vision/examples/
face_check.rs`: `imaging::decode_upload`, the quality gate on its 512 px downscale, then
`FaceDetector::detect`, YuNet on the CPU with two intra-op threads, as `api` runs them), and
runs it on:
- the Open Images photos of `data/splits/faces_openimages.tsv` (`data/faces.py`), and
- the dev cases' plant photos (`eval/manifest.jsonl`; never the test split),
each as three uploads (VARIANTS): the photo as it is, and at most 3,200 and 2,048 px on the
long side (PIL Lanczos, JPEG quality 85, as #41 proposes; a smaller photo is only re-encoded).
One process per variant and threshold, one after another, so the times aren't shared. It needs
`--models-dir` (the `api` set of `python3 -m ml.models.fetch --for api`) and ONNX Runtime 1.31
(`--ort`, by default the venv's `onnxruntime`); `--eval-photos` points at the evaluation photos
when they live in another checkout. Raw lines go to
training/runs/faces/raw/<set>-<variant>-<threshold>.jsonl. `--pass-a-long-side N` runs with pass A
at N px instead (`-aN.jsonl`), which `score` unites with the shipped run to weigh a third pass.

`score` follows evaluation 4: a face counts when it is 20 px or more across (its box's width) in
the stored image (1,600 px long side), and it is caught when the union of the detected boxes,
grown 25% on each side as `api` grows them before blurring, covers 90% or more of its box. It
reports recall per variant and threshold, per size, for faces under 40 px at full resolution,
and for occluded and truncated faces; detected boxes that match no face; the share of plant
photos with any box; the changes it weighs (more growth, a third pass, a minimum box size); the
quality scores' drift between sizes; and bytes and milliseconds per size. Crops of every missed
face and every unmatched box at `--crop-thresholds`, and pages of them, go to
training/runs/faces/crops/. The report is eval/reports/<out>.md and .json; `--notes` adds what a
person saw in the crops after the headline.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import statistics
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

from PIL import Image, ImageDraw

from ml.training.data import faces as F

ROOT = Path(__file__).resolve().parents[3]
BACKEND = ROOT / "backend"
EXAMPLE = BACKEND / "target/release/examples/face_check"
RUNS = ROOT / "ml/training/runs/faces"
CROPS = RUNS / "crops"
MANIFEST = ROOT / "ml/training/eval/manifest.jsonl"
EVAL_PHOTOS = ROOT / "ml/training/eval/photos"
REPORTS = ROOT / "ml/training/eval/reports"
VENV_ORT = sorted((ROOT / ".venv").glob("lib/python3*/site-packages/onnxruntime/capi/libonnxruntime.so*"))

VARIANTS = {"original": None, "3200": 3200, "2048": 2048}
THRESHOLDS = (0.3, 0.2, 0.15)
SHIPPED = 0.3
JPEG_QUALITY = 85
GROW = 0.25          # FaceConfig::grow
MERGE_IOU = 0.2      # FaceConfig::merge_iou
GROWS = (0.25, 0.35, 0.5)  # the growths the proposals table tries
COVERED = 0.90       # evaluation 4: the blurred area covers 90% of the face's box
MIN_FACE = 20        # evaluation 4: faces 20 px or more across in the stored image
MIN_BLUR = 40.0      # QualityConfig::min_blur_score
SIZES = ((0, 20), (20, 40), (40, 80), (80, 160), (160, 100_000))
SETS = ("openimages", "dev")
MIN_BOX = (0, 8, 12, 16)  # px across in the stored image: the minimum-box rule the report estimates


# --- geometry -----------------------------------------------------------------------------------

def grown(box: dict, share: float = GROW) -> tuple[float, float, float, float]:
    """`FaceBox::grown`: each side out by `share` of the box, clamped to the image; (x0, y0, x1, y1)."""
    dx, dy = box["w"] * share, box["h"] * share
    return (max(0.0, box["x"] - dx), max(0.0, box["y"] - dy),
            min(1.0, box["x"] + box["w"] + dx), min(1.0, box["y"] + box["h"] + dy))


def covered_share(face: tuple[float, float, float, float], rects: list[tuple[float, float, float, float]]) -> float:
    """The share of `face`'s area inside the union of `rects`, all as (x0, y0, x1, y1); exact."""
    fx0, fy0, fx1, fy1 = face
    area = (fx1 - fx0) * (fy1 - fy0)
    if area <= 0:
        return 0.0
    clipped = [(max(x0, fx0), max(y0, fy0), min(x1, fx1), min(y1, fy1)) for x0, y0, x1, y1 in rects]
    clipped = [r for r in clipped if r[2] > r[0] and r[3] > r[1]]
    if not clipped:
        return 0.0
    xs = sorted({v for r in clipped for v in (r[0], r[2])})
    ys = sorted({v for r in clipped for v in (r[1], r[3])})
    inside = 0.0
    for xa, xb in zip(xs, xs[1:]):
        for ya, yb in zip(ys, ys[1:]):
            cx, cy = (xa + xb) / 2, (ya + yb) / 2
            if any(r[0] <= cx <= r[2] and r[1] <= cy <= r[3] for r in clipped):
                inside += (xb - xa) * (yb - ya)
    return inside / area


def face_rect(face: dict) -> tuple[float, float, float, float]:
    return face["x0"], face["y0"], face["x1"], face["y1"]


def size_name(lo: int, hi: int) -> str:
    return f"{lo}+ px" if hi >= 100_000 else f"{lo}-{hi} px"


SIZE_NAMES = [size_name(lo, hi) for lo, hi in SIZES]


def size_bucket(px: float) -> str:
    """The name of the size bucket a face of `px` pixels across falls in."""
    return next(size_name(lo, hi) for lo, hi in SIZES if lo <= px < hi)


def union_merge(boxes: list[dict], iou: float = MERGE_IOU) -> list[dict]:
    """`faces::union_merge`: boxes that overlap by more than `iou`, or where one holds the other,
    become their union (the higher score), until nothing changes."""
    def rect(b):
        return b["x"], b["y"], b["x"] + b["w"], b["y"] + b["h"]

    def inter(a, b):
        (ax0, ay0, ax1, ay1), (bx0, by0, bx1, by1) = rect(a), rect(b)
        return max(0.0, min(ax1, bx1) - max(ax0, bx0)) * max(0.0, min(ay1, by1) - max(ay0, by0))

    boxes = [dict(b) for b in boxes]
    while True:
        for i in range(len(boxes)):
            for j in range(i + 1, len(boxes)):
                a, b = boxes[i], boxes[j]
                n = inter(a, b)
                area_a, area_b = a["w"] * a["h"], b["w"] * b["h"]
                u = area_a + area_b - n
                if (u > 0 and n / u > iou) or (n > 0 and (n >= area_b * 0.999 or n >= area_a * 0.999)):
                    (ax0, ay0, ax1, ay1), (bx0, by0, bx1, by1) = rect(a), rect(b)
                    x0, y0, x1, y1 = min(ax0, bx0), min(ay0, by0), max(ax1, bx1), max(ay1, by1)
                    boxes[i] = {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0, "score": max(a["score"], b["score"])}
                    boxes[j] = boxes[-1]
                    boxes.pop()
                    break
            else:
                continue
            break
        else:
            return boxes


def blurred_share(boxes: list[dict], grow: float = GROW) -> float:
    """The share of the image inside the union of the grown boxes."""
    return covered_share((0.0, 0.0, 1.0, 1.0), [grown(b, grow) for b in boxes])


def matches_a_face(box: dict, faces: list[dict]) -> bool:
    """Whether a detected box is on an annotated face: its centre inside the face's box grown
    25%, or a third or more of the face's box inside it."""
    cx, cy = box["x"] + box["w"] / 2, box["y"] + box["h"] / 2
    for f in faces:
        dx, dy = (f["x1"] - f["x0"]) * GROW, (f["y1"] - f["y0"]) * GROW
        if f["x0"] - dx <= cx <= f["x1"] + dx and f["y0"] - dy <= cy <= f["y1"] + dy:
            return True
        b = (box["x"], box["y"], box["x"] + box["w"], box["y"] + box["h"])
        if covered_share(face_rect(f), [b]) >= 1 / 3:
            return True
    return False


# --- variants -----------------------------------------------------------------------------------

def make_variant(src: Path, dest: Path, long_side: int | None) -> int:
    """The upload a phone would send: the photo itself (`None`), or at most `long_side` px on
    the long side with Lanczos at JPEG quality 85. Returns the bytes."""
    if long_side is None:
        return src.stat().st_size
    if not dest.exists():
        im = Image.open(src).convert("RGB")
        if max(im.size) > long_side:
            s = long_side / max(im.size)
            im = im.resize((max(1, round(im.size[0] * s)), max(1, round(im.size[1] * s))), Image.Resampling.LANCZOS)
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(".part")
        im.save(tmp, "JPEG", quality=JPEG_QUALITY)
        tmp.rename(dest)
    return dest.stat().st_size


def openimages_photos() -> list[dict]:
    lines = F.INDEX.read_text().splitlines()
    rows = [dict(zip(lines[0].split("\t"), line.split("\t"))) for line in lines[1:]]
    return [{"key": r["image_id"], "path": F.FACES / r["file"], "split": r["split"],
             "width": int(r["width"]), "height": int(r["height"]), "bytes": int(r["bytes"])} for r in rows]


def dev_photos(manifest: Path | None = None, photos: Path | None = None) -> list[dict]:
    """The dev cases' photos only: the test split is scored once, on Oct 24."""
    manifest, photos = manifest or MANIFEST, photos or EVAL_PHOTOS
    out = []
    for line in manifest.read_text().splitlines():
        case = json.loads(line)
        if case["split"] != "dev":
            continue
        for i, image in enumerate(case["images"]):
            out.append({"key": f"{case['case_id']}#{i}", "path": photos / image["file"], "kind": image["kind"]})
    return out


def variant_path(photo_set: str, key: str, variant: str) -> Path:
    return RUNS / "variants" / photo_set / variant / f"{key.replace('/', '_').replace('#', '_')}.jpg"


def variants_of(photo_set: str, photos: list[dict]) -> dict[str, dict[str, tuple[Path, int]]]:
    """For each variant, each photo's upload file and its bytes."""
    out: dict[str, dict[str, tuple[Path, int]]] = {}
    for v, long_side in VARIANTS.items():
        out[v] = {}
        for p in photos:
            dest = p["path"] if long_side is None else variant_path(photo_set, p["key"], v)
            out[v][p["key"]] = (dest, make_variant(p["path"], dest, long_side))
    return out


# --- running the example ------------------------------------------------------------------------

def raw_path(photo_set: str, variant: str, threshold: float, pass_a: int | None = None) -> Path:
    """The example's output; `pass_a` names a run with pass A at another size (an extra pass)."""
    suffix = "" if pass_a is None else f"-a{pass_a}"
    return RUNS / "raw" / f"{photo_set}-{variant}-{threshold}{suffix}.jsonl"


def build() -> None:
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "vision", "--example", "face_check"],
                   cwd=BACKEND, check=True)


def run_example(paths: list[Path], threshold: float, models_dir: Path, ort: Path, out: Path,
                pass_a: int | None = None) -> None:
    env = os.environ | {"ORT_DYLIB_PATH": str(ort)}
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_suffix(".part")
    extra = [] if pass_a is None else ["--pass-a-long-side", str(pass_a)]
    with tmp.open("w") as f:
        subprocess.run([str(EXAMPLE), "--models-dir", str(models_dir), "--score-threshold", str(threshold), *extra],
                       input="".join(f"{p}\n" for p in paths), text=True, stdout=f, check=True, env=env)
    tmp.rename(out)


def cmd_run(args) -> int:
    ort = Path(args.ort) if args.ort else (VENV_ORT[-1] if VENV_ORT else None)
    if ort is None or not ort.exists():
        raise SystemExit("no ONNX Runtime library: give --ort or install onnxruntime in .venv")
    build()
    sets = {"openimages": openimages_photos, "dev": lambda: dev_photos(photos=args.eval_photos)}
    for name in args.sets:
        photos = sets[name]()
        files = variants_of(name, photos)
        for t in args.thresholds:
            for v in args.variants:
                out = raw_path(name, v, t, args.pass_a_long_side)
                if out.exists() and not args.force:
                    continue
                print(f"{name} {v} at {t}: {len(photos)} photos", file=sys.stderr, flush=True)
                run_example([files[v][p["key"]][0] for p in photos], t, Path(args.models_dir).expanduser(), ort, out,
                            args.pass_a_long_side)
    return 0


# --- scoring ------------------------------------------------------------------------------------

def read_raw(path: Path) -> dict[str, dict]:
    out = {}
    for line in path.read_text().splitlines():
        r = json.loads(line)
        out[str(Path(r["path"]).resolve())] = r
    return out


def score_photo(result: dict, faces: list[dict], size: tuple[int, int], grow: float = GROW) -> list[dict]:
    """Each annotated face of one photo: its sizes, attributes, coverage and whether caught."""
    stored = F.fit_long_side(size)
    rects = [grown(b, grow) for b in result.get("faces", [])]
    out = []
    for i, f in enumerate(faces):
        cov = covered_share(face_rect(f), rects)
        out.append({"face": i, "stored_px": round(F.face_width(f, stored), 1),
                    "full_px": round(F.face_width(f, size), 1), "occluded": f["occluded"],
                    "truncated": f["truncated"], "covered": round(cov, 3), "caught": cov >= COVERED})
    return out


def recall(faces: list[dict]) -> dict:
    n = len(faces)
    caught = sum(f["caught"] for f in faces)
    return {"faces": n, "caught": caught, "recall": round(caught / n, 4) if n else None}


def summary(values: list[float], digits: int = 1) -> dict:
    if not values:
        return {}
    v = sorted(values)
    q = lambda p: v[min(len(v) - 1, int(p * len(v)))]  # noqa: E731
    return {"n": len(v), "median": round(statistics.median(v), digits), "mean": round(statistics.fmean(v), digits),
            "p10": round(q(0.1), digits), "p90": round(q(0.9), digits), "max": round(v[-1], digits)}


def crop_sheets(items: list[tuple[Image.Image, str]], prefix: Path, side: int = 200, cols: int = 8,
                per_page: int = 48) -> None:
    """Contact sheets of the crops, `per_page` to a page: `<prefix>-01.jpg`, `-02`, ..."""
    for page, start in enumerate(range(0, len(items), per_page), 1):
        chunk = items[start:start + per_page]
        rows = (len(chunk) + cols - 1) // cols
        sheet = Image.new("RGB", (cols * side, rows * (side + 28)), "white")
        d = ImageDraw.Draw(sheet)
        for i, (im, label) in enumerate(chunk):
            im = im.copy()
            im.thumbnail((side, side))
            x, y = (i % cols) * side, (i // cols) * (side + 28)
            sheet.paste(im, (x, y))
            d.text((x + 2, y + side + 2), label[:32], fill="black")
            d.text((x + 2, y + side + 14), label[32:64], fill="black")
        prefix.parent.mkdir(parents=True, exist_ok=True)
        sheet.save(f"{prefix}-{page:02}.jpg", quality=90)


def save_crop(im: Image.Image, name: str) -> None:
    CROPS.mkdir(parents=True, exist_ok=True)
    im.save(CROPS / name, quality=90)


def crop_around(path: Path, rect: tuple[float, float, float, float], boxes: list[tuple[tuple[float, float, float, float], str]],
                pad: float = 1.5, min_px: int = 96) -> Image.Image:
    """A crop of the photo around `rect` (normalised), with `boxes` drawn in their colours."""
    im = Image.open(path).convert("RGB")
    w, h = im.size
    x0, y0, x1, y1 = rect[0] * w, rect[1] * h, rect[2] * w, rect[3] * h
    half = max((x1 - x0), (y1 - y0), min_px / pad) * pad / 2
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    box = (int(max(0, cx - half)), int(max(0, cy - half)), int(min(w, cx + half)), int(min(h, cy + half)))
    crop = im.crop(box)
    d = ImageDraw.Draw(crop)
    lw = max(2, int((box[2] - box[0]) / 150))
    for (a, b, c, e), colour in boxes:
        d.rectangle([a * w - box[0], b * h - box[1], c * w - box[0], e * h - box[1]], outline=colour, width=lw)
    return crop


def rounded(box: dict) -> dict:
    return {k: round(v, 4) for k, v in box.items()}


def stored_width(box: dict, size: tuple[int, int]) -> float:
    """A detected box's width in pixels of the stored image of a photo of `size`."""
    return box["w"] * F.fit_long_side(size)[0]


def min_box_tradeoff(oi_results: list[tuple[dict, dict]], gt: dict[str, list[dict]],
                     dev_results: list[dict]) -> dict[str, dict]:
    """Recall and the dev photos' false-blur share if boxes narrower than m px in the stored image
    were dropped after merging, for each m of MIN_BOX. An estimate for a rule `api` doesn't have."""
    out = {}
    for m in MIN_BOX:
        faces = []
        for p, r in oi_results:
            size = (p["width"], p["height"])
            kept = {"faces": [b for b in r["faces"] if stored_width(b, size) >= m]}
            faces += [f for f in score_photo(kept, gt[p["key"]], size) if f["stored_px"] >= MIN_FACE]
        hit = sum(any(stored_width(b, (r["width"], r["height"])) >= m for b in r["faces"]) for r in dev_results)
        out[str(m)] = {"recall": recall(faces), "dev_with_a_box": hit,
                       "dev_share": round(hit / len(dev_results), 4) if dev_results else None}
    return out


def proposals(oi: list[dict], gt: dict[str, list[dict]], dev: list[dict], files: dict) -> list[dict]:
    """Recall and false-blur cost of the changes the report weighs, on the original uploads at the
    shipped threshold: more growth, and an extra whole-image pass at another size (the union of
    the shipped run and a run with pass A at that size, merged as `api` merges)."""
    def results(name, photos, pass_a=None):
        raw = read_raw(raw_path(name, "original", SHIPPED, pass_a))
        return {p["key"]: raw[str(files[name]["original"][p["key"]][0].resolve())] for p in photos}

    base_oi, base_dev = results("openimages", oi), results("dev", dev)
    setups = [("shipped", None)]
    setups += [(f"extra pass at {n} px", n) for n in sorted(
        int(p.stem.rsplit("-a", 1)[1]) for p in (RUNS / "raw").glob(f"openimages-original-{SHIPPED}-a*.jsonl"))]
    out = []
    for label, n in setups:
        if n is None:
            boxes_oi = {k: r["faces"] for k, r in base_oi.items()}
            boxes_dev = {k: r["faces"] for k, r in base_dev.items()}
        else:
            extra_oi, extra_dev = results("openimages", oi, n), results("dev", dev, n)
            boxes_oi = {k: union_merge(r["faces"] + extra_oi[k]["faces"]) for k, r in base_oi.items()}
            boxes_dev = {k: union_merge(r["faces"] + extra_dev[k]["faces"]) for k, r in base_dev.items()}
        for g in GROWS:
            faces = [f | {"photo": p["key"]} for p in oi
                     for f in score_photo({"faces": boxes_oi[p["key"]]}, gt[p["key"]], (p["width"], p["height"]), g)
                     if f["stored_px"] >= MIN_FACE]
            shares = [blurred_share(b, g) for b in boxes_dev.values()]
            out.append({"setup": label, "grow": g, "recall": recall(faces),
                        "dev_with_a_box": sum(bool(b) for b in boxes_dev.values()), "dev_photos": len(boxes_dev),
                        "dev_blurred_share_mean": round(statistics.fmean(shares), 4),
                        "dev_blurred_share_max": round(max(shares), 4),
                        "missed": [f"{f['photo']}#{f['face']}" for f in faces if not f["caught"]]})
    return out


def cmd_score(args) -> int:
    date = args.date or dt.date.today().isoformat()
    out_name = args.out or f"faces-{date}"
    oi = openimages_photos()
    dev = dev_photos(photos=args.eval_photos)
    by_split: dict[str, set[str]] = defaultdict(set)
    for p in oi:
        by_split[p["split"]].add(p["key"])
    gt: dict[str, list[dict]] = {}
    for split, ids in by_split.items():
        gt |= F.read_faces(F.TABLES / F.SPLITS[split][0], ids)
    files = {"openimages": variants_of("openimages", oi), "dev": variants_of("dev", dev)}
    thresholds = sorted({float(p.stem.split("-")[2]) for p in (RUNS / "raw").glob("openimages-*.jsonl")
                         if len(p.stem.split("-")) == 3})
    crops = CROPS
    crop_at = set(args.crop_thresholds)
    for old in crops.glob("*.jpg"):
        old.unlink()
    report: dict = {"date": date, "cpu": cpu_name(), "photos": len(oi), "faces": sum(len(gt[p["key"]]) for p in oi),
                    "dev_photos": len(dev), "thresholds": thresholds, "variants": list(VARIANTS),
                    "selection": json.loads((F.FACES / "selection.json").read_text())
                    if (F.FACES / "selection.json").exists() else None,
                    "recall": {}, "unmatched_boxes": {}, "dev_false_faces": {}, "min_box": {}, "missed": [],
                    "quality": {},
                    "cost": {}, "per_photo": {}}
    sheets: dict[str, list[tuple[Image.Image, str]]] = defaultdict(list)

    for t in thresholds:
        for v in VARIANTS:
            raw = read_raw(raw_path("openimages", v, t))
            scored, unmatched, oi_results = [], [], []
            for p in oi:
                path, _ = files["openimages"][v][p["key"]]
                r = raw[str(path.resolve())]
                if "error" in r:
                    raise SystemExit(f"{p['key']} {v}: {r['error']}")
                fs = score_photo(r, gt[p["key"]], (p["width"], p["height"]))
                oi_results.append((p, r))
                for f in fs:
                    f["photo"] = p["key"]
                scored += fs
                if t == SHIPPED:
                    report["per_photo"].setdefault(p["key"], {})[v] = {
                        "boxes": [rounded(b) for b in r["faces"]], "covered": [f["covered"] for f in fs]}
                for k, b in enumerate(r["faces"]):
                    if not matches_a_face(b, gt[p["key"]]):
                        unmatched.append({"photo": p["key"], "box": k, "score": round(b["score"], 3),
                                          "w_stored_px": round(b["w"] * F.fit_long_side((p["width"], p["height"]))[0], 1)})
                        if t in crop_at:
                            rect = (b["x"], b["y"], b["x"] + b["w"], b["y"] + b["h"])
                            im = crop_around(path, rect, [(rect, "blue")])
                            save_crop(im, f"unmatched-{v}-{t}-{p['key']}-{k}.jpg")
                            sheets[f"unmatched-{v}-{t}"].append((im, f"{p['key']} {b['score']:.2f}"))
                for f in fs:
                    if f["stored_px"] >= MIN_FACE and not f["caught"]:
                        report["missed"].append({"variant": v, "threshold": t, **{k: f[k] for k in (
                            "photo", "face", "stored_px", "full_px", "occluded", "truncated", "covered")}})
                        if t in crop_at:
                            face = gt[p["key"]][f["face"]]
                            rects = [(grown(b), "blue") for b in r["faces"]]
                            im = crop_around(path, face_rect(face), [(face_rect(face), "red")] + rects)
                            save_crop(im, f"missed-{v}-{t}-{p['key']}-{f['face']}.jpg")
                            sheets[f"missed-{v}-{t}"].append(
                                (im, f"{p['key']}#{f['face']} {f['stored_px']:.0f}px cov {f['covered']:.2f}"))
            counted = [f for f in scored if f["stored_px"] >= MIN_FACE]
            key = f"{v}@{t}"
            report["recall"][key] = {
                "all": recall(counted),
                "by_stored_size": {b: recall([f for f in scored if size_bucket(f["stored_px"]) == b])
                                   for b in SIZE_NAMES},
                "under_40_full_all": recall([f for f in scored if f["full_px"] < 40]),
                "under_40_full_counted": recall([f for f in counted if f["full_px"] < 40]),
                "stored_20_40": recall([f for f in counted if f["stored_px"] < 40]),
                "occluded": recall([f for f in counted if f["occluded"] == 1]),
                "truncated": recall([f for f in counted if f["truncated"] == 1]),
                "every_face": recall(scored),
            }
            report["unmatched_boxes"][key] = {"boxes": len(unmatched), "photos": len({u["photo"] for u in unmatched})}
            if t == SHIPPED:
                report["unmatched_boxes"][key]["list"] = unmatched

            # the dev plant photos: any box at all is a blur over the plant photo
            raw_dev = read_raw(raw_path("dev", v, t))
            hits, dev_results = [], []
            for p in dev:
                path, _ = files["dev"][v][p["key"]]
                r = raw_dev[str(path.resolve())]
                if "error" in r:
                    raise SystemExit(f"{p['key']} {v}: {r['error']}")
                dev_results.append(r)
                if r["faces"]:
                    hits.append({"photo": p["key"], "boxes": [rounded(b) for b in r["faces"]]})
                    for k, b in enumerate(r["faces"] if t in crop_at and v == "original" else []):
                        rect = (b["x"], b["y"], b["x"] + b["w"], b["y"] + b["h"])
                        im = crop_around(path, rect, [(rect, "blue"), (grown(b), "orange")], pad=2.5)
                        save_crop(im, f"dev-{v}-{t}-{p['key'].replace('#', '_')}-{k}.jpg")
                        sheets[f"dev-{v}-{t}"].append((im, f"{p['key']} {b['score']:.2f}"))
            report["dev_false_faces"][key] = {"photos": len(dev), "with_a_box": len(hits),
                                              "share": round(len(hits) / len(dev), 4),
                                              "list": hits if t == SHIPPED else [{"photo": h["photo"]} for h in hits]}
            report["min_box"][key] = min_box_tradeoff(oi_results, gt, dev_results)

    report["proposals"] = proposals(oi, gt, dev, files)

    # quality-score drift and cost per size, at the shipped threshold
    for name, photos in (("openimages", oi), ("dev", dev)):
        raws = {v: read_raw(raw_path(name, v, SHIPPED)) for v in VARIANTS}
        res = {v: {p["key"]: raws[v][str(files[name][v][p["key"]][0].resolve())] for p in photos} for v in VARIANTS}
        drift = {}
        for v in VARIANTS:
            if v == "original":
                continue
            resized = [p for p in photos if max(res["original"][p["key"]]["width"], res["original"][p["key"]]["height"]) > int(v)]
            ratio = [res[v][p["key"]]["blur_score"] / res["original"][p["key"]]["blur_score"] for p in resized]
            dexp = [abs(res[v][p["key"]]["exposure"] - res["original"][p["key"]]["exposure"]) for p in resized]
            flips = [p["key"] for p in photos
                     if (res[v][p["key"]]["blur_score"] < MIN_BLUR) != (res["original"][p["key"]]["blur_score"] < MIN_BLUR)]
            drift[v] = {"resized_photos": len(resized), "blur_ratio": summary(ratio, 3),
                        "exposure_abs_diff": summary(dexp, 4), "blur_gate_changes": flips}
        report["quality"][name] = {
            "blur_score": {v: summary([r["blur_score"] for r in res[v].values()], 1) for v in VARIANTS},
            "under_min_blur": {v: sum(r["blur_score"] < MIN_BLUR for r in res[v].values()) for v in VARIANTS},
            "drift_from_original": drift,
        }
        cost = {}
        for v in VARIANTS:
            up = [files[name][v][p["key"]][1] / 1e6 for p in photos]
            cost[v] = {
                "upload_mb": summary(up, 2),
                "stored_webp_kb": summary([r["stored"]["bytes"] / 1e3 for r in res[v].values()], 0),
                "megapixels": summary([r["width"] * r["height"] / 1e6 for r in res[v].values()], 1),
                **{f"{k}_ms": summary([r["ms"][k] for r in res[v].values()], 0)
                   for k in ("decode", "quality", "detect", "store", "total")},
            }
        if name == "openimages":
            cost["flickr_original_mb"] = summary([p["bytes"] / 1e6 for p in photos], 2)
        report["cost"][name] = cost

    for name, items in sheets.items():
        crop_sheets(items, crops / f"sheet-{name}")
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{out_name}.json").write_text(json.dumps(report, indent=1) + "\n")
    notes = Path(args.notes).read_text() if args.notes else ""
    (REPORTS / f"{out_name}.md").write_text(markdown(report, out_name, notes))
    print(f"wrote {REPORTS / out_name}.md and .json; crops and sheets in {crops}")
    return 0


def cpu_name() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return f"{line.split(':', 1)[1].strip()}, {os.cpu_count()} threads"
    except OSError:
        pass
    return "unknown"


def pct(r: dict) -> str:
    return "–" if not r["faces"] else f"{100 * r['recall']:.1f}% ({r['caught']}/{r['faces']})"


def frac(r: dict) -> str:
    return "–" if not r["faces"] else f"{r['caught']}/{r['faces']}"


def spread(d: dict, unit: str = "", digits: int = 0) -> str:
    if not d:
        return "–"
    f = f"{{:,.{digits}f}}"
    return f"{f.format(d['median'])}{unit} ({f.format(d['p10'])}–{f.format(d['p90'])})"


def markdown(r: dict, name: str, notes: str = "") -> str:
    """The report's tables; `notes` (what the crops show, written after looking at them) goes
    after the headline."""
    shipped = [f"{v}@{SHIPPED}" for v in r["variants"]]
    head = ", ".join(f"{v} {pct(r['recall'][k]['all'])}" for v, k in zip(r["variants"], shipped))
    dev_head = ", ".join(f"{v} {r['dev_false_faces'][k]['with_a_box']}/{r['dev_false_faces'][k]['photos']}"
                         for v, k in zip(r["variants"], shipped))
    out = [
        f"# Face check: {name}", "",
        f"{r['photos']} Open Images V7 photos (validation and test splits) of people near plants, with "
        f"{r['faces']} annotated faces (`ml/training/data/splits/faces_openimages.tsv`), and the "
        f"{r['dev_photos']} photos of the dev cases (`ml/training/eval/manifest.jsonl`). Each went through "
        "`vision`'s `face_check` example: `decode_upload`, the quality gate, then `FaceDetector::detect` "
        "(YuNet 2026may on the CPU, the whole image at 1,280 px and 1,280 px tiles of the full image at "
        "stride 960) with `FaceConfig::default()` but the threshold, as three uploads: the photo as it is "
        "(`original`), and at most 3,200 and 2,048 px on the long side (Lanczos, JPEG quality 85).",
        "",
        f"A face counts when it is {MIN_FACE} px or more across (its box's width) in the stored image "
        f"(1,600 px long side). It is caught when the detected boxes, grown {GROW:.0%} on each side as "
        f"`api` grows them, cover {COVERED:.0%} or more of its box ([evaluation 4](../../../../docs/vision/"
        "evaluation.md#upload-and-privacy-control-plane)). Target: 98%.", "",
        f"**At the shipped threshold {SHIPPED}: {head}. Dev plant photos with a face box: {dev_head}.**", "",
    ]
    if notes.strip():
        out += [notes.strip(), ""]
    out += ["## Recall", "",
            "| Upload | Threshold | Faces 20 px+ stored | 20–40 px stored | Under 40 px at full size (any size stored) "
            "| Occluded | Truncated | Every face, any size |",
            "|---|---|---|---|---|---|---|---|"]
    for t in r["thresholds"]:
        for v in r["variants"]:
            x = r["recall"][f"{v}@{t}"]
            out.append(f"| {v} | {t} | {pct(x['all'])} | {pct(x['stored_20_40'])} | {pct(x['under_40_full_all'])} "
                       f"| {pct(x['occluded'])} | {pct(x['truncated'])} | {pct(x['every_face'])} |")
    out += ["", "## Caught by face size in the stored image", "",
            "Faces under 20 px don't count toward the target; they are shown for issue #41.", "",
            "| Upload | Threshold | " + " | ".join(SIZE_NAMES) + " |",
            "|---|---|" + "---|" * len(SIZE_NAMES)]
    for t in r["thresholds"]:
        for v in r["variants"]:
            x = r["recall"][f"{v}@{t}"]["by_stored_size"]
            out.append(f"| {v} | {t} | " + " | ".join(frac(x[b]) for b in SIZE_NAMES) + " |")
    missed = [m for m in r["missed"] if m["threshold"] == SHIPPED]
    out += ["", f"## Faces missed at {SHIPPED}", ""]
    if missed:
        out += ["| Upload | Photo | Face | px stored | px full size | Covered | Occluded | Truncated |",
                "|---|---|---|---|---|---|---|---|"]
        out += [f"| {m['variant']} | `{m['photo']}` | {m['face']} | {m['stored_px']:.0f} | {m['full_px']:.0f} "
                f"| {m['covered']:.0%} | {m['occluded']} | {m['truncated']} |" for m in missed]
    else:
        out.append("None.")
    out += ["", "## Boxes on no annotated face (Open Images photos)", "",
            "A box counts here when its centre is outside every face's box grown 25% and it holds less than a "
            "third of any face's box.", "",
            "| Upload | Threshold | Boxes | Photos |", "|---|---|---|---|"]
    for t in r["thresholds"]:
        for v in r["variants"]:
            x = r["unmatched_boxes"][f"{v}@{t}"]
            out.append(f"| {v} | {t} | {x['boxes']} | {x['photos']} |")
    out += ["", "## Dev plant photos with a face box", "",
            "Every box blurs part of a plant photo (target: 2% of photos or less).", "",
            "| Upload | Threshold | Photos with a box | Share | Photos |", "|---|---|---|---|---|"]
    for t in r["thresholds"]:
        for v in r["variants"]:
            x = r["dev_false_faces"][f"{v}@{t}"]
            names = (", ".join(f"`{h['photo']}`" for h in x["list"]) or "–") if len(x["list"]) <= 8 else "in the JSON"
            out.append(f"| {v} | {t} | {x['with_a_box']}/{x['photos']} | {100 * x['share']:.1f}% | {names} |")
    out += ["", "## Threshold and a minimum box size", "",
            "An estimate for a rule `api` doesn't have: drop merged boxes narrower than m px in the stored image "
            "(1,600 px long side) before blurring. Each cell: recall of faces 20 px+ stored / dev plant photos with a "
            "box left. Original uploads; the JSON has every size.", "",
            "| Upload | Threshold | " + " | ".join(f"m = {m}" for m in MIN_BOX) + " |",
            "|---|---|" + "---|" * len(MIN_BOX)]
    for v in r["variants"][:1]:
        for t in r["thresholds"]:
            x = r["min_box"][f"{v}@{t}"]
            out.append(f"| {v} | {t} | " + " | ".join(
                f"{100 * x[str(m)]['recall']['recall']:.1f}% / {100 * x[str(m)]['dev_share']:.1f}%" for m in MIN_BOX) + " |")
    if r.get("proposals"):
        out += ["", f"## Changes weighed, original uploads at {SHIPPED}", "",
                "Growth is `FaceConfig::grow`. An extra pass is the union of the shipped run and a run with pass A "
                "at that size, merged as `api` merges: a third, whole-image pass `api` doesn't have. Blurred area is "
                "the share of each dev plant photo inside the grown boxes, averaged over all of them.", "",
                "| Setup | Growth | Recall, faces 20 px+ stored | Dev photos with a box | Dev area blurred, mean (max) "
                "| Still missed |", "|---|---|---|---|---|---|"]
        for x in r["proposals"]:
            out.append(f"| {x['setup']} | {x['grow']:.0%} | {pct(x['recall'])} | {x['dev_with_a_box']}/{x['dev_photos']} "
                       f"| {100 * x['dev_blurred_share_mean']:.1f}% ({100 * x['dev_blurred_share_max']:.0f}%) "
                       f"| {len(x['missed'])} |")
    out += ["", "## Quality scores across upload sizes (threshold irrelevant)", "",
            "The gate scores a 512 px downscale. Ratio and difference against the `original` upload, on the photos "
            "the size actually resized; a gate change is a photo on the other side of `min_blur_score` "
            f"{MIN_BLUR:.0f} than its original.", "",
            "| Photos | Upload | Resized | Blur score median (p10–p90) | Blur ratio to original, median (p10–p90) "
            "| Exposure difference, median (max) | Under 40 | Gate changes |",
            "|---|---|---|---|---|---|---|---|"]
    for name_, q in r["quality"].items():
        for v in r["variants"]:
            d = q["drift_from_original"].get(v)
            ratio = "–" if not d or not d["blur_ratio"] else spread(d["blur_ratio"], "", 3)
            dexp = "–" if not d or not d["exposure_abs_diff"] else \
                f"{d['exposure_abs_diff']['median']:.4f} ({d['exposure_abs_diff']['max']:.4f})"
            out.append(f"| {name_} | {v} | {d['resized_photos'] if d else '–'} | {spread(q['blur_score'][v])} | {ratio} "
                       f"| {dexp} | {q['under_min_blur'][v]} | {len(d['blur_gate_changes']) if d else '–'} |")
    out += ["", "## Cost per upload", "",
            f"On this machine ({r['cpu']}), ONNX Runtime with 2 intra-op threads as `api` runs it, one upload at a "
            f"time, at threshold {SHIPPED}. Not the control plane's VM. Store is the blur, the resize to 1,600 px "
            "and the WebP encode. Medians, with p10–p90.", "",
            "| Photos | Upload | Upload MB | Megapixels | Decode ms | Gate ms | Detect ms | Store ms | Total ms "
            "| Stored WebP KB |", "|---|---|---|---|---|---|---|---|---|---|"]
    for name_, c in r["cost"].items():
        for v in r["variants"]:
            x = c[v]
            out.append(f"| {name_} | {v} | {spread(x['upload_mb'], '', 2)} | {spread(x['megapixels'], '', 1)} "
                       f"| {spread(x['decode_ms'])} | {spread(x['quality_ms'])} | {spread(x['detect_ms'])} "
                       f"| {spread(x['store_ms'])} | {spread(x['total_ms'])} | {spread(x['stored_webp_kb'])} |")
        if "flickr_original_mb" in c:
            out.append(f"| {name_} | Flickr original, before `faces.py` re-encoded it at quality 95 "
                       f"| {spread(c['flickr_original_mb'], '', 2)} | | | | | | | |")
    sel = r.get("selection")
    if sel:
        out += ["", "## Selection", "",
                f"{sel['candidates']} candidates (a face box, a verified plant label, no group or depiction face "
                f"box, CC BY 2.0, an original of 400 KB or more). Tried in a fixed order per bucket of the smallest "
                f"face's width in the stored image, up to 20 per bucket: {sel['tried']} tried, {sel['kept']} kept.", "",
                "| Smallest face | Candidates | Kept | Original gone | Under 2,000 px | Not downloaded | Other |",
                "|---|---|---|---|---|---|---|"]
        named = ("kept", "original gone", "long side under 2000 px", "not downloaded")
        for b, n in sel["candidates_per_bucket"].items():
            st = sel["tried_per_bucket"].get(b, {})
            other = sum(k for s_, k in st.items() if s_ not in named)
            out.append(f"| {b} | {n} | " + " | ".join(str(st.get(s_, 0)) for s_ in named) + f" | {other} |")
        left = "; ".join(f"{k}: {v:,}" for k, v in sel["left_out"].items())
        out += ["", f"Left out before downloading: {left}."]
    return "\n".join(out) + "\n"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="build the example and run it on every variant and threshold")
    r.add_argument("--models-dir", default="~/.cache/pdm-models", help="the api set of ml.models.fetch")
    r.add_argument("--ort", help="libonnxruntime (default: the venv's onnxruntime)")
    r.add_argument("--thresholds", type=float, nargs="+", default=list(THRESHOLDS))
    r.add_argument("--force", action="store_true", help="run again over existing raw files")
    r.add_argument("--sets", nargs="+", choices=SETS, default=list(SETS))
    r.add_argument("--variants", nargs="+", choices=list(VARIANTS), default=list(VARIANTS))
    r.add_argument("--pass-a-long-side", type=int,
                   help="run with pass A at this size instead (FaceConfig::pass_a_long_side), to score an extra "
                        "whole-image pass; written as <set>-<variant>-<threshold>-a<size>.jsonl")
    s = sub.add_parser("score", help="score the raw files and write the report")
    s.add_argument("--date")
    s.add_argument("--out")
    s.add_argument("--notes", help="a Markdown file of what the crops show, put after the headline")
    s.add_argument("--crop-thresholds", type=float, nargs="+", default=[SHIPPED],
                   help="thresholds whose missed faces and false boxes are cropped (dev photos: the original only)")
    for p in (r, s):
        p.add_argument("--eval-photos", type=Path, help="the evaluation photos (default: ml/training/eval/photos)")
    args = ap.parse_args(argv)
    return cmd_run(args) if args.cmd == "run" else cmd_score(args)


if __name__ == "__main__":
    sys.exit(main())
