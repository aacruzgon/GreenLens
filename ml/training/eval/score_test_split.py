"""Score the evaluation's test split once (task M18; devpost.md 10; evaluation 4): the numbers for Devpost.

    python -m ml.training.eval.score_test_split [--split test] --plant <plant_check report .json> \\
        --vlm <vlm_bench run directory> --faces <face_check raw .jsonl> [--out test-split-<date>]

Nothing is fitted or chosen here. Every model runs from the final manifest's files (M17) at the values
it ships: the label matrix the API asks for at the `bioclip` entry's `species_temperature`, the disease
head the seed's `classifier_labels.yaml` names at its file's temperature, and the plant check at
`leaf_detector`'s `tau_leaf` and `plant_probe`'s `tau_plant`.

- **T1, plant check:** read from `plant_check --cases <manifest> --case-split test --pair
  <probe>:<tau_leaf>:<tau_plant>` (`--plant`). A case passes when any of its plant shots passes. The
  run must be on this manifest's cases, at the shipped pair.
- **T2, T3, species:** `species_check`'s zero-shot per case (every plant shot embedded whole, the shots
  averaged with the worker's weights), on the cases of the species head's five crops.
- **T4, species with the VLM:** calibrate's species stage (each shot stored at 1,600 px), joined per case
  with the VLM's species guess: BioCLIP 2's top 1 at 0.8 or more, the VLM naming the same species at
  medium or high, and both wrong, over the VLM's finished jobs.
- **T5–T7, disease head:** calibrate's leaf photos (one crop, or the whole close-up), the head restricted
  to the case's true species. T5 is the true problem among the top 3 the worker sends, on photos with a
  problem, beside guessing 3 of the species' classes. T6 is the macro F1 of the top 1 per crop: the mean
  over the classes that are a photo's label or its top 1. T7 is the ECE (15 bins) at the file's temperature.
- **T8–T10, VLM:** the `vlm_bench run` directory at 1,003,520 pixels and 2 jobs at once (`--vlm`):
  calls valid on the first try (two per finished job), jobs finished after one retry, the species
  guess, and the job time at p95.
- **T17, T18, upload:** `face_check run --sets test --variants original --thresholds 0.3` (`--faces`):
  the photos the quality gate rejects, and the photos with a face box.
- **T11–T16** need cases through the API (J5's end-to-end harness), which wasn't built: not measured.

A row without its input is "not measured", with the reason. Each rate is k of n with its 95% Wilson
interval, and a target is met or missed only when the whole interval is on one side of it; otherwise it
is undecided at that count (evaluation 4). Every row is given on all the cases and without the 7 that
were dev cases when choices were made (devpost 2). T5 is also given on the `sure` photos alone, and
without those 7 and the two test leaves whose repeats were in dev.

`--split dev` scores the dev cases the same way, so the code can be checked against the committed dev
reports before the test split is scored. Writes `reports/<out>.md` and `.json`; the JSON keeps each
case's result and the disease head's logits per leaf photo (no image data).
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import shlex
import sys
import tomllib
from collections import Counter
from pathlib import Path

import numpy as np

from ml.training.eval import calibrate as cal

ROOT = Path(__file__).resolve().parents[3]
EVAL = ROOT / "ml/training/eval"
MODELS = ROOT / "ml/models"
REPORTS = EVAL / "reports"
PIXELS = 1003520          # the worker's VLM_MAX_PIXELS on the 5070 Ti (D1)
AT_ONCE = 2               # VLM_SLOTS (D1): two jobs at once
SPECIES_HEAD = "species-head-v1-2026-10-20"   # its crops are the five crops, as in species_check
PLANT_KINDS = ("whole_plant", "leaf_closeup", "leaf_underside", "flower_fruit")
Z = 1.959964              # the 95% two-sided normal quantile
# Test cases that were dev cases when v3c, the VLM or the calibration were chosen (devpost 2).
FORMER_DEV = ("apple-inat-401743982", "apple-inat-401742038", "apple-inat-401735945", "squash-inat-407881529",
              "corn-maize-bd-2026-gray-leaf-spot-img-500", "tomato-tomato-bd-2025-late-blight-late-blight-7",
              "corn-maize-bd-2026-healthy-leaf-img-43")
# Test leaves whose repeat photos were dev cases when v3c was chosen (devpost 2).
REPEAT_LEAVES = ("tomato-tomato-bd-iphone-2025-early-blight-img-2624",
                 "tomato-tomato-bd-iphone-2025-late-blight-img-5537-02d")
NOT_BUILT = "not measured: needs cases through the API, and J5's end-to-end harness wasn't built"


# --- counting -----------------------------------------------------------------------------------

def wilson(k: int, n: int, z: float = Z) -> list[float] | None:
    """The 95% Wilson score interval of k successes in n."""
    if n == 0:
        return None
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [max(0.0, c - h), min(1.0, c + h)]


def verdict(interval: list[float] | None, target: float, at_least: bool) -> str:
    """Met or missed only when the whole interval is on one side of the target (evaluation 4)."""
    if interval is None:
        return "no cases"
    lo, hi = interval
    if at_least:
        return "met" if lo >= target else "missed" if hi < target else "undecided"
    return "met" if hi <= target else "missed" if lo > target else "undecided"


def rate(k: int, n: int, target: float | None = None, at_least: bool = True) -> dict:
    ci = wilson(k, n)
    out = {"k": int(k), "n": int(n), "rate": k / n if n else None, "interval": ci}
    if target is not None:
        out.update(target=target, at_least=at_least, verdict=verdict(ci, target, at_least))
    return out


def subsets(case_ids) -> dict[str, set[str]]:
    """Every case, and the cases left once the former dev cases are taken out."""
    ids = set(case_ids)
    return {"all": ids, "without_former_dev": ids - set(FORMER_DEV)}


def file_id(path: Path) -> dict:
    path = Path(path).resolve()
    return {"file": str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()[:16]}


# --- what the manifest ships --------------------------------------------------------------------

def shipped(entries: list[dict]) -> dict:
    """The values the final manifest ships for each scored model."""
    from ml.training.labels import current

    by = {e["name"]: e for e in entries}
    head_version = current.classifier_labels_version()
    head = next((e for e in entries if e["name"].startswith("disease_head") and e["version"] == head_version), None)
    if head is None:
        raise SystemExit(f"the manifest has no disease head {head_version}, the one classifier_labels.yaml names")
    matrix = current.matrix_entry(entries)
    return {"species_temperature": float(by["bioclip"]["config"]["species_temperature"]),
            "matrix": {"version": matrix["version"], "file": matrix["file"]},
            "disease_head": {"version": head["version"], "file": head["file"]},
            "plant_pair": {"set": by["plant_probe"]["version"], "tau_leaf": float(by["leaf_detector"]["config"]["tau_leaf"]),
                           "tau_plant": float(by["plant_probe"]["config"]["tau_plant"])}}


# --- T1: plant check ----------------------------------------------------------------------------

def plant_passes(doc: dict, cases: list[dict], pair: dict, manifest_sha: str) -> dict[str, bool]:
    """Each case's plant check at the shipped pair, from a `plant_check --cases` report on these cases."""
    if doc.get("cases_file", {}).get("sha256") != manifest_sha[:16]:
        raise SystemExit(f"--plant scored {doc.get('cases_file')}, not this manifest ({manifest_sha[:16]})")
    if doc.get("cases") != len(cases):
        raise SystemExit(f"--plant scored {doc.get('cases')} cases; the split has {len(cases)}")
    r = next((p for p in doc["pairs"] if p["set"] == pair["set"] and p["tau_leaf"] == pair["tau_leaf"]
              and p["tau_plant"] == pair["tau_plant"]), None)
    if r is None:
        raise SystemExit(f"--plant has no pair {pair['set']} {pair['tau_leaf']} / {pair['tau_plant']}, the shipped one")
    ids = {c["case_id"] for c in cases}
    failing = {f["case"] for f in r["failing_cases"]}
    if failing - ids:
        raise SystemExit(f"--plant fails cases outside the split: {sorted(failing - ids)}")
    return {c["case_id"]: c["case_id"] not in failing for c in cases}


