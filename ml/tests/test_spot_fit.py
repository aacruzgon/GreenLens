"""The spot segmenter's NumPy copy (spot_fit) and PlantSeg's selection (data/plantseg)."""

import numpy as np
from PIL import Image

from ml.training.data import plantseg
from ml.training.eval import spot_fit as sf

GREEN = [60, 140, 50]


def classes(pixels, lv, t=None):
    h, s, v = sf.hsv(np.array(pixels, dtype=np.uint8))
    return sf.classify(h, s, v, lv, t or sf.DEFAULTS).tolist()


def test_the_rust_unit_tests_pixels_classify_the_same():
    # spots.rs, brown_black_and_yellow_against_green
    lv = float(sf.hsv(np.array([GREEN], dtype=np.uint8))[2][0])
    assert classes([GREEN, [110, 70, 30], [15, 15, 10], [200, 190, 40]], lv) == [sf.LEAF, sf.NECROTIC, sf.NECROTIC, sf.HALO]


def test_only_pixels_inside_the_leaf_are_classified():
    # spots.rs, only_pixels_inside_the_leaf_are_classified
    rgb = np.array([[GREEN, [110, 70, 30]], [GREEN, GREEN]], dtype=np.uint8)
    leaf = np.array([[255, 255], [0, 255]], dtype=np.uint8)
    assert sf.segment(rgb, leaf, sf.DEFAULTS).tolist() == [[sf.LEAF, sf.NECROTIC], [sf.OUTSIDE, sf.LEAF]]


def test_hue_follows_the_rust_formula_on_every_branch():
    h, s, v = sf.hsv(np.array([[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 0, 128], [7, 7, 7], [0, 0, 0]], dtype=np.uint8))
    assert np.allclose(h, [0, 120, 240, 360 - 30.117647058823536, 0, 0])
    assert np.allclose(s, [1, 1, 1, 1, 0, 0]) and np.allclose(v[:3], 1)


def test_the_leaf_value_is_the_element_at_half_the_length():
    assert sf.leaf_value(np.array([0.4, 0.1, 0.3, 0.2])) == 0.3   # sorted [.1 .2 .3 .4][2]
    assert sf.leaf_value(np.array([])) == 0.5


def leaves_from(photos):
    """A Leaves set from (lesion, h, s, v) arrays per photo, one leaf each."""
    parts = []
    for i, (gt, h, s, v) in enumerate(photos):
        n = len(gt)
        parts.append({"h": np.array(h, float), "s": np.array(s, float), "v": np.array(v, float),
                      "lv": np.full(n, sf.leaf_value(np.array(v, float))), "gt": np.array(gt, bool),
                      "photo": np.full(n, i), "leaf": np.full(n, i), "w": np.ones(n)})
    return sf.Leaves(parts)


def test_iou_per_photo_and_the_mean():
    # photo 0: one lesion pixel found, one leaf pixel called a lesion -> IoU 1/2; photo 1: exact -> 1
    brown, green = (30.0, 0.6, 0.3), (110.0, 0.6, 0.6)
    p0 = ([1, 0, 0, 0], *zip(brown, brown, green, green))
    p1 = ([1, 0, 0], *zip(brown, green, green))
    c = leaves_from([p0, p1]).counts(sf.DEFAULTS)
    assert c["tp"].tolist() == [1, 1] and c["fp"].tolist() == [1, 0] and c["fn"].tolist() == [0, 0]
    les, leaf, both = sf.ious(c)
    assert les.tolist() == [0.5, 1.0] and np.allclose(leaf, [2 / 3, 1.0])   # photo 0: 2 healthy of 3 called healthy
    assert np.isclose(sf.mean_iou(c), (7 / 12 + 1) / 2)


def test_coordinate_descent_moves_a_threshold_only_when_it_pays_and_is_repeatable():
    # Lesions are pale-ish brown with saturation 0.2, under the default brown_saturation 0.25.
    rng = np.random.default_rng(1)
    photos = []
    for _ in range(6):
        n = 200
        gt = rng.random(n) < 0.3
        h = np.where(gt, 30.0, 105.0)
        s = np.where(gt, 0.2, 0.6)
        v = np.where(gt, 0.35, 0.55)
        photos.append((gt, h, s, v))
    leaves = leaves_from(photos)
    assert sf.mean_iou(leaves.counts(sf.DEFAULTS)) < 1.0
    fitted, log = sf.coordinate_descent(leaves, sf.DEFAULTS)
    assert sf.mean_iou(leaves.counts(fitted)) == 1.0
    assert fitted != sf.DEFAULTS and log[0]["mean_iou"] < log[-1]["mean_iou"]
    assert sum(fitted[k] != v for k, v in sf.DEFAULTS.items()) == 1   # ties keep the defaults
    assert sf.coordinate_descent(leaves, sf.DEFAULTS)[0] == fitted


def test_plantseg_selection_keeps_large_known_photos_capped_by_half(tmp_path, monkeypatch):
    root = tmp_path / "plantseg"
    rows = []
    for i in range(5):
        for split, folder in (("Training", "train"), ("Test", "test")):
            name = f"tomato_early_blight_{folder}{i}.jpg"
            for sub, ext in (("images", ".jpg"), ("annotations", ".png")):
                path = root / sub / folder / name.replace(".jpg", ext)
                path.parent.mkdir(parents=True, exist_ok=True)
                Image.new("RGB", (4, 4)).save(path)
            rows.append({"Name": name, "Plant": "Tomato", "Disease": "tomato early blight", "Split": split,
                         "Resolution": "800x600" if i else "300x200", "Label file": name.replace(".jpg", ".png"),
                         "License": "CC-BY-NC", "URL": f"https://example.org/{i}"})
    rows.append({**rows[1], "Plant": "Wheat"})   # not one of our crops
    monkeypatch.setattr(plantseg, "CAP", 3)
    chosen = plantseg.select(rows, root)
    assert {r["half"] for r in chosen} == {"fit", "holdout"}
    assert sum(r["half"] == "fit" for r in chosen) == 3 and sum(r["half"] == "holdout" for r in chosen) == 3
    assert all(r["crop"] == "tomato" and r["width"] == 800 for r in chosen)
    assert chosen == plantseg.select(rows, root)
