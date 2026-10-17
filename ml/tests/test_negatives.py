"""The plant-check negatives list: which COCO images are kept and how they are grouped."""

from ml.training.data.negatives import select

CATS = [
    {"id": 1, "name": "person", "supercategory": "person"},
    {"id": 2, "name": "dog", "supercategory": "animal"},
    {"id": 3, "name": "chair", "supercategory": "furniture"},
    {"id": 4, "name": "car", "supercategory": "vehicle"},
    {"id": 5, "name": "potted plant", "supercategory": "furniture"},
    {"id": 6, "name": "broccoli", "supercategory": "food"},
]


def coco(per_image: dict[int, list[int]]) -> dict:
    anns = [
        {"id": 100 * i + k, "image_id": i, "category_id": c}
        for i, cs in per_image.items()
        for k, c in enumerate(cs)
    ]
    images = [{"id": i, "file_name": f"{i:012d}.jpg"} for i in per_image]
    return {"images": images, "annotations": anns, "categories": CATS}


def groups(per_image: dict[int, list[int]]) -> dict[int, str]:
    return {int(n[:12]): g for n, g in select(coco(per_image))}


def test_potted_plant_and_broccoli_images_are_dropped():
    assert groups({1: [5, 1], 2: [6], 3: [4]}) == {3: "outdoor"}


def test_indoor_beats_people_pets_beats_outdoor():
    assert groups({1: [1, 3], 2: [1, 2, 4], 3: [4]}) == {1: "indoor", 2: "people_pets", 3: "outdoor"}


def test_unannotated_image_is_unlabelled():
    assert groups({1: []}) == {1: "unlabelled"}


def test_output_is_sorted_by_file_name():
    names = [n for n, _ in select(coco({3: [4], 1: [4], 2: [4]}))]
    assert names == sorted(names)