def t1(passes: dict[str, bool]) -> dict:
    return {name: rate(sum(passes[i] for i in ids), len(ids), 0.99) for name, ids in subsets(passes).items()}


# --- T2–T4: species -----------------------------------------------------------------------------

def species_rows(cases: list[dict], photos: Path) -> list[dict]:
    """Every plant shot of the cases with a species, as `species_check` reads the dev cases."""
    return [{"path": photos / im["file"], "species_id": c["truth"]["species_id"], "case": c["case_id"], "kind": im["kind"]}
            for c in cases if c["truth"].get("species_id") for im in c["images"] if im["kind"] in PLANT_KINDS]


def zero_shot_cases(rows: list[dict], X: np.ndarray, matrix_rows: np.ndarray, ids: list[str], t: float,
                    crops: list[str]) -> dict[str, dict]:
    """`species_check`'s zero-shot per case over the matrix's species rows, on the cases of `crops`."""
    from ml.training.eval.species_check import by_case
    from ml.training.heads.train_disease import softmax
    from ml.training.heads.train_species import LOGIT_SCALE

    p = softmax(LOGIT_SCALE / t * X @ matrix_rows.T)
    truth, pc = by_case(rows, p)
    case_ids = sorted({r["case"] for r in rows})       # by_case's order
    top3 = np.asarray(ids)[np.argsort(-pc, 1)[:, :3]]
    return {cid: {"truth": tr, "top3": [str(s) for s in top], "top1_right": bool(top[0] == tr), "top3_right": bool(tr in top)}
            for cid, tr, top in zip(case_ids, truth, top3) if tr in crops}


