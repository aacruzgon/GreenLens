"""manifest.cpu.toml stays the fp32 twin of manifest.toml (ml/models/cpu_manifest.py)."""

import tomllib
from pathlib import Path

MODELS = Path(__file__).resolve().parents[1] / "models"


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
        else:
            assert c == g, f"{name} differs: rerun python -m ml.models.cpu_manifest"
        assert {k: v for k, v in c.items() if k not in ("file", "sha256", "precision")} == \
               {k: v for k, v in g.items() if k not in ("file", "sha256", "precision")}


def test_file_names_are_unique_for_the_release():
    """The release stores files flat, by name; each name must stand for one path."""
    paths = {e["file"] for m in ("manifest.toml", "manifest.cpu.toml") for e in entries(m).values()}
    names = [Path(f).name for f in paths]
    assert len(names) == len(set(names))
