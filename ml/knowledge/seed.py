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
from dataclasses import dataclass, field, replace
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


def is_signed(record: dict) -> bool:
    """Someone has reviewed it: a draft from extract.py has `reviewed_by` still empty."""
    return bool(str(record.get("reviewed_by") or "").strip())


def signed(seed: Seed) -> Seed:
    """The part of the seed that loads (#143): the species, problems and organisms someone has
    signed; the treatments a signed problem links, or that no problem links; every product, soil
    condition, source and chunk; and only the links between records that load. A chunk tagged with
    a draft keeps its text, with that tag empty. Drafts stay in the seed for their review, so a
    slice can merge before it's signed and loads on the first build after.

    The records it changes are copies, which keep their files in `paths`."""
    out = replace(seed, paths=dict(seed.paths))

    def keep(rows: list[dict], **links: set[str]) -> list[dict]:
        """`rows` with each list-of-ids field in `links` cut to the ids it names."""
        kept = []
        for r in rows:
            new = {**r, **{f: [x for x in r.get(f) or [] if x in ids] for f, ids in links.items()
                           if f in r}}
            out.paths[id(new)] = seed.paths.get(id(r), seed.root)
            kept.append(new)
        return kept

    species = {s["id"] for s in seed.species if is_signed(s)}
    problems = {p["id"] for p in seed.problems if is_signed(p)}
    organisms = {o["id"] for o in seed.organisms if is_signed(o)}
    linked = {t for p in seed.problems for t in p.get("treatments") or []}
    by_signed = {t for p in seed.problems if p["id"] in problems for t in p.get("treatments") or []}
    treatments = {t["id"] for t in seed.treatments if t["id"] in by_signed or t["id"] not in linked}

    out.species = [s for s in seed.species if s["id"] in species]
    out.organisms = keep([o for o in seed.organisms if o["id"] in organisms], preys_on=organisms)
    out.treatments = [t for t in seed.treatments if t["id"] in treatments]
    out.problems = keep([p for p in seed.problems if p["id"] in problems], affects=species,
                        caused_by=organisms, treatments=treatments)
    for p in out.problems:
        p["looks_like"] = [l for l in p.get("looks_like") or [] if l["problem"] in problems]
    loaded = {"crop": species, "problem": problems, "organism": organisms}
    out.chunks = []
    for c in seed.chunks:
        new = {**c, **{f: None for f, ids in loaded.items() if c.get(f) and c[f] not in ids}}
        out.paths[id(new)] = seed.paths.get(id(c), seed.root)
        out.chunks.append(new)
    return out
