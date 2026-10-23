"""The label versions the API asks for: the knowledge seed's `label_set_version`, which names the
BioCLIP label matrix, and its `classifier_labels_version`, which names the disease head
(contracts §10). Both are computed from the seed as `ml.knowledge.build_migration` computes the
`knowledge_meta` it writes into knowledge.surql, and `build_migration --check` keeps that file equal
to the seed. During a change-over the manifest lists the old matrix or head beside the new one
(docs/vision/README.md §6), so scripts that score the worker's stages pick by version, not by order.
"""

from __future__ import annotations

import json
from pathlib import Path

from ml.knowledge import build_migration as bm
from ml.knowledge.seed import SEED, load


def label_set_version(seed: Path = SEED, manifest: Path = bm.MANIFEST) -> str:
    """The seed's label_set_version, which api puts in every perception payload."""
    labels = bm.bioclip_labels(load(seed))
    return bm.label_set_version(labels, bm.bioclip_revision(manifest), json.loads(bm.TEMPLATES.read_text()))


def classifier_labels_version(seed: Path = SEED) -> str | None:
    """classifier_labels.yaml's model_version, the disease head the API reads classes for; None
    before the file exists."""
    cl = load(seed).classifier_labels
    return cl["model_version"] if cl else None


def matrix_entry(entries: list[dict], seed: Path = SEED) -> dict:
    """The manifest entry of the label matrix the API asks for."""
    want = label_set_version(seed)
    for e in entries:
        if e["name"].startswith("bioclip_labels") and e.get("label_set_version") == want:
            return e
    raise SystemExit(f"the manifest has no bioclip_labels entry for label_set_version {want} "
                     "(python -m ml.training.labels.bioclip_text builds it from the seed)")
