"""One record per labelled image across every dataset, from the mapping files
(training.md §4, step 3).

A mapping file names its source, the folder per published split, and for each class folder
(the image's folder relative to its split folder) a `species_id` and `problem_id`. A class
with `problem_id: null`, or one missing from the mapping, is dropped and counted.

Two optional keys group a dataset's files (dedupe.py uses them):
- `originals`: `strip` is a regex of file-name suffixes that augmentation added; removing them
  repeatedly gives the original photo's name, and files with the same original are copies of
  one photo. `drop_if_name_in` lists sources whose file names mark a copy of their photos
  (for example a set that re-published PlantDoc's web photos); those files are dropped. When
  the names don't link copies, `phash: N` groups them by perceptual hash within N bits instead,
  flipped, turned a quarter or turned by any angle (copies.py).
- `sessions`: when each photo was taken. `pattern` is a regex whose first group is a Unix time
  in milliseconds in the file name; `exif: true` reads the photo's EXIF DateTimeOriginal (or
  DateTime) and camera model instead, a photo with neither being a session of its own. Files
  taken on the same UTC day form one session (one visit to a field); with `gap_s`, files of one
  camera taken at most that many seconds after the previous one do instead (one plant or row,
  for sets whose days hold most of their photos).

One optional key thins a large set: `cap: {per_class: N}` keeps, per (species, problem), the N
files that come first in the order of the SHA-256 of "<dataset>:<path under the split folder>",
so the sample is fixed and doesn't depend on file order.
"""

from __future__ import annotations

import collections
import dataclasses
import datetime
import hashlib
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


def exif_time(path: Path) -> tuple[datetime.datetime, str] | None:
    """When the photo was taken (EXIF DateTimeOriginal, else DateTime) and the camera model."""
    from PIL import Image

    with Image.open(path) as im:
        ex = im.getexif()
        stamp = ex.get_ifd(0x8769).get(36867) or ex.get(306)
        model = str(ex.get(272) or "").strip()
    try:
        return datetime.datetime.strptime(str(stamp).strip(), "%Y:%m:%d %H:%M:%S"), model
    except ValueError:
        return None


def sessions_by_gap(times: list[tuple[int, datetime.datetime, str]], gap_s: float, dataset: str) -> dict[int, str]:
    """(record index, time, camera) → session: one camera's photos at most `gap_s` apart, chained."""
    out, last = {}, {}
    for i, t, cam in sorted(times, key=lambda x: (x[2], x[1], x[0])):
        prev = last.get(cam)
        if prev is None or (t - prev[0]).total_seconds() > gap_s:
            prev = (t, f"{dataset}:{cam or '?'}:{t.isoformat()}")
        last[cam] = (t, prev[1])
        out[i] = prev[1]
    return out


def load(mapping: Path) -> tuple[list[Record], collections.Counter]:
    m = yaml.safe_load(mapping.read_text())
    root = RAW / m["source"] / m.get("root", ".")
    records, dropped = [], collections.Counter()
    orig = m.get("originals") or {}
    strip = re.compile(orig["strip"], re.IGNORECASE) if orig.get("strip") else None
    copied = set().union(*(source_names(src, strip) for src in orig.get("drop_if_name_in", [])))
    sess_cfg = m.get("sessions") or {}
    session = re.compile(sess_cfg["pattern"]) if sess_cfg.get("pattern") else None
    copy_of = None
    if orig.get("phash") is not None:
        from ml.training.data import copies

        files = [p for f in m["splits"].values() for p in sorted((root / f).resolve().rglob("*"))
                 if p.suffix.lower() in IMAGE_EXT and p.is_file()]
        copy_of = copies.groups(files, int(orig["phash"]))
    cap = (m.get("cap") or {}).get("per_class")
    times: list[tuple[int, datetime.datetime, str]] = []
    for split, folder in m["splits"].items():
        base = (root / folder).resolve()
        found = []
        for p in sorted(base.rglob("*")):
            if p.suffix.lower() not in IMAGE_EXT or not p.is_file():
                continue
            label = p.parent.relative_to(base).as_posix()
            c = m["classes"].get(label)
            if not c or not c.get("problem_id"):
                dropped[label] += 1
                continue
            found.append((p, label, c))
        if cap:
            order = lambda f: hashlib.sha256(f"{m['dataset']}:{f[0].relative_to(base).as_posix()}".encode()).digest()
            by_target = collections.defaultdict(list)
            for f in sorted(found, key=order):
                by_target[(f[2]["species_id"], f[2]["problem_id"])].append(f)
            for fs in by_target.values():
                for p, label, _ in fs[cap:]:
                    dropped[f"{label} (over the cap of {cap} per class)"] += 1
            found = sorted((f for fs in by_target.values() for f in fs[:cap]), key=lambda f: f[0])
        for p, label, c in found:
            original = sess = None
            if copy_of is not None:
                original = f"{m['dataset']}:{copy_of[p].relative_to(root.resolve()).as_posix()}"
            elif orig:
                name = original_name(p.name, strip)
                if name in copied:
                    dropped[f"{label} (copy of a {'/'.join(orig['drop_if_name_in'])} photo)"] += 1
                    continue
                original = f"{m['dataset']}:{name}"
            if session is not None or sess_cfg.get("exif"):
                if session is not None:
                    hit = session.search(p.name)
                    if hit is None:
                        raise ValueError(f"{p}: no timestamp for `sessions`")
                    taken = (datetime.datetime.fromtimestamp(int(hit[1]) / 1000, datetime.UTC), "")
                else:
                    taken = exif_time(p)
                if taken is not None:
                    times.append((len(records), *taken))
                    sess = f"{m['dataset']}:{taken[0].date().isoformat()}"
            records.append(Record(m["dataset"], split, p, label, c["species_id"], c["problem_id"], original, sess))
    if sess_cfg.get("gap_s"):
        by_gap = sessions_by_gap(times, float(sess_cfg["gap_s"]), m["dataset"])
        records = [dataclasses.replace(r, session=by_gap.get(i)) for i, r in enumerate(records)]
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
