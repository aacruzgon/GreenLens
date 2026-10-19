"""Reads the knowledge seed (contracts §10, files) into plain dicts, as the JSON its YAML means.

    ml/knowledge/seed/
    ├─ sources.yaml              a list of SourceSeed
    ├─ species/<id>.yaml         SpeciesSeed
    ├─ problems/<id>.yaml        ProblemSeed (the `disease` table)
    ├─ organisms/<id>.yaml       OrganismSeed
    ├─ treatments/<id>.yaml      TreatmentSeed
    ├─ products.yaml             ProductsSeed
    ├─ soil_conditions.yaml      a list of SoilConditionSeed
    ├─ classifier_labels.yaml    ClassifierLabels, written by training
    ├─ chunks/<source_id>.jsonl  ChunkSeed, one per line
    └─ images/                   reference photos named by SpeciesSeed.reference_images

YAML 1.1 reads `2026-10-20` as a date and `no` as false. The seed's dates are strings in the
domain types and an unquoted `no` is a typo waiting to happen, so the loader resolves only
`true` and `false` as booleans and leaves timestamps as strings. Every file keeps its path, so
an error can name it.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
SEED = ROOT / "ml/knowledge/seed"

# Folders of one-record files, by the table they load into.
FOLDERS = {"species": "species", "problems": "disease", "organisms": "organism",
           "treatments": "treatment"}


class SeedLoader(yaml.SafeLoader):
    """SafeLoader without the timestamp resolver, and with `true`/`false` as the only booleans."""


SeedLoader.yaml_implicit_resolvers = {
    first: [(tag, rx) for tag, rx in resolvers
            if tag not in ("tag:yaml.org,2002:timestamp", "tag:yaml.org,2002:bool")]
    for first, resolvers in yaml.SafeLoader.yaml_implicit_resolvers.items()
}
SeedLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool", re.compile(r"^(?:true|True|TRUE|false|False|FALSE)$"),
    list("tTfF"))


def read_yaml(path: Path):
    return yaml.load(path.read_text(encoding="utf-8"), Loader=SeedLoader)


@dataclass
class Seed:
    """The whole seed. `paths` maps every record (by `id()`, so two records with the same seed id
    keep their own files) to the file it came from."""
    root: Path
    sources: list[dict] = field(default_factory=list)
    species: list[dict] = field(default_factory=list)
    problems: list[dict] = field(default_factory=list)
    organisms: list[dict] = field(default_factory=list)
    treatments: list[dict] = field(default_factory=list)
    products: dict = field(default_factory=lambda: {"products": []})
    soil_conditions: list[dict] = field(default_factory=list)
    classifier_labels: dict | None = None
    chunks: list[dict] = field(default_factory=list)
    paths: dict[int, Path] = field(default_factory=dict)

    def path(self, record: dict, default: Path) -> Path:
        return self.paths.get(id(record), default)


def load(root: Path = SEED) -> Seed:
    """Reads every file; records are in id order. A missing optional file reads as empty."""
    seed = Seed(root=root)
    if (p := root / "sources.yaml").exists():
        seed.sources = read_yaml(p) or []
        for s in seed.sources if isinstance(seed.sources, list) else []:
            seed.paths[id(s)] = p
    for folder in FOLDERS:
        records = []
        for p in sorted((root / folder).glob("*.yaml")):
            d = read_yaml(p)
            records.append(d)
            seed.paths[id(d)] = p
        setattr(seed, folder, sorted(records, key=lambda d: str(d.get("id", ""))
                                     if isinstance(d, dict) else ""))
    if (p := root / "products.yaml").exists():
        seed.products = read_yaml(p) or {"products": []}
    if (p := root / "soil_conditions.yaml").exists():
        seed.soil_conditions = read_yaml(p) or []
    if (p := root / "classifier_labels.yaml").exists():
        seed.classifier_labels = read_yaml(p)
    for p in sorted((root / "chunks").glob("*.jsonl")):
        for n, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
            if line.strip():
                c = json.loads(line)
                seed.chunks.append(c)
                seed.paths[id(c)] = p.with_name(f"{p.name}:{n}")
    return seed
