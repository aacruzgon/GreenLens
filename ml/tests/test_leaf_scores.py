"""`leaf_scores.leaf_score` decodes as the worker's `detector::decode` then `top_score`."""

import numpy as np

from ml.training.eval.leaf_scores import leaf_score

CFG = {"score_threshold": 0.35, "min_side_px": 24}


def logit(p: float) -> float:
    return float(np.log(p / (1 - p)))


def test_top_kept_score():
    dets = np.array([[0.5, 0.5, 0.2, 0.2], [0.3, 0.3, 0.4, 0.4]], np.float32)
    logits = np.array([[logit(0.6), 0.0], [logit(0.9), 0.0]], np.float32)
    assert abs(leaf_score(dets, logits, (1000, 800), CFG) - 0.9) < 1e-5


def test_below_score_threshold_is_not_kept():
    dets = np.array([[0.5, 0.5, 0.5, 0.5]], np.float32)
    assert leaf_score(dets, np.array([[logit(0.34), 0.0]], np.float32), (1000, 800), CFG) == 0.0
    assert leaf_score(dets, np.array([[logit(0.36), 0.0]], np.float32), (1000, 800), CFG) > 0.35


def test_a_box_under_min_side_on_the_stored_image_is_not_kept():
    logits = np.array([[logit(0.95), 0.0]], np.float32)
    small = np.array([[0.5, 0.5, 0.02, 0.5]], np.float32)   # 20 px wide on a 1000 px image
    assert leaf_score(small, logits, (1000, 800), CFG) == 0.0
    assert leaf_score(small, logits, (1600, 1200), CFG) > 0.9   # 32 px on a larger stored image
