#!/usr/bin/env python3
"""J5's scorer: reads `e2e.py`'s JSONL and writes `reports/e2e-<date>.json` and `.md` (evaluation 9).

Per stage, on the dev cases that went through the API:

- **Upload:** photos the quality gate refused (422).
- **Plant check:** cases with a plant that didn't end `no_plant_found`.
- **Species:** the working species (BioCLIP and the VLM, as `species_suggested` names it) top-1, and
  BioCLIP's top-3 from the plant's species candidates.
- **Problem:** the diagnosis's primary among the case's true problems (top-1), and any of the first
  3 ranked candidates (top-3), on the cases whose true problems the knowledge base holds.
- **End to end (evaluation 4), T11–T16 in the order of its table's targets:** T11 precision of
  `confirmed`; coverage (report); T12 healthy plants `confirmed` with a problem; T13 sick plants
  diagnosed `healthy`; T14 no-plant and unusable cases ending correctly; T15 `label_only` species
  `confirmed` with another crop's disease; T16 wall time from submit to `diagnosed` at p95 on the
  GPU path, without the time spent waiting on the user (the fallback's is reported).
- **Workers:** which VLM path served each scan (the rig's VLM or the fallback's hosted model, from
  `model_versions`: the API doesn't name a job's worker), and the healthy workers at submit.
- **Diagnosis margin (evaluation 5, contracts open question 3):** the final diagnosis's candidate
  scores are logged, so `confirmed` is recomputed for margins 1–6: the top candidate above zero
  with a matched indicator, leading the second by the margin, and no question open. `healthy`
  doesn't depend on the margin. The margins consistent with the live statuses are given, and the
  smallest margin with 90% precision is proposed. A different margin could have asked a question
  the live run didn't, which only `unsure` answers here would leave unchanged; the sweep says so.
- **Indicator weights (evaluation 5, open question 4):** at that margin, the cases left `uncertain`
  with a lead of exactly the margin minus one, and how many of them were right.

Each rate is k of n with its 95% Wilson interval; a target is met or missed only when the whole
interval is on one side (evaluation 4). The JSON keeps one row per case, so `--rebuild` writes the
markdown again from the JSON alone.

    python3 ml/training/eval/score.py ml/training/runs/e2e/j5-2026-10-25.jsonl [--name e2e-2026-10-25]
    python3 ml/training/eval/score.py --rebuild ml/training/eval/reports/e2e-2026-10-25.json
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import shlex
import subprocess
import sys
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
REPORTS = HERE / "reports"
MANIFEST = HERE / "manifest.jsonl"
SEED = ROOT / "ml/knowledge/seed"
Z = 1.959963984540054
MARGINS = range(1, 7)
PRECISION_TARGET = 0.90
MIN_CONFIRMED = 10      # a margin is proposed only on at least this many `confirmed` scans


# --- small statistics (as score_test_split's, without numpy) ----------------------------------------

def wilson(k: int, n: int, z: float = Z) -> list[float] | None:
    if n == 0:
        return None
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [max(0.0, c - h), min(1.0, c + h)]


def verdict(interval, target: float, at_least: bool) -> str:
    if interval is None:
        return "not measured"
    lo, hi = interval
    if at_least:
        return "met" if lo >= target else "missed" if hi < target else "undecided"
    return "met" if hi <= target else "missed" if lo > target else "undecided"


def rate(k: int, n: int, target: float | None = None, at_least: bool = True, cases=None) -> dict:
    r = {"k": k, "n": n, "rate": (k / n) if n else None, "interval": wilson(k, n)}
    if target is not None:
        r.update(target=target, at_least=at_least, verdict=verdict(r["interval"], target, at_least))
    if cases is not None:
        r["cases"] = sorted(cases)
    return r


def pct(values: list[float], q: float) -> float | None:
    """The q-th percentile, linear between closest ranks."""
    xs = sorted(v for v in values if v is not None)
    if not xs:
        return None
    pos = (len(xs) - 1) * q
    lo, hi = math.floor(pos), math.ceil(pos)
    if lo == hi:
        return xs[lo] if math.isinf(xs[lo]) else round(xs[lo], 1)
    if math.isinf(xs[hi]):              # an unfinished scan, counted as slower than any finished one
        return math.inf
    return round(xs[lo] + (xs[hi] - xs[lo]) * (pos - lo), 1)


# --- inputs -------------------------------------------------------------------------------------

def read_records(paths: list[Path]) -> list[dict]:
    """One record per case: the last line wins, except that an error never replaces a result."""
    by_case: dict[str, dict] = {}
    for path in paths:
        for line in path.read_text().splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            old = by_case.get(r["case_id"])
            if old and r.get("outcome") == "error" and old.get("outcome") != "error":
                continue
            by_case[r["case_id"]] = r
    return [by_case[k] for k in sorted(by_case)]


def knowledge(seed: Path = SEED) -> dict:
    import yaml
    species = {}
    for f in sorted((seed / "species").glob("*.yaml")):
        doc = yaml.safe_load(f.read_text()) or {}
        species[doc.get("id", f.stem)] = doc.get("profile")
    problems = {}
    for f in sorted((seed / "problems").glob("*.yaml")):
        doc = yaml.safe_load(f.read_text()) or {}
        problems[doc.get("id", f.stem)] = sorted(doc.get("affects") or [])
    return {"species_profile": species, "problem_affects": problems}


def file_id(path: Path) -> dict:
    data = path.read_bytes()
    try:
        rel = str(path.resolve().relative_to(ROOT))
    except ValueError:
        rel = str(path)
    return {"file": rel, "sha256": hashlib.sha256(data).hexdigest()}


def git_sha() -> str | None:
    try:
        return subprocess.run(["git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"], capture_output=True,
                              text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


# --- one row per case ---------------------------------------------------------------------------

def lead_of(cands: list[dict]) -> float | None:
    if not cands:
        return None
    top = cands[0].get("score") or 0
    second = (cands[1].get("score") or 0) if len(cands) > 1 else 0
    return top - second


def status_at(row: dict, margin: float) -> str | None:
    """The final diagnosis's status had the fusion margin been `margin` (contracts 7)."""
    if row["status"] is None:
        return None
    if row["status"] == "healthy":
        return "healthy"
    if row["open_question"]:
        return "uncertain"
    if row["top_score"] is not None and row["top_score"] > 0 and row["top_matched"] >= 1 and row["lead"] >= margin:
        return "confirmed"
    return "uncertain"


