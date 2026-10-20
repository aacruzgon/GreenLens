"""Validates the knowledge seed: the JSON Schema from the domain types, then the loader's
whole-set checks (contracts §10, checks and generated files).

    python3 -m ml.knowledge.validate                 # ml/knowledge/seed
    python3 -m ml.knowledge.validate --seed DIR      # another seed, e.g. a test fixture
    python3 -m ml.knowledge.validate --no-dose-check # skip the cargo step (never for a real build)
    python3 -m ml.knowledge.validate --drafts        # also pass records not yet reviewed

Every problem is reported, not only the first. Exit code 0 when the seed passes, 1 when it
doesn't, 2 when the dose check can't run.

- The schemas are ml/knowledge/schema/seed.schema.json, which a domain test keeps equal to the
  Rust types, so a misspelt key fails here as it would in the API.
- The dose check is the agent crate's own (`cargo run -p agent --example dose_check`), run on
  every treatment text and soil condition, with the seed's products: a port to Python would
  drift from the 400 lines of rules the plans and chat are checked with.
- A species, problem or organism with an empty `reviewed_by` is a draft from extract.py, and
  fails: model output is never loaded unreviewed (training.md §10, review). `--drafts` runs
  every other check on the drafts. A problem's `reviewed_by` and `reviewed_at` are signed
  together: a draft has both empty, and one without the other fails even with `--drafts`.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import Counter
from collections.abc import Callable
from pathlib import Path

import jsonschema

from ml.knowledge.seed import ROOT, SEED, Seed, load

SCHEMA = ROOT / "ml/knowledge/schema/seed.schema.json"
BACKEND = ROOT / "backend"
SINGULAR = {"sources": "source", "species": "species", "problems": "problem",
            "organisms": "organism", "treatments": "treatment"}

# A dose checker takes the ProductsSeed and [{"id", "text"}] and returns the failures,
# [{"id", "rule", "matched", "reason"}].
DoseChecker = Callable[[dict, list[dict]], list[dict]]


class DoseCheckUnavailable(Exception):
    pass


def cargo_dose_check(products: dict, texts: list[dict]) -> list[dict]:
    """The agent crate's dose check, through its example binary."""
    if not texts:
        return []
    try:
        run = subprocess.run(
            ["cargo", "run", "-q", "-p", "agent", "--example", "dose_check"],
            cwd=BACKEND, input=json.dumps({"products": products, "texts": texts}),
            capture_output=True, text=True, check=False)
    except FileNotFoundError as e:
        raise DoseCheckUnavailable("cargo is not installed (setup: backend role)") from e
    if run.returncode != 0:
        raise DoseCheckUnavailable(run.stderr.strip() or f"exit code {run.returncode}")
    return [json.loads(line) for line in run.stdout.splitlines() if line.strip()]


def schemas() -> tuple[dict, dict]:
    d = json.loads(SCHEMA.read_text(encoding="utf-8"))
    return d["schemas"], d["measurable_from"]


def _schema_errors(name: str, schema: dict, value, where: str) -> list[str]:
    v = jsonschema.validators.validator_for(schema)(
        schema, format_checker=jsonschema.FormatChecker())
    out = []
    for e in sorted(v.iter_errors(value), key=lambda e: list(e.absolute_path)):
        at = "/".join(str(p) for p in e.absolute_path) or "(top)"
        out.append(f"{where}: {name} at {at}: {e.message}")
    return out


