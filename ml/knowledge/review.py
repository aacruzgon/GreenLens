"""Rebuilds the record sections of the review files from the YAML (training.md §10, review).

    python3 -m ml.knowledge.review          # every record with reviewed_by still empty
    python3 -m ml.knowledge.review --all    # every record, signed or not

extract.py writes each review file once, from the draft the model sent back. Once a reviewer has
edited the YAML by hand, the top of the file no longer shows what will be signed. This rewrites
the record sections from the current YAML (a problem's indicators, conditions, look-alikes,
treatments and products; an organism's recognition and prey; a species' care), each line followed
by the excerpts it cites, and keeps the rest of the file as it is: the header, the model notes
(headed "as first drafted"), every "Changes after" section and discriminate.py's table.

The YAML doesn't carry chunk ids, so a line's ids are read from the old file, under the same
indicator, look-alike, treatment or product. Treatments and products are shared, so theirs are
pooled across every problem's review file. A line added by hand has none until the reviewer
writes "  - `<chunk id>`" under it; the next run then quotes that excerpt. Excerpts are quoted from
seed/chunks, and one the seed doesn't carry (a facts-only source or a withheld chunk) is linked
instead, as extract.py does, so the script needs no model and no build/ directory.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

from ml.knowledge import discriminate, extract
from ml.knowledge.seed import ROOT, SEED, load, read_yaml

PLAN = ROOT / "ml/knowledge/plan.yaml"

ITEM = re.compile(r"^- \*\*([^*]+)\*\*")
CITE = re.compile(r"^\s+- `([^`]+)`")
NOTES = "## Model notes (as first drafted)"
KEPT = ("## Changes after", discriminate.SECTION)


def split(body: str) -> tuple[str, list[tuple[str, str]]]:
    """The text before the first `## ` heading, and each section as (heading, whole text)."""
    parts = re.split(r"(?m)^(?=## )", body)
    return parts[0], [(p.splitlines()[0], p) for p in parts[1:]]


def cites(section: str) -> dict[str | None, list[str]]:
    """Chunk ids per item (`- **key**`) of a section; those before any item are under None."""
    out: dict[str | None, list[str]] = {None: []}
    key = None
    for line in section.splitlines():
        if m := ITEM.match(line):
            key = m.group(1)
            out.setdefault(key, [])
        elif m := CITE.match(line):
            out.setdefault(key, []).append(m.group(1))
    return out


def quoter(seed, plan: dict):
    """extract._quote over the committed chunks, linking any cited chunk the seed doesn't carry."""
    facts_only = set(plan.get("facts_only_sources", []))
    by_hand = plan.get("withheld_chunks") or {}
    urls = {s["id"]: s["url"] for s in seed.sources}
    committed = {c["id"]: c for c in seed.chunks}

    def quote(ids) -> str:
        known = dict(committed)
        for i in ids or []:
            if i not in known:
                src = i.rsplit(".", 1)[0]
                withheld = (None if src in facts_only else f"withheld at review: {by_hand[i]}"
                            if i in by_hand else "dose check")
                known[i] = {"source_id": src, "withheld": withheld}
        return extract._quote(known, ids or [], facts_only, urls)
    return quote


def _products(seed, texts: list[str]) -> list[dict]:
    """The products.yaml entries named in `texts`, matched as the plan validator matches them."""
    out = []
    for p in seed.products["products"]:
        name = re.escape(p["name"]).replace(r"\ ", r"\s+")
        if any(re.search(rf"\b{name}(?:s|es)?\b", t, re.I) for t in texts):
            out.append(p)
    return out


