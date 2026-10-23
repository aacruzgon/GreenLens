"""Writes the knowledge seed's classifier_labels.yaml from the manifest's disease head
(training.md §8; contracts §10, files).

    python -m ml.training.labels.classifier_labels            # write the file, set classifier_supported
    python -m ml.training.labels.classifier_labels --check    # exit 1 if either is stale
    python -m ml.training.labels.classifier_labels --head disease-head-v3c-2026-10-20

The rows are the head's labels, by output index, from its manifest entry's `labels` (a copy of
the head file's that `python -m ml.models.check` compares with the file), so `--check` runs where
the model files aren't, as in CI. With several disease heads listed (a change-over, README 6) and
no `--head`, it takes the one the seed's classifier_labels.yaml already names, which is
knowledge.surql's `classifier_labels_version`.

A species goes in only when it has a species file and every one of its classes maps: each problem
is signed in the seed's problems/ (or is `healthy`). A draft from extract.py, with `reviewed_by`
still empty, doesn't count, since build_migration refuses to load it. A species with a class the
seed lacks stays out whole, so it never gets a diagnosis from only some of its classes, and its
problems are left to the VLM until their profiles are signed. Each species file's `classifier_supported` line is set to
whether the species went in, as the loader requires.

The worker reads the head file, not this one, so it still returns a left-out class's slug; the
API finds no profile for it (the #51 discussion, Oct 21).
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

import yaml

from ml.knowledge.seed import SEED, load
from ml.models.check import head_rows
from ml.training.labels import current

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "ml/models/manifest.toml"
OUT_NAME = "classifier_labels.yaml"
NO_FILE = "species file"
SUPPORTED = re.compile(r"^classifier_supported: (true|false)$", re.M)
HEADER = ("# Written by python -m ml.training.labels.classifier_labels from {file};\n"
          "# don't edit by hand. Left out until their problems are in the seed: {left}.\n")


def head_entry(manifest: Path, version: str | None, seed: Path = SEED) -> dict:
    """The disease head named by `version`; without one, the manifest's only head, or during a
    change-over the one the seed's classifier_labels.yaml names."""
    heads = [e for e in tomllib.loads(manifest.read_text())["model"]
             if e["name"].startswith("disease_head")]
    want = version or (current.classifier_labels_version(seed) if len(heads) > 1 else None)
    if want:
        heads = [e for e in heads if e["version"] == want]
    if len(heads) != 1:
        names = ", ".join(e["version"] for e in heads) or "none"
        raise SystemExit(f"{manifest}: need exactly one disease head (found {names}"
                         f"{f' for {want}' if want else ''}); pass --head")
    return heads[0]


def rows(labels: list[dict], problems: set[str], species: set[str]) -> tuple[list[dict], dict[str, list[str]]]:
    """The rows of every species with a species file whose classes all map, and what each
    left-out species lacks: its missing problems, and NO_FILE when it has no species file."""
    missing: dict[str, list[str]] = {}
    for l in labels:
        lacks = missing.setdefault(l["species_id"], [] if l["species_id"] in species else [NO_FILE])
        if l["problem_id"] != "healthy" and l["problem_id"] not in problems:
            lacks.append(l["problem_id"])
    keep = [{"index": l["index"], "label": l["label"], "species_id": l["species_id"],
             "problem_id": l["problem_id"]}
            for l in labels if not missing[l["species_id"]]]
    return keep, {s: m for s, m in missing.items() if m}


def render(version: str, file: str, keep: list[dict], left: dict[str, list[str]]) -> str:
    body = yaml.safe_dump({"model_version": version, "labels": keep},
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


def signed_problems(seed) -> set[str]:
    """The ids of the seed's signed problems: a draft's `reviewed_by` is still empty."""
    return {p["id"] for p in seed.problems if str(p.get("reviewed_by") or "").strip()}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--head", help="the disease head's version, when the manifest lists several")
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--check", action="store_true", help="exit 1 if the seed isn't what the head gives")
    args = ap.parse_args()

    entry = head_entry(args.manifest, args.head, args.seed)
    try:
        labels = head_rows(entry)
    except ValueError as e:
        raise SystemExit(f"{args.manifest}: {entry['name']}: {e}") from e
    seed = load(args.seed)
    keep, left = rows(labels, signed_problems(seed), {s["id"] for s in seed.species})
    text = render(entry["version"], entry["file"], keep, left)
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
    print(f"{len(keep)} of {len(labels)} classes ({', '.join(kept) or 'none'}) → {out}")
    for s, m in sorted(left.items()):
        problems = [x for x in m if x != NO_FILE]
        why = (["no species file"] if NO_FILE in m else []) + ([f"no problem {', '.join(problems)}"] if problems else [])
        print(f"left out {s}: {'; '.join(why)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
