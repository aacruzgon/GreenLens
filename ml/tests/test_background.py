"""`detector.background.pick`: indoor and people/pets only, no vegetation, a licence that allows
commercial use and adaptation, never a hold-out photo, deterministic."""

from ml.training.detector.background import licences, pick, plant_fraction

BY = "Attribution License"


def test_pick_skips_outdoor_and_is_deterministic():
    rows = [(f"{i:012d}.jpg", g) for i, g in enumerate(["indoor", "outdoor", "people_pets", "unlabelled"] * 25)]
    plant = {name: 0.0 for name, _ in rows}
    licence = {name: BY for name, _ in rows}
    first = pick(rows, 30, plant, licence)
    assert len(first) == 30
    assert {g for _, g in first} <= {"indoor", "people_pets"}
    assert first == pick(list(reversed(rows)), 30, plant, licence) == sorted(first)
    assert len(pick(rows, 1000, plant, licence)) == 50


def test_pick_skips_vegetation_and_uncovered_photos():
    rows = [("a.jpg", "indoor"), ("b.jpg", "people_pets"), ("c.jpg", "indoor"), ("d.jpg", "people_pets")]
    plant = {"a.jpg": 0.0, "b.jpg": 0.002, "c.jpg": 0.4}  # d.jpg isn't in COCO-Stuff
    assert pick(rows, 10, plant, {name: BY for name, _ in rows}) == [("a.jpg", "indoor")]


def test_pick_keeps_reusable_licences_and_skips_the_hold_out():
    names = ["by.jpg", "by_sa.jpg", "free.jpg", "nc.jpg", "nd.jpg", "unknown.jpg", "holdout.jpg"]
    rows = [(n, "indoor") for n in names]
    licence = {"by.jpg": BY, "by_sa.jpg": "Attribution-ShareAlike License",
               "free.jpg": "No known copyright restrictions", "nc.jpg": "Attribution-NonCommercial License",
               "nd.jpg": "Attribution-NoDerivs License", "holdout.jpg": BY}
    picked = pick(rows, 10, {n: 0.0 for n in names}, licence, {"holdout.jpg"})
    assert [n for n, _ in picked] == ["by.jpg", "by_sa.jpg", "free.jpg"]


def test_licences_maps_file_names_to_licence_names():
    annotations = {"licenses": [{"id": 4, "name": BY}, {"id": 2, "name": "Attribution-NonCommercial License"}],
                   "images": [{"file_name": "a.jpg", "license": 4}, {"file_name": "b.jpg", "license": 2}]}
    assert licences(annotations) == {"a.jpg": BY, "b.jpg": "Attribution-NonCommercial License"}


def test_plant_fraction_sums_the_plant_supercategory():
    stuff = {
        "categories": [{"id": 169, "supercategory": "plant"}, {"id": 97, "supercategory": "plant"},
                       {"id": 118, "supercategory": "ground"}],
        "images": [{"id": 1, "file_name": "a.jpg", "width": 10, "height": 10},
                   {"id": 2, "file_name": "b.jpg", "width": 10, "height": 10}],
        "annotations": [{"image_id": 1, "category_id": 169, "area": 10.0},
                        {"image_id": 1, "category_id": 97, "area": 5.0},
                        {"image_id": 2, "category_id": 118, "area": 50.0}],
    }
    assert plant_fraction(stuff) == {"a.jpg": 0.15, "b.jpg": 0.0}