def t2_t3(zs: dict[str, dict]) -> tuple[dict, dict]:
    sets = subsets(zs)
    return ({name: rate(sum(zs[i]["top1_right"] for i in ids), len(ids), 0.85) for name, ids in sets.items()},
            {name: rate(sum(zs[i]["top3_right"] for i in ids), len(ids), 0.95) for name, ids in sets.items()})


def stored_species_cases(cases: list[dict], photos: Path, matrix: dict, embed_images, t: float) -> dict[str, dict]:
    """Calibrate's species stage per case at temperature `t`: each shot stored at 1,600 px, embedded,
    its softmax over the matrix's species rows averaged with the worker's weights. A case whose species
    the matrix can't name is kept, and is always wrong."""
    ids = [l["id"] for l in matrix["labels"] if l["table"] == "species"]
    rows = matrix["rows"][[i for i, l in enumerate(matrix["labels"]) if l["table"] == "species"]]
    scored = [(c, [im for im in c["images"] if im["kind"] in cal.SHOT_WEIGHT]) for c in cases if c["truth"].get("species_id")]
    scored = [(c, shots) for c, shots in scored if shots]
    files = [photos / im["file"] for _, shots in scored for im in shots]
    emb = dict(zip(files, embed_images([cal.stored(f) for f in files]))) if files else {}
    out = {}
    for c, shots in scored:
        p = cal.species_probs([emb[photos / im["file"]] @ rows.T for im in shots], [cal.SHOT_WEIGHT[im["kind"]] for im in shots], t)
        k = int(p.argmax())
        out[c["case_id"]] = {"truth": c["truth"]["species_id"], "top": ids[k], "confidence": round(float(p[k]), 4)}
    return out


def t4(bioclip: dict[str, dict], guesses: dict[str, dict]) -> dict:
    """Cases where BioCLIP 2 (0.8 or more) and the VLM (medium or high) agree on a wrong species, over
    the VLM's finished jobs; a case BioCLIP has no score for counts as no agreement."""
    def wrong_together(cid):
        b, g = bioclip.get(cid), guesses[cid]
        return bool(b and b["confidence"] >= cal.AGREEMENT and g["confidence"] is not None and g["confidence"] >= cal.MEDIUM
                    and g["species_id"] == b["top"] and b["top"] != b["truth"])
    return {name: {**rate(sum(wrong_together(i) for i in ids), len(ids), 0.03, at_least=False),
                   "cases": sorted(i for i in ids if wrong_together(i))}
            for name, ids in subsets(guesses).items()}


# --- T5–T7: disease head ------------------------------------------------------------------------

