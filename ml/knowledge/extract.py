"""Step 4 of the ingestion (training.md §10, profile extraction): tagged chunks → draft profiles.

    python3 -m ml.knowledge.extract              # draft every plan.yaml entry with no seed file yet
    python3 -m ml.knowledge.extract --only tomato_early_blight --redo

For each problem, organism and species in the plan, the strongest model (claude-opus-5-5, at
high effort) reads the tagged chunks about it, from every source, each with its chunk id, and
drafts the record in the seed format. The chunks come from build/tags (tag.py), where a chunk
lists every problem and organism it covers, so a chunk comparing two problems reaches both, and
the chunks of plan.yaml's `facts_only_sources` are there too. Every indicator, condition,
look-alike, treatment and product comes back with the chunk ids it was taken from. They go into
a review file next to the draft (<id>.review.md) with each cited excerpt in full, so the reviewer
can check each line against its source; a facts-only excerpt is named and linked, not quoted.

A draft is written with `reviewed_by: ""` and `reviewed_at: ""`, which build_migration.py
leaves out of knowledge.surql until it's signed (seed.signed; validate.py --drafts checks the rest), and with `discriminating: false`
everywhere: discriminate.py proposes the three. Treatments are shared: the model sees the seed's
treatments and reuses one by id where it fits, and each is reviewed, with its text, in the review
file of every problem that links it. Products named in a treatment go to products.yaml with a
broad-spectrum flag for the plan validator. The schema is the seed's own (seed.schema.json, from
the domain types), sanitised for the API like every hosted call.

The API compiles a schema into a grammar and refuses one that grows too large, so chunk ids and
existing treatment ids are plain strings in the schema, checked here instead: an id the model
wasn't given is dropped and listed in the review file.
"""

from __future__ import annotations

import argparse
import copy
import itertools
import json
import re
import sys
from datetime import date
from pathlib import Path

import yaml

from ml.knowledge import llm, validate
from ml.knowledge.seed import SEED, load, read_yaml
from ml.knowledge.tag import PLAN, TAGS

EFFORT = "high"
MAX_TOKENS = 16000

RULES = """You draft records for a garden plant-health app's knowledge base from excerpts of \
university extension and government fact sheets. The app uses them to check a diagnosis \
against photos and to write a gardener's action plan, so a wrong line becomes confident wrong \
advice. A teammate reviews every draft against its excerpts.

Rules:
- Use only the given excerpts. Leave a field empty or null when they don't support it, and say \
what is missing in `notes`. Never fill a gap from general knowledge.
- Cite, for every item, the ids of the excerpts it comes from.
- Never write a dose, a rate, a dilution, a mixing ratio or a spray interval, even when an \
excerpt gives one. Name a product only generically ("a copper fungicide labelled for \
tomatoes"), and tell the gardener to follow its label.
- Write in plain, short sentences a home gardener understands. Metric units only: convert \
inches to mm or cm and °F to °C."""

