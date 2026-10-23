"""`training.eval.check_leakage`: copies of a training image are caught, other photos are not."""

import json

import numpy as np
import pytest
from PIL import Image, ImageDraw

from ml.pdm_vision import preprocess as pp
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


class FakeEmbedder:
    """Stands in for BioCLIP 2: an image's centred 32 × 32 grey, so a copy scores near 1 and another
    picture near 0. Like the real one, it fails on a file it can't read."""

    def __init__(self):
        self.seen: list = []

    def run(self, tensors: np.ndarray) -> np.ndarray:
        g = tensors.mean(axis=1).reshape(len(tensors), 32, 7, 32, 7).mean(axis=(2, 4)).reshape(len(tensors), -1)
        return (g - g.mean(axis=1, keepdims=True)).astype(np.float32)

    def files(self, paths) -> np.ndarray:
        self.seen += list(paths)
        return self.run(np.stack([pp.to_tensor(pp.load_rgb(p), pp.BIOCLIP) for p in paths]))


@pytest.fixture
def tree(tmp_path, monkeypatch):
    """A raw folder with two sources, two evaluation photos (a copy and a fresh one), and the
    module's output paths in tmp_path."""
    raw, photos = tmp_path / "raw", tmp_path / "photos"
    save(picture(1), raw / "plantdoc" / "leaf.jpg")
    save(picture(2), raw / "tom2024" / "other.jpg")
    save(picture(1).resize((400, 300)), photos / "inat" / "copy.jpg")
    save(picture(3), photos / "inat" / "fresh.jpg")
    (tmp_path / "manifest.jsonl").write_text("".join(json.dumps({"case_id": f"c{i}", "images": [{"file": f"inat/{n}.jpg"}]}) + "\n"
                                                     for i, n in enumerate(("copy", "fresh"))))
    (tmp_path / "sources.yaml").write_text("datasets:\n  - name: plantdoc\n  - name: tom2024\n  - name: coco_annotations_2017\n")
    for name, value in {"CACHE": tmp_path / "cache", "REPORTS": tmp_path / "reports", "RUNS": tmp_path / "runs",
                        "ROOT": tmp_path, "COCO_FOLDERS": ()}.items():
        monkeypatch.setattr(cl, name, value)
    return tmp_path


def args_for(tmp, *extra):
    return cl.parse_args(["--manifest", str(tmp / "manifest.jsonl"), "--photos", str(tmp / "photos"), "--raw", str(tmp / "raw"),
                          "--sources", str(tmp / "sources.yaml"), "--allowed", str(tmp / "allowed.tsv"), "--out", "t", *extra])


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


def test_distances_are_even():
    hashes = np.array([cl.phash(np.random.default_rng(s).random((32, 32), dtype=np.float32)) for s in range(50)], dtype=np.uint64)
    assert np.all(np.bitwise_count(hashes) == 32)
    assert np.all(cl.hamming(hashes, hashes) % 2 == 0)


def test_an_unreadable_training_file_never_matches(tmp_path):
    a = save(picture(1), tmp_path / "a.png")
    (tmp_path / "broken.jpg").write_bytes(b"not an image")
    assert cl.file_hash(tmp_path / "broken.jpg") is None
    train, readable = cl.training_hashes([tmp_path / "broken.jpg", a], cache=tmp_path / "cache", workers=1)
    assert readable.tolist() == [False, True]
    dist, _ = cl.nearest_by_hash([cl.gray32(a)], train, readable)
    assert dist[0, 0] == 64 and dist[0, 1] == 0


def test_a_blank_image_is_readable_and_matches_a_blank_photo(tmp_path):
    black = save(Image.new("RGB", (200, 150)), tmp_path / "black.png")
    assert cl.file_hash(black) == 0
    train, readable = cl.training_hashes([black], cache=tmp_path / "cache", workers=1)
    assert readable.tolist() == [True]
    dist, _ = cl.nearest_by_hash([cl.gray32(save(Image.new("RGB", (80, 60)), tmp_path / "photo.jpg"))], train, readable)
    assert dist[0, 0] == 0


