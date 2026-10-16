"""Check that the manifests, sources.toml, the model files and NOTICES.md agree
(docs/vision/README.md §6).

    python -m ml.models.check                  # files checked where present in ml/models/files
    python -m ml.models.check --files DIR      # e.g. a fetch.py destination

Fails (exit 1) unless:
- every entry of manifest.toml and manifest.cpu.toml has a sources.toml row for its SHA-256;
- one path never names two different files across the manifests (paths in pdm-models are
  never overwritten);
- a label matrix or disease head entry's name ends in its version, and its `label_set_version`
  or `classifier_labels_version` equals that version;
- NOTICES.md has a `## <entry name>` section for every entry;
- for each file present locally: its SHA-256 matches; a label matrix's or head's own version
  equals its entry's; a head's `features` names that manifest's `bioclip` file
  (`<version>+<first 16 hex of its SHA-256>`, ml/models/FORMATS.md), as the worker requires;
- when the knowledge seed's classifier_labels.yaml exists, a head's labels equal its rows in
  index order (skipped, with a note, until it does).

The temperature lives only in the head file (FORMATS.md), so it isn't compared with the manifest.
Standard library only, plus PyYAML for the seed comparison when the seed exists.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
MANIFESTS = ("manifest.toml", "manifest.cpu.toml")
SEED_LABELS = ROOT / "ml/knowledge/seed/classifier_labels.yaml"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def check(models: Path, files: Path) -> tuple[list[str], list[str]]:
    """Return (problems, notes)."""
    problems: list[str] = []
    notes: list[str] = []
    rows = {r["sha256"] for r in tomllib.loads((models / "sources.toml").read_text()).get("file", [])}
    notices = (models / "NOTICES.md").read_text() if (models / "NOTICES.md").exists() else ""
    if not notices:
        problems.append("NOTICES.md is missing")
    sections = set(re.findall(r"^## (.+?)\s*$", notices, re.M))
    paths: dict[str, str] = {}
    hashes: dict[str, str] = {}

    for name in MANIFESTS:
        entries = tomllib.loads((models / name).read_text())["model"]
        bioclip = next((e for e in entries if e["name"] == "bioclip"), None)
        for e in entries:
            where = f"{name}: {e['name']}"
            if e["sha256"] not in rows:
                problems.append(f"{where}: no sources.toml row for {e['sha256']}")
            if paths.setdefault(e["file"], e["sha256"]) != e["sha256"]:
                problems.append(f"{where}: {e['file']} names two different files across the manifests")
            if e["name"] not in sections:
                problems.append(f"{where}: NOTICES.md has no '## {e['name']}' section")
            key = ("label_set_version" if e["name"].startswith("bioclip_labels")
                   else "classifier_labels_version" if e["name"].startswith("disease_head") else None)
            if key:
                if not e["name"].endswith(e["version"]):
                    problems.append(f"{where}: the name doesn't end in version {e['version']!r}")
                if e.get(key) != e["version"]:
                    problems.append(f"{where}: {key} {e.get(key)!r} differs from version {e['version']!r}")

            path = files / e["file"]
            if not path.is_file():
                continue
            if e["file"] not in hashes:
                hashes[e["file"]] = sha256(path)
            if hashes[e["file"]] != e["sha256"]:
                problems.append(f"{where}: {path} doesn't match its SHA-256")
                continue
            if not e["file"].endswith(".json") or not key:
                continue
            doc = json.loads(path.read_text())
            own = doc.get("label_set_version") if key == "label_set_version" else doc.get("version")
            if own != e["version"]:
                problems.append(f"{where}: the file's version {own!r} differs from the entry's {e['version']!r}")
            if key == "classifier_labels_version" and bioclip:
                want = f"{doc['features'].split('+', 1)[0]}+{bioclip['sha256'][:16]}"
                if doc.get("features") != want:
                    problems.append(f"{where}: features {doc.get('features')!r}, but this manifest's "
                                    f"bioclip file is {want!r}; the worker would refuse it")
                problems += seed_problems(where, doc, notes)
    return problems, notes


def seed_problems(where: str, head: dict, notes: list[str]) -> list[str]:
    if not SEED_LABELS.exists():
        msg = f"{SEED_LABELS.relative_to(ROOT)} doesn't exist yet: head labels not compared with the seed"
        if msg not in notes:
            notes.append(msg)
        return []
    import yaml  # the ML environment has it; only needed once the seed exists

    seed = yaml.safe_load(SEED_LABELS.read_text())
    rows = seed if isinstance(seed, list) else seed.get("labels", [])
    want = [(r.get("species_id"), r.get("problem_id")) for r in rows]
    got = [(l.get("species_id"), l.get("problem_id")) for l in head["labels"]]
    return [] if got == want else [f"{where}: labels differ from {SEED_LABELS.name}'s rows in index order"]


def main() -> int:
    ap = argparse.ArgumentParser(description="Check the manifests, sources, files and notices agree.")
    ap.add_argument("--files", type=Path, default=HERE / "files")
    args = ap.parse_args()
    problems, notes = check(HERE, args.files.expanduser())
    for n in notes:
        print(f"note: {n}")
    for p in problems:
        print(f"FAIL: {p}")
    print("ok" if not problems else f"{len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