PROBLEM_RULES = RULES + """

For a plant problem:
- Write each symptom the excerpts describe as one checkable indicator: `text` states it; \
`question` is a yes/no question a person could answer by looking at a photo, in plain words a \
small vision model reads well ("Do the brown spots have rings inside them, like a target?"). \
One sign per indicator.
- Give a `measure` only when an excerpt states a size or a count, converted to the quantity's \
unit (lesion_diameter_mm, hole_diameter_mm and the others). A size that comes with other signs \
becomes two indicators: one for the size, with the measure, and one for the signs. The phone \
measures each quantity only in some shots: {measurable}. A size stated for any other shot gets \
no indicator of its own; put it in the sign's text in relative words ("pinpoint specks", \
"raised, scabby spots").
- Set `where` and `image_kinds` from where the symptom shows: leaf_closeup for leaf spots, \
leaf_underside for what is under the leaf, whole_plant for wilting, yellowing or stunting, \
flower_fruit for fruit symptoms.
- `weight`: 3 for a symptom an excerpt calls characteristic, typical or diagnostic, or that \
sets the problem apart; 2 for a common one; 1 for one that "may" or "sometimes" appear.
- Aim for 4 to 8 indicators when the excerpts support them, covering leaves first.
- `looks_like`: every listed problem an excerpt compares this one with or says is confused \
with it, and the sign the excerpts give that tells them apart. When the excerpts give sizes or \
textures for both, use them; say "no sign in the excerpts" only when none does.
- `conditions`: the weather that favours it, as the excerpts state it.
- `treatments`: integrated pest management first: cultural, mechanical and biological steps \
before chemical ones. Reuse an existing treatment id when its text says the same thing.
- A chemical treatment names its active ingredient or group as the excerpts do ("a pyrethroid \
such as bifenthrin"), never only "an insecticide", so the app can tell whether it harms \
beneficials. Keep any warning the excerpts attach to it, such as a variety it must not be used \
on, or a days-to-harvest wait, in words, without numbers of a rate.
- A protectant (copper, chlorothalonil, mancozeb) works only before infection: word it as \
prevention, started before symptoms or for next season, when an excerpt says so.
- `products`: every pest-control product a treatment names, by its active ingredient or type \
in one or two lowercase words, as it appears in the treatment text ("copper", "chlorothalonil", \
"insecticidal soap", "pyrethroid"). `broad_spectrum` is true for anything that kills beneficial \
insects or mites on contact or through its residue: the contact and systemic insecticides and \
miticides (organophosphates, carbamates, pyrethroids, pyrethrins, neonicotinoids, spinosad), \
insecticidal soap and every oil (neem, horticultural, canola), sulfur, which is also sold as a \
fungicide, and insect-killing fungi such as Beauveria bassiana. It is false only for what the \
pest must eat (Bacillus thuringiensis) and for the other fungicides and bactericides (copper, \
chlorothalonil, mancozeb). When unsure, true: a wrong true only offers a gentler step."""

ORGANISM_RULES = RULES + """

For an organism: `recognition` says how to recognise it in a photo (size, colour, shape, where \
it sits), in one to three sentences. `preys_on` lists only pests an excerpt says it eats or \
parasitises."""

SPECIES_RULES = RULES + """

For a crop: the care fields, as the excerpts state them. `hardiness_zones` is "annual" for a \
crop grown as an annual."""


def _defs(sch: dict, name: str) -> dict:
    return copy.deepcopy(sch[name]["$defs"])


def _prune(schema: dict) -> dict:
    """Keeps only the $defs the schema reaches: every definition adds to the API's grammar."""
    defs, keep, todo = schema.get("$defs", {}), set(), [schema]
    while todo:
        text = json.dumps({k: v for k, v in todo.pop().items() if k != "$defs"})
        for name in defs:
            if f'"#/$defs/{name}"' in text and name not in keep:
                keep.add(name)
                todo.append(defs[name])
    schema["$defs"] = {k: v for k, v in defs.items() if k in keep}
    return schema


def _enum(values) -> dict:
    return {"type": "string", "enum": sorted(set(values))}


def _cited(chunk_ids) -> dict:
    """Chunk ids, checked after the reply (`_known`), not by an enum: see the module docstring."""
    return {"type": "array", "items": {"type": "string"},
            "description": "Ids of the excerpts this comes from, as given in square brackets."}