def macro_f1(pairs: list[tuple[int, int]]) -> float | None:
    """Mean F1 over the classes that are a label or a top 1 among (label, top 1) pairs."""
    classes = sorted({c for p in pairs for c in p})
    if not classes:
        return None
    f1 = []
    for c in classes:
        tp = sum(y == c and k == c for y, k in pairs)
        fp = sum(y != c and k == c for y, k in pairs)
        fn = sum(y == c and k != c for y, k in pairs)
        f1.append(2 * tp / (2 * tp + fp + fn))
    return float(np.mean(f1))


def disease(items: list, head: dict, cases: list[dict]) -> dict:
    """T5 on its three sets, T6 per crop and T7, at the head file's temperature."""
    healthy = {l["index"] for l in head["labels"] if l["problem_id"] == "healthy"}
    t = float(head["temperature"])
    sure = {c["case_id"] for c in cases if c.get("label_confidence") == "sure"}
    left_out = set(FORMER_DEV) | set(REPEAT_LEAVES)
    sets = {"all": lambda i: True, "sure": lambda i: i[2] in sure, "without_former_dev_and_repeats": lambda i: i[2] not in left_out}
    t5 = {}
    for name, keep in sets.items():
        s = cal.disease_scores([i for i in items if keep(i)], t, healthy)
        n = s.get("with_problem", 0)
        t5[name] = {**rate(round(s["top3_sent"] * n) if n else 0, n, 0.85), "guessing": s.get("chance_top3_sent")}
    t6 = {sp: {"photos": sum(i[3] == sp for i in items),
               "macro_f1": macro_f1([(int(i[1]), int(np.argmax(i[0]))) for i in items if i[3] == sp])}
          for sp in sorted({i[3] for i in items})}
    every = cal.disease_scores(items, t, healthy)
    return {"temperature": t, "photos": len(items), "t5": t5, "t6": t6,
            "t7": {"ece": every.get("ece"), "target": 0.05, "verdict": None if every.get("ece") is None else
                   ("met" if every["ece"] <= 0.05 else "missed"), "photos": len(items)},
            "top1": every.get("top1"), "confusion": cal.confusion(items, head), "inputs": cal.disease_inputs(items)}


# --- T8–T10: VLM --------------------------------------------------------------------------------

def vlm_run(run: Path, at_once: int, case_ids: set[str]) -> tuple[dict, list[dict], dict]:
    """A `vlm_bench run` directory's meta, rows and bench cases at PIXELS and `at_once`."""
    from ml.training.eval import vlm_bench

    d = Path(run) / f"px{PIXELS}-c{at_once}"
    meta = json.loads((d / "meta.json").read_text())
    if meta.get("max_pixels") != PIXELS or meta.get("concurrency") != at_once:
        raise SystemExit(f"{d}: run at {meta.get('max_pixels')} pixels, {meta.get('concurrency')} at once")
    cases_file = d / "cases.jsonl" if (d / "cases.jsonl").exists() else Path(meta["cases"])
    bench = {b["case_id"]: b for b in map(json.loads, filter(str.strip, cases_file.read_text().splitlines()))}
    if set(bench) - case_ids:
        raise SystemExit(f"{d}: bench cases outside the split: {sorted(set(bench) - case_ids)[:5]}")
    rows = [r for r in vlm_bench.read_rows(d / "out.jsonl") if r.get("case_id") in bench]
    return meta | {"cases_file": file_id(cases_file), "out": file_id(d / "out.jsonl")}, rows, bench


def species_guesses(rows: list[dict]) -> dict[str, dict]:
    """The VLM's species guess per finished job."""
    return {r["case_id"]: {"species_id": r["result"]["findings"]["species_guess"].get("species_id"),
                           "confidence": r["result"]["findings"]["species_guess"].get("confidence")}
            for r in rows if r["ok"]}


