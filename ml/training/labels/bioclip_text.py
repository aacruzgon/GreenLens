"""The BioCLIP label matrix (training.md §8; models.md §6, step 1).

    python -m ml.training.labels.bioclip_text                      # from ml/knowledge/seed
    python -m ml.training.labels.bioclip_text --labels labels.yaml # a stand-in list

Each species' and organism's `bioclip_label` is wrapped in the 80 OpenAI ImageNet templates,
embedded with BioCLIP 2's text tower, averaged and re-normalised (pybioclip's method). The
result is an N × 768 matrix written as `bioclip_labels_<label_set_version>.json`
(ml/models/FORMATS.md).
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import yaml

from ml.pdm_vision import headfile

ROOT = Path(__file__).resolve().parents[3]
WEIGHTS = ROOT / "ml/models/files/bioclip2"
SEED = ROOT / "ml/knowledge/seed"
OUT = ROOT / "ml/models/files/bioclip_labels"
REVISION = "2957b322090f9cb17ae72c71981c7218a28d81e0"
TEMPLATES_NAME = "openai-imagenet-80"
TEMPLATES_FILE = ROOT / "ml/models/templates/openai-imagenet-80.json"


def templates() -> list[str]:
    """open_clip's 80 OpenAI ImageNet templates with `{label}` in place, from the committed file
    the knowledge loader reads too (ml/models/FORMATS.md)."""
    return json.loads(TEMPLATES_FILE.read_text())


def label_set_version(labels: list[dict]) -> str:
    """First 16 hex digits of SHA-256 over the revision, the template list and the sorted
    labels, serialised as in ml/models/FORMATS.md, so the knowledge loader computes the same."""
    doc = {
        "model": f"bioclip-2@{REVISION}",
        "templates": templates(),
        "labels": sorted(f"{l['table']}:{l['id']}\t{l['label']}" for l in labels),
    }
    blob = json.dumps(doc, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(blob).hexdigest()[:16]


def from_seed() -> list[dict]:
    labels = []
    for table, folder in (("species", "species"), ("organism", "organisms")):
        for f in sorted((SEED / folder).glob("*.yaml")):
            d = yaml.safe_load(f.read_text())
            labels.append({"id": d["id"], "table": table, "label": d["bioclip_label"]})
    return labels


def embed(labels: list[dict], device: str = "cuda") -> np.ndarray:
    # Imported here, so label_set_version and templates import without torch (CI's ml job).
    import open_clip
    import torch

    model, _, _ = open_clip.create_model_and_transforms(f"local-dir:{WEIGHTS}")
    tokenizer = open_clip.get_tokenizer(f"local-dir:{WEIGHTS}")
    model = model.eval().to(device)
    rows = []
    with torch.no_grad():
        for l in labels:
            texts = [t.format(label=l["label"]) for t in templates()]
            e = model.encode_text(tokenizer(texts).to(device)).float()
            e = e / e.norm(dim=-1, keepdim=True)
            m = e.mean(0)
            rows.append((m / m.norm()).cpu().numpy())
    return np.stack(rows).astype(np.float32)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--labels", type=Path, help="YAML list of {id, table, label}, instead of the seed")
    args = ap.parse_args()
    labels = yaml.safe_load(args.labels.read_text()) if args.labels else from_seed()
    if not labels:
        raise SystemExit("no labels: the seed has no species/ or organisms/ files yet; pass --labels")
    version = label_set_version(labels)
    rows = embed(labels)
    path = OUT / f"bioclip_labels_{version}.json"
    headfile.write_label_matrix(
        path,
        label_set_version=version,
        model=f"bioclip-2@{REVISION}",
        templates=TEMPLATES_NAME,
        rows=rows,
        labels=labels,
    )
    print(f"{len(labels)} labels → {path} (label_set_version {version})")


if __name__ == "__main__":
    main()
