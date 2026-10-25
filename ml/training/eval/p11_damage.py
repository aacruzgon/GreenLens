"""P11's counts from a recording file of the fake VLM server (#216 steps 3 and 4).

    python -m ml.training.eval.p11_damage RECORDINGS [--bench-run DIR [--cases JSONL]] [--json OUT]

For every findings call in RECORDINGS (the proxy's raw replies, before decoding), it counts the
damage findings and applies the worker's trace rule (`names_a_trace` and `damage_findings` in
`backend/bins/worker/src/vlm/decode.rs`, `findings@6`): a finding whose `trace` names no animal
and nothing an animal left is dropped, and so is a repeat. It lists every dropped trace, and
flags those that describe a disease sign instead (a spot, lesion, blight, mould, yellowing…).

With `--bench-run`, the calls are matched in order to that `vlm_bench` run's `out.jsonl` (one job
at a time, so each job's calls follow each other), and the kept count is checked against the
damage the worker actually decoded for each case: the Python rule here must agree with the Rust.
With `--cases` too, the kept findings are split between the cases with an animal on the plant
(a seed problem of `kind: pest`, or an unboxed organism) and the rest.

For the worker's seven recorded calls it also reports P5's conditions: the species by id, the soil
readings, text in English, and the indicator reasons cut at their cap.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# decode.rs: NEGATIONS, TRACE_ROOTS, TRACE_WORDS (findings@6).
NEGATIONS = {"no", "not", "none", "nothing", "without", "absent", "never"}
TRACE_ROOTS = (
    "insect", "larva", "caterpillar", "worm", "grub", "maggot", "beetle", "weevil", "aphid", "mite",
    "thrip", "whitefl", "butterfl", "sawfl", "bug", "hopper", "psyllid", "earwig", "cricket", "borer",
    "leafmin", "wasp", "nymph", "pupa", "cocoon", "snail", "bird", "deer", "rabbit", "rodent", "mouse",
    "mice", "vole", "squirrel", "chipmunk", "groundhog", "raccoon",
    "frass", "excrement", "honeydew", "web", "silk", "tunnel", "burrow", "slime", "bite", "bitten",
    "chew", "gnaw", "nibbl", "footprint", "molehill",
)
TRACE_WORDS = {
    "ant", "ants", "bee", "bees", "egg", "eggs", "fly", "flies", "moth", "moths", "slug", "slugs",
    "miner", "miners", "mole", "moles", "droppings", "cut", "cuts", "teeth", "tooth", "track",
    "tracks", "trail", "trails", "mounds",
}
MAX_ITEMS = 10
SEED_PROBLEMS = Path(__file__).resolve().parents[2] / "knowledge/seed/problems"
# Words of a disease sign, for flagging dropped traces only (not the worker's rule).
DISEASE = re.compile(r"spot|lesion|blight|rust|mou?ld|mildew|rot|yellow|brown|necro|discolo|wilt|halo|"
                     r"ring|chloro|fung|pustule|streak|blotch|scorch|curl|dead|dry|dried|tan\b|grey|gray|"
                     r"black|stripe|mosaic|decay|patch|death|tissue|infect|disease|sore|canker")
NON_LATIN = re.compile(r"[^\x00-ɏ -⁯°-¿]")


def trace_word(word: str) -> bool:
    singular = word[:-1] if word.endswith("s") else word
    return word in TRACE_WORDS or any(word.startswith(r) or singular.endswith(r) for r in TRACE_ROOTS)


def names_a_trace(trace: str) -> bool:
    for part in re.split(r"[,;.:()]", trace.lower()):
        for clause in part.split(" but "):
            words = re.findall(r"[^\W\d_]+", clause)   # runs of letters, as Rust's is_alphabetic
            if not any(w in NEGATIONS for w in words) and any(trace_word(w) for w in words):
                return True
    return False


def box(b):
    """A raw box as decode compares it: none when it has no area (`convert_box`)."""
    if not b or len(b) != 4 or b[2] <= b[0] or b[3] <= b[1]:
        return None
    return b


def kept_damage(raw: list[dict]) -> tuple[list[dict], list[dict]]:
    """The worker's damage list from a raw one: (kept, dropped by the trace rule)."""
    traced = [d for d in raw if names_a_trace(d.get("trace") or "")]
    dropped = [d for d in raw if not names_a_trace(d.get("trace") or "")]
    kept, seen = [], set()
    for d in traced[:MAX_ITEMS]:
        key = (d.get("pattern"), d.get("image"), json.dumps(box(d.get("bbox_2d"))),
               d.get("other_description") if d.get("pattern") == "other" else None)
        if key in seen:
            continue
        seen.add(key)
        kept.append(d)
    return kept, dropped


def content(rec: dict) -> dict | None:
    try:
        return json.loads(rec["response"]["choices"][0]["message"]["content"])
    except (KeyError, IndexError, TypeError, ValueError):
        return None


def findings_calls(doc: dict) -> list[tuple[int, dict]]:
    return [(i, c) for i, r in enumerate(doc["recordings"])
            if r["schema_name"] == "findings" and (c := content(r)) is not None]


def pest_problems(folder: Path = SEED_PROBLEMS) -> set[str]:
    """The seed's problems of `kind: pest`: an animal made the damage."""
    out = set()
    for p in folder.glob("*.yaml"):
        text = p.read_text()
        if re.search(r"(?m)^kind: pest$", text) and (m := re.search(r"(?m)^id: (\S+)$", text)):
            out.add(m.group(1))
    return out


