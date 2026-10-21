"""Builds backend/migrations/knowledge.surql from the reviewed seed (contracts §10, the
migration; database README §1, the knowledge file).

    python3 -m ml.knowledge.build_migration            # validate, embed, write the file
    python3 -m ml.knowledge.build_migration --check    # exit 1 if the committed file is stale

The file holds the whole knowledge set and replaces it: knowledge_meta first, then every edge
deleted, then per table the rows missing from the seed deleted and every record upserted in id
order, then the edges written again. Every write ends with RETURN NONE, and the file has no
USE, BEGIN, COMMIT or DEFINE: api wraps it in one transaction. The same seed gives the same bytes:
tables in a fixed order, records by id, object keys sorted, each float in the shortest form that
reads back as the same f32.

It also writes ml/knowledge/build/ (not committed): summary.json with label_set_version, the
BioCLIP label list and the species and organism options, and the reference photos as WebP under
their RustFS keys (photos.py makes them). build/images/ is cleared on every run, so it holds exactly
the photos in the seed. The seed-upload image doesn't use build/: it makes the same photos from the
seed and checks their keys against this file (upload.py). Removed species, problem and
organism ids are printed, so the reviewer sees them.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import shutil
import sys
import tomllib
from pathlib import Path

import numpy as np

from ml.knowledge import embed, validate
from ml.knowledge.photos import reference_image
from ml.knowledge.seed import ROOT, SEED, Seed, load

OUT = ROOT / "backend/migrations/knowledge.surql"
BUILD = ROOT / "ml/knowledge/build"
MANIFEST = ROOT / "ml/models/manifest.toml"
TEMPLATES = ROOT / "ml/models/templates/openai-imagenet-80.json"
MAX_BYTES = 48 * 1024 * 1024
# Edges, in the order they are written; each is deleted before the tables and rebuilt after.
EDGES = ("susceptible_to", "caused_by", "manages", "preys_on", "looks_like")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")


class BuildError(Exception):
    pass


# --- SurrealQL values -------------------------------------------------------------------------

def string(s: str) -> str:
    if CONTROL.search(s):
        raise BuildError(f"a control character in {s[:40]!r}")
    return "'" + (s.replace("\\", "\\\\").replace("'", "\\'").replace("\n", "\\n")
                  .replace("\r", "\\r").replace("\t", "\\t")) + "'"


def f32(x: float) -> str:
    """The shortest decimal that reads back as the same f32, always with a point."""
    if not math.isfinite(x):
        raise BuildError(f"a non-finite number: {x}")
    return np.format_float_positional(np.float32(x), unique=True, trim="0")


class Raw(str):
    """Already SurrealQL: a record id or a computed expression."""


def rec(table: str, key) -> Raw:
    if isinstance(key, str):
        return Raw(f"{table}:`{key}`")
    return Raw(f"{table}:[{', '.join(value(k) for k in key)}]")


def value(v) -> str:
    if isinstance(v, Raw):
        return str(v)
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return f32(v)
    if isinstance(v, str):
        return string(v)
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(value(x) for x in v) + "]"
    if isinstance(v, dict):
        items = [(k, x) for k, x in sorted(v.items()) if x is not None]
        return "{ " + ", ".join(f"{k}: {value(x)}" for k, x in items) + " }" if items else "{}"
    raise BuildError(f"can't write {type(v).__name__} {v!r}")


def upsert(r: Raw, content: dict) -> str:
    return f"UPSERT {r} CONTENT {value(content)} RETURN NONE;\n"


def delete_stale(table: str, keep: list[Raw]) -> str:
    if not keep:
        return f"DELETE {table} RETURN NONE;\n"
    return f"DELETE {table} WHERE id NOT IN [{', '.join(keep)}] RETURN NONE;\n"


# --- Generated values -------------------------------------------------------------------------

def bioclip_revision(manifest: Path = MANIFEST) -> str:
    """The BioCLIP 2 revision the manifest's `bioclip` entry was exported from."""
    for m in tomllib.loads(manifest.read_text())["model"]:
        if m["name"] == "bioclip":
            hit = re.search(r"imageomics/bioclip-2@([0-9a-f]{40})", m["source"])
            if hit:
                return hit.group(1)
    raise BuildError(f"{manifest}: no bioclip entry with an imageomics/bioclip-2@<revision> source")


def bioclip_labels(seed: Seed) -> list[dict]:
    return ([{"id": s["id"], "table": "species", "label": s["bioclip_label"]} for s in seed.species]
            + [{"id": o["id"], "table": "organism", "label": o["bioclip_label"]}
               for o in seed.organisms])