def case_row(r: dict, kb: dict) -> dict:
    t = r["truth"]
    d = r.get("diagnosis") or {}
    cands = d.get("candidates") or []
    sp = r.get("species") or {}
    sug = sp.get("suggested") or {}
    bio = sorted([c for c in (sug.get("candidates") or sp.get("plant_candidates") or []) if c.get("source") == "bioclip"],
                 key=lambda c: -(c.get("score") or 0))
    timing = r.get("timing") or {}
    failure = (r.get("failure") or {}).get("code")
    true_problems = t.get("problems") or []
    known = [p for p in true_problems if p in kb["problem_affects"]]
    primary = d.get("primary")
    uploads = r.get("uploads") or []
    return {
        "case_id": r["case_id"],
        "truth_outcome": t["outcome"],
        "truth_species": t.get("species_id"),
        "truth_problems": true_problems,
        "known_problems": known,
        "look_alike": bool(t.get("look_alike")),
        "label_confidence": r.get("label_confidence"),
        "species_profile": kb["species_profile"].get(t.get("species_id")),
        "outcome": r.get("outcome"),
        "error": r.get("error"),
        "failure": failure,
        "status": d.get("status") if r.get("outcome") == "planned" or d else None,
        "low_evidence": d.get("low_evidence"),
        "primary": primary,
        "primary_affects": kb["problem_affects"].get(primary) if primary else None,
        "ranked": [c["problem_id"] for c in cands],
        "scores": [c.get("score") for c in cands],
        "top_score": cands[0].get("score") if cands else None,
        "top_matched": len(cands[0].get("matched") or []) if cands else 0,
        "lead": lead_of(cands),
        "open_question": bool(r.get("open_question")),
        "questions": [{"kind": q.get("kind"), "indicator_id": q.get("indicator_id"), "photo_kind": q.get("photo_kind"),
                       "answer": q.get("answer")} for q in r.get("questions") or []],
        "working_species": sug.get("working_species_id") if sug else None,
        "species_rerun": bool((r.get("species_answer") or {}).get("rerun")),
        "bioclip_top3": [c.get("species_id") for c in bio[:3]],
        "photos_sent": len(uploads),
        "photos_refused": [u["file"] + f" ({u.get('error')})" for u in uploads if u.get("status") == 422],
        "vlm_path": r.get("vlm_path"),
        "workers_at_submit": r.get("workers_at_submit"),
        "model_versions": d.get("model_versions"),
        "machine_s": timing.get("machine_s"),
        "diagnosed_s": timing.get("diagnosed_s"),
        "first_diagnosis_s": timing.get("first_diagnosis_s"),
        "plan_ready_s": timing.get("plan_ready_s"),
        "awaiting_answer_s": timing.get("awaiting_answer_s"),
        "harness_s": r.get("harness_s"),
        "scan_id": r.get("scan_id"),
        "policy": r.get("policy"),
    }


