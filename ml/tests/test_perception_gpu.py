"""`training.eval.perception_gpu`: the cap sets, the scan's batches and the small check model."""

import numpy as np

from ml.training.eval import perception_gpu as pg


def test_caps_default_to_the_manifest_and_override_by_key():
    es = pg.entries(pg.ROOT / "ml/models")   # no manifest there: falls back to ml/models/manifest.toml
    caps = pg.parse_caps("manifest", es)
    assert caps == {"leaf_detector": 8, "sam_encoder": 2, "sam_decoder": 16, "bioclip": 16}
    assert pg.parse_caps("det=4, clip=32", es) == {**caps, "leaf_detector": 4, "bioclip": 32}


def test_a_scan_is_cut_into_batches_of_the_cap():
    assert pg.chunks(46, 16) == [16, 16, 14] and pg.chunks(6, 8) == [6] and pg.chunks(40, 8) == [8] * 5


def test_the_small_model_has_a_dynamic_batch(tmp_path):
    import onnxruntime as ort

    pg.small_model(tmp_path / "m.onnx")
    s = ort.InferenceSession(str(tmp_path / "m.onnx"), providers=["CPUExecutionProvider"])
    for b in (1, 4):
        assert s.run(None, {"x": np.zeros((b, 3, 64, 64), np.float32)})[0].shape == (b, 10)