def test_training_hashes_are_cached_and_refreshed_when_a_file_changes(tmp_path, monkeypatch):
    raw = tmp_path / "raw"
    files = [save(picture(i), raw / "set" / f"{i}.png") for i in range(3)]
    first, _ = cl.training_hashes(cl.training_files(raw), cache=tmp_path / "cache", workers=1)
    monkeypatch.setattr(cl, "ProcessPoolExecutor", lambda *a, **k: (_ for _ in ()).throw(AssertionError("rehashed")))
    assert np.array_equal(cl.training_hashes(cl.training_files(raw), cache=tmp_path / "cache")[0], first)
    monkeypatch.undo()
    save(picture(9), files[1])
    again, _ = cl.training_hashes(cl.training_files(raw), cache=tmp_path / "cache", workers=1)
    assert again[0] == first[0] and again[1] != first[1]


def test_a_run_reports_matches_and_fails_until_they_are_allowed(tree):
    args = args_for(tree, "--no-bioclip")
    assert cl.run(args) == 1
    report = json.loads((tree / "reports" / "t.json").read_text())
    assert [(m["photo"], m["training"], m["allowed"]) for m in report["matches"]] == [("inat/copy.jpg", "plantdoc/leaf.jpg", False)]
    assert report["training_images_by_folder"] == {"plantdoc": 1, "tom2024": 1}
    assert (tree / "runs" / "t" / "matches.jpg").exists()
    (tree / "allowed.tsv").write_text("photo\ttraining\twhy\ninat/copy.jpg\tplantdoc/leaf.jpg\tchecked by eye\n")
    assert cl.run(args) == 0


def test_a_run_stops_when_a_source_has_no_images(tree):
    (tree / "sources.yaml").write_text("datasets:\n  - name: plantdoc\n  - name: tom2024\n  - name: ibean\n")
    with pytest.raises(SystemExit, match="ibean"):
        cl.run(args_for(tree, "--no-bioclip"))


def test_a_relative_manifest_path_is_reported(tree, monkeypatch):
    monkeypatch.chdir(tree)
    args = args_for(tree, "--no-bioclip")
    args.manifest = cl.Path("manifest.jsonl")
    cl.run(args)
    assert "(`manifest.jsonl`)" in (tree / "reports" / "t.md").read_text()


def test_the_report_records_the_manifests_hash(tree):
    cl.run(args_for(tree, "--no-bioclip"))
    sha = cl.hashlib.sha256((tree / "manifest.jsonl").read_bytes()).hexdigest()
    assert json.loads((tree / "reports" / "t.json").read_text())["manifest_sha256"] == sha
    assert f"SHA-256 is `{sha}`" in (tree / "reports" / "t.md").read_text()


def test_bioclip_skips_an_unreadable_training_file(tree, monkeypatch):
    (tree / "raw" / "tom2024" / "broken.jpg").write_bytes(b"not an image")
    fake = FakeEmbedder()
    monkeypatch.setattr(cl, "embedder", lambda: fake)
    assert cl.run(args_for(tree)) == 1
    assert not any(p.name == "broken.jpg" for p in fake.seen)
    report = json.loads((tree / "reports" / "t.json").read_text())
    assert report["unreadable_training_images"] == ["tom2024/broken.jpg"]
    match = report["matches"][0]
    assert (match["photo"], match["training"]) == ("inat/copy.jpg", "plantdoc/leaf.jpg") and match["cosine"] > 0.95


def test_calibration_measures_each_edit(tree, monkeypatch):
    for i in range(4):
        save(picture(10 + i), tree / "raw" / "tom2024" / f"more-{i}.jpg")
    monkeypatch.setattr(cl, "embedder", lambda: FakeEmbedder())
    cl.run(args_for(tree, "--calibrate", "4"))
    cal = json.loads((tree / "reports" / "t.json").read_text())["calibration"]
    assert cal["images"] == 4 and list(cal["edits"]) == list(cl.EDITS)
    for name in ("half size", "rotated 90°", "mirrored"):
        assert cal["edits"][name]["hash_caught"] == 1.0, (name, cal["edits"][name])
    assert all(0 <= e["either_caught"] <= 1 and e["cosine_min"] <= 1.0001 for e in cal["edits"].values())
    assert "## Calibration" in (tree / "reports" / "t.md").read_text()
