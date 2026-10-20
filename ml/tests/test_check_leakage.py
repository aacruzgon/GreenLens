"""`training.eval.check_leakage`: copies of a training image are caught, other photos are not."""

import argparse
import json

import numpy as np
from PIL import Image, ImageDraw

from ml.training.eval import check_leakage as cl


def picture(seed: int, size=(640, 480)) -> Image.Image:
    """A photo-like image: smooth colour fields and a few shapes, different for each seed."""
    rng = np.random.default_rng(seed)
    w, h = size
    y, x = np.mgrid[0:h, 0:w] / max(w, h)
    base = np.stack([np.sin(x * rng.uniform(2, 9) + rng.uniform(0, 6)) * np.cos(y * rng.uniform(2, 9)) for _ in range(3)], -1)
    im = Image.fromarray(((base + 1) * 127).astype(np.uint8))
    d = ImageDraw.Draw(im)
    for _ in range(6):
        x0, y0 = rng.integers(0, w - 80), rng.integers(0, h - 80)
        d.ellipse((x0, y0, x0 + rng.integers(30, 160), y0 + rng.integers(30, 160)), fill=tuple(int(c) for c in rng.integers(0, 255, 3)))
    return im


def save(im: Image.Image, path, **kw):
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path, **kw)
    return path


def test_copies_match_and_other_photos_dont(tmp_path):
    a = picture(1)
    original = cl.file_hash(save(a, tmp_path / "a.png"))
    train = np.array([original, cl.file_hash(save(picture(2), tmp_path / "b.png"))], dtype=np.uint64)
    copies = {
        "smaller, recompressed": save(a.resize((320, 240)), tmp_path / "small.jpg", quality=60),
        "rotated": save(a.rotate(90, expand=True), tmp_path / "rot.png"),
        "mirrored": save(a.transpose(Image.Transpose.FLIP_LEFT_RIGHT), tmp_path / "flip.png"),
    }
    dist, near = cl.nearest_by_hash([cl.gray32(p) for p in copies.values()], train)
    for (name, _), row, n in zip(copies.items(), dist, near):
        assert n == 0 and row[0] <= 4, (name, row)
    other, _ = cl.nearest_by_hash([cl.gray32(save(picture(3), tmp_path / "c.png"))], train)
    assert other.min() > 16


def test_an_unreadable_training_file_never_matches(tmp_path):
    a = save(picture(1), tmp_path / "a.png")
    (tmp_path / "broken.jpg").write_bytes(b"not an image")
    assert cl.file_hash(tmp_path / "broken.jpg") is None
    dist, _ = cl.nearest_by_hash([cl.gray32(a)], np.array([0, cl.file_hash(a)], dtype=np.uint64))
    assert dist[0, 0] == 64 and dist[0, 1] == 0


def test_training_hashes_are_cached_and_refreshed_when_a_file_changes(tmp_path, monkeypatch):
    raw = tmp_path / "raw"
    files = [save(picture(i), raw / "set" / f"{i}.png") for i in range(3)]
    first = cl.training_hashes(cl.training_files(raw), cache=tmp_path / "cache", workers=1)
    monkeypatch.setattr(cl, "ProcessPoolExecutor", lambda *a, **k: (_ for _ in ()).throw(AssertionError("rehashed")))
    assert np.array_equal(cl.training_hashes(cl.training_files(raw), cache=tmp_path / "cache"), first)
    monkeypatch.undo()
    save(picture(9), files[1])
    again = cl.training_hashes(cl.training_files(raw), cache=tmp_path / "cache", workers=1)
    assert again[0] == first[0] and again[1] != first[1]


def test_a_run_reports_matches_and_fails_until_they_are_allowed(tmp_path, monkeypatch):
    raw, photos = tmp_path / "raw", tmp_path / "photos"
    save(picture(1), raw / "plantdoc" / "leaf.jpg")
    save(picture(2), raw / "tom2024" / "other.jpg")
    save(picture(1).resize((400, 300)), photos / "inat" / "copy.jpg")
    save(picture(3), photos / "inat" / "fresh.jpg")
    manifest = tmp_path / "manifest.jsonl"
    manifest.write_text("".join(json.dumps({"case_id": f"c{i}", "images": [{"file": f"inat/{n}.jpg"}]}) + "\n"
                                for i, n in enumerate(("copy", "fresh"))))
    monkeypatch.setattr(cl, "CACHE", tmp_path / "cache")
    monkeypatch.setattr(cl, "REPORTS", tmp_path / "reports")
    monkeypatch.setattr(cl, "RUNS", tmp_path / "runs")
    monkeypatch.setattr(cl, "ROOT", tmp_path)
    args = argparse.Namespace(manifest=manifest, photos=photos, raw=raw, allowed=tmp_path / "allowed.tsv",
                              hamming=8, cosine=0.98, no_bioclip=True, out="t", date="2026-10-20")
    assert cl.run(args) == 1
    report = json.loads((tmp_path / "reports" / "t.json").read_text())
    assert [(m["photo"], m["training"], m["allowed"]) for m in report["matches"]] == [("inat/copy.jpg", "plantdoc/leaf.jpg", False)]
    assert (tmp_path / "runs" / "t" / "matches.jpg").exists()
    (tmp_path / "allowed.tsv").write_text("photo\ttraining\twhy\ninat/copy.jpg\tplantdoc/leaf.jpg\tchecked by eye\n")
    assert cl.run(args) == 0
