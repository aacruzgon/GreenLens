"""The JSON files for the heads and label matrices (training.md §6 and §8; backend vision.md §5).

The field names below are those of ml/models/FORMATS.md, which the Rust perception worker
reads (backend/crates/vision/src/perception/heads.rs). Matrices are row-major. Every file names its format and version so the worker can refuse what it
doesn't know.
"""

from __future__ import annotations

import base64
import json
from pathlib import Path

import numpy as np

LINEAR_HEAD = "pdm-linear-head/1"
LABEL_MATRIX = "pdm-label-matrix/1"


def b64_f32(a: np.ndarray) -> str:
    return base64.b64encode(np.ascontiguousarray(a, dtype="<f4").tobytes()).decode("ascii")


def f32_b64(s: str, shape: tuple[int, ...]) -> np.ndarray:
    a = np.frombuffer(base64.b64decode(s), dtype="<f4")
    if a.size != int(np.prod(shape)):
        raise ValueError(f"expected {shape}, got {a.size} values")
    return a.reshape(shape).astype(np.float32)


def write_linear_head(
    path: Path,
    *,
    kind: str,
    version: str,
    features: str,
    weight: np.ndarray,
    bias: np.ndarray,
    temperature: float,
    labels: list[dict],
    extra: dict | None = None,
) -> None:
    """A linear head on an embedding: `logits = weight · e + bias`, then `/ temperature`.

    `kind` is `disease_head`, `species_head` or `plant_probe`. `weight` is `[classes, dim]`.
    `labels[i]` describes output `i` (for the disease head: `index`, `label`, `species_id`,
    `problem_id`, where `problem_id` is a slug or `"healthy"`).
    """
    classes, dim = weight.shape
    if bias.shape != (classes,) or len(labels) != classes:
        raise ValueError("weight, bias and labels disagree on the class count")
    core = {"format", "kind", "version", "features", "dim", "classes", "temperature", "labels", "weight", "bias"}
    if clash := core & set(extra or {}):
        raise ValueError(f"extra fields would overwrite core fields: {sorted(clash)}")
    doc = {
        "format": LINEAR_HEAD,
        "kind": kind,
        "version": version,
        "features": features,
        "dim": dim,
        "classes": classes,
        "temperature": float(temperature),
        "labels": labels,
        "weight": b64_f32(weight),
        "bias": b64_f32(bias),
        **(extra or {}),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=1) + "\n")


def read_linear_head(path: Path) -> dict:
    doc = json.loads(path.read_text())
    if doc.get("format") != LINEAR_HEAD:
        raise ValueError(f"{path}: not a {LINEAR_HEAD} file")
    doc["weight"] = f32_b64(doc["weight"], (doc["classes"], doc["dim"]))
    doc["bias"] = f32_b64(doc["bias"], (doc["classes"],))
    return doc


def write_label_matrix(
    path: Path,
    *,
    label_set_version: str,
    model: str,
    templates: str,
    rows: np.ndarray,
    labels: list[dict],
) -> None:
    """`N × dim` L2-normalised text embeddings; `labels[i]` is `{id, table, label}` with `table`
    `species` or `organism`."""
    n, dim = rows.shape
    if len(labels) != n:
        raise ValueError("rows and labels disagree")
    doc = {
        "format": LABEL_MATRIX,
        "label_set_version": label_set_version,
        "model": model,
        "templates": templates,
        "dim": dim,
        "count": n,
        "labels": labels,
        "rows": b64_f32(rows),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=1) + "\n")


def read_label_matrix(path: Path) -> dict:
    doc = json.loads(path.read_text())
    if doc.get("format") != LABEL_MATRIX:
        raise ValueError(f"{path}: not a {LABEL_MATRIX} file")
    doc["rows"] = f32_b64(doc["rows"], (doc["count"], doc["dim"]))
    return doc
