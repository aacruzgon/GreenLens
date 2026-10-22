"""The label matrix the API asks for: the manifest's `bioclip_labels_*` entry whose version is
the loaded knowledge set's `label_set_version` (contracts §10). During a change-over the
manifest lists the old matrix beside the new one (docs/vision/README.md §6), so scripts that
score the worker's species stage pick by version, not by order.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
KNOWLEDGE = ROOT / "backend/migrations/knowledge.surql"
_LSV = re.compile(r"^UPSERT knowledge_meta:`current` CONTENT \{.*\blabel_set_version: '([0-9a-f]{16})'", re.M)


def label_set_version(knowledge: Path = KNOWLEDGE) -> str:
    """knowledge.surql's label_set_version, which api puts in every perception payload."""
    hit = _LSV.search(knowledge.read_text(encoding="utf-8"))
    if not hit:
        raise SystemExit(f"{knowledge}: no knowledge_meta label_set_version")
    return hit.group(1)


def matrix_entry(entries: list[dict], knowledge: Path = KNOWLEDGE) -> dict:
    """The manifest entry of the label matrix the API asks for."""
    want = label_set_version(knowledge)
    for e in entries:
        if e["name"].startswith("bioclip_labels") and e.get("label_set_version") == want:
            return e
    raise SystemExit(f"the manifest has no bioclip_labels entry for label_set_version {want} "
                     "(python -m ml.training.labels.bioclip_text builds it from the seed)")
