"""Writes the knowledge seed's classifier_labels.yaml from the manifest's disease head
(training.md §8; contracts §10, files).

    python -m ml.training.labels.classifier_labels            # write the file, set classifier_supported
    python -m ml.training.labels.classifier_labels --check    # exit 1 if either is stale
    python -m ml.training.labels.classifier_labels --head disease-head-v3c-2026-10-20

The rows are the head file's own labels, by output index. A species goes in only when every one
of its classes maps: each problem is in the seed's problems/ (or is `healthy`). A species with a
class the seed lacks stays out whole, so it never gets a diagnosis from only some of its classes,
and its problems are left to the VLM until their profiles are signed. Each species file's
`classifier_supported` line is set to whether the species went in, as the loader requires.

The worker reads the head file, not this one, so it still returns a left-out class's slug; the
API finds no profile for it (the #51 discussion, Oct 21).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

import yaml

from ml.knowledge.seed import SEED, load

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "ml/models/manifest.toml"
FILES = ROOT / "ml/models/files"
OUT_NAME = "classifier_labels.yaml"
SUPPORTED = re.compile(r"^classifier_supported: (true|false)$", re.M)
HEADER = ("# Written by python -m ml.training.labels.classifier_labels from {file};\n"
          "# don't edit by hand. Left out until their problems are in the seed: {left}.\n")


def head_entry(manifest: Path, version: str | None) -> dict:
    heads = [e for e in tomllib.loads(manifest.read_text())["model"]
             if e["name"].startswith("disease_head")]
    if version:
        heads = [e for e in heads if e["version"] == version]
    if len(heads) != 1:
        names = ", ".join(e["version"] for e in heads) or "none"
        raise SystemExit(f"{manifest}: need exactly one disease head (found {names}); pass --head")
    return heads[0]


def rows(head: dict, problems: set[str]) -> tuple[list[dict], dict[str, list[str]]]:
    """The rows of every species whose classes all map, and each left-out species' missing problems."""
    missing: dict[str, list[str]] = {}
    for l in head["labels"]:
        missing.setdefault(l["species_id"], [])
        if l["problem_id"] != "healthy" and l["problem_id"] not in problems:
            missing[l["species_id"]].append(l["problem_id"])
    keep = [{"index": l["index"], "label": l["label"], "species_id": l["species_id"],
             "problem_id": l["problem_id"]}
            for l in head["labels"] if not missing[l["species_id"]]]
    return keep, {s: m for s, m in missing.items() if m}


def render(head: dict, file: str, keep: list[dict], left: dict[str, list[str]]) -> str:
    body = yaml.safe_dump({"model_version": head["version"], "labels": keep},
                          sort_keys=False, allow_unicode=True)
    return HEADER.format(file=file, left=", ".join(sorted(left)) or "none") + body


def species_files(seed_dir: Path, supported: set[str]) -> dict[Path, str]:
    """Each species file whose classifier_supported line must change, with its new text."""
    out = {}
    for p in sorted((seed_dir / "species").glob("*.yaml")):
        text = p.read_text(encoding="utf-8")
        sid = yaml.safe_load(text)["id"]
        if len(SUPPORTED.findall(text)) != 1:
            raise SystemExit(f"{p}: expected one top-level `classifier_supported: true|false` line")
        new = SUPPORTED.sub(f"classifier_supported: {str(sid in supported).lower()}", text)
        if new != text:
            out[p] = new
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--head", help="the disease head's version, when the manifest lists several")
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    ap.add_argument("--files", type=Path, default=FILES, help="where the manifest's files are")
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--check", action="store_true", help="exit 1 if the seed isn't what the head gives")
    args = ap.parse_args()

    entry = head_entry(args.manifest, args.head)
    path = args.files / entry["file"]
    head = json.loads(path.read_text())
    if head.get("version") != entry["version"]:
        raise SystemExit(f"{path}: version {head.get('version')!r}, the entry says {entry['version']!r}")
    seed = load(args.seed)
    keep, left = rows(head, {p["id"] for p in seed.problems})
    text = render(head, entry["file"], keep, left)
    out = args.seed / OUT_NAME
    species = species_files(args.seed, {r["species_id"] for r in keep})
    stale = not out.exists() or out.read_text(encoding="utf-8") != text or bool(species)

    if args.check:
        if stale:
            print(f"{out} or a species' classifier_supported is stale: "
                  "run python -m ml.training.labels.classifier_labels", file=sys.stderr)
            return 1
        print(f"ok: {out.name} is current", file=sys.stderr)
        return 0
    out.write_text(text, encoding="utf-8", newline="\n")
    for p, new in species.items():
        p.write_text(new, encoding="utf-8", newline="\n")
        print(f"{p.relative_to(args.seed.parent)}: classifier_supported changed")
    kept = sorted({r["species_id"] for r in keep})
    print(f"{len(keep)} of {head['classes']} classes ({', '.join(kept) or 'none'}) → {out}")
    for s, m in sorted(left.items()):
        print(f"left out {s}: no problem {', '.join(m)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
