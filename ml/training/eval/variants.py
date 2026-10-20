"""Phone-condition variants of the evaluation cases (evaluation.md §3, "Variants").

    python -m ml.training.eval.variants [--transforms tilt,dim,...] [--manifest PATH] [--out PATH]

Published photos are sharp, well lit and centred; the app's often won't be. A variant asks whether
a case is still answered right when its photos are taken the way users take them: tilted, in dim
light, under a colour cast, slightly out of focus, from further away, recompressed by a messaging
app, or with the problem off to one side.

A variant is not a new case. It has its original's plant, truth and split, so it goes in its own
file, manifest.variants.jsonl, and the harness scores each transform as a slice beside the
originals, never pooled with them. Only evaluation cases get variants: a rotated or recoloured
training photo is still that photo to a model (training.md §4 counts augmented copies as
duplicates), so it would measure memory.

Only `diagnosed` and `healthy` cases get variants, and a variant keeps the outcome, species,
problems, soil and indicators of its original's truth. Boxes, masks, depth and measurements are
dropped, since tilting and cropping move them. Each transform is mild enough that a person can
still see the problem, and is seeded by the case and the transform, so a rerun writes the same
bytes. Photos go to photos/variants/<transform>/<the original's path>, and --out is rewritten.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

from ml.training.eval import inat


def _rng(case_id: str, name: str) -> np.random.Generator:
    return np.random.default_rng(int(hashlib.sha256(f"variant|{case_id}|{name}".encode()).hexdigest()[:16], 16))


def _float(im: Image.Image) -> np.ndarray:
    return np.asarray(im.convert("RGB"), np.float32) / 255.0


def _image(a: np.ndarray) -> Image.Image:
    return Image.fromarray((np.clip(a, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8))


def largest_upright_rect(w: int, h: int, angle: float) -> tuple[float, float]:
    """The largest axis-aligned rectangle inside a w x h image rotated by `angle` radians."""
    a = abs(angle) % math.pi
    a = math.pi - a if a > math.pi / 2 else a
    sin_a, cos_a = math.sin(a), math.cos(a)
    long_side, short_side = max(w, h), min(w, h)
    if short_side <= 2 * sin_a * cos_a * long_side or abs(sin_a - cos_a) < 1e-10:
        x = short_side / 2
        return (x / sin_a, x / cos_a) if w >= h else (x / cos_a, x / sin_a)
    cos_2a = cos_a * cos_a - sin_a * sin_a
    return (w * cos_a - h * sin_a) / cos_2a, (h * cos_a - w * sin_a) / cos_2a


def tilt(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """Rotated 8-20 degrees either way, cropped to the rotated photo so no corner is blank."""
    deg = float(rng.uniform(8, 20) * rng.choice([-1, 1]))
    rot = im.rotate(deg, resample=Image.Resampling.BICUBIC, expand=True)
    cw, ch = largest_upright_rect(im.width, im.height, math.radians(deg))
    cx, cy = rot.width / 2, rot.height / 2
    return rot.crop((math.ceil(cx - cw / 2) + 1, math.ceil(cy - ch / 2) + 1,
                     math.floor(cx + cw / 2) - 1, math.floor(cy + ch / 2) - 1))


def dim(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """About 1-1.3 stops under-exposed, with the sensor noise a phone adds in low light."""
    a = _float(im) * rng.uniform(0.40, 0.55)
    return _image(a + rng.normal(0, rng.uniform(0.012, 0.025), a.shape))


def warm(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """An evening-sun or incandescent cast."""
    return _image(_float(im) * np.array([rng.uniform(1.10, 1.18), 1.0, rng.uniform(0.76, 0.86)], np.float32))


def cool(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """The blue cast of open shade or an overcast day."""
    return _image(_float(im) * np.array([rng.uniform(0.80, 0.88), 1.0, rng.uniform(1.10, 1.18)], np.float32))


def blur(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """Slightly out of focus: a Gaussian blur of 0.15-0.25% of the long side (0.3% hid rust pustules)."""
    return im.filter(ImageFilter.GaussianBlur(max(im.size) * rng.uniform(0.0015, 0.0025)))


def far(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """Taken from further away or zoomed: a quarter to a third of the pixels on the subject."""
    s = rng.uniform(0.25, 0.35)
    small = im.resize((max(1, round(im.width * s)), max(1, round(im.height * s))), Image.Resampling.BILINEAR)
    return small.resize(im.size, Image.Resampling.BILINEAR)


def jpeg(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """Sent through a messaging app: 1280 px on the long side at JPEG quality 30-40."""
    scale = min(1.0, 1280 / max(im.size))
    small = im.resize((round(im.width * scale), round(im.height * scale)), Image.Resampling.BILINEAR)
    buf = io.BytesIO()
    small.convert("RGB").save(buf, "JPEG", quality=int(rng.integers(30, 41)))
    return Image.open(io.BytesIO(buf.getvalue())).convert("RGB")


def offcentre(im: Image.Image, rng: np.random.Generator) -> Image.Image:
    """Framed carelessly: 60-70% of each side, pushed towards a random corner."""
    f = rng.uniform(0.60, 0.70)
    w, h = round(im.width * f), round(im.height * f)
    x = round((im.width - w) * rng.choice([rng.uniform(0, 0.2), rng.uniform(0.8, 1.0)]))
    y = round((im.height - h) * rng.choice([rng.uniform(0, 0.2), rng.uniform(0.8, 1.0)]))
    return im.crop((x, y, x + w, y + h))


TRANSFORMS = {f.__name__: f for f in (tilt, dim, warm, cool, blur, far, jpeg, offcentre)}
OUTCOMES = ("diagnosed", "healthy")
KEPT_TRUTH = ("outcome", "species_id", "problems", "soil", "indicators")


def variant(c: dict, name: str, photos: Path) -> dict:
    """The variant case of `c` under transform `name`, its photos written under photos/variants."""
    if "derived_from" in c:
        raise ValueError(f"{c['case_id']} is itself a variant")
    images = []
    for i, img in enumerate(c["images"]):
        rel = f"variants/{name}/{img['file']}"
        dest = photos / rel
        with Image.open(photos / img["file"]) as im:
            out = TRANSFORMS[name](im.convert("RGB"), _rng(f"{c['case_id']}|{i}", name))
        dest.parent.mkdir(parents=True, exist_ok=True)
        out.save(dest, "JPEG", quality=92)
        images.append({"file": rel, "sha256": inat.sha256(dest), "kind": img["kind"]})
    truth = {k: c["truth"][k] for k in KEPT_TRUTH if k in c["truth"]}
    v = {**c, "case_id": f"{c['case_id']}~{name}", "images": images,
         "truth": {**truth, "organisms": [], "damage": []}, "derived_from": c["case_id"], "transform": name}
    if c.get("credit"):
        v["credit"] = f"{c['credit']}; {name} variant"
    return v


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--transforms", default=",".join(TRANSFORMS), help=f"comma-separated, from {', '.join(TRANSFORMS)}")
    ap.add_argument("--photos", type=Path, default=inat.PHOTOS)
    ap.add_argument("--manifest", type=Path, default=inat.MANIFEST)
    ap.add_argument("--out", type=Path, default=inat.MANIFEST.with_name("manifest.variants.jsonl"))
    args = ap.parse_args(argv)
    names = [t for t in args.transforms.split(",") if t]
    if unknown := [t for t in names if t not in TRANSFORMS]:
        raise SystemExit(f"unknown transform(s) {', '.join(unknown)}; use {', '.join(TRANSFORMS)}")
    cases = [json.loads(line) for line in args.manifest.read_text().splitlines() if line.strip()]
    cases = [c for c in cases if c["truth"]["outcome"] in OUTCOMES]
    out = [variant(c, name, args.photos) for c in cases for name in names]
    args.out.write_text("".join(json.dumps(v, ensure_ascii=False) + "\n" for v in out))
    print(f"{len(out)} variants of {len(cases)} cases ({', '.join(names)}) in {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
