"""Step 2 of the ingestion (training.md §10, tagging): build/chunks → seed/chunks.

    python3 -m ml.knowledge.tag                          # claude-sonnet-5-5, medium effort
    python3 -m ml.knowledge.tag --model claude-opus-5-5  # re-tag a source that failed its spot check

Each chunk gets `crop`, `problem`, `organism` (a seed or plan slug, or null), `topic` and
`relevant`, and the lists `problems` and `organisms`: every one the chunk describes or compares,
so a chunk that sets early blight against Septoria reaches both drafts. The enums are built from
plan.yaml and the seed, so a tag can't name an unknown id. The model sees the chunk's heading
path, which often names the problem the text doesn't. Haiku 5.5 dropped different relevant chunks
on each run of the tomato slice (training.md §10, running it), so Sonnet 5.5 is the default.

A chunk is dropped when it isn't relevant (navigation, legal text, other crops, the publisher's
own business). One the dose check fails on stays out of the seed, which never carries a rate, a
dose or a mixing ratio, wherever the page put one (`withheld`); extraction still reads it, for
the advice around the rate, and the drafts it writes are dose-checked themselves. plan.yaml's
`withheld_chunks` withholds the chunks a reviewer named the same way. Kept chunks go
to seed/chunks/<source_id>.jsonl in the ChunkSeed shape; every decision goes to
build/tags/<source_id>.jsonl, which extract.py reads, and the script prints a 10% sample per
source for the teammate's spot check.

A source in plan.yaml's `facts_only_sources` has no reuse grant: its chunks are tagged and
extraction reads them from build/tags, so its facts reach the drafts in our own words with a
citation, but its wording never goes into seed/chunks.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

from ml.knowledge import llm, validate
from ml.knowledge.chunk import CHUNKS
from ml.knowledge.seed import ROOT, SEED, load, read_yaml

PLAN = ROOT / "ml/knowledge/plan.yaml"
TAGS = ROOT / "ml/knowledge/build/tags"
TOPICS = ["symptoms", "conditions", "management", "care", "recognition"]
EFFORT = "medium"
MAX_TOKENS = 4000

SYSTEM = """You tag excerpts from university extension and government fact sheets for a garden \
plant-health app's knowledge base. Each excerpt is shown with the headings it sits under.

Decide:
- relevant: true when the excerpt says something a home gardener can use about a crop, a plant \
problem or an organism: how it looks, what favours it, how it spreads, how to manage or prevent \
it, or how to grow the crop. False for page furniture, publication notes, legal or editorial \
text, notices about a diagnostic clinic or sending samples, research outlooks, market or \
industry figures, excerpts only about crops other than the ones listed, and excerpts that are \
only a pesticide list, a spray schedule or a rate table. An excerpt about a listed crop stays \
relevant when its problem or organism isn't in the lists: tag it with null there. How a pest \
damages the plant, how a disease spreads by seed, and which cultivars resist it are all relevant.
- crop: the crop the excerpt is about, or null when it applies to many crops or none.
- problem: the plant problem the excerpt is about, or null when it covers several or none. Use \
the headings: a paragraph under "Early Blight" is about early blight even if it doesn't say so.
- organism: the pest or beneficial organism the excerpt is about, or null.
- problems: every listed problem the excerpt describes, compares or gives advice for, including \
when it covers several. A paragraph that tells early blight from Septoria leaf spot lists both; \
advice for "all three bacterial diseases" lists all three. Empty when it names none.
- organisms: every listed organism the excerpt describes, the same way.
- topic: symptoms (what the damage or disease looks like), conditions (weather, spread, life \
cycle, survival), management (prevention, control, treatment), care (growing the crop) or \
recognition (what the organism itself looks like).

Choose ids only from the lists given. When nothing in a list fits, use null; never guess an id \
for a problem or organism the list doesn't have.