# --- the metrics ----------------------------------------------------------------------------------

def metrics(rows: list[dict]) -> dict:
    ran = [r for r in rows if r["outcome"] in ("planned", "failed", "rejected")]
    done = [r for r in ran if r["outcome"] == "planned"]
    sick = [r for r in done if r["truth_problems"]]
    known_sick = [r for r in sick if r["known_problems"]]
    healthy = [r for r in done if r["truth_outcome"] == "healthy"]
    confirmed = [r for r in done if r["status"] == "confirmed"]
    with_plant = [r for r in ran if r["truth_outcome"] in ("diagnosed", "healthy")]
    no_plant = [r for r in ran if r["truth_outcome"] in ("no_plant_found", "photos_unusable", "rejected_at_upload")]
    right = lambda r: r["primary"] in r["truth_problems"]  # noqa: E731

    def ending_right(r):
        return (r["outcome"] == "rejected" or r["failure"] in ("no_plant_found", "photos_unusable"))

    def wrong_crop(r):
        return (r["species_profile"] == "label_only" and r["status"] == "confirmed" and r["primary_affects"] is not None
                and r["truth_species"] not in r["primary_affects"])

    label_only = [r for r in done if r["species_profile"] == "label_only"]
    suggested = [r for r in ran if r["working_species"] is not None]   # scans that reached species_suggested
    m = {
        "outcomes": dict(Counter(r["outcome"] for r in rows)),
        "statuses": dict(Counter(r["status"] or r["failure"] or r["outcome"] for r in rows)),
        "upload": rate(sum(len(r["photos_refused"]) for r in ran), sum(r["photos_sent"] for r in ran), 0.05, False,
                       [r["case_id"] for r in ran if r["photos_refused"]]),
        "plant_check": rate(sum(1 for r in with_plant if r["failure"] != "no_plant_found" and r["outcome"] != "rejected"),
                            len(with_plant), 0.99, True,
                            [r["case_id"] for r in with_plant if r["failure"] == "no_plant_found" or r["outcome"] == "rejected"]),
        "species_top1": rate(sum(1 for r in suggested if r["working_species"] == r["truth_species"]), len(suggested), 0.85, True,
                             [r["case_id"] for r in suggested if r["working_species"] != r["truth_species"]]),
        "species_reruns": sum(1 for r in done if r["species_rerun"]),
        "species_bioclip_top3": rate(sum(1 for r in done if r["truth_species"] in r["bioclip_top3"]),
                                     sum(1 for r in done if r["bioclip_top3"]), 0.95, True),
        "problem_top1": rate(sum(1 for r in known_sick if right(r)), len(known_sick), None, True,
                             [r["case_id"] for r in known_sick if not right(r)]),
        "problem_top3": rate(sum(1 for r in known_sick if set(r["ranked"][:3]) & set(r["truth_problems"])),
                             len(known_sick)),
        "T11_confirmed_precision": rate(sum(1 for r in confirmed if right(r)), len(confirmed), 0.90, True,
                                        [r["case_id"] for r in confirmed if not right(r)]),
        "coverage": rate(sum(1 for r in sick if r["status"] == "confirmed"), len(sick)),
        "coverage_known": rate(sum(1 for r in known_sick if r["status"] == "confirmed"), len(known_sick)),
        "T12_healthy_called_sick": rate(sum(1 for r in healthy if r["status"] == "confirmed"), len(healthy), 0.10, False,
                                        [r["case_id"] for r in healthy if r["status"] == "confirmed"]),
        "T13_sick_called_healthy": rate(sum(1 for r in sick if r["status"] == "healthy"), len(sick), 0.05, False,
                                        [r["case_id"] for r in sick if r["status"] == "healthy"]),
        "lookalike_called_healthy": rate(sum(1 for r in done if r["truth_outcome"] == "diagnosed" and not r["truth_problems"]
                                             and r["status"] == "healthy"),
                                         sum(1 for r in done if r["truth_outcome"] == "diagnosed" and not r["truth_problems"])),
        "T14_no_plant_correct": rate(sum(1 for r in no_plant if ending_right(r)), len(no_plant), 0.90, True),
        "T15_label_only_wrong_crop": rate(sum(1 for r in label_only if wrong_crop(r)), len(label_only), 0.0, False,
                                          [r["case_id"] for r in label_only if wrong_crop(r)]),
        "statuses_by_truth": {k: dict(Counter(r["status"] for r in done if r["truth_outcome"] == k
                                               and (k != "diagnosed" or r["truth_problems"])))
                              for k in ("healthy", "diagnosed")},
    }
    m["statuses_by_truth"]["lookalike"] = dict(Counter(r["status"] for r in done if r["truth_outcome"] == "diagnosed"
                                                       and not r["truth_problems"]))
    # time
    t = {}
    for path in ("rig_vlm", "fallback", "none"):
        xs = [r for r in done if r["vlm_path"] == path]
        if not xs:
            continue
        t[path] = {"n": len(xs),
                   "machine_p50": pct([r["machine_s"] for r in xs], 0.5), "machine_p95": pct([r["machine_s"] for r in xs], 0.95),
                   "first_diagnosis_p50": pct([r["first_diagnosis_s"] for r in xs], 0.5),
                   "first_diagnosis_p95": pct([r["first_diagnosis_s"] for r in xs], 0.95),
                   "plan_ready_p50": pct([r["plan_ready_s"] for r in xs], 0.5),
                   "plan_ready_p95": pct([r["plan_ready_s"] for r in xs], 0.95),
                   "continued": sum(1 for r in xs if (r["awaiting_answer_s"] or 0) > 300)}
    m["time"] = t
    gpu = t.get("rig_vlm")
    # A case that timed out or stopped short has no diagnosis, so its path is unknown and its time is at least the
    # harness's timeout. Leaving it out would drop exactly the slowest scans, so for the verdict each one counts as
    # a GPU scan slower than any finished one: T16 is met only if the p95 stays under 90 s even then.
    unfinished = [r["case_id"] for r in rows if r["outcome"] not in ("planned", "failed", "rejected")]
    gpu_times = [r["machine_s"] for r in done if r["vlm_path"] == "rig_vlm" and r["machine_s"] is not None]
    worst = pct(gpu_times + [math.inf] * len(unfinished), 0.95) if gpu_times else None
    m["T16_wall_time_gpu_p95"] = {"p95": gpu["machine_p95"] if gpu else None, "n": gpu["n"] if gpu else 0, "target_s": 90,
                                  "unfinished": len(unfinished), "unfinished_cases": unfinished,
                                  "verdict": ("not measured" if not gpu or gpu["machine_p95"] is None else
                                              "missed" if gpu["machine_p95"] > 90 else
                                              "met" if worst is not None and worst <= 90 else "undecided")}
    m["workers"] = {"vlm_path": dict(Counter(r["vlm_path"] for r in done)),
                    "at_submit": dict(Counter(",".join(r["workers_at_submit"] or ["?"]) for r in ran)),
                    "model_versions": dict(Counter(json.dumps(r["model_versions"], sort_keys=True) for r in done))}
    q = [x for r in done for x in r["questions"]]
    m["questions"] = {"asked": len(q), "by_kind": dict(Counter(x["kind"] for x in q)),
                      "answers": dict(Counter(str(x["answer"]) for x in q)),
                      "scans_with_question": sum(1 for r in done if r["questions"]),
                      "open_at_end": sum(1 for r in done if r["open_question"])}
    m["margin"] = margin_sweep(done)
    return m


