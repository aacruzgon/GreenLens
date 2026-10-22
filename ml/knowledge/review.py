"""Rebuilds the record sections of the review files from the YAML (training.md §10, review).

    python3 -m ml.knowledge.review          # every record with reviewed_by still empty
    python3 -m ml.knowledge.review --all    # every record, signed or not

extract.py writes each review file once, from the draft the model sent back. Once a reviewer has
edited the YAML by hand, the top of the file no longer shows what will be signed. This rewrites
the record sections from the current YAML (a problem's indicators, conditions, look-alikes,
treatments and products; an organism's recognition and prey; a species' care), each line followed
by the excerpts it cites, and keeps the rest of the file as it is: the header, the model notes
(headed "as first drafted"), and every other section, such as the "Changes after" sections,
discriminate.py's table or a reviewer's own. A reviewer's section placed between the record
sections moves after them, in its old order.

The YAML doesn't carry chunk ids, so a line's ids are read from the old file, under the same
indicator, look-alike, treatment or product. A treatment or product keeps the ids of its own file,
so a citation a reviewer removes stays removed; only one newly linked to the problem starts from
the ids the other problems' files give it. A line added by hand has none until the reviewer
writes "  - `<chunk id>`" under it; the next run then quotes that excerpt.

Excerpts are quoted from seed/chunks, and one the seed doesn't carry (a facts-only source or a
withheld chunk) is linked instead, as extract.py does, so the script needs no model. With
build/tags, it also knows which chunks the dose check withheld, so an id tag.py never saw, or one
it dropped, is an error, facts-only ones included; without it, build/chunks shows whether the page
has the chunk. With neither, an id is only checked for its source, and the run lists the ids it
couldn't check on stderr. The line written for an id never depends on which of these the machine has.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

from ml.knowledge import discriminate, extract, validate
from ml.knowledge.seed import SEED, load, read_yaml

PLAN = validate.PLAN

ITEM = re.compile(r"^- \*\*([^*]+)\*\*")
CITE = re.compile(r"^\s+- `([^`]+)`")
NOTES = "## Model notes (as first drafted)"
# The sections this script writes, by heading, alone or followed by " (…)"; every other one, such
# as a reviewer's "## Treatments to check", is kept as it is.
OURS = ("## Model notes", "## Indicators", "## Conditions", "## Looks like", "## Treatments",
        "## Products", "## Recognition", "## Preys on", "## Care")


def ours(heading: str) -> bool:
    return any(heading == h or heading.startswith(h + " (") for h in OURS)


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


def _jsonl(folder: Path | None) -> dict[str, dict[str, dict]]:
    """{source id: {chunk id: row}} from a folder of <source>.jsonl files, empty without one."""
    out = {}
    for path in sorted(folder.glob("*.jsonl")) if folder and folder.is_dir() else []:
        rows = (json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip())
        out[path.stem] = {d["id"]: d for d in rows}
    return out


def quoter(seed, plan: dict, tags: Path | None = None, chunks: Path | None = None):
    """extract._quote over the committed chunks, linking any cited chunk the seed doesn't carry; the
    list it adds (id, reason) to for every cited id that names no excerpt; and the list of cited ids
    it couldn't check, having neither build/tags nor build/chunks for their source. The line written
    for an id is the same either way, so runs on different machines don't rewrite each other's."""
    facts_only = set(plan.get("facts_only_sources", []))
    by_hand = plan.get("withheld_chunks") or {}
    urls = {s["id"]: s["url"] for s in seed.sources}
    committed = {c["id"]: c for c in seed.chunks}
    decided = _jsonl(tags)       # build/tags: every chunk tag.py saw, with what it decided
    on_page = _jsonl(chunks)     # build/chunks: every chunk of the page, before tagging
    unknown: list[tuple[str, str]] = []
    unchecked: list[str] = []

    def withheld(i: str, src: str) -> str | None:
        """Why the seed doesn't carry `i`: None for a facts-only source, a reason, or raises KeyError."""
        if src not in urls:
            raise KeyError("its source isn't in sources.yaml")
        d = decided.get(src, {}).get(i)
        if src in decided:
            if d is None:
                raise KeyError(f"tag.py never saw it (build/tags/{src}.jsonl)")
            if d.get("dropped"):
                raise KeyError(f"tag.py dropped it ({d['dropped']})")
        elif src in on_page:
            if i not in on_page[src]:
                raise KeyError(f"the page has no such chunk (build/chunks/{src}.jsonl)")
        elif i not in unchecked:
            unchecked.append(i)
        if src in facts_only:
            return None
        if i in by_hand:
            return f"withheld at review: {by_hand[i]}"
        if d is not None and not d.get("withheld"):
            raise KeyError("tag.py kept it, but seed/chunks doesn't have it: run tag.py again")
        return "dose check"

    def quote(ids) -> str:
        known, lines = dict(committed), []
        for i in sorted(set(ids or [])):
            if i in known:
                continue
            src = i.rsplit(".", 1)[0]
            try:
                known[i] = {"source_id": src, "withheld": withheld(i, src)}
            except KeyError as e:
                unknown.append((i, e.args[0]))
                lines.append(f"  - `{i}`: **unknown chunk id**: {e.args[0]}")
        cited = [i for i in ids or [] if i in known]
        return "\n".join(([extract._quote(known, cited, facts_only, urls)] if cited or not lines else []) + lines)
    return quote, unknown, unchecked


