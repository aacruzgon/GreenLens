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
