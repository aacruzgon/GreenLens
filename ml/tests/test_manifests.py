"""The manifests, sources.toml and NOTICES.md stay consistent (docs/vision/README.md §6)."""

import tomllib
from pathlib import Path

import pytest
import yaml

from ml.models import check

MODELS = Path(__file__).resolve().parents[1] / "models"
HEADS = ("disease_head", "species_head", "plant_probe")


def entries(name: str) -> dict:
    return {e["name"]: e for e in tomllib.loads((MODELS / name).read_text())["model"]}


def test_cpu_manifest_is_the_fp32_twin():
    gpu, cpu = entries("manifest.toml"), entries("manifest.cpu.toml")
    assert gpu.keys() == cpu.keys(), "rerun python -m ml.models.cpu_manifest"
    for name, g in gpu.items():
        c = cpu[name]
        if g["file"].endswith("-fp16.onnx"):
            assert c["file"] == g["file"].replace("-fp16.onnx", "-fp32.onnx")
            assert c["precision"] == "fp32"
        elif name.startswith(HEADS):
            assert c["file"] == g["file"].replace(".json", ".cpu.json")
        else:
            assert c == g, f"{name} differs: rerun python -m ml.models.cpu_manifest"
        assert {k: v for k, v in c.items() if k not in ("file", "sha256", "precision")} == \
               {k: v for k, v in g.items() if k not in ("file", "sha256", "precision")}


def test_manifests_sources_and_notices_agree(tmp_path):
    """check.py without local files: source rows, unique paths, versions and notices."""
    problems, _ = check.check(MODELS, tmp_path)
    assert problems == []


def test_plant_probe_file_rules():
    bioclip = {"sha256": "b9016a6865e188c0" + "0" * 48}
    good = {"kind": "plant_probe", "classes": 1, "version": "p1", "features": "bioclip-2@2957b322+b9016a6865e188c0"}
    assert check.features_problems("m: plant_probe", good, bioclip, "p1") == []
    for bad, needle in (({"features": "bioclip-2@2957b322+0000000000000000"}, "would refuse"),
                        ({"version": "p0"}, "version"),
                        ({"classes": 2}, "one-class")):
        problems = check.features_problems("m: plant_probe", {**good, **bad}, bioclip, "p1")
        assert any(needle in p for p in problems), (bad, problems)


def test_seed_rows_are_compared_by_index(tmp_path, monkeypatch):
    """classifier_labels.yaml may map fewer classes than the head; each row must match its index,
    and a species it maps must have all of the head's classes."""
    labels = check.head_rows({"labels": ["apple/apple_scab", "tomato/healthy", "tomato/tomato_leaf_mold"]})
    seed = tmp_path / "classifier_labels.yaml"
    monkeypatch.setattr(check, "SEED_LABELS", seed)

    def problems(version: str, rows: list[dict]) -> list[str]:
        seed.write_text(yaml.safe_dump({"model_version": version, "labels": rows}))
        return check.seed_problems("m: disease_head_h1", "h1", labels, [])

    tomato = [dict(l) for l in labels[1:]]
    assert problems("h1", tomato) == []
    assert problems("h0", tomato) == [], "another head of a change-over isn't compared"
    wrong = [tomato[0], {**tomato[1], "problem_id": "tomato_late_blight"}]
    assert any("index 2" in p for p in problems("h1", wrong))
    assert any("isn't one of the head's" in p for p in problems("h1", [*tomato, {**tomato[0], "index": 5}]))
    assert any("without tomato/tomato_leaf_mold" in p for p in problems("h1", tomato[:1]))

    assert check.seed_head_problems("m", [{"name": "disease_head_h1", "version": "h1"}]) == []
    assert "no disease head" in check.seed_head_problems("m", [{"name": "disease_head_h0", "version": "h0"}])[0]


def test_a_head_entry_lists_its_labels():
    """The copy of the head's labels that classifier_labels --check and the seed comparison read
    where the model files aren't (CI); check.py compares it with the file where it is."""
    rows = check.head_rows({"labels": ["corn/healthy", "corn/corn_common_rust"]})
    assert rows[1] == {"index": 1, "label": "corn/corn_common_rust", "species_id": "corn", "problem_id": "corn_common_rust"}
    for bad in ({}, {"labels": []}, {"labels": ["corn"]}, {"labels": ["/healthy"]}):
        with pytest.raises(ValueError):
            check.head_rows(bad)
    for name in check.MANIFESTS:
        for n, e in entries(name).items():
            if n.startswith("disease_head"):
                assert len(check.head_rows(e)) > 1, (name, n)


def test_each_manifest_has_the_label_matrix_knowledge_names():
    """api asks every perception job for knowledge.surql's label_set_version; a worker on these
    manifests must have that matrix, or real scans are refused (#51, Oct 20)."""
    from ml.training.labels import current

    version = current.label_set_version()
    for name in check.MANIFESTS:
        entry = current.matrix_entry(list(entries(name).values()))
        assert entry["version"] == version and entry["name"] == f"bioclip_labels_{version}"


def test_knowledge_names_the_seeds_versions():
    """knowledge.surql's knowledge_meta is what the API loads; it must carry the versions the seed
    gives, or it is stale (python -m ml.knowledge.build_migration)."""
    from ml.training.labels import current

    knowledge = MODELS.parent.parent / "backend/migrations/knowledge.surql"
    meta = next(l for l in knowledge.read_text().splitlines() if l.startswith("UPSERT knowledge_meta:`current`"))
    assert f"label_set_version: '{current.label_set_version()}'" in meta
    assert f"classifier_labels_version: '{current.classifier_labels_version() or ''}'" in meta