def problem_schema(sch: dict, chunk_ids, look_alikes, treatments) -> dict:
    defs = _defs(sch, "ProblemSeed")
    defs.update({k: v for k, v in _defs(sch, "TreatmentSeed").items() if k not in defs})
    indicator = defs["IndicatorSeed"]
    indicator["properties"].pop("discriminating")
    indicator["properties"].pop("id")
    indicator["properties"]["slug"] = {"type": "string",
                                       "description": "Short snake_case name, unique in this record."}
    indicator["properties"]["chunk_ids"] = _cited(chunk_ids)
    indicator["required"] = ["slug", "text", "question", "weight", "where", "image_kinds",
                             "measure", "chunk_ids"]
    props = {
        "kind": {"$ref": "#/$defs/ProblemKind"},
        "prevalence": {"$ref": "#/$defs/Prevalence"},
        "damage_patterns": {"type": "array", "items": {"$ref": "#/$defs/DamagePattern"}},
        "indicators": {"type": "array", "items": {"$ref": "#/$defs/IndicatorSeed"}},
        "conditions": {"$ref": "#/$defs/Conditions"},
        "conditions_chunk_ids": _cited(chunk_ids),
        "new_treatments": {"type": "array", "items": {
            "type": "object", "additionalProperties": False,
            "required": ["id", "kind", "text", "chunk_ids"],
            "properties": {"id": {"type": "string", "description": "snake_case"},
                           "kind": {"$ref": "#/$defs/TreatmentKind"},
                           "text": {"type": "string"}, "chunk_ids": _cited(chunk_ids)}}},
        "products": {"type": "array", "items": {
            "type": "object", "additionalProperties": False,
            "required": ["name", "broad_spectrum", "chunk_ids"],
            "properties": {"name": {"type": "string"}, "broad_spectrum": {"type": "boolean"},
                           "chunk_ids": _cited(chunk_ids)}}},
        "notes": {"type": "string", "description": "For the reviewer: gaps and doubts."},
    }
    required = list(props)
    if look_alikes:
        props["looks_like"] = {"type": "array", "items": {
            "type": "object", "additionalProperties": False,
            "required": ["problem", "how_to_tell", "chunk_ids"],
            "properties": {"problem": _enum(look_alikes), "how_to_tell": {"type": "string"},
                           "chunk_ids": _cited(chunk_ids)}}}
        required.append("looks_like")
    if treatments:
        props["existing_treatments"] = {"type": "array", "items": {"type": "string"},
                                        "description": "Ids from the list of existing treatments."}
        required.append("existing_treatments")
    for k in ("ProblemSeed", "IndicatorSeed"):
        defs.pop(k, None)
    defs["IndicatorSeed"] = indicator
    return _prune({"type": "object", "additionalProperties": False, "required": required,
                   "properties": props, "$defs": defs})


def organism_schema(sch: dict, chunk_ids, prey) -> dict:
    defs = _defs(sch, "OrganismSeed")
    props = {
        "taxon_group": {"$ref": "#/$defs/TaxonGroup"},
        "role": {"$ref": "#/$defs/SeedOrganismRole"},
        "recognition": {"type": "string"},
        "recognition_chunk_ids": _cited(chunk_ids),
        "notes": {"type": "string"},
    }
    if prey:
        props["preys_on"] = {"type": "array", "items": _enum(prey)}
        props["preys_on_chunk_ids"] = _cited(chunk_ids)
    return _prune({"type": "object", "additionalProperties": False, "required": list(props),
                   "properties": props, "$defs": defs})


def species_schema(sch: dict, chunk_ids) -> dict:
    defs = _defs(sch, "SpeciesSeed")
    care = defs.pop("Care")
    return {"type": "object", "additionalProperties": False,
            "required": ["care", "care_chunk_ids", "notes"],
            "properties": {"care": care, "care_chunk_ids": _cited(chunk_ids),
                           "notes": {"type": "string"}},
            "$defs": {k: v for k, v in defs.items() if k in ("LightNeed", "WaterNeed", "Range")}}


def _known(reply, given: set[str]) -> list[str]:
    """Drops, in place, every cited chunk id the model wasn't given; returns what it dropped."""
    dropped = []
    if isinstance(reply, dict):
        for k, v in reply.items():
            if k.endswith("chunk_ids") and isinstance(v, list):
                dropped += [i for i in v if i not in given]
                reply[k] = [i for i in v if i in given]
            else:
                dropped += _known(v, given)
    elif isinstance(reply, list):
        for v in reply:
            dropped += _known(v, given)
    return dropped


def excerpts(chunks: list[dict]) -> str:
    return "\n\n".join(f"[{c['id']}] ({c['topic']})\n{c['text']}" for c in chunks)