def margin_sweep(done: list[dict]) -> dict:
    sick = [r for r in done if r["truth_problems"]]
    healthy = [r for r in done if r["truth_outcome"] == "healthy"]
    sweep = []
    for mg in MARGINS:
        conf = [r for r in done if status_at(r, mg) == "confirmed"]
        k = sum(1 for r in conf if r["primary"] in r["truth_problems"])
        sweep.append({"margin": mg, "precision": rate(k, len(conf), PRECISION_TARGET, True),
                      "coverage": rate(sum(1 for r in sick if status_at(r, mg) == "confirmed"), len(sick)),
                      "healthy_confirmed": rate(sum(1 for r in healthy if status_at(r, mg) == "confirmed"), len(healthy))})
    consistent = [mg for mg in MARGINS if all(status_at(r, mg) == r["status"] for r in done
                                               if r["status"] in ("confirmed", "uncertain"))]
    # Evaluation 5's rule is the point estimate (90% or more); a floor on n keeps 1 of 1 from proposing a margin,
    # and the report gives the interval's verdict beside it.
    chosen = next((s["margin"] for s in sweep if s["precision"]["n"] >= MIN_CONFIRMED
                   and s["precision"]["rate"] >= PRECISION_TARGET), None)
    weights = None
    if chosen:
        near = [r for r in done if r["status"] != "healthy" and not r["open_question"] and r["top_score"] is not None
                and r["top_score"] > 0 and r["top_matched"] >= 1 and r["lead"] == chosen - 1]
        weights = {"margin": chosen, "lead": chosen - 1,
                   "uncertain_at_lead": rate(sum(1 for r in near if r["primary"] in r["truth_problems"]), len(near),
                                             cases=[r["case_id"] for r in near])}
    leads = Counter(r["lead"] for r in done if r["lead"] is not None and r["status"] != "healthy")
    proposed_verdict = next((s["precision"]["verdict"] for s in sweep if s["margin"] == chosen), None)
    return {"sweep": sweep, "consistent_with_live": consistent, "proposed": chosen, "proposed_verdict": proposed_verdict,
            "min_confirmed": MIN_CONFIRMED, "weights": weights,
            "leads": {str(k): v for k, v in sorted(leads.items())},
            "cases_with_scores": sum(1 for r in done if r["scores"])}