def label_set_version(labels: list[dict], revision: str, templates: list[str]) -> str:
    """ml/models/FORMATS.md, computing label_set_version: the same bytes as bioclip_text.py and
    store's label_set_version."""
    doc = {"model": f"bioclip-2@{revision}", "templates": templates,
           "labels": sorted(f"{l['table']}:{l['id']}\t{l['label']}" for l in labels)}
    blob = json.dumps(doc, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(blob).hexdigest()[:16]


def header(seed: Seed) -> str:
    """Names the seed by its path from the repository root, however it was passed, so the same
    seed gives the same bytes."""
    root = seed.root.resolve()
    where = root.relative_to(ROOT) if root.is_relative_to(ROOT) else root.name
    return f"-- Generated by ml/knowledge/build_migration.py from the seed in {where}. Do not edit.\n"


def upserted_ids(text: str, table: str) -> set[str]:
    return set(re.findall(rf"^UPSERT {table}:`([^`]+)`", text, re.M))


# --- The file ---------------------------------------------------------------------------------

def build(seed: Seed, embedder, revision: str, templates: list[str]) -> tuple[str, dict, dict]:
    """The migration text, the summary and the reference photos ({storage key: WebP bytes})."""
    labels = bioclip_labels(seed)
    lsv = label_set_version(labels, revision, templates)
    cl = seed.classifier_labels
    meta = {"label_set_version": lsv,
            "classifier_labels_version": cl["model_version"] if cl else "",
            "embedding_model": embedder.name, "embedding_dimension": embed.DIMENSION}
    srcs = lambda ids: [rec("source", i) for i in ids]
    parts = [header(seed), upsert(rec("knowledge_meta", "current"), meta)]
    parts += [f"DELETE {e} RETURN NONE;\n" for e in EDGES]

    def table(name: str, rows: list[tuple[Raw, dict]]):
        rows = sorted(rows, key=lambda r: r[0])
        parts.append(delete_stale(name, [r for r, _ in rows]))
        parts.extend(upsert(r, c) for r, c in rows)

    table("source", [(rec("source", s["id"]), {k: v for k, v in s.items() if k != "id"})
                     for s in seed.sources])
    photos: dict[str, bytes] = {}
    species_rows = []
    for s in seed.species:
        images = []
        for img in s.get("reference_images") or []:
            key, data = reference_image(seed, s["id"], img)
            photos[key] = data
            images.append({"storage_key": key, "credit": img["credit"],
                           "source": rec("source", img["source_id"])})
        species_rows.append((rec("species", s["id"]), {
            "scientific_name": s["scientific_name"], "common_names": s["common_names"],
            "profile": s["profile"], "care": s.get("care"),
            "classifier_supported": s["classifier_supported"],
            "bioclip_label": s["bioclip_label"], "sources": srcs(s["sources"]),
            "reviewed_by": s["reviewed_by"], "reference_images": images}))
    table("species", species_rows)
    table("disease", [(rec("disease", p["id"]), {
        "name": p["name"], "kind": p["kind"], "damage_patterns": p["damage_patterns"],
        "prevalence": p["prevalence"], "indicators": p["indicators"],
        "conditions": p["conditions"], "sources": srcs(p["sources"]),
        "reviewed_by": p["reviewed_by"], "reviewed_at": p["reviewed_at"]}) for p in seed.problems])
    table("organism", [(rec("organism", o["id"]), {
        "name": o["name"], "scientific_name": o["scientific_name"],
        "taxon_group": o["taxon_group"], "role": o["role"], "recognition": o["recognition"],
        "reporting_url": o.get("reporting_url"), "bioclip_label": o["bioclip_label"],
        "sources": srcs(o["sources"]), "reviewed_by": o["reviewed_by"]}) for o in seed.organisms])
    table("treatment", [(rec("treatment", t["id"]), {
        "kind": t["kind"], "text": t["text"], "sources": srcs(t["sources"])})
        for t in seed.treatments])
    table("product", [(rec("product", re.sub(r"[^a-z0-9]+", "_", p["name"].lower()).strip("_")),
                       {"name": p["name"], "broad_spectrum": p["broad_spectrum"]})
                      for p in seed.products["products"]])
    table("classifier_label", [(rec("classifier_label", [cl["model_version"], l["index"]]), {
        "label": l["label"], "species": rec("species", l["species_id"]),
        "problem": None if l["problem_id"] == "healthy" else rec("disease", l["problem_id"])})
        for l in (cl["labels"] if cl else [])])
    table("soil_condition", [(rec("soil_condition", [s["aspect"], s["value"]]), {
        "description": s["description"], "what_to_do": s["what_to_do"],
        "sources": srcs(s["sources"])}) for s in seed.soil_conditions])
    chunks = sorted(seed.chunks, key=lambda c: c["id"])
    vectors = embedder.embed_documents([c["text"] for c in chunks])
    table("knowledge_chunk", [(rec("knowledge_chunk", c["id"]), {
        "source": rec("source", c["source_id"]),
        "crop": rec("species", c["crop"]) if c.get("crop") else None,
        "problem": rec("disease", c["problem"]) if c.get("problem") else None,
        "organism": rec("organism", c["organism"]) if c.get("organism") else None,
        "topic": c["topic"], "text": c["text"], "text_hash": embed.text_hash(c["text"]),
        "embedding": [float(x) for x in v], "embedding_model": embedder.name})
        for c, v in zip(chunks, vectors)])

    edges = {e: [] for e in EDGES}
    for p in seed.problems:
        edges["susceptible_to"] += [(rec("species", s), rec("disease", p["id"])) for s in p["affects"]]
        edges["caused_by"] += [(rec("disease", p["id"]), rec("organism", o)) for o in p["caused_by"]]
        edges["manages"] += [(rec("treatment", t), rec("disease", p["id"])) for t in p["treatments"]]
        for l in p["looks_like"]:
            a, b = rec("disease", p["id"]), rec("disease", l["problem"])
            edges["looks_like"] += [(a, b, l["how_to_tell"]), (b, a, l["how_to_tell"])]
    for o in seed.organisms:
        edges["preys_on"] += [(rec("organism", o["id"]), rec("organism", x))
                              for x in o.get("preys_on") or []]
    for e in EDGES:
        for edge in sorted(edges[e]):
            if e == "looks_like":
                a, b, how = edge
                parts.append(f"INSERT RELATION INTO looks_like {{ id: looks_like:[{a}, {b}], "
                             f"in: {a}, out: {b}, how_to_tell: {string(how)} }} RETURN NONE;\n")
            else:
                parts.append(f"RELATE {edge[0]}->{e}->{edge[1]} RETURN NONE;\n")

    text = "".join(parts)
    if len(text.encode()) > MAX_BYTES:
        raise BuildError(f"the file is {len(text.encode()) / 2**20:.1f} MiB; the limit is 48 MiB")
    summary = {
        **meta, "labels": sorted(labels, key=lambda l: (l["table"], l["id"])),
        "species_options": [{"id": s["id"], "name": s["common_names"][0]} for s in seed.species],
        "organism_options": [{"id": o["id"], "name": o["name"]} for o in seed.organisms],
        "counts": {"sources": len(seed.sources), "species": len(seed.species),
                   "problems": len(seed.problems), "organisms": len(seed.organisms),
                   "treatments": len(seed.treatments), "chunks": len(seed.chunks)},
    }
    return text, summary, photos


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--out", type=Path, default=OUT)
    ap.add_argument("--check", action="store_true", help="exit 1 if --out isn't what the seed gives")
    args = ap.parse_args()
    seed = load(args.seed)
    try:
        errors = validate.check(seed)
    except validate.DoseCheckUnavailable as e:
        print(f"the dose check can't run: {e}", file=sys.stderr)
        return 2
    if errors:
        print("\n".join(errors))
        print(f"FAIL: {len(errors)} problem(s); no migration written", file=sys.stderr)
        return 1
    try:
        text, summary, photos = build(seed, embed.CachedEmbedder(), bioclip_revision(),
                                      json.loads(TEMPLATES.read_text()))
    except BuildError as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1
    old = args.out.read_text(encoding="utf-8").replace("\r\n", "\n") if args.out.exists() else ""
    if args.check:
        if old != text:
            print(f"{args.out} is stale: run python3 -m ml.knowledge.build_migration",
                  file=sys.stderr)
            return 1
        print(f"ok: {args.out} is current", file=sys.stderr)
        return 0
    for t in ("species", "disease", "organism"):
        for i in sorted(upserted_ids(old, t) - upserted_ids(text, t)):
            print(f"removed {t} {i}")
    args.out.write_text(text, encoding="utf-8", newline="\n")
    BUILD.mkdir(parents=True, exist_ok=True)
    (BUILD / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n")
    shutil.rmtree(BUILD / "images", ignore_errors=True)
    for key, data in photos.items():
        (BUILD / "images" / key).parent.mkdir(parents=True, exist_ok=True)
        (BUILD / "images" / key).write_bytes(data)
    c = summary["counts"]
    print(f"ok: {args.out.relative_to(ROOT) if args.out.is_relative_to(ROOT) else args.out}, "
          f"{len(text.encode()) / 1024:.0f} KiB: {c['species']} species, {c['problems']} problems, "
          f"{c['organisms']} organisms, {c['chunks']} chunks; label_set_version "
          f"{summary['label_set_version']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