def _sources(chunks_by_id: dict, ids) -> list[str]:
    return sorted({chunks_by_id[i]["source_id"] for i in ids if i in chunks_by_id})


def _quote(chunks_by_id: dict, ids, facts_only: set[str] = frozenset(), urls: dict | None = None) -> str:
    """Each cited excerpt in full (at most about 200 words), or its id and link for a facts-only
    source (plan.yaml), whose wording isn't committed, and for a withheld chunk."""
    out = []
    for i in sorted(set(ids)):
        c = chunks_by_id.get(i)
        if not c:
            continue
        if c["source_id"] in facts_only or c.get("withheld"):
            w = c.get("withheld")
            why = "wording not committed" if not w else "it gives a rate" if w.startswith("dose check") else w
            out.append(f"  - `{i}` ({(urls or {}).get(c['source_id'], c['source_id'])}, {why}: run "
                       f"fetch and chunk, then read build/chunks/{c['source_id']}.jsonl)")
        else:
            out.append(f"  - `{i}`: {c['text'].replace(chr(10), ' ')}")
    return "\n".join(out) or "  - (no excerpt cited)"


def tagged_chunks(tags: Path = TAGS) -> list[dict]:
    """The chunks tag.py kept, with their `problems` and `organisms` lists."""
    out = []
    for path in sorted(tags.glob("*.jsonl")):
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                c = json.loads(line)
                if not c.get("dropped"):
                    out.append({**c, "source_id": c.get("source_id") or path.stem})
    return out