def check(seed: Seed, dose: DoseChecker | None = cargo_dose_check, drafts: bool = False) -> list[str]:
    """Every problem with the seed, as one line each naming its file. Empty when it passes.
    With `drafts`, records nobody has reviewed yet pass too."""
    sch, measurable = schemas()
    errors: list[str] = []
    rel = lambda p: str(p.relative_to(seed.root)) if p.is_relative_to(seed.root) else str(p)
    path = lambda folder, record: rel(seed.path(record, seed.root / folder))

    # 1. The schema, file by file.
    if not isinstance(seed.sources, list):
        errors.append("sources.yaml: must be a list of sources")
        seed.sources = []
    for s in seed.sources:
        errors += _schema_errors("SourceSeed", sch["SourceSeed"], s, "sources.yaml")
    for folder, name in (("species", "SpeciesSeed"), ("problems", "ProblemSeed"),
                         ("organisms", "OrganismSeed"), ("treatments", "TreatmentSeed")):
        for d in getattr(seed, folder):
            # A draft's empty reviewed_at isn't a date; check 8 reports it, not the schema.
            shape = ({**d, "reviewed_at": "1970-01-01"}
                     if folder == "problems" and isinstance(d, dict) and d.get("reviewed_at") == "" else d)
            errors += _schema_errors(name, sch[name], shape, path(folder, d))
    errors += _schema_errors("ProductsSeed", sch["ProductsSeed"], seed.products, "products.yaml")
    if not isinstance(seed.soil_conditions, list):
        errors.append("soil_conditions.yaml: must be a list of soil conditions")
        seed.soil_conditions = []
    for s in seed.soil_conditions:
        errors += _schema_errors("SoilConditionSeed", sch["SoilConditionSeed"], s,
                                 "soil_conditions.yaml")
    if seed.classifier_labels is not None:
        errors += _schema_errors("ClassifierLabels", sch["ClassifierLabels"],
                                 seed.classifier_labels, "classifier_labels.yaml")
    for c in seed.chunks:
        errors += _schema_errors("ChunkSeed", sch["ChunkSeed"], c, path("chunks", c))
    if errors:
        return errors  # the set checks below assume the shapes are right

    # 2. File names follow the ids.
    for folder in ("species", "problems", "organisms", "treatments"):
        for d in getattr(seed, folder):
            p = seed.path(d, seed.root / folder / f"{d['id']}.yaml")
            if p.stem != d["id"]:
                errors.append(f"{rel(p)}: the file must be named {d['id']}.yaml")
    for c in seed.chunks:
        p = seed.path(c, seed.root / "chunks" / f"{c['source_id']}.jsonl")
        stem = p.name.split(":")[0].removesuffix(".jsonl")
        if stem != c["source_id"] or not c["id"].startswith(f"{c['source_id']}."):
            errors.append(f"{rel(p)}: chunk {c['id']} must be <source_id>.<n> in "
                          f"chunks/{c['source_id']}.jsonl")

    # 3. IDs are unique across every table, and so are indicator ids.
    ids = Counter()
    for folder in ("sources", "species", "problems", "organisms", "treatments", "chunks"):
        for d in getattr(seed, folder):
            ids[d["id"]] += 1
    for i, n in sorted(ids.items()):
        if n > 1:
            errors.append(f"id {i!r} is used {n} times; an id is unique across all tables")
    indicator_ids = Counter(ind["id"] for p in seed.problems for ind in p["indicators"])
    for i, n in sorted(indicator_ids.items()):
        if n > 1:
            errors.append(f"indicator {i!r} is used {n} times")
    names = Counter(p["name"].lower() for p in seed.products["products"])
    for n, k in sorted(names.items()):
        if k > 1:
            errors.append(f"products.yaml: {n!r} is listed {k} times (names are matched "
                          "case-insensitively)")
    soil_keys = Counter((s["aspect"], s["value"]) for s in seed.soil_conditions)
    for (a, v), k in sorted(soil_keys.items()):
        if k > 1:
            errors.append(f"soil_conditions.yaml: {a} {v} is listed {k} times")

    # 4. References resolve.
    known = {t: {d["id"] for d in getattr(seed, t)}
             for t in ("sources", "species", "problems", "organisms", "treatments")}

    def refs(where: str, kind: str, values, table: str):
        for v in values or []:
            if v not in known[table]:
                errors.append(f"{where}: {kind} {v!r} is not a known {SINGULAR[table]} id")

    for s in seed.species:
        w = path("species", s)
        refs(w, "source", s["sources"], "sources")
        for img in s.get("reference_images") or []:
            refs(w, "reference image source", [img["source_id"]], "sources")
    for p in seed.problems:
        w = path("problems", p)
        refs(w, "affects", p["affects"], "species")
        refs(w, "caused_by", p["caused_by"], "organisms")
        refs(w, "treatment", p["treatments"], "treatments")
        refs(w, "source", p["sources"], "sources")
        refs(w, "looks_like", [l["problem"] for l in p["looks_like"]], "problems")
    for o in seed.organisms:
        w = path("organisms", o)
        refs(w, "preys_on", o.get("preys_on"), "organisms")
        refs(w, "source", o["sources"], "sources")
    for t in seed.treatments:
        refs(path("treatments", t), "source", t["sources"], "sources")
    for s in seed.soil_conditions:
        refs(f"soil_conditions.yaml ({s['aspect']} {s['value']})", "source", s["sources"], "sources")
    for c in seed.chunks:
        w = path("chunks", c)
        refs(w, "source", [c["source_id"]], "sources")
        refs(w, "crop", [c["crop"]] if c.get("crop") else [], "species")
        refs(w, "problem", [c["problem"]] if c.get("problem") else [], "problems")
        refs(w, "organism", [c["organism"]] if c.get("organism") else [], "organisms")

    # 5. Problems: indicators, exactly 3 discriminating, measures, look-alikes written once.
    pairs = Counter()
    for p in seed.problems:
        w = path("problems", p)
        for ind in p["indicators"]:
            if not ind["id"].startswith(f"{p['id']}."):
                errors.append(f"{w}: indicator {ind['id']!r} must start with {p['id']}.")
            m = ind.get("measure")
            if m:
                if not set(measurable.get(m["quantity"], [])) & set(ind["image_kinds"]):
                    errors.append(f"{w}: {ind['id']} measures {m['quantity']}, which none of "
                                  f"{ind['image_kinds']} can measure (only "
                                  f"{measurable.get(m['quantity'], [])})")
                if m.get("min") is None and m.get("max") is None:
                    errors.append(f"{w}: {ind['id']}'s measure needs min or max")
                if (m.get("min") is not None and m.get("max") is not None
                        and m["min"] >= m["max"]):
                    errors.append(f"{w}: {ind['id']}'s measure has min >= max")
        n = sum(1 for ind in p["indicators"] if ind["discriminating"])
        if n != 3:
            errors.append(f"{w}: {n} discriminating indicators; exactly 3 are needed")
        t = p["conditions"].get("temp_c")
        if t and t["min"] >= t["max"]:
            errors.append(f"{w}: conditions.temp_c has min >= max")
        for l in p["looks_like"]:
            if l["problem"] == p["id"]:
                errors.append(f"{w}: a problem can't look like itself")
            pairs[frozenset((p["id"], l["problem"]))] += 1
    for pair, n in sorted(pairs.items(), key=lambda kv: sorted(kv[0])):
        if n > 1:
            a, b = sorted(pair)
            errors.append(f"looks_like between {a} and {b} is written {n} times; write it once, "
                          "in one of the two files (the loader writes both directions)")

    # 6. Species: care, the classifier, reference photos.
    labelled = set()
    if seed.classifier_labels:
        for l in seed.classifier_labels["labels"]:
            if l["species_id"] not in known["species"]:
                errors.append(f"classifier_labels.yaml: index {l['index']}: unknown species "
                              f"{l['species_id']!r}")
            if l["problem_id"] != "healthy" and l["problem_id"] not in known["problems"]:
                errors.append(f"classifier_labels.yaml: index {l['index']}: unknown problem "
                              f"{l['problem_id']!r}")
            labelled.add(l["species_id"])
        dup = Counter(l["index"] for l in seed.classifier_labels["labels"])
        errors += [f"classifier_labels.yaml: index {i} is listed {n} times"
                   for i, n in sorted(dup.items()) if n > 1]
    sources = {s["id"]: s for s in seed.sources}
    for s in seed.species:
        w = path("species", s)
        if s["profile"] == "full" and not s.get("care"):
            errors.append(f"{w}: a full species needs care")
        if s.get("care"):
            ph = s["care"]["soil_ph"]
            if ph["min"] >= ph["max"]:
                errors.append(f"{w}: care.soil_ph has min >= max")
        if s["classifier_supported"] != (s["id"] in labelled):
            has = "has" if s["id"] in labelled else "has no"
            errors.append(f"{w}: classifier_supported is {str(s['classifier_supported']).lower()}"
                          f" but classifier_labels.yaml {has} class for it")
        imgs = s.get("reference_images") or []
        if len(imgs) > 3:
            errors.append(f"{w}: {len(imgs)} reference images; at most 3")
        for img in imgs:
            if not (seed.root / "images" / img["file"]).is_file():
                errors.append(f"{w}: reference image images/{img['file']} is missing")
            src = sources.get(img["source_id"])
            if src is not None and not src.get("licence"):
                errors.append(f"{w}: reference image source {img['source_id']!r} has no licence")

    # 7. Organisms: invasive ones say where to report them.
    for o in seed.organisms:
        if o["role"] == "invasive" and not o.get("reporting_url"):
            errors.append(f"{path('organisms', o)}: an invasive organism needs reporting_url")

    # 8. Every record was reviewed by a teammate, and a problem's review is dated.
    for folder in ("species", "problems", "organisms"):
        for d in getattr(seed, folder):
            by = str(d.get("reviewed_by") or "").strip()
            at = str(d.get("reviewed_at") or "").strip() if folder == "problems" else by
            if by and not at:
                errors.append(f"{path(folder, d)}: reviewed_by is set but reviewed_at is empty; "
                              "sign both")
            elif at and not by:
                errors.append(f"{path(folder, d)}: reviewed_at is set but reviewed_by is empty; "
                              "sign both")
            elif not by and not drafts:
                errors.append(f"{path(folder, d)}: not reviewed yet (reviewed_by is empty)")

    # 9. The dose check, on everything the seed advises.
    if dose is not None:
        texts = [{"id": f"{path('treatments', t)}: text", "text": t["text"]}
                 for t in seed.treatments]
        for s in seed.soil_conditions:
            w = f"soil_conditions.yaml ({s['aspect']} {s['value']})"
            texts += [{"id": f"{w}: description", "text": s["description"]},
                      {"id": f"{w}: what_to_do", "text": s["what_to_do"]}]
        for f in dose(seed.products, texts):
            errors.append(f"{f['id']}: fails the dose check ({f['rule']}): {f['reason']}")
    return errors


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--no-dose-check", action="store_true")
    ap.add_argument("--drafts", action="store_true", help="pass records not yet reviewed")
    args = ap.parse_args()
    seed = load(args.seed)
    try:
        errors = check(seed, None if args.no_dose_check else cargo_dose_check, args.drafts)
    except DoseCheckUnavailable as e:
        print(f"the dose check can't run: {e}", file=sys.stderr)
        return 2
    for e in errors:
        print(e)
    counts = (f"{len(seed.sources)} sources, {len(seed.species)} species, {len(seed.problems)} "
              f"problems, {len(seed.organisms)} organisms, {len(seed.treatments)} treatments, "
              f"{len(seed.chunks)} chunks")
    print(f"{'FAIL' if errors else 'ok'}: {counts}; {len(errors)} problem(s)", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