# --- the report -----------------------------------------------------------------------------------

def k_of_n(r: dict | None) -> str:
    if not r or not r.get("n"):
        return "not measured (0 cases)"
    lo, hi = r["interval"]
    return f"{r['k']} of {r['n']} ({100 * r['rate']:.1f}%; {100 * lo:.0f}%–{100 * hi:.0f}%)"


def target_str(r: dict) -> str:
    if r.get("target") is None:
        return "Report"
    t = r["target"]
    if t == 0:
        return "0"
    return f"{100 * t:.0f}% or {'more' if r['at_least'] else 'less'}"


def listed(ids, limit=12) -> str:
    ids = list(ids or [])
    if not ids:
        return "none"
    s = ", ".join(f"`{i}`" for i in ids[:limit])
    return s + (f" and {len(ids) - limit} more" if len(ids) > limit else "")


def markdown(rep: dict) -> str:
    m, inp = rep["metrics"], rep["inputs"]
    pol = rep.get("policy") or {}
    out = [f"# End to end through the API: {rep['name']}", ""]
    out.append(f"`{rep['command']}` on {rep['made_at'][:10]} at {rep.get('git_sha') or '?'}, over "
               + ", ".join(f"`{f['file']}` (SHA-256 `{f['sha256'][:16]}`)" for f in inp["runs"])
               + f": {rep['cases']} dev cases through `{rep.get('base_url')}` in garden `{rep.get('garden_id')}`. "
               f"The manifest is `{inp['manifest']['file']}` (SHA-256 `{inp['manifest']['sha256'][:16]}`). "
               f"Made by [e2e.py](../e2e.py) and [score.py](../score.py) (J5, evaluation 9)." + (
                   f" Rebuilt from its JSON by `{rep['rebuilt']['command']}` on {rep['rebuilt']['at'][:10]}." if rep.get("rebuilt") else ""))
    out.append("")
    out.append(f"How the cases were sent: questions answered by `{pol.get('answers')}`"
               + (" (**an upper bound: the label leaks into every answer**)" if pol.get("answers") == "oracle" else
                  " (a case's indicator labels where it has them, else `unsure`; photo requests left to the 10-minute continuation)")
               + "; species " + {"confirm": "suggested by BioCLIP and the VLM, then the confirmation card answered with the true species "
                                 "(a wrong suggestion reruns the scan; the species rows score the suggestion)",
                                 "name": "named on the new plant", "none": "suggested and never confirmed"}.get(pol.get("species"), "?") + "; "
               f"a case without a close-up {'also sends its whole-plant photo as a `leaf_closeup`' if pol.get('closeup_copy') else 'sends only its own photos'}.")
    out.append("")
    out.append(f"Outcomes: {', '.join(f'{k} {v}' for k, v in sorted(m['outcomes'].items()))}. "
               f"Final statuses: {', '.join(f'{k} {v}' for k, v in sorted(m['statuses'].items(), key=lambda x: str(x[0])))}.")
    out.append("")
    out.append("Each rate is k of n (rate; 95% Wilson interval). A target is met or missed only when the whole interval "
               "is on one side of it, otherwise undecided at that count (evaluation 4). T11–T16 are evaluation 4's "
               "end-to-end targets in its table's order.")
    out.append("")
    out.append("| Row | What | Target | Dev | Verdict |")
    out.append("|---|---|---|---|---|")

    def row(label, what, r):
        out.append(f"| {label} | {what} | {target_str(r)} | {k_of_n(r)} | {r.get('verdict', 'report')} |")

    row("", "Upload: photos the quality gate refused", m["upload"])
    row("", "Plant check: cases with a plant that pass", m["plant_check"])
    row("", "Species: working species top-1 (BioCLIP and the VLM)", m["species_top1"])
    row("", "Species: BioCLIP top-3", m["species_bioclip_top3"])
    row("", "Problem: primary among the true problems, cases whose problems the knowledge base holds", m["problem_top1"])
    row("", "Problem: a true problem among the first 3 candidates, same cases", m["problem_top3"])
    row("T11", "Precision of `confirmed`: the primary is among the case's true problems", m["T11_confirmed_precision"])
    row("", "Coverage: diseased cases that end `confirmed`", m["coverage"])
    row("", "Coverage, cases whose problems the knowledge base holds", m["coverage_known"])
    row("T12", "Healthy plants `confirmed` with a problem", m["T12_healthy_called_sick"])
    row("T13", "Sick plants diagnosed `healthy`", m["T13_sick_called_healthy"])
    row("", "Look-alikes (a problem the knowledge base doesn't hold) diagnosed `healthy`", m["lookalike_called_healthy"])
    row("T14", "No-plant and unusable cases that end correctly", m["T14_no_plant_correct"])
    row("T15", "`label_only` species `confirmed` with another crop's disease", m["T15_label_only_wrong_crop"])
    t16 = m["T16_wall_time_gpu_p95"]
    out.append(f"| T16 | Wall time from submit to `diagnosed`, p95, GPU path, without time waiting on the user | 90 s or less | "
               f"{t16['p95'] if t16['p95'] is not None else 'not measured'} s on {t16['n']} scans"
               + (f", {t16['unfinished']} unfinished counted as slower" if t16.get("unfinished") else "")
               + f" | {t16['verdict']} |")
    fb = m["time"].get("fallback")
    out.append(f"| | Same on the fallback worker | Report | "
               f"{(str(fb['machine_p95']) + ' s on ' + str(fb['n']) + ' scans') if fb else 'not measured (0 scans)'} | report |")
    out += ["", "## Details", ""]
    out.append(f"- **Statuses by truth.** Healthy: {m['statuses_by_truth']['healthy']}; diseased: "
               f"{m['statuses_by_truth']['diagnosed']}; look-alikes: {m['statuses_by_truth']['lookalike']}.")
    out.append(f"- **Wrong `confirmed`:** {listed(m['T11_confirmed_precision'].get('cases'))}. "
               f"**Healthy called sick:** {listed(m['T12_healthy_called_sick'].get('cases'))}. "
               f"**Sick called healthy:** {listed(m['T13_sick_called_healthy'].get('cases'))}.")
    out.append(f"- **Species.** {m['species_reruns']} scans reran after the user's species answer. "
               f"**Wrong suggestion:** {listed(m['species_top1'].get('cases'))}. "
               f"**Wrong primary:** {listed(m['problem_top1'].get('cases'))}.")
    out.append(f"- **Photos refused:** {listed(m['upload'].get('cases'))}.")
    if t16.get("unfinished"):
        out.append(f"- **Unfinished** (timed out or stopped short; counted as slower than any finished scan in T16): "
                   f"{listed(t16['unfinished_cases'])}.")
    q = m["questions"]
    out.append(f"- **Questions.** {q['asked']} asked on {q['scans_with_question']} scans ({q['by_kind']}); answers {q['answers']}; "
               f"{q['open_at_end']} scans ended with a question open (continued after 10 minutes).")
    for path, t in m["time"].items():
        out.append(f"- **Time, {path}** ({t['n']} scans): submit to `diagnosed` without waiting p50 {t['machine_p50']} s, "
                   f"p95 {t['machine_p95']} s; to the first diagnosis p50 {t['first_diagnosis_p50']} s, p95 "
                   f"{t['first_diagnosis_p95']} s; to `plan_ready` p50 {t['plan_ready_p50']} s, p95 {t['plan_ready_p95']} s "
                   f"({t['continued']} continued after a question).")
    w = m["workers"]
    out.append(f"- **Workers.** VLM path per scan: {w['vlm_path']} (from the diagnosis's `model_versions`; the API doesn't "
               f"name the worker of a job). Healthy workers at submit: "
               + "; ".join(f"{k} ({v})" for k, v in w["at_submit"].items()) + ".")
    out.append("- **Model versions:** " + "; ".join(f"`{k}` ({v})" for k, v in w["model_versions"].items()) + ".")
    mg = m["margin"]
    out += ["", "## Diagnosis margin (evaluation 5)", ""]
    out.append(f"Recomputed from the final diagnosis's logged scores on {mg['cases_with_scores']} scans with candidates. "
               f"Margins consistent with every live `confirmed` and `uncertain`: {mg['consistent_with_live'] or 'none'}. "
               "A scan continued with its question open stays `uncertain` at any margin; a margin that would have "
               "left a live `confirmed` ambiguous could have asked a question, which an `unsure` answer leaves as scored.")
    out.append("")
    out.append("| Margin | Precision of `confirmed` | Coverage (diseased) | Healthy `confirmed` |")
    out.append("|---|---|---|---|")
    for s in mg["sweep"]:
        mark = " (proposed)" if s["margin"] == mg["proposed"] else ""
        out.append(f"| {s['margin']}{mark} | {k_of_n(s['precision'])} | {k_of_n(s['coverage'])} | {k_of_n(s['healthy_confirmed'])} |")
    out.append("")
    out.append(f"Leads of the top candidate over the second (not healthy): {mg['leads'] or 'none'}. "
               + (f"Proposed margin: **{mg['proposed']}**, the smallest whose precision is 90% or more on at least "
                  f"{mg['min_confirmed']} `confirmed` scans (evaluation 5); its 95% interval "
                  + ("clears 90%." if mg.get("proposed_verdict") == "met" else
                     "doesn't clear 90% yet, so the margin rests on the point estimate.")
                  if mg["proposed"] else f"No margin reaches 90% precision on at least {mg.get('min_confirmed', 1)} "
                  "`confirmed` scans."))
    if mg.get("weights"):
        wt = mg["weights"]
        out.append(f"Indicator weights (open question 4): at margin {wt['margin']}, scans left `uncertain` with a lead of "
                   f"exactly {wt['lead']}: {k_of_n(wt['uncertain_at_lead'])} right ({listed(wt['uncertain_at_lead'].get('cases'))}).")
    out += ["", "## Cases", "", "| Case | Truth | Status | Primary (score, lead) | Working species | VLM | To the diagnosis (s) |",
            "|---|---|---|---|---|---|---|"]
    for r in rep["rows"]:
        truth = r["truth_outcome"] + (" " + ", ".join(r["truth_problems"]) if r["truth_problems"] else "")
        status = r["status"] or r["failure"] or r["outcome"]
        prim = f"{r['primary']} ({r['top_score']}, {r['lead']})" if r["primary"] else "—"
        out.append(f"| `{r['case_id']}` | {truth} ({r['truth_species']}) | {status} | {prim} | {r['working_species']} | "
                   f"{r['vlm_path']} | {r['machine_s']} |")
    return "\n".join(out) + "\n"


