"""`detector.background.pick`: indoor and people/pets only, no vegetation, deterministic."""

from ml.training.detector.background import pick, plant_fraction


def test_pick_skips_outdoor_and_is_deterministic():
    rows = [(f"{i:012d}.jpg", g) for i, g in enumerate(["indoor", "outdoor", "people_pets", "unlabelled"] * 25)]
    plant = {name: 0.0 for name, _ in rows}
    first = pick(rows, 30, plant)
    assert len(first) == 30
    assert {g for _, g in first} <= {"indoor", "people_pets"}
    assert first == pick(list(reversed(rows)), 30, plant) == sorted(first)
    assert len(pick(rows, 1000, plant)) == 50


def test_pick_skips_vegetation_and_uncovered_photos():
    rows = [("a.jpg", "indoor"), ("b.jpg", "people_pets"), ("c.jpg", "indoor"), ("d.jpg", "people_pets")]
    plant = {"a.jpg": 0.0, "b.jpg": 0.002, "c.jpg": 0.4}  # d.jpg isn't in COCO-Stuff
    assert pick(rows, 10, plant) == [("a.jpg", "indoor")]


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