Crops: {crops}
Problems: {problems}
Organisms: {organisms}"""


def ids(seed, plan: dict) -> tuple[dict, dict, dict]:
    """{id: name} for the crops, problems and organisms, from the seed and the plan."""
    crops = {s["id"]: (s.get("common_names") or [s["id"]])[0] for s in seed.species}
    problems = {p["id"]: p.get("name", p["id"]) for p in seed.problems}
    organisms = {o["id"]: o.get("name", o["id"]) for o in seed.organisms}
    crops |= {s["id"]: s["name"] for s in plan.get("species", [])}
    problems |= {p["id"]: p["name"] for p in plan.get("problems", [])}
    organisms |= {o["id"]: o["name"] for o in plan.get("organisms", [])}
    return crops, problems, organisms


def schema(crops, problems, organisms) -> dict:
    nullable = lambda values: {"anyOf": [{"type": "string", "enum": sorted(values)}, {"type": "null"}]}
    listed = lambda values: {"type": "array", "items": {"type": "string", "enum": sorted(values)}}
    return {"type": "object", "additionalProperties": False,
            "required": ["relevant", "crop", "problem", "organism", "problems", "organisms", "topic"],
            "properties": {"relevant": {"type": "boolean"}, "crop": nullable(crops),
                           "problem": nullable(problems), "organism": nullable(organisms),
                           "problems": listed(problems), "organisms": listed(organisms),
                           "topic": {"type": "string", "enum": TOPICS}}}


def system_prompt(crops, problems, organisms) -> str:
    listing = lambda d: "; ".join(f"{k} ({v})" for k, v in sorted(d.items())) or "none"
    return SYSTEM.format(crops=listing(crops), problems=listing(problems), organisms=listing(organisms))


def tag_chunk(model: llm.Llm, model_id: str, system: str, sch: dict, c: dict) -> dict:
    user = f"Headings: {c['heading'] or '(none)'}\n\nExcerpt:\n{c['text']}"
    ask = lambda n: model.call(model=model_id, system=system, user=user, schema=sch, effort=EFFORT,
                               max_tokens=n)
    try:
        t = dict(ask(MAX_TOKENS))
    except llm.LlmCutOff:
        # A long think on a hard chunk: once more with room (cached under its own key).
        t = dict(ask(MAX_TOKENS * 4))
    # The single tags are part of the lists, so a reader of the lists misses nothing.
    for one, many in (("problem", "problems"), ("organism", "organisms")):
        t[many] = sorted(set(t[many]) | ({t[one]} if t[one] else set()))
    return t


def run(seed_root: Path = SEED, plan_path: Path = PLAN, chunks: Path = CHUNKS, tags: Path = TAGS,
        model: llm.Llm | None = None, model_id: str = llm.TAG_MODEL, dose=validate.cargo_dose_check,
        sanitize=llm.sanitize, sources: list[str] | None = None, sample_seed: int = 0) -> dict:
    """Tags every chunk file (or only `sources`); returns {source_id: {kept, dropped, sample}}."""
    seed = load(seed_root)
    plan = read_yaml(plan_path) if plan_path.exists() else {}
    crops, problems, organisms = ids(seed, plan)
    facts_only = set(plan.get("facts_only_sources", []))
    by_hand = plan.get("withheld_chunks") or {}
    system, sch = system_prompt(crops, problems, organisms), sanitize(schema(crops, problems, organisms))
    model = model or llm.Llm()
    tags.mkdir(parents=True, exist_ok=True)
    (seed_root / "chunks").mkdir(parents=True, exist_ok=True)
    summary = {}
    for path in sorted(chunks.glob("*.jsonl")):
        source_id = path.stem
        if sources and source_id not in sources:
            continue
        records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
        decided = []
        for c in records:
            t = tag_chunk(model, model_id, system, sch, c)
            decided.append({**c, **t, "dropped": None if t["relevant"] else "not relevant"})
        failing = {f["id"]: f for f in dose(seed.products, [{"id": d["id"], "text": d["text"]}
                                                            for d in decided if not d["dropped"]])}
        for d in decided:
            d["withheld"] = (f"dose check: {failing[d['id']]['reason']}" if d["id"] in failing
                             else f"withheld at review: {by_hand[d['id']]}" if d["id"] in by_hand else None)
        kept = [{"id": d["id"], "source_id": source_id, "crop": d["crop"], "problem": d["problem"],
                 "organism": d["organism"], "topic": d["topic"], "text": d["text"]}
                for d in decided if not d["dropped"] and not d["withheld"]]
        target = seed_root / "chunks" / f"{source_id}.jsonl"
        if kept and source_id not in facts_only:
            target.write_text("".join(json.dumps(k, ensure_ascii=False) + "\n" for k in kept),
                              encoding="utf-8", newline="\n")
        elif target.exists():
            target.unlink()
        (tags / f"{source_id}.jsonl").write_text(
            "".join(json.dumps(d, ensure_ascii=False) + "\n" for d in decided), encoding="utf-8")
        rng = random.Random(f"{sample_seed}:{source_id}")
        sample = sorted(rng.sample([d["id"] for d in decided], max(1, round(len(decided) / 10))))
        summary[source_id] = {"kept": len(kept), "dropped": sum(1 for d in decided if d["dropped"]),
                              "withheld": sum(1 for d in decided if d["withheld"]), "sample": sample,
                              "committed": bool(kept) and source_id not in facts_only}
    return summary


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--model", default=llm.TAG_MODEL)
    ap.add_argument("--source", action="append", help="tag only this source (repeatable)")
    args = ap.parse_args()
    model = llm.Llm()
    try:
        summary = run(args.seed, model=model, model_id=args.model, sources=args.source)
    except (llm.LlmError, validate.DoseCheckUnavailable) as e:
        print(f"FAIL {e}", file=sys.stderr)
        return 2
    for source_id, s in summary.items():
        where = "" if s["committed"] or not s["kept"] else " (facts only: not in seed/chunks)"
        print(f"{source_id}: kept {s['kept']}{where}, dropped {s['dropped']}, withheld {s['withheld']}; "
              f"spot-check {', '.join(s['sample'])}", file=sys.stderr)
    print(model.usage.report(), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
