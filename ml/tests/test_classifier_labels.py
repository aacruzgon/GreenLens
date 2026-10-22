"""classifier_labels.yaml from the disease head (training.md §8)."""

from ml.training.labels import classifier_labels as cl


def label(i: int, species: str, problem: str) -> dict:
    return {"index": i, "label": f"{species}/{problem}", "species_id": species, "problem_id": problem}


HEAD = {"version": "h1", "classes": 5, "labels": [
    label(0, "apple", "apple_scab"), label(1, "apple", "healthy"),
    label(2, "tomato", "healthy"), label(3, "tomato", "tomato_early_blight"),
    label(4, "tomato", "tomato_leaf_mold")]}


def test_a_species_goes_in_only_when_every_class_maps():
    keep, left = cl.rows(HEAD, {"tomato_early_blight", "tomato_leaf_mold"})
    assert [r["index"] for r in keep] == [2, 3, 4]
    assert left == {"apple": ["apple_scab"]}, "apple's healthy class doesn't bring it in alone"
    keep, left = cl.rows(HEAD, {"tomato_early_blight"})
    assert keep == [] and set(left) == {"apple", "tomato"}


def test_the_rows_keep_the_head_label_and_index():
    keep, _ = cl.rows(HEAD, {"apple_scab", "tomato_early_blight", "tomato_leaf_mold"})
    assert keep == HEAD["labels"]
    text = cl.render(HEAD, "disease_head/h1.json", keep, {})
    assert text.startswith("# Written by") and "model_version: h1" in text


def test_classifier_supported_follows_the_rows(tmp_path):
    species = tmp_path / "species"
    species.mkdir()
    for sid, flag in (("apple", "true"), ("tomato", "false"), ("corn", "false")):
        (species / f"{sid}.yaml").write_text(f"id: {sid}\nprofile: label_only\nclassifier_supported: {flag}\n")
    out = cl.species_files(tmp_path, {"tomato"})
    assert {p.stem: t.splitlines()[-1] for p, t in out.items()} == {
        "apple": "classifier_supported: false", "tomato": "classifier_supported: true"}
