"""The head and label-matrix formats (ml/models/FORMATS.md). The Rust loader tests against the
same vector."""

import numpy as np
import pytest

from ml.pdm_vision import headfile
from ml.training.labels.bioclip_text import label_set_version, templates

VECTOR = [
    {"table": "species", "id": "strawberry", "label": "Fragaria × ananassa with common name strawberry"},
    {"table": "species", "id": "tomato", "label": "Solanum lycopersicum with common name tomato"},
    {"table": "organism", "id": "ladybug_larva", "label": "Coccinellidae larva with common name ladybug larva"},
]


def test_label_set_version_vector():
    assert label_set_version(VECTOR) == "129f0395e8d89bd5"
    assert label_set_version(list(reversed(VECTOR))) == "129f0395e8d89bd5"


def test_templates_match_open_clip():
    from open_clip.zero_shot_metadata import OPENAI_IMAGENET_TEMPLATES

    assert templates() == [t("{label}") for t in OPENAI_IMAGENET_TEMPLATES]


def test_linear_head_round_trip(tmp_path):
    w = np.random.default_rng(0).standard_normal((3, 768)).astype(np.float32)
    b = np.arange(3, dtype=np.float32)
    labels = [{"index": i} for i in range(3)]
    headfile.write_linear_head(tmp_path / "h.json", kind="disease_head", version="v", features="f",
                               weight=w, bias=b, temperature=0.5, labels=labels)
    doc = headfile.read_linear_head(tmp_path / "h.json")
    assert np.array_equal(doc["weight"], w) and np.array_equal(doc["bias"], b)


def test_extra_cannot_overwrite_core(tmp_path):
    with pytest.raises(ValueError):
        headfile.write_linear_head(tmp_path / "h.json", kind="k", version="v", features="f",
                                   weight=np.zeros((1, 4), np.float32), bias=np.zeros(1, np.float32),
                                   temperature=1.0, labels=[{}], extra={"weight": "x"})
