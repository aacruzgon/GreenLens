"""The day-2 VLM benchmark (dev-roadmap J2; evaluation 8): the worker's own calls on the
evaluation cases, per candidate model, image size and number of jobs in flight.

    python -m ml.training.eval.vlm_bench cases [--split dev] [--out PATH]
    python -m ml.training.eval.vlm_bench run --label fp8 [--max-pixels 602112 1003520 ...] [--concurrency 1 2]
    python -m ml.training.eval.vlm_bench score ml/training/runs/vlm-bench/fp8 ml/training/runs/vlm-bench/qwen35 [--out NAME] [--cases FILE]

`cases` turns the manifest's cases into `vlm` job payloads. The candidates, their indicator
questions and the species and organism options are the worker's own fixtures
(`backend/bins/worker/src/vlm/tests/fixtures/`, tomato and corn): the knowledge base the API
would draw them from isn't loaded yet. So a case of another species is left out, and a
diagnosed case whose problem isn't one of its fixture's three candidates is kept for validity,
timing and false signs, but not scored for diagnosis. A case without a leaf close-up (a one-photo
case, sent as the whole_plant shot) also sends its photo to the indicator call as a leaf close-up,
since a question gets only the images its indicator names. The case offers its shots and that
close-up; the Rust test picks and orders each call's images with the pipeline's routing rules.

`run` drives the Rust test `bench_cases` (backend/bins/worker/src/vlm/tests/bench.rs), which
sends each case through the worker's real `analyse` (its prompts, schemas, image preparation and
validation) to the served model, for every `--max-pixels` and `--concurrency` given. It samples
the GPU's memory with `nvidia-smi` and vLLM's KV-cache usage from `/metrics` while it runs. The
server isn't restarted: start the candidate with `VLM_SLOTS` at least the largest concurrency and
its pixel limit at least the largest `--max-pixels` (vlm.md 4), and run once per candidate with
its own `--label`.

`score` reports per run:
- validity: jobs that finished, and calls retried after a first answer that didn't decode;
- timing: wall time per job (p50, p95) and per call kind;
- species: the VLM's species guess against the truth;
- diagnosis from the indicator answers: each candidate scores its share of "yes" answers, and the
  top candidate (none when every share is 0 or two tie for the top) is compared with the truth, on
  `sure` cases, also split by the case's photo size (under 1 MP, or 1 MP and up);
- false signs: on healthy cases, the share answered "yes"; on every case, damage findings and
  organisms, which these leaf photos don't show (a disease spot called `leaf_mines` is the known
  error, #45). A healthy case with an animal on it (`organisms_unboxed`: a pest the knowledge base
  doesn't name, its box not drawn yet) is left out of the healthy counts, since a "yes" there may be
  right;
- look-alikes: on cases with a `look_alike` (an abiotic problem the knowledge base doesn't name),
  the share that end with no candidate on top, which is what the scan should do (evaluation.md §3);
- unsure: the share of answers that are `unsure`;
- per indicator, for each model at the worker's default 1,003,520 pixels and one job at once: its
  answers on the `sure` cases that have its problem and on the other `sure` cases. A `probable` case
  counts on neither side, since its true "yes" would otherwise read as a false one.
The cases and the raw runs stay in the git-ignored `ml/training/runs/vlm-bench/`; the report is
`ml/training/eval/reports/vlm-bench-<out>.md` and `.json`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import signal
import subprocess
import sys
import threading
import time
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path

from ml.training.eval import inat

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "backend/bins/worker/src/vlm/tests/fixtures"
FIXTURE_OF = {"tomato": "tomato_leaf.json", "corn": "corn_leaf.json"}
RUNS = ROOT / "ml/training/runs/vlm-bench"      # git-ignored: the cases hold this machine's photo paths
REPORTS = ROOT / "ml/training/eval/reports"     # committed: the scored reports
CASES = RUNS / "cases.jsonl"
JOB = "01JAX5AAAAAAAAAAAAAAAAAAAA"   # the job id the Rust test's context uses
CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
# The order the pipeline sends shots in: the whole plant first, then the close-ups.
KIND_ORDER = ("whole_plant", "leaf_closeup", "leaf_underside", "flower_fruit", "soil_surface")
INDICATOR_KINDS = ("leaf_closeup", "leaf_underside", "whole_plant", "flower_fruit")
DEFAULT_PIXELS = (602112, 1003520, 1404928, 2007040)   # about 0.6, 1.0, 1.4 and 2.0 MP, evaluation 8
DIAGNOSIS_NOTE = ("Diagnosis is the top candidate by its unweighted share of \"yes\" answers. The payload carries no "
                  "indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.")


def ulid(seed: str) -> str:
    """A valid ULID derived from `seed`: 128 bits in 26 Crockford base-32 digits."""
    n = int.from_bytes(hashlib.sha256(seed.encode()).digest()[:16], "big")
    return "".join(CROCKFORD[(n >> (5 * i)) & 31] for i in reversed(range(26)))


def fixture(species: str) -> dict:
    return json.loads((FIXTURES / FIXTURE_OF[species]).read_text())


def bench_case(c: dict, photos: Path) -> dict | None:
    """The bench line for one manifest case, or None when no fixture covers its species."""
    sp = c["truth"].get("species_id")
    if sp not in FIXTURE_OF or c["truth"]["outcome"] not in ("diagnosed", "healthy"):
        return None
    fx = fixture(sp)
    shots = sorted(enumerate(c["images"]), key=lambda t: KIND_ORDER.index(t[1]["kind"]) if t[1]["kind"] in KIND_ORDER else 99)
    images, paths = [], {}
    for i, img in shots:
        iid = ulid(f"{c['case_id']}|{i}")
        images.append({"image_id": iid, "kind": img["kind"], "width": 1, "height": 1,
                       "url": f"/internal/jobs/{JOB}/images/{iid}"})
        paths[iid] = str((photos / img["file"]).resolve())
    indicator = [im for k in INDICATOR_KINDS for im in images if im["kind"] == k]
    whole = next((im for im in images if im["kind"] == "whole_plant"), None)
    if whole and not any(im["kind"] == "leaf_closeup" for im in images):
        # A one-photo case is a leaf photo sent as the whole_plant shot (evaluation 2). A question only
        # gets the images its indicator's `image_kinds` names, and one with none can only be unsure, so
        # the indicator call also gets the photo as the leaf close-up it is (review of #86, point 1).
        iid = ulid(f"{c['case_id']}|closeup")
        indicator.insert(0, {**whole, "image_id": iid, "kind": "leaf_closeup", "url": f"/internal/jobs/{JOB}/images/{iid}"})
        paths[iid] = paths[whole["image_id"]]
    payload = {k: fx[k] for k in ("working_species", "species_options", "organism_options", "candidates")}
    payload["findings_images"] = images
    payload["indicator_images"] = indicator or images[:1]
    candidates = [cand["problem_id"] for cand in fx["candidates"]]
    truth = c["truth"]
    kept = {k: truth[k] for k in ("look_alike", "organisms_unboxed") if truth.get(k)}
    return {
        "case_id": c["case_id"], "payload": payload, "photos": paths,
        "truth": {"outcome": truth["outcome"], "species_id": sp, "problems": truth.get("problems", []), **kept},
        "label_confidence": c.get("label_confidence"), "split": c.get("split"),
        "scored_for_diagnosis": plain_healthy(truth) or bool(set(truth.get("problems", [])) & set(candidates)),
    }


def plain_healthy(truth: dict) -> bool:
    """A healthy case with no animal on it: one whose every "yes" is a false sign."""
    return truth["outcome"] == "healthy" and not truth.get("organisms_unboxed") and not truth.get("organisms")


def cases(args) -> int:
    manifest = [json.loads(line) for line in args.manifest.read_text().splitlines() if line.strip()]
    picked = [c for c in manifest if c.get("split") == args.split]
    out, skipped = [], Counter()
    for c in picked:
        b = bench_case(c, args.photos)
        if b is None:
            skipped[c["truth"].get("species_id") or "none"] += 1
        else:
            out.append(b)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text("".join(json.dumps(b) + "\n" for b in out))
    scored = sum(b["scored_for_diagnosis"] for b in out)
    print(f"{len(out)} {args.split} cases in {args.out} ({scored} scored for diagnosis); left out: {dict(skipped) or 'none'}")
    return 0


class Sampler:
    """Peak GPU memory of the VLM's card (`nvidia-smi --id`, all cards when `gpu` is None) and
    vLLM KV-cache usage (`/metrics`) while a run lasts."""

    def __init__(self, base_url: str, gpu: str | None = None, every: float = 0.5):
        self.base_url, self.every = base_url.rstrip("/"), every
        self.query = ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"] + ([f"--id={gpu}"] if gpu else [])
        self.gpu_mib, self.kv = 0, 0.0
        self._stop = threading.Event()
        self._t = threading.Thread(target=self._loop, daemon=True)

    def _loop(self):
        while not self._stop.is_set():
            try:
                used = subprocess.run(self.query, capture_output=True, text=True, timeout=5).stdout.split()
                self.gpu_mib = max([self.gpu_mib] + [int(u) for u in used])
            except (OSError, ValueError, subprocess.TimeoutExpired):
                pass
            try:
                with urllib.request.urlopen(f"{self.base_url}/metrics", timeout=5) as r:
                    for line in r.read().decode().splitlines():
                        if line.startswith(("vllm:kv_cache_usage_perc", "vllm:gpu_cache_usage_perc")):
                            self.kv = max(self.kv, float(line.rsplit(" ", 1)[1]))
            except (OSError, ValueError):
                pass
            self._stop.wait(self.every)

    def __enter__(self):
        self._t.start()
        return self

    def __exit__(self, *exc):
        self._stop.set()
        self._t.join()


def served(base_url: str) -> list[dict]:
    req = urllib.request.Request(f"{base_url.rstrip('/')}/v1/models")
    if os.environ.get("VLM_API_KEY"):
        req.add_header("Authorization", f"Bearer {os.environ['VLM_API_KEY']}")
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())["data"]


def server_up(base_url: str) -> bool:
    try:
        with urllib.request.urlopen(f"{base_url.rstrip('/')}/health", timeout=10) as r:
            return r.status == 200
    except OSError:
        return False


def read_rows(path: Path) -> list[dict]:
    """The lines of an `out.jsonl`; a line cut short by a killed run is skipped."""
    rows = []
    for line in path.read_text().splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            continue
    return rows


def finished(d: Path, n_cases: int) -> bool:
    """A run already done in full: it exited 0 and wrote a line per case."""
    try:
        meta = json.loads((d / "meta.json").read_text())
        return meta.get("exit") == 0 and len(read_rows(d / "out.jsonl")) == n_cases
    except (OSError, ValueError):
        return False


_child: subprocess.Popen | None = None


def _stop_child(signum, _frame):
    # The test runs in its own process group, so a signal to this script (the night script's
    # `timeout`, Ctrl-C) doesn't reach it: stop it here, then leave.
    if _child is not None and _child.poll() is None:
        os.killpg(_child.pid, signal.SIGKILL)
    sys.exit(128 + signum)


def run(args) -> int:
    global _child
    signal.signal(signal.SIGTERM, _stop_child)
    signal.signal(signal.SIGINT, _stop_child)
    models = served(args.base_url)
    n_cases = sum(1 for line in args.cases.read_text().splitlines() if line.strip())
    # A release build: the image preparation inside the timed `analyse` is unoptimised in the dev
    # profile (review of #86, point 6).
    build = subprocess.run(["cargo", "test", "--release", "-p", "worker", "--no-run"], cwd=ROOT / "backend")
    if build.returncode:
        print("the worker's tests don't build", file=sys.stderr)
        return build.returncode
    out_dir = RUNS / args.label
    failed = 0
    for mp in args.max_pixels:
        for conc in args.concurrency:
            d = out_dir / f"px{mp}-c{conc}"
            if not args.again and finished(d, n_cases):
                print(f"{args.label}: max_pixels {mp}, {conc} at once: done already", file=sys.stderr, flush=True)
                continue
            if not server_up(args.base_url):
                print(f"{args.label}: the server at {args.base_url} is down; stopping", file=sys.stderr, flush=True)
                return 3
            d.mkdir(parents=True, exist_ok=True)
            # The run keeps its own copy of the cases, so a later `cases` can't change what it is scored on.
            cases = d / "cases.jsonl"
            cases.write_text(args.cases.read_text())
            env = {**os.environ, "VLM_BENCH_BASE_URL": args.base_url, "VLM_BENCH_CASES": str(cases.resolve()),
                   "VLM_BENCH_OUT": str((d / "out.jsonl").resolve()), "VLM_BENCH_MAX_PIXELS": str(mp),
                   "VLM_BENCH_CONCURRENCY": str(conc)}
            print(f"{args.label}: max_pixels {mp}, {conc} at once", file=sys.stderr, flush=True)
            started = time.time()
            with Sampler(args.base_url, args.gpu) as s:
                _child = subprocess.Popen(["cargo", "test", "--release", "-p", "worker", "bench_cases", "--", "--ignored", "--nocapture"],
                                          cwd=ROOT / "backend", env=env, start_new_session=True)
                try:
                    code = _child.wait(timeout=args.timeout)
                except subprocess.TimeoutExpired:
                    os.killpg(_child.pid, signal.SIGKILL)
                    _child.wait()
                    code = "timeout"
                _child = None
            # Cases fail fast once the server is gone and the test still exits 0, so a run that ends
            # with the server down isn't finished: a resume runs it again (review of #86, point 4).
            down = not server_up(args.base_url)
            if down:
                code = "server_down"
            meta = {"label": args.label, "max_pixels": mp, "concurrency": conc, "models": models,
                    "seconds": round(time.time() - started, 1), "peak_gpu_mib": s.gpu_mib, "peak_kv_cache": s.kv,
                    "exit": code, "cases": str(cases.resolve()), "gpu": args.gpu, "profile": "release"}
            (d / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
            if code != 0:
                failed += 1
                print(f"  bench_cases exited {code}; see {d}", file=sys.stderr, flush=True)
            if down:
                print(f"{args.label}: the server at {args.base_url} went down; stopping", file=sys.stderr, flush=True)
                return 3
    return 1 if failed else 0


def pct(values: list[float], q: float) -> float | None:
    if not values:
        return None
    v = sorted(values)
    return v[min(len(v) - 1, round(q * (len(v) - 1)))]


def score_run(rows: list[dict], cases_by_id: dict[str, dict]) -> dict:
    jobs = len(rows)
    ok = [r for r in rows if r["ok"]]
    retried = sum(len(r["calls"]) - len({c["schema"] for c in r["calls"]}) for r in rows)
    by_kind = defaultdict(list)
    for r in rows:
        for c in r["calls"]:
            by_kind[c["schema"].split("@")[0]].append(c["ms"])
    species = diag_hit = diag_n = healthy_clean = healthy_n = yes_on_healthy = answers_n = unsure = ties = 0
    look_clean = look_n = healthy_with_animals = 0
    by_size = {"under_1mp": [0, 0], "1mp_and_up": [0, 0]}   # diagnosis hits and cases, by the photo's size
    damage, organisms = Counter(), 0
    # Evaluation 5's certainty words: each finding by the confidence its word mapped to. Only the
    # species guess has a truth on these cases; damage and organisms are counted (P11's evidence:
    # these leaves show disease, not animals).
    certainty = {"species_guess": defaultdict(lambda: [0, 0]), "damage": Counter(), "organisms": Counter()}
    # Each indicator's answers on the sure cases whose truth is its problem, and on the other sure cases.
    per_indicator = defaultdict(lambda: {"truth": Counter(), "other": Counter()})
    for r in ok:
        case = cases_by_id[r["case_id"]]
        truth = case["truth"]
        f, a = r["result"]["findings"], r["result"]["indicators"]["answers"]
        species += f["species_guess"].get("species_id") == truth["species_id"]
        g = certainty["species_guess"][conf_key(f["species_guess"].get("confidence"))]
        g[0] += 1
        g[1] += f["species_guess"].get("species_id") == truth["species_id"]
        certainty["damage"].update(conf_key(d.get("confidence")) for d in f["damage"])
        certainty["organisms"].update(conf_key(o.get("confidence")) for o in f["organisms"])
        damage.update(d["pattern"] for d in f["damage"])
        organisms += len(f["organisms"])
        answers_n += len(a)
        unsure += sum(x["answer"] == "unsure" for x in a)
        cand_of = {i["indicator_id"]: c["problem_id"] for c in case["payload"]["candidates"] for i in c["indicators"]}
        share = defaultdict(list)
        for x in a:
            share[cand_of.get(x["indicator_id"])].append(x["answer"] == "yes")
            if case["label_confidence"] != "sure":
                continue   # a probable case counts on neither side, or its true "yes" would read as a false one
            on_truth = cand_of.get(x["indicator_id"]) in truth.get("problems", [])
            per_indicator[x["indicator_id"]]["truth" if on_truth else "other"][x["answer"]] += 1
        means = {k: sum(v) / len(v) for k, v in share.items() if k}
        best = max(means.values(), default=0.0)
        leaders = [k for k, v in means.items() if v == best]
        # No diagnosis when nothing is "yes", or when candidates tie for the top: a model answering "yes"
        # to everything would otherwise always "diagnose" the first candidate (review of #86, point 3).
        top = leaders[0] if best > 0 and len(leaders) == 1 else None
        ties += best > 0 and len(leaders) > 1
        if plain_healthy(truth):
            healthy_n += 1
            healthy_clean += top is None
            yes_on_healthy += sum(x["answer"] == "yes" for x in a)
        elif truth["outcome"] == "healthy":
            healthy_with_animals += 1
        elif truth.get("look_alike"):
            look_n += 1
            look_clean += top is None   # no disease on top, as the scan should end
        elif case["scored_for_diagnosis"] and case["label_confidence"] == "sure":
            diag_n += 1
            diag_hit += top in truth["problems"]
            if case.get("megapixels") is not None:
                b = by_size["under_1mp" if case["megapixels"] < 1.0 else "1mp_and_up"]
                b[0] += top in truth["problems"]
                b[1] += 1
    healthy_answers = sum(len(r["result"]["indicators"]["answers"]) for r in ok
                          if plain_healthy(cases_by_id[r["case_id"]]["truth"]))
    # Every job that ran, failed ones too, so a slow tail that times out still counts (review of #86, point 5).
    job_ms = [r["job_ms"] for r in rows if r.get("job_ms")]
    return {
        "jobs": jobs, "finished": len(ok), "retried_calls": retried,
        "job_ms_p50": pct(job_ms, 0.5), "job_ms_p95": pct(job_ms, 0.95),
        "call_ms_p50": {k: pct(v, 0.5) for k, v in sorted(by_kind.items())},
        "species_agree": species / len(ok) if ok else None,
        "diagnosis_top1_sure": diag_hit / diag_n if diag_n else None, "diagnosis_n": diag_n,
        "diagnosis_by_photo_size": {k: {"hits": h, "n": n} for k, (h, n) in by_size.items()},
        "healthy_no_candidate": healthy_clean / healthy_n if healthy_n else None, "healthy_n": healthy_n,
        "yes_on_healthy": yes_on_healthy / healthy_answers if healthy_answers else None,
        "healthy_with_animals_left_out": healthy_with_animals,
        "look_alike_no_candidate": look_clean / look_n if look_n else None, "look_alike_n": look_n,
        "unsure_share": unsure / answers_n if answers_n else None, "top_ties": ties,
        "damage_findings": dict(damage), "organisms_reported": organisms, "reasons": reason_lengths(rows),
        "indicators": {k: {side: dict(c) for side, c in v.items()} for k, v in sorted(per_indicator.items())},
        "certainty": {"species_guess": {k: {"n": n, "right": h} for k, (n, h) in sorted(certainty["species_guess"].items())},
                      "damage": dict(sorted(certainty["damage"].items())),
                      "organisms": dict(sorted(certainty["organisms"].items()))},
    }


def reason_lengths(rows: list[dict]) -> dict:
    """The indicator reasons as decoded: their lengths in characters, and how many end without a full
    stop. On Qwen3.5 a reason with none is one the schema's `maxLength` stopped (#183)."""
    rs = [x.get("reason") or "" for r in rows if r["ok"] for x in r["result"]["indicators"]["answers"]]
    lengths = [len(x) for x in rs]
    return {"n": len(rs), "median": pct(lengths, 0.5), "p90": pct(lengths, 0.9), "max": max(lengths, default=None),
            "no_closing_stop": sum(not x.rstrip().endswith((".", "!", "?")) for x in rs)}