def vlm(rows: list[dict], bench: dict[str, dict]) -> dict:
    """T8–T10 on each set of cases: calls valid on the first try (two per finished job), jobs finished
    after one retry, the species guess and the job time at p95."""
    from ml.training.eval import vlm_bench

    out = {}
    for name, ids in subsets(bench).items():
        mine = [r for r in rows if r["case_id"] in ids]
        s = vlm_bench.score_run(mine, bench)
        finished, calls = s["finished"], 2 * s["finished"]
        right = sum(r["result"]["findings"]["species_guess"].get("species_id") == bench[r["case_id"]]["truth"]["species_id"]
                    for r in mine if r["ok"])
        out[name] = {"jobs": len(ids), "ran": len(mine), "finished": finished, "retried_calls": s["retried_calls"],
                     "t8_first_try": rate(calls - s["retried_calls"], calls, 0.99),
                     "t8_after_one_retry": rate(finished, len(ids)),
                     "t9_species": rate(right, finished),
                     "t10_job_p95_s": None if s["job_ms_p95"] is None else round(s["job_ms_p95"] / 1000, 1),
                     "job_p50_s": None if s["job_ms_p50"] is None else round(s["job_ms_p50"] / 1000, 1)}
        p95 = out[name]["t10_job_p95_s"]
        out[name]["t10_verdict"] = None if p95 is None else ("met" if p95 < 150 else "missed")
    return out


# --- T17, T18: upload ---------------------------------------------------------------------------

def upload_photos(raw: Path, cases: list[dict], photos: Path) -> dict[str, dict]:
    """Each photo's quality verdict and face boxes from a `face_check` raw file, keyed `<case>#<n>`."""
    by_path = {}
    for line in filter(str.strip, Path(raw).read_text().splitlines()):
        r = json.loads(line)
        by_path[str(Path(r["path"]).resolve())] = r
    out = {}
    for c in cases:
        for n, im in enumerate(c["images"]):
            r = by_path.get(str((photos / im["file"]).resolve()))
            if r is None or "error" in r:
                raise SystemExit(f"{raw}: no result for {im['file']}" + (f" ({r['error']})" if r else ""))
            out[f"{c['case_id']}#{n}"] = {"case": c["case_id"], "rejected": r["verdict"], "face_boxes": len(r["faces"])}
    return out


def upload(per_photo: dict[str, dict]) -> dict:
    out = {}
    for name, ids in subsets(p["case"] for p in per_photo.values()).items():
        mine = [p for p in per_photo.values() if p["case"] in ids]
        out[name] = {"t17_rejected": rate(sum(p["rejected"] is not None for p in mine), len(mine), 0.05, at_least=False),
                     "t18_face_box": rate(sum(p["face_boxes"] > 0 for p in mine), len(mine), 0.02, at_least=False),
                     "reasons": dict(Counter(p["rejected"] for p in mine if p["rejected"] is not None))}
    return out


# --- the report ---------------------------------------------------------------------------------

def k_of_n(r: dict | None) -> str:
    if not r or r.get("n") is None:
        return "not measured"
    if not r["n"]:
        return "no cases"
    lo, hi = r["interval"]
    return f"{r['k']} of {r['n']} ({r['rate']:.1%}; {lo:.0%}–{hi:.0%})"


def row(label: str, what: str, target: str, all_: str, without: str, met: str) -> str:
    return f"| {label} | {what} | {target} | {all_} | {without} | {met} |"


def both(rows: dict | None, key: str | None = None) -> tuple[str, str, str]:
    """A T row's two cells and its verdicts, on every case and without the former dev cases."""
    if rows is None:
        return "not measured", "not measured", ""
    a, w = (rows["all"], rows["without_former_dev"]) if key is None else (rows["all"][key], rows["without_former_dev"][key])
    met = " / ".join(x.get("verdict", "") for x in (a, w)) if "verdict" in a else "report"
    return k_of_n(a), k_of_n(w), met


