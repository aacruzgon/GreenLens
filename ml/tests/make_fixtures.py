"""Reference outputs for the Rust heads (docs/vision/dev-roadmap.md, task M6).

    python -m ml.tests.make_fixtures

Writes ml/tests/fixtures/perception_reference.json: for a few PlantDoc test photos, the BioCLIP 2
embedding the rig computes (the manifest's fp16 export), the species top 3 against the manifest's
label matrix, and the disease head's top 3 restricted to the photo's species, each computed the
way backend vision.md §6 describes. A Rust test loads the same head and matrix files, feeds it
these embeddings, and expects the same ids in the same order, with probabilities within 1e-4.
"""

from __future__ import annotations

import json
import tomllib
from pathlib import Path

import numpy as np

from ml.pdm_vision import headfile
from ml.pdm_vision.features import Embedder

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "ml/models/manifest.toml"
FILES = ROOT / "ml/models/files"
RAW = ROOT / "ml/training/data/raw"
OUT = ROOT / "ml/tests/fixtures/perception_reference.json"
PER_SPECIES = {"tomato": 2, "corn": 1, "apple": 1, "potato": 1, "pepper": 1}


def softmax(z: np.ndarray) -> np.ndarray:
    e = np.exp(z - z.max())
    return e / e.sum()


def main() -> None:
    entries = {e["name"]: e for e in tomllib.loads(MANIFEST.read_text())["model"]}
    head_entry = next(e for n, e in entries.items() if n.startswith("disease_head"))
    labels_entry = next(e for n, e in entries.items() if n.startswith("bioclip_labels"))
    head = headfile.read_linear_head(FILES / head_entry["file"])
    matrix = headfile.read_label_matrix(FILES / labels_entry["file"])
    species_rows = [i for i, l in enumerate(matrix["labels"]) if l["table"] == "species"]
    species_ids = [matrix["labels"][i]["id"] for i in species_rows]
    text = matrix["rows"][species_rows]
    species_temperature = entries["bioclip"].get("config", {}).get("species_temperature", 1.0)

    rows = [l.split("\t") for l in (ROOT / "ml/training/data/splits/test.tsv").read_text().splitlines()[1:]]
    picked, left = [], dict(PER_SPECIES)
    for dataset, path, species, _problem in rows:
        if dataset == "plantdoc" and left.get(species, 0) > 0 and species in species_ids:
            picked.append((path, species))
            left[species] -= 1
    emb = Embedder(FILES / entries["bioclip"]["file"]).files([RAW / p for p, _ in picked])

    labels = head["labels"]
    species_of_class = np.asarray([l["species_id"] for l in labels])
    cases = []
    for (path, species), e in zip(picked, emb):
        sp = softmax((100.0 / species_temperature) * text @ e)
        top_sp = np.argsort(-sp)[:3]
        logits = head["weight"] @ e + head["bias"]
        idx = np.where(species_of_class == species)[0]
        p = softmax(logits[idx] / head["temperature"])
        order = np.argsort(-p)[:3]
        cases.append({
            "image": path,
            "species_id": species,
            "embedding": [round(float(x), 7) for x in e],
            "species_top3": [{"species_id": species_ids[i], "p": round(float(sp[i]), 6)} for i in top_sp],
            "disease_top3": [{"problem_id": labels[idx[j]]["problem_id"], "p": round(float(p[j]), 6)} for j in order],
        })

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({
        "about": "Reference outputs of the perception heads; see ml/tests/make_fixtures.py",
        "bioclip": {"version": entries["bioclip"]["version"], "sha256": entries["bioclip"]["sha256"]},
        "label_matrix": {"file": labels_entry["file"], "label_set_version": matrix["label_set_version"],
                         "species_temperature": species_temperature},
        "disease_head": {"file": head_entry["file"], "version": head["version"], "temperature": head["temperature"]},
        "tolerance": 1e-4,
        "cases": cases,
    }, indent=1) + "\n")
    print(f"{OUT}: {len(cases)} cases")


if __name__ == "__main__":
    main()