def _products(seed, texts: list[str]) -> list[dict]:
    """The products.yaml entries named in `texts`, matched as the plan validator matches them."""
    out = []
    for p in seed.products["products"]:
        name = re.escape(p["name"]).replace(r"\ ", r"\s+")
        if any(re.search(rf"\b{name}(?:s|es)?\b", t, re.I) for t in texts):
            out.append(p)
    return out


def problem_sections(p: dict, seed, old: dict, quote, shared: dict) -> list[str]:
    """`shared` holds every problem file's ids for a treatment or product; this file's own win."""
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
        own = old.get("## Treatments", {})
        ids = own[tid] if tid in own else shared["## Treatments"].get(tid)
        lines += [f"- **{tid}** ({t['kind']}): {t['text']}",
                  quote(ids) if ids else f"  - Sources: {', '.join(t['sources'])}. No excerpt cited here.", ""]
    named = _products(seed, [steps[t]["text"] for t in p["treatments"]])
    if named:
        lines += ["## Products", ""]
        own = old.get("## Products", {})
        for x in named:
            ids = own[x["name"]] if x["name"] in own else shared["## Products"].get(x["name"])
            lines += [f"- **{x['name']}**, broad-spectrum: {x['broad_spectrum']}", quote(ids), ""]
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
    kept = [t for h, t in old_sections if not ours(h)]
    out = head.rstrip() + "\n\n" + NOTES + "\n" + notes.rstrip() + "\n\n" + "\n".join(sections).rstrip() + "\n"
    for t in kept:
        out += "\n" + t.rstrip() + "\n"
    return out


def run(seed_root: Path = SEED, plan_path: Path = PLAN, all_records: bool = False,
        tags: Path | None = None, chunks: Path | None = None) -> tuple[list[str], list[str], list[str]]:
    """The review files rewritten; the cited ids that name no excerpt, one line each; and the cited
    ids not in seed/chunks that couldn't be checked. `tags` and `chunks` default to build/tags and
    build/chunks for the real seed."""
    seed = load(seed_root)
    if seed_root.resolve() == SEED.resolve():
        tags, chunks = tags or extract.TAGS, chunks or validate.BUILT_CHUNKS
    quote, unknown, unchecked = quoter(seed, read_yaml(plan_path) if plan_path.exists() else {}, tags, chunks)
    shared: dict[str, dict[str, list[str]]] = {"## Treatments": {}, "## Products": {}}
    for path in sorted((seed_root / "problems").glob("*.review.md")):
        for h, t in split(path.read_text(encoding="utf-8"))[1]:
            if h in shared:
                for key, ids in cites(t).items():
                    if key:
                        pool = shared[h].setdefault(key, [])
                        pool += [i for i in ids if i not in pool]
    done, errors = [], []
    for folder, records in (("problems", seed.problems), ("organisms", seed.organisms), ("species", seed.species)):
        for r in records:
            if r.get("reviewed_by") and not all_records:
                continue
            path = seed.path(r, seed_root / folder / f"{r['id']}.yaml").with_suffix(".review.md")
            if not path.exists():
                continue
            body = path.read_text(encoding="utf-8")
            start = len(unknown)
            _, old_sections = split(body)
            old = {h.split(" (")[0] if h.startswith("## Recognition") else h: cites(t) for h, t in old_sections}
            sections = (problem_sections(r, seed, old, quote, shared) if folder == "problems" else
                        organism_sections(r, old, quote) if folder == "organisms" else
                        species_sections(r, old, quote))
            errors += sorted({f"{folder}/{path.name}: cites `{i}`, an unknown chunk id: {why}"
                              for i, why in unknown[start:]})
            new = rebuild(body, sections, old_sections)
            if new != body:
                path.write_text(new, encoding="utf-8", newline="\n")
                done.append(f"{folder}/{path.name}")
    return done, errors, sorted(unchecked)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()
    done, errors, unchecked = run(args.seed, all_records=args.all)
    for line in done:
        print(f"rewrote {line}", file=sys.stderr)
    if unchecked:
        print(f"{len(unchecked)} cited id(s) not in seed/chunks weren't checked, with no build/tags or "
              f"build/chunks for their source; run fetch, chunk and tag to check them: {', '.join(unchecked)}",
              file=sys.stderr)
    for line in errors:
        print(line)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