def markdown(rep: dict, out: str) -> str:
    s, inputs = rep["shipped"], rep["inputs"]
    lines = [f"# Test split, scored once: {out}", "",
             f"`{rep['command']}`, on {rep['date']}, over `{inputs['manifest']['file']}` (SHA-256 `{inputs['manifest']['sha256']}`), "
             f"{rep['split']} split: {rep['cases']} cases. The model manifest is `ml/models/manifest.toml` (SHA-256 "
             f"`{inputs['models_manifest']['sha256']}`). Nothing was fitted or chosen on these cases ([score_test_split.py](../score_test_split.py)).", "",
             f"Shipped values: label matrix `{s['matrix']['version']}` at `species_temperature` {s['species_temperature']}; disease head "
             f"`{s['disease_head']['version']}` at its file's temperature {rep['disease']['temperature'] if rep['disease'] else 'n/a'}; plant check "
             f"`{s['plant_pair']['set']}` at `τ_leaf` {s['plant_pair']['tau_leaf']} and `τ_plant` {s['plant_pair']['tau_plant']}.", "",
             "Each rate is k of n (rate; 95% Wilson interval). A target is met or missed only when the whole interval is on one side "
             "of it, otherwise undecided at that count (evaluation 4). **Without the 7** leaves out the test cases that were dev cases "
             f"when choices were made: {', '.join(f'`{c}`' for c in FORMER_DEV)}.", "",
             "| Row | What | Target | Every case | Without the 7 | Met (every case / without the 7) |",
             "|---|---|---|---|---|---|"]
    lines.append(row("T1", "Plant check: cases with a plant that pass", "99% or more", *both(rep["t1"])))
    lines.append(row("T2", "Species, BioCLIP 2: top-1 on the five crops", "85% or more", *both(rep["t2"])))
    lines.append(row("T3", "Species, BioCLIP 2: top-3 on the five crops", "95% or more", *both(rep["t3"])))
    lines.append(row("T4", "BioCLIP 2 (0.8 or more) and the VLM (medium or high) agree on a wrong species", "3% or less", *both(rep["t4"])))
    d = rep["disease"]
    if d:
        t5 = d["t5"]
        guess = t5["all"]["guessing"]
        lines.append(row("T5", "Disease head: true problem in the top 3, per leaf photo with a problem"
                         + (f" (guessing {guess:.0%})" if guess is not None else ""),
                         "85% or more", k_of_n(t5["all"]), k_of_n(t5["without_former_dev_and_repeats"]) + "; also without the 2 repeat leaves",
                         f"{t5['all']['verdict']} / {t5['without_former_dev_and_repeats']['verdict']}"))
        lines.append(row("T5, `sure`", "The same on the `sure` cases' photos", "85% or more", k_of_n(t5["sure"]), "", t5["sure"]["verdict"]))
        lines.append(row("T6", "Disease head: macro F1 of the top-1, per crop", "Report",
                         "; ".join(f"{sp} {v['macro_f1']:.2f} ({v['photos']} photos)" for sp, v in d["t6"].items() if v["macro_f1"] is not None),
                         "", "report"))
        lines.append(row("T7", f"Disease head: ECE (15 bins) at the file's temperature, {d['photos']} leaf photos", "0.05 or less",
                         f"{d['t7']['ece']:.3f}" if d["t7"]["ece"] is not None else "no photos", "", d["t7"]["verdict"] or ""))
    else:
        for label in ("T5", "T6", "T7"):
            lines.append(row(label, "Disease head", "", "not measured", "not measured", ""))
    v = rep["vlm"]
    if v:
        a, w = v["all"], v["without_former_dev"]
        lines.append(row("T8", "VLM: calls valid on the first try (two per finished job)", "99% or more",
                         k_of_n(a["t8_first_try"]), k_of_n(w["t8_first_try"]), f"{a['t8_first_try']['verdict']} / {w['t8_first_try']['verdict']}"))
        lines.append(row("T8, retry", "VLM: jobs finished after one retry", "100%", f"{a['finished']} of {a['jobs']}",
                         f"{w['finished']} of {w['jobs']}", "met" if a["finished"] == a["jobs"] else "missed"))
        lines.append(row("T9", "VLM: species guess, top-1, on its finished jobs", "Report beside BioCLIP 2's",
                         k_of_n(a["t9_species"]), k_of_n(w["t9_species"]), "report"))
        lines.append(row("T10", f"VLM: job time at p95, {rep['vlm_meta']['concurrency']} at once", "Under 150 s",
                         f"{a['t10_job_p95_s']} s (p50 {a['job_p50_s']} s)", f"{w['t10_job_p95_s']} s", a["t10_verdict"] or ""))
    else:
        for label in ("T8", "T9", "T10"):
            lines.append(row(label, "VLM", "", rep["missing"].get("vlm", "not measured"), "", ""))
    for label in ("T11", "T12", "T13", "T14", "T15", "T16"):
        lines.append(row(label, "End to end through the API", "", NOT_BUILT, "", ""))
    lines.append(row("T17", "Upload: photos the blur and exposure check rejects", "5% or less", *both(rep["upload"], "t17_rejected")))
    lines.append(row("T18", "Upload: photos with a face box (YuNet at 0.3, as uploaded)", "2% or less", *both(rep["upload"], "t18_face_box")))
    lines += ["", "## Details", ""]
    if rep["t1"]:
        fails = [c for c, ok in rep["plant_passes"].items() if not ok]
        lines.append(f"- **T1.** Cases failing the plant check: {', '.join(f'`{c}`' for c in sorted(fails)) or 'none'}.")
    if rep["species_cases"]:
        wrong = {c: r for c, r in rep["species_cases"].items() if not r["top1_right"]}
        lines.append(f"- **T2, T3.** Wrong top-1: " + ("; ".join(f"`{c}` ({r['truth']}) → {', '.join(r['top3'])}" for c, r in sorted(wrong.items())) or "none") + ".")
    if rep["t4"]:
        lines.append(f"- **T4.** Cases where both agree on a wrong species: {', '.join(f'`{c}`' for c in rep['t4']['all']['cases']) or 'none'}.")
    if d:
        lines.append(f"- **T5–T7.** {d['photos']} leaf photos; top-1 {d['top1']:.0%}. Top-1 answers per true class: " + " ".join(
            f"**{sp}:** " + "; ".join(f"{cal.short(t)} ({sum(c.values())}) → " + ", ".join(f"{cal.short(p)} {n}" for p, n in c.items())
                                      for t, c in rows_.items()) + "." for sp, rows_ in d["confusion"].items()))
        lines.append(f"- **T5, left out:** " + (", ".join(f"{v} {k}" for k, v in rep["disease_left_out"].items()) or "none") + ".")
    if v:
        m = rep["vlm_meta"]
        lines.append(f"- **T8–T10.** Run `{m.get('label')}` on {m.get('model')}, {m['max_pixels']:,} pixels, {m['concurrency']} at once: "
                     f"{v['all']['ran']} of {v['all']['jobs']} jobs ran, {v['all']['retried_calls']} calls retried, peak GPU "
                     f"{m.get('peak_gpu_mib', 0):,} MiB. Bench cases `{m['cases_file']['file']}` (SHA-256 `{m['cases_file']['sha256']}`).")
    if rep["upload"]:
        lines.append(f"- **T17, T18.** Rejected for: " + (", ".join(f"{k} {n}" for k, n in rep["upload"]["all"]["reasons"].items()) or "none") + ".")
    if rep["missing"]:
        lines.append("- **Not measured:** " + "; ".join(f"{k}: {why}" for k, why in rep["missing"].items()) + ".")
    lines += ["", "Inputs: " + "; ".join(f"{k} `{v['file']}` (`{v['sha256']}`)" for k, v in inputs.items() if isinstance(v, dict) and "file" in v) + "."]
    return "\n".join(lines) + "\n"