def write(rep: dict, reports: Path, name: str) -> tuple[Path, Path]:
    reports.mkdir(parents=True, exist_ok=True)
    j, md = reports / f"{name}.json", reports / f"{name}.md"
    j.write_text(json.dumps(rep, indent=1, sort_keys=False) + "\n")
    md.write_text(markdown(rep))
    return j, md


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="J5: score e2e.py's JSONL per stage (evaluation 9).")
    ap.add_argument("runs", nargs="*", type=Path, help="e2e.py JSONL files; a later line for a case wins")
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    ap.add_argument("--reports", type=Path, default=REPORTS)
    ap.add_argument("--name", help="report stem; default e2e-<today>")
    ap.add_argument("--rebuild", type=Path, help="write the markdown again from a report's JSON")
    args = ap.parse_args(argv)
    if args.rebuild:
        rep = json.loads(args.rebuild.read_text())
        rep["metrics"] = metrics(rep["rows"])
        rep["rebuilt"] = {"command": "python3 ml/training/eval/score.py --rebuild " + shlex.quote(str(args.rebuild)),
                          "at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")}
        name = args.rebuild.stem
        j, md = write(rep, args.rebuild.parent, name)
        print(f"rebuilt {md} from {j}")
        return 0
    if not args.runs:
        ap.error("give e2e.py's JSONL, or --rebuild REPORT.json")
    records = read_records(args.runs)
    if not records:
        raise SystemExit("no records in the JSONL")
    if any(r.get("split") != "dev" for r in records):
        raise SystemExit("a record isn't a dev case: the test split isn't scored again (evaluation 4)")
    manifest = {json.loads(line)["case_id"]: json.loads(line) for line in args.manifest.read_text().splitlines() if line.strip()}
    changed = [r["case_id"] for r in records if r["case_id"] in manifest and manifest[r["case_id"]]["truth"] != r["truth"]]
    if changed:
        print(f"warning: {len(changed)} case(s) have another truth in the manifest now; scoring the manifest's: "
              f"{changed[:5]}", file=sys.stderr)
        for r in records:
            if r["case_id"] in changed:
                r["truth"] = manifest[r["case_id"]]["truth"]
    policies = {json.dumps(r.get("policy"), sort_keys=True) for r in records}
    if len(policies) > 1:
        raise SystemExit("the runs answered with different policies; score each policy on its own: " + "; ".join(sorted(policies)))
    kb = knowledge()
    rows = [case_row(r, kb) for r in records]
    first = records[0]
    name = args.name or f"e2e-{dt.date.today().isoformat()}"
    rep = {
        "name": name,
        "made_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "git_sha": git_sha(),
        "command": "python3 ml/training/eval/score.py " + " ".join(shlex.quote(a) for a in (argv if argv is not None else sys.argv[1:])),
        "split": "dev",
        "cases": len(rows),
        "base_url": first.get("base_url"),
        "garden_id": first.get("garden_id"),
        "run_names": sorted({r.get("run_name") for r in records}),
        "policy": first.get("policy"),
        "inputs": {"runs": [file_id(p) for p in args.runs], "manifest": file_id(args.manifest)},
        "knowledge": {"label_only": sorted(k for k, v in kb["species_profile"].items() if v == "label_only"),
                      "problems": sorted(kb["problem_affects"])},
        "rows": rows,
    }
    rep["metrics"] = metrics(rows)
    j, md = write(rep, args.reports, name)
    print(f"wrote {md} and {j.name}: {len(rows)} cases, {rep['metrics']['outcomes']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
