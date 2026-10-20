"""`training.heads.crops.decode` against the worker's `vision::perception::detector::decode`."""

import numpy as np

from ml.training.heads.crops import decode

CFG = {"score_threshold": 0.35, "min_side_px": 24, "max_per_image": 40, "nms_iou": 0.8}


def sigmoid(x):
    return float(1 / (1 + np.exp(-np.float32(x))))


def test_the_worker_test_vector_keeps_one_box():
    # detector.rs `keeps_confident_large_boxes_and_drops_duplicates`: a leaf, its duplicate, a tiny box
    dets = np.array([[0.5, 0.5, 0.4, 0.4], [0.5, 0.5, 0.41, 0.4], [0.1, 0.1, 0.005, 0.005]], np.float32)
    logits = np.array([[3.0, -3.0], [2.0, -3.0], [4.0, -3.0]], np.float32)
    boxes, scores = decode(dets, logits, (1000, 1000), CFG)
    assert len(boxes) == 1
    assert abs(scores[0] - sigmoid(3.0)) < 1e-6
    assert abs(boxes[0][0] - 0.3) < 1e-6 and abs(boxes[0][2] - 0.4) < 1e-6


def test_boxes_are_clamped_sorted_and_capped():
    dets = np.array([[0.05, 0.5, 0.2, 0.2], [0.5, 0.5, 0.1, 0.1], [0.8, 0.2, 0.1, 0.1]], np.float32)
    logits = np.array([[0.0], [2.0], [1.0]], np.float32)   # sigmoid 0.5, 0.88, 0.73
    boxes, scores = decode(dets, logits, (1000, 1000), {**CFG, "max_per_image": 2})
    assert list(np.round(scores, 2)) == [0.88, 0.73]
    boxes, _ = decode(dets, logits, (1000, 1000), CFG)
    assert boxes[2][0] == 0.0 and abs(boxes[2][2] - 0.15) < 1e-6   # cx 0.05, w 0.2: clamped at the left edge


def test_small_boxes_and_low_scores_are_dropped():
    dets = np.array([[0.5, 0.5, 0.02, 0.5], [0.5, 0.5, 0.5, 0.5]], np.float32)
    logits = np.array([[5.0], [-1.0]], np.float32)
    boxes, scores = decode(dets, logits, (1000, 1000), CFG)   # 20 px wide; sigmoid(-1) = 0.27
    assert len(boxes) == 0 and len(scores) == 0


def test_views_are_made_per_crop_cached_and_repeatable(tmp_path):
    from types import SimpleNamespace

    from PIL import Image

    from ml.pdm_vision.features import file_sha256
    from ml.training.heads.crops import CropEmbedder

    photos = []
    for i, n_boxes in enumerate((2, 0)):   # a photo with two leaves kept, one with none
        p = tmp_path / f"{i}.jpg"
        Image.new("RGB", (400, 300), (40 * i, 120, 60)).save(p)
        boxes = np.array([[0.1, 0.1, 0.3, 0.4], [0.5, 0.2, 0.4, 0.5]], np.float32)[:n_boxes]
        np.savez(tmp_path / f"{file_sha256(p)}.npz", boxes=boxes, scores=np.ones(n_boxes, np.float32),
                 emb=np.zeros((n_boxes, 768), np.float32))
        photos.append(p)
    seen = []

    def images(ims):
        seen.append([im.size for im in ims])
        return np.random.default_rng(len(seen)).normal(size=(len(ims), 768))

    ce = SimpleNamespace(dir=tmp_path, crop_grow=0.1, emb=SimpleNamespace(images=images),
                         path=lambda k: tmp_path / f"{k}.npz", views_dir=lambda v: tmp_path / f"views{v}")
    assert CropEmbedder.run_views(ce, photos, 3) == 2
    assert [len(v) for v in CropEmbedder.load_views(ce, photos, 3)] == [6, 3]   # 2 crops x 3; the whole photo x 3
    first = list(seen)
    assert CropEmbedder.run_views(ce, photos, 3) == 0                          # cached
    for f in (tmp_path / "views3").iterdir():
        f.unlink()
    CropEmbedder.run_views(ce, photos, 3)
    assert seen[len(first):] == first                                           # seeded by the photo: the same views
