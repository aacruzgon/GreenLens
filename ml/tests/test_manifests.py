"""The manifests, sources.toml and NOTICES.md stay consistent (docs/vision/README.md §6)."""

import tomllib
from pathlib import Path

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
