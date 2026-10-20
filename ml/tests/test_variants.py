"""`training.eval.variants`: the phone-condition transforms and the variant cases."""

import json
import math

import numpy as np
import pytest
from PIL import Image

from ml.training.eval import variants as va


def leaf(w=400, h=300):
    a = np.zeros((h, w, 3), np.uint8)
    a[..., 1] = 160
    a[h // 3 : h // 2, w // 3 : w // 2] = (120, 80, 40)  # a brown spot
    return Image.fromarray(a)


@pytest.mark.parametrize("name", list(va.TRANSFORMS))
def test_each_transform_changes_the_photo_and_is_repeatable(name):
    im = leaf()
    a = va.TRANSFORMS[name](im, va._rng("case", name))
    b = va.TRANSFORMS[name](im, va._rng("case", name))
    assert np.array_equal(np.asarray(a), np.asarray(b))
    assert a.size != im.size or not np.array_equal(np.asarray(a), np.asarray(im))


def test_a_tilted_photo_has_no_blank_corners():
    out = va.tilt(leaf(), va._rng("case", "tilt"))
    a = np.asarray(out)
    for y, x in ((0, 0), (0, -1), (-1, 0), (-1, -1)):
        assert a[y, x, 1] > 100, (y, x, a[y, x])


def test_the_upright_rectangle_fits_inside_the_rotated_photo():
    w, h = va.largest_upright_rect(400, 300, math.radians(15))
    assert 0 < w < 400 and 0 < h < 300
    assert va.largest_upright_rect(400, 300, 0.0) == pytest.approx((400, 300))


def test_colour_casts_go_the_right_way():
    grey = Image.new("RGB", (10, 10), (128, 128, 128))
    r, _, b = np.asarray(va.warm(grey, va._rng("c", "warm")))[0, 0].astype(int)
    assert r > 128 > b
    r, _, b = np.asarray(va.cool(grey, va._rng("c", "cool")))[0, 0].astype(int)
    assert r < 128 < b


@pytest.fixture
def manifest(tmp_path):
    (tmp_path / "team").mkdir()
    leaf().save(tmp_path / "team/t1.jpg")
    case = {"case_id": "tomato-0001", "split": "dev", "plant_key": "p1",
            "images": [{"file": "team/t1.jpg", "sha256": "x", "kind": "whole_plant", "depth": "team/t1.png"}],
            "truth": {"outcome": "diagnosed", "species_id": "tomato", "problems": ["tomato_early_blight"],
                      "organisms": [{"organism_id": None, "image": 0, "bbox": [0, 0, 1, 1]}], "damage": [],
                      "leaves": [{"image": 0, "bbox": [0, 0, 1, 1]}], "measured": {"plant_height_cm": 40}},
            "label_basis": "team_consensus", "label_confidence": "sure", "source": "team",
            "licence": "team-internal", "labelled_by": "a", "reviewed_by": "b"}
    unusable = {**case, "case_id": "none-0001", "truth": {**case["truth"], "outcome": "photos_unusable"}}
    path = tmp_path / "manifest.jsonl"
    path.write_text(json.dumps(case) + "\n" + json.dumps(unusable) + "\n")
    return tmp_path, path


def test_variants_keep_the_truth_they_can_and_live_in_their_own_file(manifest):
    root, path = manifest
    out = root / "manifest.variants.jsonl"
    args = ["--photos", str(root), "--manifest", str(path), "--out", str(out), "--transforms", "tilt,dim"]
    assert va.main(args) == 0 and va.main(args) == 0
    vs = [json.loads(line) for line in out.read_text().splitlines()]
    assert [v["case_id"] for v in vs] == ["tomato-0001~tilt", "tomato-0001~dim"]
    v = vs[0]
    assert (v["derived_from"], v["transform"], v["split"], v["plant_key"]) == ("tomato-0001", "tilt", "dev", "p1")
    assert v["truth"] == {"outcome": "diagnosed", "species_id": "tomato", "problems": ["tomato_early_blight"],
                          "organisms": [], "damage": []}
    assert v["images"] == [{"file": "variants/tilt/team/t1.jpg", "sha256": v["images"][0]["sha256"], "kind": "whole_plant"}]
    assert (root / "variants/tilt/team/t1.jpg").exists()
    assert len(path.read_text().splitlines()) == 2  # the manifest itself is untouched
    with pytest.raises(ValueError, match="itself a variant"):
        va.variant(v, "dim", root)
    with pytest.raises(SystemExit, match="unknown"):
        va.main([*args[:-1], "tilt,sharpen"])
