"""One record per labelled image across every dataset, from the mapping files
(training.md §4, step 3).

A mapping file names its source, the folder per published split, and for each class folder
(the image's folder relative to its split folder) a `species_id` and `problem_id`. A class
with `problem_id: null`, or one missing from the mapping, is dropped and counted.

Two optional keys group a dataset's files (dedupe.py uses them):
- `originals`: `strip` is a regex of file-name suffixes that augmentation added; removing them
  repeatedly gives the original photo's name, and files with the same original are copies of
  one photo. `drop_if_name_in` lists sources whose file names mark a copy of their photos
  (for example a set that re-published PlantDoc's web photos); those files are dropped.
- `sessions`: `pattern` is a regex whose first group is a Unix time in milliseconds in the file
  name; files taken on the same UTC day form one session (one visit to a field).
"""

from __future__ import annotations

import collections
import datetime
import re
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
    original: str | None = None   # "<dataset>:<original photo name>" when `originals` is set
    session: str | None = None    # "<dataset>:<UTC day>" when `sessions` is set

    @property
    def target(self) -> str:
        return f"{self.species_id}/{self.problem_id}"


def original_name(name: str, strip: re.Pattern | None) -> str:
    """The file name without its extension and, repeatedly, without the suffixes in `strip`;
    lower case."""
    s = Path(name).stem if Path(name).suffix.lower() in IMAGE_EXT else name
    while strip is not None:
        t = strip.sub("", s)
        if t == s:
            break
        s = t
    return s.lower()


def source_names(source: str, strip: re.Pattern | None) -> set[str]:
    return {original_name(p.name, strip) for p in (RAW / source).rglob("*") if p.suffix.lower() in IMAGE_EXT}


def load(mapping: Path) -> tuple[list[Record], collections.Counter]:
    m = yaml.safe_load(mapping.read_text())
    root = RAW / m["source"] / m.get("root", ".")
    records, dropped = [], collections.Counter()
    orig = m.get("originals") or {}
    strip = re.compile(orig["strip"], re.IGNORECASE) if orig.get("strip") else None
    copied = set().union(*(source_names(src, strip) for src in orig.get("drop_if_name_in", [])))
    session = re.compile(m["sessions"]["pattern"]) if m.get("sessions") else None
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
            original = sess = None
            if orig:
                name = original_name(p.name, strip)
                if name in copied:
                    dropped[f"{label} (copy of a {'/'.join(orig['drop_if_name_in'])} photo)"] += 1
                    continue
                original = f"{m['dataset']}:{name}"
            if session is not None:
                hit = session.search(p.name)
                if hit is None:
                    raise ValueError(f"{p}: no timestamp for `sessions`")
                day = datetime.datetime.fromtimestamp(int(hit[1]) / 1000, datetime.UTC).date()
                sess = f"{m['dataset']}:{day.isoformat()}"
            records.append(Record(m["dataset"], split, p, label, c["species_id"], c["problem_id"], original, sess))
    return records, dropped


def load_all(names: list[str] | None = None, dropped_out: dict | None = None) -> list[Record]:
    """Every dataset's records; `dropped_out`, if given, receives each dataset's drop counts."""
    out = []
    for f in sorted((HERE / "mappings").glob("*.yaml")):
        if names and f.stem not in names:
            continue
        recs, dropped = load(f)
        if dropped_out is not None:
            dropped_out[f.stem] = dropped
        print(f"{f.stem}: {len(recs)} images" + (f", dropped {dict(dropped)}" if dropped else ""))
        out += recs
    return out