def answer_changes(rows: list[dict], base: list[dict]) -> dict:
    """The indicator answers that differ from another run's on the same case and indicator."""
    def answers(rs):
        return {(r["case_id"], x["indicator_id"]): x["answer"] for r in rs if r["ok"]
                for x in r["result"]["indicators"]["answers"]}
    mine, theirs = answers(rows), answers(base)
    shared = mine.keys() & theirs.keys()
    return {"shared": len(shared), "changed": sum(mine[k] != theirs[k] for k in shared)}


def conf_key(c) -> str:
    """A finding's mapped confidence as a key: the worker's 0.4 / 0.65 / 0.85, or `none`."""
    return "none" if c is None else f"{float(c):.2f}"


def run_dirs(root: Path) -> list[Path]:
    def key(d: Path):
        px, c = d.name[2:].split("-c")
        return int(px), int(c)
    return sorted((d for d in root.glob("px*-c*") if d.is_dir()), key=key)


def with_megapixels(case: dict) -> dict:
    """The case with `megapixels`, its largest photo's size before the worker resizes it, or None
    when a photo isn't on this machine."""
    from PIL import Image

    try:
        mp = max(w * h for w, h in (Image.open(p).size for p in case["photos"].values())) / 1e6
    except OSError:
        mp = None
    return {**case, "megapixels": mp}


