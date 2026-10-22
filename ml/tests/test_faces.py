"""The Open Images face photos (ml/training/data/faces.py): selection, sizes and making the
original upright, on synthetic tables and images; no network."""

from __future__ import annotations

import base64
import hashlib
import io
import urllib.error

import numpy as np
import pytest
from PIL import Image

from ml.training.data import faces as F


def box(group=0, depiction=0, x0=0.1, x1=0.2) -> dict:
    return {"x0": x0, "x1": x1, "y0": 0.1, "y1": 0.25, "occluded": 0, "truncated": 0,
            "group": group, "depiction": depiction}


def info(size=1_000_000, licence="https://creativecommons.org/licenses/by/2.0/") -> dict:
    return {"License": licence, "OriginalSize": str(size)}


def test_select_keeps_verified_plants_with_individual_faces_only():
    faces = {"a": [box()], "b": [box(), box(group=1)], "c": [box(depiction=1)], "d": [box()], "e": [box()],
             "f": [box()], "g": [box()]}
    plants = {k: {"Plant"} for k in "abcdefg" if k != "d"}
    infos = {"a": info(), "b": info(), "c": info(), "d": info(), "e": info(licence="by-nc"), "f": info(size=10)}
    keep, why = F.select(faces, plants, infos)
    assert keep == ["a"]
    assert why == {"a group or depiction face box": 2, "no verified plant label": 1, "licence not CC BY 2.0": 1,
                   f"original under {F.MIN_BYTES:,} bytes": 1, "no image information": 1}


def test_tables_are_read_by_label_and_verified_confidence(tmp_path):
    (tmp_path / "boxes.csv").write_text(
        "ImageID,Source,LabelName,Confidence,XMin,XMax,YMin,YMax,IsOccluded,IsTruncated,IsGroupOf,IsDepiction,IsInside\n"
        "i1,xclick,/m/0dzct,1,0.1,0.2,0.3,0.5,1,0,0,0,0\n"
        "i1,xclick,/m/01g317,1,0,1,0,1,0,0,0,0,0\n"
        "i2,xclick,/m/0dzct,1,0.5,0.6,0.1,0.2,0,1,0,1,0\n")
    (tmp_path / "labels.csv").write_text(
        "ImageID,Source,LabelName,Confidence\n"
        "i1,verification,/m/05s2s,1\n"
        "i2,verification,/m/0c9ph5,0\n"
        "i2,verification,/m/01g317,1\n"
        "i3,crowdsource-verification,/m/03fp41,1.0\n")
    faces = F.read_faces(tmp_path / "boxes.csv")
    assert set(faces) == {"i1", "i2"} and len(faces["i1"]) == 1
    assert faces["i1"][0] == {"x0": 0.1, "x1": 0.2, "y0": 0.3, "y1": 0.5, "occluded": 1, "truncated": 0,
                              "group": 0, "depiction": 0}
    assert faces["i2"][0]["depiction"] == 1
    assert set(F.read_faces(tmp_path / "boxes.csv", {"i2"})) == {"i2"}
    assert F.read_plant_labels(tmp_path / "labels.csv") == {"i1": {"Plant"}, "i3": {"Houseplant"}}


def test_sizes_follow_the_stored_image():
    assert F.fit_long_side((4032, 3024)) == (1600, 1200)
    assert F.fit_long_side((1200, 900)) == (1200, 900)          # never upscaled
    assert F.stored_shape((1024, 768)) == (1600, 1200)          # a small copy gives the stored shape
    assert F.face_width(box(x0=0.1, x1=0.125), (1600, 1200)) == pytest.approx(40.0)
    assert [F.bucket_of(w) for w in (0, 19.9, 20, 79.9, 80, 5000)] == [0, 0, 1, 2, 3, 4]
    assert F.bucket_name(1) == "20-40 px" and F.bucket_name(4) == "160+ px"


