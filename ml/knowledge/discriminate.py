"""Step 4 of the ingestion, continued (training.md §10, choosing the discriminating indicators).

    python3 -m ml.knowledge.discriminate              # every problem with reviewed_by still empty
    python3 -m ml.knowledge.discriminate --all        # every problem, reviewed or not
    python3 -m ml.knowledge.discriminate --keep-flags # refresh the tables, leave the YAML alone

Each problem asks exactly 3 indicators: the highest-weighted ones that differ from its
`looks_like` neighbours. The script measures "differ"; the reviewer decides:

1. Every indicator's `text` is embedded with the pinned text embedder, as a document.
2. An indicator's overlap is its highest cosine similarity to any indicator of any neighbour
   (`looks_like` in either problem's file).
3. It is shared when the overlap is SHARED (0.85) or more.
4. The proposal is the top 3 non-shared indicators by weight, ties broken by lower overlap,
   preferring a set where at least two can be seen in a leaf_closeup or a whole_plant shot. With
   fewer than 3 non-shared, the lowest-overlap shared ones fill it and the profile is flagged.
5. The flags are written into the draft, and a table of every indicator with its closest pair
   and their similarity goes into the review file, where the reviewer can override.
6. Once a reviewer has overridden flags, `--keep-flags` refreshes the tables only: the YAML's own
   set is shown beside the proposal, and the reviewer's notes under the old table are kept.
"""

from __future__ import annotations

import argparse
import re
import sys
from itertools import combinations
from pathlib import Path

import numpy as np

from ml.knowledge import embed
from ml.knowledge.seed import SEED, load

SHARED = 0.85
COMMON_SHOTS = {"leaf_closeup", "whole_plant"}
SECTION = "## Discriminating indicators"


def neighbours(problems: list[dict]) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {p["id"]: set() for p in problems}
    for p in problems:
        for l in p.get("looks_like", []):
            out.setdefault(p["id"], set()).add(l["problem"])
            out.setdefault(l["problem"], set()).add(p["id"])
    return out


def choose(indicators: list[dict], overlap: dict[str, tuple[float, str | None]]) -> tuple[list[str], bool]:
    """The 3 ids proposed, and whether shared indicators had to fill the set."""
    ov = lambda i: overlap[i["id"]][0]
    ranked = sorted(indicators, key=lambda i: (-i["weight"], ov(i)))
    free = [i for i in ranked if ov(i) < SHARED]
    shared = sorted((i for i in ranked if ov(i) >= SHARED), key=ov)
    if len(free) < 3:
        return [i["id"] for i in free + shared[:3 - len(free)]], True
    # The best-ranked triple with at least two indicators in the common shots, else the top 3.
    for triple in combinations(free, 3):
        if sum(bool(COMMON_SHOTS & set(i["image_kinds"])) for i in triple) >= 2:
            return [i["id"] for i in triple], False
    return [i["id"] for i in free[:3]], False


def _review_section(p: dict, overlap: dict, picked: list[str], flagged: bool, near: dict,
                    kept: set[str] | None = None) -> str:
    lines = [SECTION, "",
             f"Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under "
             f"{SHARED} cosine similarity. Neighbours: {', '.join(sorted(near)) or 'none'}.", ""]
    if kept is not None:
        lines += ["The flags in the YAML were set at review and are kept (`--keep-flags`); the proposal "
                  "is shown beside them.", ""]
    if flagged:
        lines += ["**Flagged:** fewer than 3 indicators differ from the neighbours; the set is filled "
                  "with the least shared ones. Look for a sign the sources give that tells them apart.", ""]
    extra = (" In the YAML |", "---|") if kept is not None else ("", "")
    lines += ["| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed |" + extra[0],
              "|---|---|---|---|---|---|" + extra[1]]
    for i in p["indicators"]:
        sim, other = overlap[i["id"]]
        mine = f" {'✓' if i['id'] in kept else ''} |" if kept is not None else ""
        lines.append(f"| `{i['id']}` | {i['weight']} | {', '.join(i['image_kinds'])} | "
                     f"{f'`{other}`' if other else '—'} | {sim:.2f} | {'✓' if i['id'] in picked else ''} |"
                     + mine)
    return "\n".join(lines) + "\n"


def _notes_after_table(body: str) -> str:
    """What a reviewer wrote under the old pair table (their overrides), kept with the flags."""
    if f"\n{SECTION}" not in body:
        return ""
    lines = body.split(f"\n{SECTION}", 1)[1].splitlines()
    last = max((n for n, l in enumerate(lines) if l.startswith("|")), default=None)
    return "" if last is None else "\n".join(lines[last + 1:]).strip()


def _write_flags(path: Path, picked: set[str]) -> None:
    """Sets `discriminating` in the YAML without rewriting the rest of the file."""
    lines, current = path.read_text(encoding="utf-8").splitlines(keepends=True), None
    for n, line in enumerate(lines):
        m = re.match(r"^\s*-\s+id:\s+(\S+)\s*$", line)
        if m:
            current = m.group(1)
        m = re.match(r"^(\s*)discriminating:\s+(true|false)\s*$", line)
        if m and current:
            lines[n] = f"{m.group(1)}discriminating: {'true' if current in picked else 'false'}\n"
    path.write_text("".join(lines), encoding="utf-8", newline="\n")


def run(seed_root: Path = SEED, embedder=None, all_problems: bool = False,
        keep_flags: bool = False) -> list[str]:
    seed = load(seed_root)
    embedder = embedder or embed.CachedEmbedder()
    texts = [i["text"] for p in seed.problems for i in p["indicators"]]
    vectors = dict(zip(((p["id"], i["id"]) for p in seed.problems for i in p["indicators"]),
                       np.asarray(embedder.embed_documents(texts), dtype=np.float32)))
    near = neighbours(seed.problems)
    by_id = {p["id"]: p for p in seed.problems}
    out = []
    for p in seed.problems:
        if p.get("reviewed_by") and not all_problems:
            continue
        overlap = {}
        for i in p["indicators"]:
            best, other = 0.0, None
            for nid in sorted(near.get(p["id"], ())):
                for j in (by_id.get(nid) or {}).get("indicators", []):
                    sim = float(vectors[(p["id"], i["id"])] @ vectors[(nid, j["id"])])
                    if sim > best:
                        best, other = sim, j["id"]
            overlap[i["id"]] = (best, other)
        picked, flagged = choose(p["indicators"], overlap)
        path = seed.path(p, seed_root / "problems" / f"{p['id']}.yaml")
        kept = {i["id"] for i in p["indicators"] if i["discriminating"]} if keep_flags else None
        if not keep_flags:
            _write_flags(path, set(picked))
        review = path.with_suffix(".review.md")
        body = review.read_text(encoding="utf-8") if review.exists() else f"# Review: `{p['id']}`\n"
        notes = _notes_after_table(body) if keep_flags else ""
        body = body.split(f"\n{SECTION}")[0].rstrip() + "\n\n" + _review_section(
            p, overlap, picked, flagged, near.get(p["id"], set()), kept) + (f"\n{notes}\n" if notes else "")
        review.write_text(body, encoding="utf-8", newline="\n")
        out.append(f"{p['id']}: {', '.join(x.split('.', 1)[1] for x in picked)}"
                   + (" (flagged: fewer than 3 differ)" if flagged else ""))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--keep-flags", action="store_true", help="refresh the review tables only")
    args = ap.parse_args()
    for line in run(args.seed, all_problems=args.all, keep_flags=args.keep_flags):
        print(line, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