def indicator_tables(report: list[dict], pixels: int = 1003520) -> list[str]:
    """Per model, each indicator's answers on the `sure` cases that have its problem and on the other
    `sure` cases (`probable` cases count on neither side), from the run at the worker's default
    `pixels` with one job at once (else its first)."""
    out = []
    for label in dict.fromkeys(r["label"] for r in report):
        runs = [r for r in report if r["label"] == label]
        r = next((x for x in runs if x["max_pixels"] == pixels and x["concurrency"] == 1), runs[0])
        out += ["", f"## Indicators: {label}, {r['max_pixels']:,} pixels, {r['concurrency']} at once", "",
                "`sure` cases only: a `probable` case counts on neither side.", "",
                "| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |", "|---|---|---|"]
        for ind, v in r["indicators"].items():
            t, o = v["truth"], v["other"]
            out.append(f"| `{ind}` | {t.get('yes', 0)} / {t.get('no', 0)} / {t.get('unsure', 0)} | "
                       f"{o.get('yes', 0)} / {o.get('no', 0)} / {o.get('unsure', 0)} |")
    return out


def certainty_tables(report: list[dict], pixels: int = 1003520) -> list[str]:
    """Per model, at the worker's default `pixels` and one job at once: the species guess right per
    mapped certainty, and the damage and organism findings per certainty (evaluation 5, step 5)."""
    out = ["", "## Certainty words", "",
           "Each finding by the confidence its certainty word mapped to (low 0.4, medium 0.65, high 0.85). "
           "Only the species guess has a truth here; these leaves show disease, so every damage or organism "
           "finding is a false sign (P11).", "",
           "| Run | Species guess right, per confidence | Damage findings, per confidence | Organisms, per confidence |",
           "|---|---|---|---|"]
    for label in dict.fromkeys(r["label"] for r in report):
        runs = [r for r in report if r["label"] == label]
        r = next((x for x in runs if x["max_pixels"] == pixels and x["concurrency"] == 1), runs[0])
        c = r.get("certainty")
        if not c:
            continue
        sp = ", ".join(f"{k}: {v['right']}/{v['n']}" for k, v in c["species_guess"].items()) or "none"
        dm = ", ".join(f"{k}: {v}" for k, v in c["damage"].items()) or "none"
        og = ", ".join(f"{k}: {v}" for k, v in c["organisms"].items()) or "none"
        out.append(f"| {label} ({r['max_pixels']:,} px, {r['concurrency']} at once) | {sp} | {dm} | {og} |")
    return out