def has_animal(truth: dict, pests: set[str]) -> bool:
    """A case whose truth has an animal on the plant: a pest problem, or an unboxed organism."""
    return bool(truth.get("organisms_unboxed")) or any(p in pests for p in truth.get("problems") or [])


def bench_check(doc: dict, run: Path, cases: dict[str, dict] | None = None) -> dict:
    """Matches the recordings to the run's jobs in order and compares kept with decoded damage.
    With the run's cases, it also splits the kept findings by whether the case has an animal."""
    rows = [json.loads(line) for f in sorted(run.glob("*/out.jsonl")) for line in f.open() if line.strip()]
    recs = doc["recordings"]
    pests = pest_problems() if cases else set()
    i, agree, differ, per_case = 0, 0, [], []
    for row in rows:
        n = len(row.get("calls") or [])
        mine = recs[i:i + n]
        i += n
        f = next((content(r) for r in mine if r["schema_name"] == "findings" and content(r) is not None), None)
        if f is None or not row.get("ok"):
            continue
        kept, dropped = kept_damage(f.get("damage") or [])
        decoded = len(row["result"]["findings"]["damage"])
        case = {"case_id": row["case_id"], "raw": len(f.get("damage") or []), "kept": len(kept),
                "decoded": decoded, "dropped": [d.get("trace") for d in dropped],
                "kept_findings": [[d.get("pattern"), d.get("trace"), d.get("other_description")] for d in kept]}
        if cases and row["case_id"] in cases:
            case["animal"] = has_animal(cases[row["case_id"]].get("truth") or {}, pests)
        per_case.append(case)
        if len(kept) == decoded:
            agree += 1
        else:
            differ.append(row["case_id"])
    out = {"jobs": len(rows), "recordings_used": i, "recordings": len(recs), "agree": agree,
           "differ": differ, "cases": per_case}
    if cases:
        for name, want in (("animal_cases", True), ("other_cases", False)):
            mine = [c for c in per_case if c.get("animal") is want]
            out[name] = {"cases": len(mine), "with_kept": sum(1 for c in mine if c["kept"]),
                         "kept": sum(c["kept"] for c in mine)}
    return out


def p5(doc: dict) -> dict:
    """P5's conditions on the worker's seven calls."""
    out = {"findings": []}
    reasons = []
    for r in doc["recordings"]:
        c = content(r)
        if c is None:
            continue
        if r["schema_name"] == "findings":
            soil = c.get("soil") or {}
            sg = c.get("species_guess") or {}
            out["findings"].append({
                "species": sg.get("species"), "other_name": sg.get("other_name"), "certainty": sg.get("certainty"),
                "soil_readings": sum(v is not None for v in soil.values()) if isinstance(soil, dict) else None,
                "damage": [(d.get("pattern"), d.get("trace")) for d in c.get("damage") or []],
                "organisms": len(c.get("organisms") or [])})
        elif isinstance(c, dict) and c and all(isinstance(v, dict) and "reason" in v for v in c.values()):
            reasons += [v["reason"] for v in c.values()]
    text = json.dumps([content(r) for r in doc["recordings"]], ensure_ascii=False)
    out["non_latin_characters"] = len(NON_LATIN.findall(text))
    out["reasons"] = {"n": len(reasons), "longest": max(map(len, reasons), default=None),
                      "no_closing_stop": sum(not x.rstrip().endswith((".", "!", "?")) for x in reasons)}
    out["retries"] = sum(1 for r in doc["recordings"] if r.get("status") != 200)
    return out


def summary(doc: dict) -> dict:
    calls = findings_calls(doc)
    raw = kept = 0
    dropped = []
    for _, c in calls:
        k, d = kept_damage(c.get("damage") or [])
        raw += len(c.get("damage") or [])
        kept += len(k)
        dropped += [{"pattern": x.get("pattern"), "trace": x.get("trace")} for x in d]
    for d in dropped:
        d["disease_sign"] = bool(DISEASE.search((d["trace"] or "").lower()))
    return {"findings_calls": len(calls), "damage_raw": raw, "damage_kept": kept, "dropped": len(dropped),
            "dropped_disease_sign": sum(d["disease_sign"] for d in dropped), "dropped_traces": dropped,
            "statuses": sorted({r.get("status") for r in doc["recordings"]})}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("recordings", type=Path)
    ap.add_argument("--bench-run", type=Path, help="the vlm_bench run these recordings were made for")
    ap.add_argument("--cases", type=Path, help="that run's cases (JSONL), to split kept damage by "
                                               "whether the case has an animal")
    ap.add_argument("--json", type=Path, help="write the counts here")
    args = ap.parse_args(argv)
    doc = json.loads(args.recordings.read_text())
    out = {"recordings": args.recordings.name, **summary(doc)}
    if args.bench_run:
        cases = None
        if args.cases:
            cases = {(c := json.loads(line))["case_id"]: c for line in args.cases.open() if line.strip()}
        out["bench"] = bench_check(doc, args.bench_run, cases)
    else:
        out["p5"] = p5(doc)
    if args.json:
        args.json.write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n")
    brief = {k: v for k, v in out.items() if k not in ("dropped_traces", "bench")}
    print(json.dumps(brief, indent=1, ensure_ascii=False))
    if "bench" in out:
        b = out["bench"]
        print(f"bench: {b['jobs']} jobs, {b['recordings_used']} of {b['recordings']} recordings used; "
              f"kept = decoded on {b['agree']}, differ on {len(b['differ'])}: {b['differ'][:10]}")
        for name in ("animal_cases", "other_cases"):
            if name in b:
                s = b[name]
                print(f"{name}: {s['kept']} kept in {s['with_kept']} of {s['cases']} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