def problem_sections(p: dict, seed, old: dict, quote, shared: dict) -> list[str]:
    steps = {t["id"]: t for t in seed.treatments}
    c = old.get("## Indicators", {})
    lines = ["## Indicators", ""]
    for ind in p["indicators"]:
        flag = ", discriminating" if ind["discriminating"] else ""
        m = f"; measure {ind['measure']}" if "measure" in ind else ""
        lines += [f"- **{ind['id']}** (weight {ind['weight']}{flag}, {ind['where']}, "
                  f"{', '.join(ind['image_kinds'])}{m})",
                  f"  - Text: {ind['text']}", f"  - Question: {ind['question']}", quote(c.get(ind["id"])), ""]
    lines += ["## Conditions", "", f"`{json.dumps(p['conditions'], ensure_ascii=False)}`", "",
              quote(old.get("## Conditions", {}).get(None)), "", "## Looks like", ""]
    c = old.get("## Looks like", {})
    theirs = [(o["id"], l) for o in seed.problems for l in o.get("looks_like", []) if l["problem"] == p["id"]]
    for l in p.get("looks_like", []):
        lines += [f"- **{l['problem']}**: {l['how_to_tell']}", quote(c.get(l["problem"])), ""]
    for other, l in theirs:
        lines += [f"- **{other}** (written in `{other}.yaml`): {l['how_to_tell']}", quote(c.get(other)), ""]
    if not p.get("looks_like") and not theirs:
        lines += ["(none)", ""]
    lines += ["## Treatments", ""]
    for tid in p["treatments"]:
        t = steps[tid]
        ids = shared["## Treatments"].get(tid)
        lines += [f"- **{tid}** ({t['kind']}): {t['text']}",
                  quote(ids) if ids else f"  - Sources: {', '.join(t['sources'])}. No excerpt cited here.", ""]
    named = _products(seed, [steps[t]["text"] for t in p["treatments"]])
    if named:
        lines += ["## Products", ""]
        for x in named:
            lines += [f"- **{x['name']}**, broad-spectrum: {x['broad_spectrum']}",
                      quote(shared["## Products"].get(x["name"])), ""]
    return lines


def organism_sections(o: dict, old: dict, quote) -> list[str]:
    recognition = next((v for k, v in old.items() if k.startswith("## Recognition")), {})
    prey = ([", ".join(o["preys_on"]), "", quote(old.get("## Preys on", {}).get(None))]
            if o.get("preys_on") else ["(none)"])
    return [f"## Recognition ({o['taxon_group']}, {o['role']})", "", o["recognition"], "",
            quote(recognition.get(None)), "", "## Preys on", "", *prey, ""]


def species_sections(s: dict, old: dict, quote) -> list[str]:
    return ["## Care", "", f"`{json.dumps(s['care'], ensure_ascii=False)}`", "",
            quote(old.get("## Care", {}).get(None)), ""]


def rebuild(body: str, sections: list[str], old_sections: list[tuple[str, str]]) -> str:
    head, _ = split(body)
    notes = next((t.split("\n", 1)[1] for h, t in old_sections if h.startswith("## Model notes")), "\n(none)\n")
    kept = [t for h, t in old_sections if h.startswith(KEPT)]
    out = head.rstrip() + "\n\n" + NOTES + "\n" + notes.rstrip() + "\n\n" + "\n".join(sections).rstrip() + "\n"
    for t in kept:
        out += "\n" + t.rstrip() + "\n"
    return out


def run(seed_root: Path = SEED, plan_path: Path = PLAN, all_records: bool = False) -> list[str]:
    seed = load(seed_root)
    quote = quoter(seed, read_yaml(plan_path) if plan_path.exists() else {})
    shared: dict[str, dict[str, list[str]]] = {"## Treatments": {}, "## Products": {}}
    for path in sorted((seed_root / "problems").glob("*.review.md")):
        for h, t in split(path.read_text(encoding="utf-8"))[1]:
            if h in shared:
                for key, ids in cites(t).items():
                    if key:
                        pool = shared[h].setdefault(key, [])
                        pool += [i for i in ids if i not in pool]
    done = []
    for folder, records in (("problems", seed.problems), ("organisms", seed.organisms), ("species", seed.species)):
        for r in records:
            if r.get("reviewed_by") and not all_records:
                continue
            path = seed.path(r, seed_root / folder / f"{r['id']}.yaml").with_suffix(".review.md")
            if not path.exists():
                continue
            body = path.read_text(encoding="utf-8")
            _, old_sections = split(body)
            old = {h.split(" (")[0] if h.startswith("## Recognition") else h: cites(t) for h, t in old_sections}
            sections = (problem_sections(r, seed, old, quote, shared) if folder == "problems" else
                        organism_sections(r, old, quote) if folder == "organisms" else
                        species_sections(r, old, quote))
            new = rebuild(body, sections, old_sections)
            if new != body:
                path.write_text(new, encoding="utf-8", newline="\n")
                done.append(f"{folder}/{path.name}")
    return done


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()
    for line in run(args.seed, all_records=args.all):
        print(f"rewrote {line}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
