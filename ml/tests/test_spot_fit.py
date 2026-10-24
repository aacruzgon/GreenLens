"""The spot segmenter's NumPy copy (spot_fit) and PlantSeg's selection (data/plantseg)."""

import json
import re

import numpy as np
from PIL import Image

from ml.training.data import plantseg
from ml.training.eval import spot_fit as sf

GREEN = [60, 140, 50]
SPOTS_RS = sf.BACKEND / "crates/vision/src/measure/spots.rs"


def shipped():
    """`SpotThresholds::default()` as spots.rs writes it, and the report its doc comment links."""
    src = SPOTS_RS.read_text()
    body = re.search(r"impl Default for SpotThresholds \{.*?Self \{(.*?)\}", src, re.S).group(1)
    consts = {k: float(v) for k, v in re.findall(r"pub const (\w+): f64 = ([\d.e+-]+);", src)}
    fields = {k: consts[v] if v in consts else float(v) for k, v in re.findall(r"(\w+): ([\w.+-]+),", body)}
    report = re.search(r"\[report\]: \.\./\.\./\.\./(ml/training/eval/reports/spots-[\d-]+)\.md", src).group(1)
    return fields, json.loads((sf.ROOT / f"{report}.json").read_text())


def fitted():
    return shipped()[1]["fitted"]


def test_spots_rs_ships_the_reports_fitted_values():
    fields, report = shipped()
    assert list(fields) == list(sf.DEFAULTS)   # every field, in spots.rs' order
    assert fields == report["fitted"]
    assert fields != sf.DEFAULTS               # colour-1's values are the report's `defaults`


def classes(pixels, lv, t=None):
    h, s, v = sf.hsv(np.array(pixels, dtype=np.uint8))
    return sf.classify(h, s, v, lv, t or sf.DEFAULTS).tolist()


def test_the_rust_unit_tests_pixels_classify_the_same():
    # spots.rs, brown_black_and_yellow_against_green, at the shipped values and at colour-1's
    lv = float(sf.hsv(np.array([GREEN], dtype=np.uint8))[2][0])
    px = [GREEN, [110, 70, 30], [15, 15, 10], [200, 190, 40]]
    assert classes(px, lv, fitted()) == [sf.LEAF, sf.NECROTIC, sf.NECROTIC, sf.HALO]
    assert classes(px, lv, sf.DEFAULTS) == [sf.LEAF, sf.NECROTIC, sf.NECROTIC, sf.HALO]


def test_the_fitted_values_move_these_pixels():
    # spots.rs, the_fitted_values_move_these_pixels
    lv = float(sf.hsv(np.array([GREEN], dtype=np.uint8))[2][0])
    px = [[120, 110, 100], [50, 60, 40], [180, 200, 40], [235, 235, 235]]
    assert classes(px, lv, fitted()) == [sf.NECROTIC, sf.LEAF, sf.LEAF, sf.NECROTIC]
    assert classes(px, lv, sf.DEFAULTS) == [sf.LEAF, sf.NECROTIC, sf.HALO, sf.NECROTIC]


def test_only_pixels_inside_the_leaf_are_classified():
    # spots.rs, only_pixels_inside_the_leaf_are_classified
    rgb = np.array([[GREEN, [110, 70, 30]], [GREEN, GREEN]], dtype=np.uint8)
    leaf = np.array([[255, 255], [0, 255]], dtype=np.uint8)
    assert sf.segment(rgb, leaf, fitted()).tolist() == [[sf.LEAF, sf.NECROTIC], [sf.OUTSIDE, sf.LEAF]]


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


def test_patches_under_the_share_of_the_leaf_are_leaf():
    # spots.rs, patches_under_the_share_of_the_leaf_are_leaf: a brown pixel at (1, 1) and a 3 x 3
    # brown patch at (5..8, 5..8) on a 10 x 10 leaf.
    rgb = np.full((10, 10, 3), GREEN, dtype=np.uint8)
    rgb[1, 1] = [110, 70, 30]
    rgb[5:8, 5:8] = [110, 70, 30]
    leaf = np.full((10, 10), 255, dtype=np.uint8)
    necrotic = lambda share: np.flatnonzero(sf.segment(rgb, leaf, {**fitted(), "min_spot_share": share}) == sf.NECROTIC).tolist()
    assert len(necrotic(0.0)) == 10
    assert necrotic(0.05) == [55, 56, 57, 65, 66, 67, 75, 76, 77]


def test_holes_are_gaps_that_dont_reach_the_box_edge():
    ring = np.ones((7, 7), bool)
    ring[2:4, 2:4] = False   # one hole
    ring[5, 5] = False       # another
    assert sf.holes(ring) == 2
    notch = np.ones((7, 7), bool)
    notch[3, 0:4] = False    # open to the box's edge: a notch, not a hole
    assert sf.holes(notch) == 0
    assert sf.holes(np.zeros((3, 3), bool)) == 0
    assert sorted(sf.hole_sizes(ring).tolist()) == [1, 4]


def test_a_photo_is_clean_when_no_leaf_keeps_a_spot_at_the_share():
    leaf = lambda sizes, holes=0: {"px": 1000, "tp": 0, "fp": int(sum(sizes)), "fn": 0, "sizes": np.array(sizes),
                                   "gt": np.zeros(len(sizes)), "hole_sizes": np.full(holes, 5)}
    photos = [[leaf([1, 2])], [leaf([1]), leaf([50])], [leaf([], holes=1)]]
    assert sf.at_share(photos, 0.0)["clean_pct"] == round(100 / 3, 1)        # only the leaf with no patch
    r = sf.at_share(photos, 0.01)                                             # patches under 10 px go
    assert r["clean_pct"] == round(200 / 3, 1) and r["clean_with_holes_pct"] == round(100 / 3, 1)


def test_the_share_is_the_largest_within_the_tolerance():
    rates = {0.0: 2.0, 1e-4: 2.5, 2e-4: 3.0, 5e-4: 3.5, 1e-3: 2.9}
    assert sf.choose_share(rates, 0.01) == 1e-3   # the largest share at most 1 point above 2.0
    assert sf.choose_share(rates, 0.0) == 0.0


def test_parity_takes_leaves_from_each_disease_in_turn():
    rows_ = [{"image": f"{d}{i}", "disease": d, "half": h} for d in ("b", "a") for i in range(3) for h in ("holdout",)]
    rows_.append({"image": "fit0", "disease": "a", "half": "fit"})
    leaf = {"class": "leaf", "mask": {"size": [1, 1], "counts": "01"}}
    regions = {r["image"]: {"regions": [leaf]} for r in rows_}
    picked = [r["image"] for r, _ in sf.parity_leaves(rows_, regions, 4)]
    assert picked == ["a0", "b0", "a1", "b1"]