def scene(w=480, h=360, seed=0) -> Image.Image:
    """A photo-like image with no symmetry: smooth noise, a gradient and an off-centre block."""
    rng = np.random.default_rng(seed)
    small = rng.random((h // 24, w // 24, 3))
    im = Image.fromarray((small * 255).astype(np.uint8)).resize((w, h), Image.Resampling.BICUBIC)
    a = np.asarray(im, dtype=np.float64)
    a[:, :, 0] += np.linspace(0, 80, w)[None, :]
    a[h // 5: h // 2, w // 8: w // 3] = [250, 40, 40]
    return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))


@pytest.mark.parametrize("turn", [0, 90, 180, 270])
def test_the_quarter_turn_matching_the_copy_is_found(turn):
    upright = scene()
    reference = F.turned(upright, turn).resize((256, 192) if turn in (0, 180) else (192, 256))
    chosen, score, why = F.best_turn(F.turn_scores(upright, reference))
    assert (chosen, why) == (turn, "") and score > 0.95


def test_a_different_photo_or_shape_is_refused():
    assert F.best_turn(F.turn_scores(scene(seed=1), scene(seed=2)))[2] == "poor match"
    assert F.best_turn(F.turn_scores(scene(480, 360), scene(480, 480)))[2] == "shape matches no turn"


def test_a_photo_that_looks_the_same_turned_round_is_ambiguous():
    a = np.zeros((300, 400), dtype=np.uint8)
    a[100:200, 150:250] = 255                     # a centred square: 0 and 180 degrees look alike
    im = Image.fromarray(a).convert("RGB")
    assert F.best_turn(F.turn_scores(im, im.resize((200, 150))))[2] == "ambiguous turn"


def jpeg_with_orientation(im: Image.Image, orientation: int) -> bytes:
    exif = Image.Exif()
    exif[0x0112] = orientation
    exif[0x010F] = "Camera maker"
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=95, exif=exif.tobytes())
    return buf.getvalue()


def test_fetch_one_applies_exif_then_the_turn_and_drops_metadata(tmp_path, monkeypatch):
    monkeypatch.setattr(F, "PHOTOS", tmp_path / "photos")
    monkeypatch.setattr(F, "CACHE", tmp_path / "cache")
    monkeypatch.setattr(F, "MIN_LONG_SIDE", 300)
    upright = scene()
    # Stored sideways with orientation 6 (turn 90 degrees clockwise to view), and Open Images
    # shows it turned once more: EXIF alone isn't enough, the copy decides.
    data = jpeg_with_orientation(upright.transpose(Image.Transpose.ROTATE_90), 6)
    shown = F.turned(upright, 180)
    monkeypatch.setattr(F, "cached", lambda path, url: data)
    row = {"OriginalURL": "http://x/o.jpg", "OriginalMD5": base64.b64encode(hashlib.md5(data).digest()).decode()}
    r = F.fetch_one("abc", "test", row, shown.resize((240, 180)))
    assert r["status"] == "kept" and (r["exif"], r["turn"]) == (6, 180) and r["bytes"] == len(data)
    saved = Image.open(tmp_path / "photos" / "abc.jpg")
    assert saved.size == (480, 360) and not saved.getexif()
    diff = np.abs(np.asarray(saved, dtype=int) - np.asarray(shown, dtype=int)).mean()
    assert diff < 4

    assert F.fetch_one("abd", "test", row | {"OriginalMD5": "x"}, shown)["status"] == "original changed (MD5)"
    monkeypatch.setattr(F, "MIN_LONG_SIDE", 2000)
    assert F.fetch_one("abe", "test", row, shown.resize((240, 180)))["status"] == "long side under 2000 px"


def test_a_gone_original_is_remembered(tmp_path, monkeypatch):
    calls = []

    def gone(url, timeout=120):
        calls.append(url)
        raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)

    monkeypatch.setattr(F, "get", gone)
    path = tmp_path / "flickr" / "x.jpg"
    assert F.cached(path, "http://x/o.jpg") is None
    assert F.cached(path, "http://x/o.jpg") is None
    assert len(calls) == 1 and path.with_suffix(".gone").exists()
