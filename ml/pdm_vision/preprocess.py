"""Image → tensor, exactly as the Rust worker does it (backend vision.md §6, `to_tensor`).

Training, export checks, calibration and evaluation all go through this module, so the heads
and thresholds see the same pixels the perception worker sends (models.md §1). A parity test
against the Rust worker on 10 images keeps the two in step (evaluation.md §9).

Mirrors of the Rust code:
- `imaging::resize`: Lanczos3 to an exact size (fast_image_resize). Pillow's LANCZOS is the
  same a=3 kernel; the parity test measures the remaining difference.
- `fit_long_side`: long side to at most N, never upscaling, `f64::round` on each side.
- `ResizeRule::{Squash, ShortestSideCrop, LongestSidePad}`.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from enum import Enum

import numpy as np
from PIL import Image, ImageOps

# CLIP normalisation (BioCLIP 2) and ImageNet normalisation (RF-DETR, SAM 2.1), models.md §3–6.
CLIP_MEAN = (0.48145466, 0.4578275, 0.40821073)
CLIP_STD = (0.26862954, 0.26130258, 0.27577711)
IMAGENET_MEAN = (0.485, 0.456, 0.406)
IMAGENET_STD = (0.229, 0.224, 0.225)


class Resize(str, Enum):
    """`ResizeRule` in the manifest (`[model.input] resize`)."""

    SQUASH = "squash"
    SHORTEST_SIDE_CROP = "shortest_side_crop"
    LONGEST_SIDE_PAD = "longest_side_pad"


@dataclass(frozen=True)
class Recipe:
    """`[model.input]` of a manifest entry."""

    side: int
    resize: Resize
    mean: tuple[float, float, float] = (0.0, 0.0, 0.0)
    std: tuple[float, float, float] = (1.0, 1.0, 1.0)
    bgr: bool = False


BIOCLIP = Recipe(224, Resize.SHORTEST_SIDE_CROP, CLIP_MEAN, CLIP_STD)
RFDETR = Recipe(512, Resize.SQUASH, IMAGENET_MEAN, IMAGENET_STD)
SAM2 = Recipe(1024, Resize.SQUASH, IMAGENET_MEAN, IMAGENET_STD)


def rust_round(x: float) -> int:
    """`f64::round`: half away from zero (Python's `round` is half to even)."""
    return int(math.floor(x + 0.5)) if x >= 0 else -int(math.floor(-x + 0.5))


def load_rgb(path) -> Image.Image:
    """Decode with the EXIF orientation applied, as the API does before storing (contracts 1)."""
    with Image.open(path) as im:
        return ImageOps.exif_transpose(im).convert("RGB")


def resize(im: Image.Image, width: int, height: int) -> Image.Image:
    """`imaging::resize`: Lanczos3 to exactly width × height."""
    if im.size == (width, height):
        return im.copy()
    return im.resize((width, height), Image.Resampling.LANCZOS)


def fit_long_side(size: tuple[int, int], long_side: int) -> tuple[int, int]:
    """`imaging::fit_long_side`: never upscales."""
    w, h = size
    long = max(w, h)
    if long <= long_side or long == 0:
        return w, h
    scale = long_side / long
    return max(rust_round(w * scale), 1), max(rust_round(h * scale), 1)


def square(im: Image.Image, recipe: Recipe) -> Image.Image:
    """The resize rule, giving a `side × side` RGB image."""
    side = recipe.side
    if recipe.resize is Resize.SQUASH:
        return resize(im, side, side)
    if recipe.resize is Resize.SHORTEST_SIDE_CROP:
        w, h = im.size
        scale = side / max(min(w, h), 1)
        rw = max(rust_round(w * scale), side)
        rh = max(rust_round(h * scale), side)
        r = resize(im, rw, rh)
        x, y = (rw - side) // 2, (rh - side) // 2
        return r.crop((x, y, x + side, y + side))
    if recipe.resize is Resize.LONGEST_SIDE_PAD:
        fitted = resize(im, *fit_long_side(im.size, side))
        padded = Image.new("RGB", (side, side))
        padded.paste(fitted, (0, 0))
        return padded
    raise ValueError(recipe.resize)


def to_tensor(im: Image.Image, recipe: Recipe) -> np.ndarray:
    """`[3, side, side]` float32, normalised by the recipe (Rust `to_tensor`)."""
    arr = np.asarray(square(im, recipe), dtype=np.float32) / 255.0
    if recipe.bgr:
        arr = arr[..., ::-1]
    arr = (arr - np.asarray(recipe.mean, np.float32)) / np.asarray(recipe.std, np.float32)
    return np.ascontiguousarray(arr.transpose(2, 0, 1), dtype=np.float32)


def batch(images: list[Image.Image], recipe: Recipe) -> np.ndarray:
    """`[n, 3, side, side]`, the layout every exported model takes."""
    return np.stack([to_tensor(im, recipe) for im in images])


def crop_box(im: Image.Image, bbox: tuple[float, float, float, float], grow: float = 0.10) -> Image.Image:
    """A leaf crop for the disease head: a normalised `[x, y, w, h]` box grown by `grow` on
    each side and clamped to the image (models.md §7, `crop_grow`)."""
    W, H = im.size
    x, y, w, h = bbox
    dx, dy = w * grow, h * grow
    x0, y0 = max(x - dx, 0.0), max(y - dy, 0.0)
    x1, y1 = min(x + w + dx, 1.0), min(y + h + dy, 1.0)
    box = (rust_round(x0 * W), rust_round(y0 * H), max(rust_round(x1 * W), rust_round(x0 * W) + 1),
           max(rust_round(y1 * H), rust_round(y0 * H) + 1))
    return im.crop(box)