def cases_label(path: Path) -> dict:
    """Which bench cases a run was scored on, so a report can be scored again: the file (it lives in
    the git-ignored runs folder), its case count and the start of its SHA-256."""
    data = path.read_bytes()
    p = path.resolve()
    return {"file": str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p),
            "cases": sum(1 for line in data.splitlines() if line.strip()), "sha256": hashlib.sha256(data).hexdigest()[:16]}


def score(args) -> int:
    report, first = [], None
    for run_root in args.runs:
        for d in run_dirs(Path(run_root)):
            try:
                meta = json.loads((d / "meta.json").read_text())
                cases_file = args.cases or Path(meta["cases"])
                cases_by_id = {b["case_id"]: with_megapixels(b) for b in map(json.loads, cases_file.read_text().splitlines())}
                rows = [r for r in read_rows(d / "out.jsonl") if r.get("case_id") in cases_by_id]
            except (OSError, ValueError, KeyError) as e:
                print(f"skipping {d}: {e}", file=sys.stderr)
                continue
            s = score_run(rows, cases_by_id)
            s["jobs"] = len(cases_by_id)   # every case, so a run cut short shows the jobs it never ran
            if first is None:
                first = (meta["label"], rows)
            else:
                s["answers_vs_first"] = {"run": first[0], **answer_changes(rows, first[1])}
            model = meta["models"][0].get("root") or meta["models"][0]["id"] if meta["models"] else None
            report.append({**{k: meta[k] for k in ("label", "max_pixels", "concurrency", "peak_gpu_mib", "peak_kv_cache")},
                           "exit": meta.get("exit"), "seconds": meta.get("seconds"), "model": model,
                           "scored_on": cases_label(cases_file), **s})
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"vlm-bench-{args.out}.json").write_text(json.dumps(report, indent=2) + "\n")
    f = lambda v, p=False: "n/a" if v is None else (f"{v:.0%}" if p else f"{v / 1000:.1f} s")
    sizes = lambda r: " / ".join(f"{d['hits']}/{d['n']}" for d in r["diagnosis_by_photo_size"].values())
    on = list({r["scored_on"]["sha256"]: r["scored_on"] for r in report}.values())
    lines = [f"# VLM benchmark: {args.out}", "",
             "Scored on " + "; ".join(f"`{c['file']}` ({c['cases']} case{'s' * (c['cases'] != 1)}, SHA-256 `{c['sha256']}`)" for c in on)
             + (" (`--cases`)" if args.cases else ", each run's own cases") + ".", "",
             "| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Look-alikes kept clean | Unsure | Damage found | Organisms | Peak GPU |",
             "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for r in report:
        dmg = ", ".join(f"{k} {v}" for k, v in r["damage_findings"].items()) or "none"
        run = r["label"] if r["exit"] == 0 else f"{r['label']} (exit {r['exit']})"
        lines.append(f"| {run} | {r['max_pixels']:,} | {r['concurrency']} | {r['finished']}/{r['jobs']} | {r['retried_calls']} | "
                     f"{f(r['job_ms_p50'])} | {f(r['job_ms_p95'])} | {f(r['species_agree'], True)} | "
                     f"{f(r['diagnosis_top1_sure'], True)} (n={r['diagnosis_n']}) | {sizes(r)} | {r['top_ties']} | {f(r['healthy_no_candidate'], True)} (n={r['healthy_n']}) | "
                     f"{f(r['yes_on_healthy'], True)} | {f(r.get('look_alike_no_candidate'), True)} (n={r.get('look_alike_n', 0)}) | {f(r['unsure_share'], True)} | {dmg} | {r['organisms_reported']} | {r['peak_gpu_mib']:,} MiB |")
    lines += ["", DIAGNOSIS_NOTE]
    if left := max((r.get("healthy_with_animals_left_out", 0) for r in report), default=0):
        lines += ["", f"{left} healthy case{'s' * (left != 1)} with an animal on {'them' if left != 1 else 'it'} "
                      f"(`organisms_unboxed`) {'are' if left != 1 else 'is'} left out of the healthy columns until the boxes are drawn."]
    lines += reason_table(report)
    lines += certainty_tables(report)
    lines += indicator_tables(report)
    (REPORTS / f"vlm-bench-{args.out}.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


def reason_table(report: list[dict]) -> list[str]:
    """Each run's indicator reasons, and its answers that differ from the first run's."""
    out = ["", "## Indicator reasons", "",
           "Each reason's length in characters as decoded, and the reasons with no closing full stop: on Qwen3.5, "
           "the sign of one the schema's `maxLength` stopped (#183). Answers changed: on the same case and "
           "indicator as the first run.", "",
           "| Run | Reasons | Median | p90 | Longest | No closing stop | Answers changed from the first run |",
           "|---|---|---|---|---|---|---|"]
    for r in report:
        q, vs = r["reasons"], r.get("answers_vs_first")
        changed = f"{vs['changed']} of {vs['shared']}" if vs else "—"
        out.append(f"| {r['label']} | {q['n']} | {q['median']} | {q['p90']} | {q['max']} | {q['no_closing_stop']} | {changed} |")
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("cases")
    c.add_argument("--split", default="dev", choices=("dev", "test"))
    c.add_argument("--manifest", type=Path, default=inat.MANIFEST)
    c.add_argument("--photos", type=Path, default=inat.PHOTOS)
    c.add_argument("--out", type=Path, default=CASES)
    r = sub.add_parser("run")
    r.add_argument("--label", required=True, help="the candidate, e.g. fp8 or qwen35-awq")
    r.add_argument("--base-url", default="http://127.0.0.1:8000")
    r.add_argument("--cases", type=Path, default=CASES)
    r.add_argument("--max-pixels", type=int, nargs="+", default=list(DEFAULT_PIXELS))
    r.add_argument("--concurrency", type=int, nargs="+", default=[1, 2])
    r.add_argument("--timeout", type=float, default=7200, help="seconds before a run is stopped (default 2 h)")
    r.add_argument("--gpu", default=os.environ.get("VLM_GPU_UUID"),
                   help="the VLM's card for the memory sample, as nvidia-smi --id takes it (default: $VLM_GPU_UUID, else every card)")
    r.add_argument("--again", action="store_true", help="rerun the settings that finished already")
    s = sub.add_parser("score")
    s.add_argument("runs", nargs="+", type=Path)
    s.add_argument("--out", default="vlm-bench")
    s.add_argument("--cases", type=Path, help="score against these bench cases (from `cases`) instead of each run's own: "
                                              "a run's rows outside them are left out, and their truth is these cases'")
    args = ap.parse_args(argv)
    return {"cases": cases, "run": run, "score": score}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