class Extractor:
    def __init__(self, seed_root: Path = SEED, plan_path: Path = PLAN, model: llm.Llm | None = None,
                 sanitize=llm.sanitize, today: date | None = None, tags: Path | None = None):
        self.root = seed_root
        self.seed = load(seed_root)
        self.plan = read_yaml(plan_path) if plan_path.exists() else {}
        self.schemas, self.measurable = validate.schemas()
        self.model = model or llm.Llm()
        self.sanitize = sanitize
        self.today = (today or date.today()).isoformat()
        # build/tags (the command line's default), else the seed's chunks, which carry one tag each.
        self.chunks = tagged_chunks(tags) if tags is not None else self.seed.chunks
        self.chunks_by_id = {c["id"]: c for c in self.chunks}
        self.facts_only = set(self.plan.get("facts_only_sources", []))
        self.urls = {s["id"]: s["url"] for s in self.seed.sources}
        self.rules = PROBLEM_RULES.replace("{measurable}", "; ".join(
            f"{q} from {', '.join(kinds)}" for q, kinds in sorted(self.measurable.items())))

    def quote(self, ids) -> str:
        return _quote(self.chunks_by_id, ids, self.facts_only, self.urls)

    # --- inputs ------------------------------------------------------------------------------

    def chunks_for(self, *, problem: str | None = None, organism: str | None = None,
                   crop: str | None = None, topics=None) -> list[dict]:
        out = []
        for c in self.chunks:
            problems = set(c.get("problems") or []) | ({c["problem"]} if c.get("problem") else set())
            organisms = set(c.get("organisms") or []) | ({c["organism"]} if c.get("organism") else set())
            if problem and problem in problems or organism and organism in organisms \
                    or crop and c.get("crop") == crop and not problems and not organisms \
                    and (topics is None or c["topic"] in topics):
                out.append(c)
        return out

    def _existing(self, folder: str) -> set[str]:
        return {p.stem for p in (self.root / folder).glob("*.yaml")}

    # --- writing -----------------------------------------------------------------------------

    def _write(self, folder: str, record: dict, review: str | None) -> Path:
        d = self.root / folder
        d.mkdir(parents=True, exist_ok=True)
        path = d / f"{record['id']}.yaml"
        if folder == "treatments":
            header = ("# DRAFT from ml/knowledge/extract.py: reviewed, with its text, in the review file of\n"
                      "# every problem that links it; loaded once those problems are signed.\n")
        else:
            signs = "reviewed_by and reviewed_at" if folder == "problems" else "reviewed_by"
            header = ("# DRAFT from ml/knowledge/extract.py: not loaded until a teammate checks it against\n"
                      f"# {record['id']}.review.md and signs {signs}.\n")
        path.write_text(header + yaml.safe_dump(record, sort_keys=False, allow_unicode=True, width=100),
                        encoding="utf-8", newline="\n")
        if review is not None:
            (d / f"{record['id']}.review.md").write_text(review, encoding="utf-8", newline="\n")
        return path

    def _add_products(self, products: list[dict]) -> list[str]:
        p = self.root / "products.yaml"
        current = read_yaml(p) if p.exists() else {"products": []}
        names = {x["name"].lower() for x in current["products"]}
        added = []
        for x in products:
            # Names are matched inside step text, so keep the bare ingredient: no article, no
            # "labelled for tomatoes".
            x = {**x, "name": re.sub(r"^(an?|the)\s+", "", x["name"].strip(), flags=re.I).lower()}
            if x["name"] not in names:
                current["products"].append({"name": x["name"], "broad_spectrum": x["broad_spectrum"]})
                names.add(x["name"].lower())
                added.append(x["name"])
        if added:
            current["products"].sort(key=lambda x: x["name"].lower())
            # safe_dump drops comments, so the header with the broad_spectrum rule is carried over.
            old = p.read_text(encoding="utf-8").splitlines(keepends=True) if p.exists() else []
            header = "".join(itertools.takewhile(lambda l: l.startswith("#") or not l.strip(), old))
            p.write_text(header + yaml.safe_dump(current, sort_keys=False, allow_unicode=True),
                         encoding="utf-8", newline="\n")
        return added

    # --- the records -------------------------------------------------------------------------

    def problem(self, entry: dict) -> str:
        pid = entry["id"]
        chunks = self.chunks_for(problem=pid)
        for org in entry.get("caused_by", []):
            chunks += [c for c in self.chunks_for(organism=org) if c not in chunks]
        if not chunks:
            return f"{pid}: no tagged excerpts; add a source or drop it from the plan"
        plan_problems = {p["id"]: p for p in self.plan.get("problems", [])}
        others = {p["id"] for p in self.seed.problems} | set(plan_problems)
        crops = set(entry.get("affects", []))
        look_alikes = sorted(o for o in others - {pid}
                             if crops & set((plan_problems.get(o) or {}).get("affects", []))
                             or any(o == p["id"] and crops & set(p.get("affects", [])) for p in self.seed.problems))
        already = {(p["id"], l["problem"]) for p in self.seed.problems for l in p.get("looks_like", [])}
        treatments = {t["id"]: t for t in self.seed.treatments}
        ids = [c["id"] for c in chunks]
        sch = self.sanitize(problem_schema(self.schemas, ids, look_alikes, sorted(treatments)))
        listing = "\n".join(f"- {t}: {treatments[t]['text']}" for t in sorted(treatments)) or "(none yet)"
        user = (f"Problem: {entry['name']} ({pid}), affecting {', '.join(entry.get('affects', []))}."
                + (f" Caused by: {', '.join(entry['caused_by'])}." if entry.get("caused_by") else "")
                + f"\n\nOther problems it may be compared with: {', '.join(look_alikes) or 'none'}."
                + f"\n\nExisting treatments you may reuse by id:\n{listing}"
                + f"\n\nExcerpts:\n\n{excerpts(chunks)}")
        ask = lambda fresh: copy.deepcopy(self.model.call(
            model=llm.EXTRACT_MODEL, system=self.rules, user=user, schema=sch, effort=EFFORT,
            max_tokens=MAX_TOKENS, fresh=fresh))
        r = ask(False)
        if not r["indicators"]:
            # An empty draft from excerpts that describe the problem is a bad reply, not an
            # answer: ask once more, past the cache, and write nothing if it is empty again.
            r = ask(True)
            if not r["indicators"]:
                return f"{pid}: the model drafted no indicators twice; nothing written"
        unknown = _known(r, set(ids))
        unknown += [t for t in r.get("existing_treatments", []) if t not in treatments]
        r["existing_treatments"] = [t for t in r.get("existing_treatments", []) if t in treatments]
        served = self.model.served or llm.EXTRACT_MODEL
        note = lambda text: r.__setitem__("notes", (r["notes"] + "\n\n" if r["notes"] else "") + text)
        if unknown:
            note(f"Dropped ids the model wasn't given: {', '.join(sorted(set(unknown)))}.")
        cited = [i for ind in r["indicators"] for i in ind["chunk_ids"]] + r["conditions_chunk_ids"] \
            + [i for t in r["new_treatments"] for i in t["chunk_ids"]] \
            + [i for l in r.get("looks_like", []) for i in l["chunk_ids"]]
        for t in r["new_treatments"]:
            # A new treatment under an id that exists is a different text: rename it, so the
            # problem links the text the model wrote for it.
            if t["id"] in treatments:
                base, n = t["id"], 2
                while f"{base}_{n}" in treatments:
                    n += 1
                t["id"] = f"{base}_{n}"
                note(f"New treatment {base} collided with an existing id; renamed {t['id']}. "
                     f"Merge the two if they say the same thing.")
            record_t = {"id": t["id"], "kind": t["kind"], "text": t["text"],
                        "sources": _sources(self.chunks_by_id, t["chunk_ids"])}
            self._write("treatments", record_t, None)
            self.seed.treatments.append(record_t)
            treatments[t["id"]] = record_t
        added = self._add_products(r["products"])
        seen, indicators, kept = set(), [], []
        for ind in r["indicators"]:
            m = ind.get("measure")
            if m and not set(self.measurable.get(m["quantity"], [])) & set(ind["image_kinds"]):
                # A size the depth camera can't take from these shots (contracts §10, measures).
                # The indicator exists only to state the size, so it goes with the measure: a
                # small model can't judge millimetres by eye.
                note(f"Dropped {ind['slug']} (\"{ind['text']}\"): {m['quantity']} can't be measured "
                     f"from {', '.join(ind['image_kinds'])}. Fold the size into a sign in relative "
                     f"words if it tells this problem apart.")
                continue
            kept.append(ind)
            slug = ind["slug"]
            while slug in seen:
                slug += "_2"
            seen.add(slug)
            indicators.append({"id": f"{pid}.{slug}", "text": ind["text"], "question": ind["question"],
                               "weight": ind["weight"], "discriminating": False, "where": ind["where"],
                               "image_kinds": ind["image_kinds"],
                               **({"measure": {k: v for k, v in ind["measure"].items() if v is not None}}
                                  if ind.get("measure") else {})})
        r["indicators"] = kept
        for l in r.get("looks_like", []):
            l["written_in"] = l["problem"] if (l["problem"], pid) in already else None
        looks_like = [{"problem": l["problem"], "how_to_tell": l["how_to_tell"]}
                      for l in r.get("looks_like", []) if not l["written_in"]]
        conditions = {k: v for k, v in r["conditions"].items() if v is not None}
        record = {"id": pid, "name": entry["name"], "kind": r["kind"], "affects": entry.get("affects", []),
                  "caused_by": entry.get("caused_by", []), "damage_patterns": r["damage_patterns"],
                  "prevalence": r["prevalence"], "indicators": indicators, "conditions": conditions,
                  "looks_like": looks_like,
                  "treatments": sorted(set(r.get("existing_treatments", [])) | {t["id"] for t in r["new_treatments"]}),
                  "sources": _sources(self.chunks_by_id, cited), "reviewed_by": "", "reviewed_at": ""}
        review = self._problem_review(record, r, added, served, treatments)
        self._write("problems", record, review)
        self.seed.problems.append(record)
        return f"{pid}: {len(indicators)} indicators, {len(record['treatments'])} treatments, " \
               f"{len(record['sources'])} sources"

    def _problem_review(self, record: dict, r: dict, added: list[str], served: str,
                        treatments: dict) -> str:
        q = self.quote
        lines = [f"# Review: {record['name']} (`{record['id']}`)", "",
                 f"Draft of {self.today} by `{served}`. Check every line against the excerpts "
                 "under it, fix or delete what they don't support, then set `reviewed_by` and "
                 "`reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.", "",
                 "## Model notes", "", r["notes"] or "(none)", "", "## Indicators", ""]
        for ind, src in zip(record["indicators"], r["indicators"]):
            m = f"; measure {ind['measure']}" if "measure" in ind else ""
            lines += [f"- **{ind['id']}** (weight {ind['weight']}, {ind['where']}, {', '.join(ind['image_kinds'])}{m})",
                      f"  - Text: {ind['text']}", f"  - Question: {ind['question']}", q(src["chunk_ids"]), ""]
        lines += ["## Conditions", "", f"`{json.dumps(record['conditions'], ensure_ascii=False)}`", "",
                  q(r["conditions_chunk_ids"]), "", "## Looks like", ""]
        for l in r.get("looks_like", []):
            where = (f" (not written here: the pair is already in {l['written_in']}.yaml; merge this "
                     "side's text into it if it adds a sign)") if l.get("written_in") else ""
            lines += [f"- **{l['problem']}**{where}: {l['how_to_tell']}", q(l["chunk_ids"]), ""]
        if not r.get("looks_like"):
            lines += ["(none)", ""]
        lines += ["## Treatments", ""]
        for t in r["new_treatments"]:
            lines += [f"- **{t['id']}** ({t['kind']}, new): {t['text']}", q(t["chunk_ids"]), ""]
        for t in r.get("existing_treatments", []):
            x = treatments.get(t, {})
            lines += [f"- **{t}** ({x.get('kind', '?')}, reused): {x.get('text', '')}",
                      f"  - Sources: {', '.join(x.get('sources', [])) or '(none)'}. Check it fits this problem.", ""]
        if r["products"]:
            lines += ["## Products", ""]
            for x in r["products"]:
                lines += [f"- **{x['name']}**, broad-spectrum: {x['broad_spectrum']}"
                          + (" (added to products.yaml)" if x["name"] in added else ""), q(x["chunk_ids"]), ""]
        return "\n".join(lines).rstrip() + "\n"

    def organism(self, entry: dict) -> str:
        oid = entry["id"]
        chunks = self.chunks_for(organism=oid)
        if not chunks:
            return f"{oid}: no tagged excerpts; add a source or drop it from the plan"
        pests = sorted({o["id"] for o in self.seed.organisms if o.get("role") in ("pest", "invasive")}
                       | {o["id"] for o in self.plan.get("organisms", [])} - {oid})
        ids = [c["id"] for c in chunks]
        sch = self.sanitize(organism_schema(self.schemas, ids, pests))
        user = (f"Organism: {entry['name']} ({entry['scientific_name']}, {oid})."
                f"\n\nOrganisms it may prey on: {', '.join(pests) or 'none'}.\n\nExcerpts:\n\n{excerpts(chunks)}")
        r = copy.deepcopy(self.model.call(model=llm.EXTRACT_MODEL, system=ORGANISM_RULES, user=user,
                                          schema=sch, effort=EFFORT, max_tokens=MAX_TOKENS))
        served = self.model.served or llm.EXTRACT_MODEL
        _known(r, set(ids))
        cited = r["recognition_chunk_ids"] + r.get("preys_on_chunk_ids", [])
        record = {"id": oid, "name": entry["name"], "scientific_name": entry["scientific_name"],
                  "taxon_group": r["taxon_group"], "role": r["role"], "recognition": r["recognition"],
                  **({"preys_on": r["preys_on"]} if r.get("preys_on") else {}),
                  "bioclip_label": f"{entry['scientific_name']} with common name {entry['name'].lower()}",
                  "sources": _sources(self.chunks_by_id, cited), "reviewed_by": ""}
        q = self.quote
        review = "\n".join([f"# Review: {entry['name']} (`{oid}`)", "",
                            f"Draft of {self.today} by `{served}`. Check it against the excerpts, "
                            "then set `reviewed_by` in the YAML. An invasive organism also needs "
                            "`reporting_url`.", "", "## Model notes", "", r["notes"] or "(none)", "",
                            f"## Recognition ({r['taxon_group']}, {r['role']})", "", r["recognition"], "",
                            q(r["recognition_chunk_ids"]), "", "## Preys on", "",
                            ", ".join(r.get("preys_on", [])) or "(none)", "",
                            q(r.get("preys_on_chunk_ids", []))]) + "\n"
        self._write("organisms", record, review)
        self.seed.organisms.append(record)
        return f"{oid}: {r['role']}, {len(record['sources'])} sources"

    def species(self, entry: dict) -> str:
        sid = entry["id"]
        chunks = self.chunks_for(crop=sid, topics={"care"})
        if not chunks:
            return f"{sid}: no care excerpts; add a source or write the care by hand"
        ids = [c["id"] for c in chunks]
        sch = self.sanitize(species_schema(self.schemas, ids))
        user = f"Crop: {entry['name']} ({entry['scientific_name']}, {sid}).\n\nExcerpts:\n\n{excerpts(chunks)}"
        r = copy.deepcopy(self.model.call(model=llm.EXTRACT_MODEL, system=SPECIES_RULES, user=user,
                                          schema=sch, effort=EFFORT, max_tokens=MAX_TOKENS))
        served = self.model.served or llm.EXTRACT_MODEL
        _known(r, set(ids))
        care = {k: v for k, v in r["care"].items() if v is not None}
        record = {"id": sid, "scientific_name": entry["scientific_name"], "common_names": [entry["name"]],
                  "profile": "full", "care": care,
                  # Until training writes classifier_labels.yaml with the confirmed slugs (#4).
                  "classifier_supported": False,
                  "bioclip_label": f"{entry['scientific_name']} with common name {entry['name'].lower()}",
                  "sources": _sources(self.chunks_by_id, r["care_chunk_ids"]), "reviewed_by": ""}
        review = "\n".join([f"# Review: {entry['name']} (`{sid}`)", "",
                            f"Draft of {self.today} by `{served}`. Check the care against the "
                            "excerpts, then set `reviewed_by`. `classifier_supported` turns true in the "
                            "pull request that adds classifier_labels.yaml.", "", "## Model notes", "",
                            r["notes"] or "(none)", "", "## Care", "",
                            f"`{json.dumps(care, ensure_ascii=False)}`", "",
                            self.quote(r["care_chunk_ids"])]) + "\n"
        self._write("species", record, review)
        self.seed.species.append(record)
        return f"{sid}: care from {len(record['sources'])} sources"

    def run(self, only: set[str] | None = None, redo: bool = False) -> list[str]:
        out = []
        steps = [("species", self.species), ("organisms", self.organism), ("problems", self.problem)]
        for folder, fn in steps:
            existing = self._existing(folder)
            for entry in self.plan.get(folder, []):
                if only and entry["id"] not in only:
                    continue
                if entry["id"] in existing and not redo:
                    continue
                if redo:
                    for table in ("species", "organisms", "problems"):
                        records = getattr(self.seed, table)
                        setattr(self.seed, table, [x for x in records if x.get("id") != entry["id"]])
                out.append(fn(entry))
        return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--only", action="append", help="draft only this id (repeatable)")
    ap.add_argument("--redo", action="store_true", help="draft again even if the file exists")
    ap.add_argument("--tags", type=Path, default=TAGS, help="tag.py's decisions (default build/tags)")
    args = ap.parse_args()
    if not any(args.tags.glob("*.jsonl")):
        print(f"FAIL no tags in {args.tags}: run fetch, chunk and tag first", file=sys.stderr)
        return 2
    ex = Extractor(args.seed, tags=args.tags)
    try:
        for line in ex.run(set(args.only) if args.only else None, args.redo):
            print(line, file=sys.stderr)
    except llm.LlmError as e:
        print(f"FAIL {e}", file=sys.stderr)
        print(ex.model.usage.report(), file=sys.stderr)
        return 2
    print(ex.model.usage.report(), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
