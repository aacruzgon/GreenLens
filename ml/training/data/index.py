"""One record per labelled image across every dataset, from the mapping files
(training.md §4, step 3).

A mapping file names its source, the folder per published split, and for each class folder
(the image's folder relative to its split folder) a `species_id` and `problem_id`. A class
with `problem_id: null`, or one missing from the mapping, is dropped and counted.
"""

from __future__ import annotations

import collections
from dataclasses import dataclass
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
RAW = HERE / "raw"
IMAGE_EXT = {".jpg", ".jpeg", ".png", ".webp", ".bmp"}


@dataclass(frozen=True)
class Record:
    dataset: str
    split: str          # the published split: train, test, validation
    path: Path
    label: str          # the dataset's class name
    species_id: str
    problem_id: str     # a slug or "healthy"

    @property
    def target(self) -> str:
        return f"{self.species_id}/{self.problem_id}"


def load(mapping: Path) -> tuple[list[Record], collections.Counter]:
    m = yaml.safe_load(mapping.read_text())
    root = RAW / m["source"] / m.get("root", ".")
    records, dropped = [], collections.Counter()
    for split, folder in m["splits"].items():
        base = (root / folder).resolve()
        for p in sorted(base.rglob("*")):
            if p.suffix.lower() not in IMAGE_EXT or not p.is_file():
                continue
            label = p.parent.relative_to(base).as_posix()
            c = m["classes"].get(label)
            if not c or not c.get("problem_id"):
                dropped[label] += 1
                continue
            records.append(Record(m["dataset"], split, p, label, c["species_id"], c["problem_id"]))
    return records, dropped


def load_all(names: list[str] | None = None) -> list[Record]:
    out = []
    for f in sorted((HERE / "mappings").glob("*.yaml")):
        if names and f.stem not in names:
            continue
        recs, dropped = load(f)
        print(f"{f.stem}: {len(recs)} images" + (f", dropped {dict(dropped)}" if dropped else ""))
        out += recs
    return out