def main(argv=None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    today = dt.date.today().isoformat()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--split", default="test", choices=("dev", "test"))
    ap.add_argument("--manifest", type=Path, default=EVAL / "manifest.jsonl")
    ap.add_argument("--photos", type=Path, default=EVAL / "photos")
    ap.add_argument("--plant", type=Path, help="plant_check's report on the split's cases (.json)")
    ap.add_argument("--vlm", type=Path, help="vlm_bench's run directory for the candidate (holds px1003520-c2/)")
    ap.add_argument("--at-once", type=int, default=AT_ONCE, help="the VLM run's jobs at once (default 2, what ships)")
    ap.add_argument("--faces", type=Path, help="face_check's raw file for the split at 0.3, as uploaded (.jsonl)")
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)
    out = args.out or f"{args.split}-split-{today}"

    data = args.manifest.read_bytes()
    manifest_sha = hashlib.sha256(data).hexdigest()
    cases = [c for c in map(json.loads, filter(str.strip, data.decode().splitlines())) if c["split"] == args.split]
    ids = {c["case_id"] for c in cases}
    entries = tomllib.loads((MODELS / "manifest.toml").read_text())["model"]
    s = shipped(entries)
    rep = {"date": today, "command": shlex.join(["python", "-m", "ml.training.eval.score_test_split", *argv]), "split": args.split,
           "cases": len(cases), "shipped": s, "missing": {},
           "inputs": {"manifest": file_id(args.manifest), "models_manifest": file_id(MODELS / "manifest.toml")},
           "t1": None, "plant_passes": {}, "t2": None, "t3": None, "species_cases": {}, "t4": None, "bioclip_stored": {},
           "disease": None, "disease_left_out": {}, "vlm": None, "vlm_meta": None, "upload": None, "upload_photos": {}}

    if args.plant:
        rep["inputs"]["plant"] = file_id(args.plant)
        rep["plant_passes"] = plant_passes(json.loads(args.plant.read_text()), cases, s["plant_pair"], manifest_sha)
        rep["t1"] = t1(rep["plant_passes"])
    else:
        rep["missing"]["T1"] = "no plant_check report given"

    from ml.pdm_vision import headfile
    from ml.pdm_vision.features import Embedder
    from ml.training.heads.crops import CropEmbedder
    from ml.training.heads.train_species import matrix_species

    embedder = Embedder()
    matrix_entry = next(e for e in entries if e.get("version") == s["matrix"]["version"] and e["name"].startswith("bioclip_labels"))
    matrix_rows, matrix_ids = matrix_species(matrix_entry)
    crops = [l["species_id"] for l in headfile.read_linear_head(cal.HEADS / SPECIES_HEAD / f"{SPECIES_HEAD}.json")["labels"]]
    rows = species_rows(cases, args.photos)
    rep["species_cases"] = zero_shot_cases(rows, embedder.files([r["path"] for r in rows]), matrix_rows, matrix_ids,
                                           s["species_temperature"], crops)
    rep["t2"], rep["t3"] = t2_t3(rep["species_cases"])
    matrix = headfile.read_label_matrix(MODELS / "files" / s["matrix"]["file"])
    rep["bioclip_stored"] = stored_species_cases(cases, args.photos, matrix, embedder.images, s["species_temperature"])

    head = headfile.read_linear_head(MODELS / "files" / s["disease_head"]["file"])
    rep["inputs"]["disease_head"] = file_id(MODELS / "files" / s["disease_head"]["file"])
    cropper = CropEmbedder()
    leaf_files = sorted({args.photos / f for c in cases for f, _ in cal.leaf_photos(c)})
    cropper.run(leaf_files)
    loaded = dict(zip(leaf_files, cropper.load(leaf_files)))
    whole = dict(zip(leaf_files, embedder.images([cal.stored(f) for f in leaf_files])))
    items, left_out = cal.disease_items(cases, args.photos, head, lambda f: loaded[f]["emb"], lambda f: whole[f])
    rep["disease"], rep["disease_left_out"] = disease(items, head, cases), dict(left_out)

    if args.vlm:
        meta, vrows, bench = vlm_run(args.vlm, args.at_once, ids)
        rep["vlm_meta"] = {k: meta.get(k) for k in ("label", "max_pixels", "concurrency", "seconds", "peak_gpu_mib", "exit", "cases_file", "out")}
        rep["vlm_meta"]["model"] = (meta["models"][0].get("root") or meta["models"][0]["id"]) if meta.get("models") else None
        rep["inputs"]["vlm_out"] = meta["out"]
        rep["vlm"] = vlm(vrows, bench)
        rep["vlm_guesses"] = species_guesses(vrows)
        rep["t4"] = t4(rep["bioclip_stored"], rep["vlm_guesses"])
    else:
        rep["missing"]["T4, T8–T10"] = "no vlm_bench run given"

    if args.faces:
        rep["inputs"]["faces"] = file_id(args.faces)
        rep["upload_photos"] = upload_photos(args.faces, cases, args.photos)
        rep["upload"] = upload(rep["upload_photos"])
    else:
        rep["missing"]["T17, T18"] = "no face_check raw file given"

    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"{out}.json").write_text(json.dumps(rep, indent=1) + "\n")
    (REPORTS / f"{out}.md").write_text(markdown(rep, out))
    print((REPORTS / f"{out}.md").read_text())
    return 0


if __name__ == "__main__":
    sys.exit(main())
