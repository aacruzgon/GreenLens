"""classifier_labels.yaml from the disease head (training.md §8)."""

import pytest

from ml.training.labels import classifier_labels as cl


def label(i: int, species: str, problem: str) -> dict:
    return {"index": i, "label": f"{species}/{problem}", "species_id": species, "problem_id": problem}


LABELS = [label(0, "apple", "apple_scab"), label(1, "apple", "healthy"),
          label(2, "tomato", "healthy"), label(3, "tomato", "tomato_early_blight"),
          label(4, "tomato", "tomato_leaf_mold")]
BOTH = {"apple", "tomato"}


def test_a_species_goes_in_only_when_every_class_maps():
    keep, left = cl.rows(LABELS, {"tomato_early_blight", "tomato_leaf_mold"}, BOTH)
    assert [r["index"] for r in keep] == [2, 3, 4]
    assert left == {"apple": ["apple_scab"]}, "apple's healthy class doesn't bring it in alone"
    keep, left = cl.rows(LABELS, {"tomato_early_blight"}, BOTH)
    assert keep == [] and set(left) == BOTH


def test_a_species_without_a_species_file_stays_out():
    keep, left = cl.rows(LABELS, {"apple_scab", "tomato_early_blight", "tomato_leaf_mold"}, {"tomato"})
    assert [r["species_id"] for r in keep] == ["tomato"] * 3
    assert left == {"apple": [cl.NO_FILE]}


def test_the_rows_keep_the_head_label_and_index():
    keep, _ = cl.rows(LABELS, {"apple_scab", "tomato_early_blight", "tomato_leaf_mold"}, BOTH)
    assert keep == LABELS
    text = cl.render("h1", "disease_head/h1.json", keep, {})
    assert text.startswith("# Written by") and "model_version: h1" in text


def test_a_change_over_takes_the_head_the_seed_names(tmp_path, monkeypatch):
    manifest = tmp_path / "manifest.toml"
    manifest.write_text("".join(f'[[model]]\nname = "disease_head_{v}"\nversion = "{v}"\n' for v in ("h1", "h2")))
    monkeypatch.setattr(cl.current, "classifier_labels_version", lambda seed=None: "h1")
    assert cl.head_entry(manifest, None)["version"] == "h1"
    assert cl.head_entry(manifest, "h2")["version"] == "h2", "--head still picks the new one"
    monkeypatch.setattr(cl.current, "classifier_labels_version", lambda seed=None: None)
    with pytest.raises(SystemExit, match="pass --head"):
        cl.head_entry(manifest, None)


def test_classifier_supported_follows_the_rows(tmp_path):
    species = tmp_path / "species"
    species.mkdir()
    for sid, flag in (("apple", "true"), ("tomato", "false"), ("corn", "false")):
        (species / f"{sid}.yaml").write_text(f"id: {sid}\nprofile: label_only\nclassifier_supported: {flag}\n")
    out = cl.species_files(tmp_path, {"tomato"})
    assert {p.stem: t.splitlines()[-1] for p, t in out.items()} == {
        "apple": "classifier_supported: false", "tomato": "classifier_supported: true"}
