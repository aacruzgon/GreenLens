"""Step 4 of the ingestion, continued (training.md §10, choosing the discriminating indicators).

    python3 -m ml.knowledge.discriminate              # every problem with reviewed_by still empty
    python3 -m ml.knowledge.discriminate --all        # every problem, reviewed or not

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


def _review_section(p: dict, overlap: dict, picked: list[str], flagged: bool, near: dict) -> str:
    lines = [SECTION, "",
             f"Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under "
             f"{SHARED} cosine similarity. Neighbours: {', '.join(sorted(near)) or 'none'}.", ""]
    if flagged:
        lines += ["**Flagged:** fewer than 3 indicators differ from the neighbours; the set is filled "
                  "with the least shared ones. Look for a sign the sources give that tells them apart.", ""]
    lines += ["| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed |",
              "|---|---|---|---|---|---|"]
    for i in p["indicators"]:
        sim, other = overlap[i["id"]]
        lines.append(f"| `{i['id']}` | {i['weight']} | {', '.join(i['image_kinds'])} | "
                     f"{f'`{other}`' if other else '—'} | {sim:.2f} | {'✓' if i['id'] in picked else ''} |")
    return "\n".join(lines) + "\n"


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


def run(seed_root: Path = SEED, embedder=None, all_problems: bool = False) -> list[str]:
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
        _write_flags(path, set(picked))
        review = path.with_suffix(".review.md")
        body = review.read_text(encoding="utf-8") if review.exists() else f"# Review: `{p['id']}`\n"
        body = body.split(f"\n{SECTION}")[0].rstrip() + "\n\n" + _review_section(
            p, overlap, picked, flagged, near.get(p["id"], set()))
        review.write_text(body, encoding="utf-8", newline="\n")
        out.append(f"{p['id']}: {', '.join(x.split('.', 1)[1] for x in picked)}"
                   + (" (flagged: fewer than 3 differ)" if flagged else ""))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()
    for line in run(args.seed, all_problems=args.all):
        print(line, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
