"""The day-2 VLM benchmark (dev-roadmap J2; evaluation 8): the worker's own calls on the
evaluation cases, per candidate model, image size and number of jobs in flight.

    python -m ml.training.eval.vlm_bench cases [--split dev] [--out PATH]
    python -m ml.training.eval.vlm_bench run --label fp8 [--max-pixels 602112 1003520 ...] [--concurrency 1 2]
    python -m ml.training.eval.vlm_bench score ml/training/runs/vlm-bench/fp8 ml/training/runs/vlm-bench/qwen35 [--out NAME]

`cases` turns the manifest's cases into `vlm` job payloads. The candidates, their indicator
questions and the species and organism options are the worker's own fixtures
(`backend/bins/worker/src/vlm/tests/fixtures/`, tomato and corn): the knowledge base the API
would draw them from isn't loaded yet. So a case of another species is left out, and a
diagnosed case whose problem isn't one of its fixture's three candidates is kept for validity,
timing and false signs, but not scored for diagnosis.

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
  top candidate (none when every share is 0) is compared with the truth, on `sure` cases;
- false signs: on healthy cases, the share answered "yes"; on every case, damage findings and
  organisms, which these leaf photos don't show (a disease spot called `leaf_mines` is the known
  error, #45);
- unsure: the share of answers that are `unsure`.
The cases and the raw runs stay in the git-ignored `ml/training/runs/vlm-bench/`; the report is
`ml/training/eval/reports/vlm-bench-<out>.md` and `.json`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import statistics
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
    payload = {k: fx[k] for k in ("working_species", "species_options", "organism_options", "candidates")}
    payload["findings_images"] = images
    payload["indicator_images"] = indicator or images[:1]
    candidates = [cand["problem_id"] for cand in fx["candidates"]]
    truth = c["truth"]
    return {
        "case_id": c["case_id"], "payload": payload, "photos": paths,
        "truth": {"outcome": truth["outcome"], "species_id": sp, "problems": truth.get("problems", [])},
        "label_confidence": c.get("label_confidence"), "split": c.get("split"),
        "scored_for_diagnosis": truth["outcome"] == "healthy" or bool(set(truth.get("problems", [])) & set(candidates)),
    }


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
    """Peak GPU memory (`nvidia-smi`) and vLLM KV-cache usage (`/metrics`) while a run lasts."""

    def __init__(self, base_url: str, every: float = 0.5):
        self.base_url, self.every = base_url.rstrip("/"), every
        self.gpu_mib, self.kv = 0, 0.0
        self._stop = threading.Event()
        self._t = threading.Thread(target=self._loop, daemon=True)

    def _loop(self):
        while not self._stop.is_set():
            try:
                used = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
                                      capture_output=True, text=True, timeout=5).stdout.split()
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


def run(args) -> int:
    models = served(args.base_url)
    out_dir = RUNS / args.label
    for mp in args.max_pixels:
        for conc in args.concurrency:
            d = out_dir / f"px{mp}-c{conc}"
            d.mkdir(parents=True, exist_ok=True)
            env = {**os.environ, "VLM_BENCH_BASE_URL": args.base_url, "VLM_BENCH_CASES": str(args.cases.resolve()),
                   "VLM_BENCH_OUT": str((d / "out.jsonl").resolve()), "VLM_BENCH_MAX_PIXELS": str(mp),
                   "VLM_BENCH_CONCURRENCY": str(conc)}
            print(f"{args.label}: max_pixels {mp}, {conc} at once", file=sys.stderr, flush=True)
            started = time.time()
            with Sampler(args.base_url) as s:
                p = subprocess.run(["cargo", "test", "-p", "worker", "bench_cases", "--", "--ignored", "--nocapture"],
                                   cwd=ROOT / "backend", env=env)
            meta = {"label": args.label, "max_pixels": mp, "concurrency": conc, "models": models,
                    "seconds": round(time.time() - started, 1), "peak_gpu_mib": s.gpu_mib, "peak_kv_cache": s.kv,
                    "exit": p.returncode, "cases": str(args.cases)}
            (d / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
            if p.returncode:
                print(f"  bench_cases exited {p.returncode}; see {d}", file=sys.stderr)
    return 0


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
    species = diag_hit = diag_n = healthy_clean = healthy_n = yes_on_healthy = answers_n = unsure = 0
    damage, organisms = Counter(), 0
    for r in ok:
        case = cases_by_id[r["case_id"]]
        truth = case["truth"]
        f, a = r["result"]["findings"], r["result"]["indicators"]["answers"]
        species += f["species_guess"].get("species_id") == truth["species_id"]
        damage.update(d["pattern"] for d in f["damage"])
        organisms += len(f["organisms"])
        answers_n += len(a)
        unsure += sum(x["answer"] == "unsure" for x in a)
        cand_of = {i["indicator_id"]: c["problem_id"] for c in case["payload"]["candidates"] for i in c["indicators"]}
        share = defaultdict(list)
        for x in a:
            share[cand_of.get(x["indicator_id"])].append(x["answer"] == "yes")
        means = {k: sum(v) / len(v) for k, v in share.items() if k}
        top = max(means, key=means.get) if means and max(means.values()) > 0 else None
        if truth["outcome"] == "healthy":
            healthy_n += 1
            healthy_clean += top is None
            yes_on_healthy += sum(x["answer"] == "yes" for x in a)
        elif case["scored_for_diagnosis"] and case["label_confidence"] == "sure":
            diag_n += 1
            diag_hit += top in truth["problems"]
    healthy_answers = sum(len(r["result"]["indicators"]["answers"]) for r in ok
                          if cases_by_id[r["case_id"]]["truth"]["outcome"] == "healthy")
    job_ms = [r["job_ms"] for r in ok]
    return {
        "jobs": jobs, "finished": len(ok), "retried_calls": retried,
        "job_ms_p50": pct(job_ms, 0.5), "job_ms_p95": pct(job_ms, 0.95),
        "call_ms_p50": {k: pct(v, 0.5) for k, v in sorted(by_kind.items())},
        "species_agree": species / len(ok) if ok else None,
        "diagnosis_top1_sure": diag_hit / diag_n if diag_n else None, "diagnosis_n": diag_n,
        "healthy_no_candidate": healthy_clean / healthy_n if healthy_n else None, "healthy_n": healthy_n,
        "yes_on_healthy": yes_on_healthy / healthy_answers if healthy_answers else None,
        "unsure_share": unsure / answers_n if answers_n else None,
        "damage_findings": dict(damage), "organisms_reported": organisms,
    }


def score(args) -> int:
    report = []
    for run_root in args.runs:
        for d in sorted(Path(run_root).glob("px*-c*")):
            meta = json.loads((d / "meta.json").read_text())
            cases_by_id = {b["case_id"]: b for b in map(json.loads, Path(meta["cases"]).read_text().splitlines())}
            rows = [json.loads(line) for line in (d / "out.jsonl").read_text().splitlines() if line.strip()]
            report.append({**{k: meta[k] for k in ("label", "max_pixels", "concurrency", "peak_gpu_mib", "peak_kv_cache")},
                           "model": meta["models"][0]["id"] if meta["models"] else None, **score_run(rows, cases_by_id)})
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / f"vlm-bench-{args.out}.json").write_text(json.dumps(report, indent=2) + "\n")
    f = lambda v, p=False: "n/a" if v is None else (f"{v:.0%}" if p else f"{v / 1000:.1f} s")
    lines = [f"# VLM benchmark: {args.out}", "",
             "| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |",
             "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for r in report:
        dmg = ", ".join(f"{k} {v}" for k, v in r["damage_findings"].items()) or "none"
        lines.append(f"| {r['label']} | {r['max_pixels']:,} | {r['concurrency']} | {r['finished']}/{r['jobs']} | {r['retried_calls']} | "
                     f"{f(r['job_ms_p50'])} | {f(r['job_ms_p95'])} | {f(r['species_agree'], True)} | "
                     f"{f(r['diagnosis_top1_sure'], True)} (n={r['diagnosis_n']}) | {f(r['healthy_no_candidate'], True)} (n={r['healthy_n']}) | "
                     f"{f(r['yes_on_healthy'], True)} | {f(r['unsure_share'], True)} | {dmg} | {r['organisms_reported']} | {r['peak_gpu_mib']:,} MiB |")
    (REPORTS / f"vlm-bench-{args.out}.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return 0


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
    s = sub.add_parser("score")
    s.add_argument("runs", nargs="+", type=Path)
    s.add_argument("--out", default="vlm-bench")
    args = ap.parse_args(argv)
    return {"cases": cases, "run": run, "score": score}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
