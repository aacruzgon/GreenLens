#!/usr/bin/env python3
"""J5: send the evaluation's dev cases through the real API, as a user would, and keep what came back.

Each case becomes one scan in one garden made for the run (`j5-<date>` by default): `POST /scans`
with a new plant, its photos uploaded, `POST /scans/{id}/submit`, the event stream followed to
`plan_ready` or `failed` with every guided question answered by the policy below, then
`GET /scans/{id}`. One JSON line per case goes to the run's JSONL (`ml/training/runs/e2e/`,
git-ignored), and `score.py` turns that into the report (evaluation 9).

The client is the smoke scripts' (`backend/tools/smoke`): `smoke.Api`, `s6.Clerk` (Clerk's test
instance mints the session tokens; free test phone numbers are found as s6 finds them) and
`s4.Stream` (the event stream with its seq checks, resumed with Last-Event-ID).

Choices (evaluation 9 states them):

- **Split.** Dev only. The test split was scored once (M18) and is never scored again, so `--split
  test` is refused.
- **Photos.** Every photo of the case, in its manifest kind, `whole_plant` first. A case with no
  `leaf_closeup` also sends its `whole_plant` photo as a `leaf_closeup`, as `vlm_bench` and
  `calibrate` do: most dev cases are one photo of a leaf, and the disease head reads only close-ups
  (`--no-closeup-copy` turns it off). A photo the quality gate refuses (422) is recorded; a case
  whose `whole_plant` is refused can't be submitted and ends `rejected`.
- **Species** (`--species`):
  - `confirm` (default): the plant is made without a species, so BioCLIP and the VLM suggest one,
    and the user answers the confirmation card (`species_suggested`) with the case's true species
    (`POST /plants/{id}/species`), as a gardener who knows what they grow would. A wrong suggestion
    starts a new run (contracts 3), which the harness follows to its plan. The species rows score
    the first suggestion, before the user's answer.
  - `name`: the true species named on the new plant; nothing is suggested.
  - `none`: the card is never answered; the run goes on with the suggested species.
- **Questions** (`--answers`):
  - `labels` (default): an indicator question gets the case's labelled answer when its truth has
    one (`truth.indicators`, keyed by the indicator's full id or its slug), else `unsure`. No dev
    case labels indicators yet, so today every answer is `unsure`: the user doesn't know, and the
    VLM's own answers decide, which is what the margin is tuned on (evaluation 5).
  - `unsure`: always `unsure`.
  - `oracle`: `yes` when the indicator belongs to one of the case's true problems, `no` otherwise.
    An upper bound that leaks the label; the report says so. Never for the numbers of record.
  - A photo request is never answered (the case has no other photo): the scheduler continues the
    scan after 10 minutes, `uncertain` with the question open, as for a user who doesn't send one.
- **Users.** The API allows 30 scans a day per user, so the run signs in as a pool of Clerk test
  users (`j5u<k>+clerk_test@example.com`). User 0 owns the garden and invites the others as
  editors when the one before reaches the limit (`429 budget_exceeded`).
- **Concurrency.** `--concurrency` scans (default 2, the 5070 Ti's `VLM_SLOTS`) are between submit
  and their first question, diagnosis or failure, the part that holds a GPU. A scan waiting on a
  question or on the plan writer doesn't count, up to `--max-open` scans in all.
- **Resuming.** A case with a line in the JSONL is skipped (a line with `outcome: error` is retried
  unless `--keep-errors`). A submitted scan stays in `<out>.state.json` until its case has a
  result, so after a kill or a case's error (a timeout, a broken stream) the retry follows that
  scan again from seq 1, with the uploads it recorded, instead of making a new scan. A failed scan
  ends its case at any run. The state file also keeps
  the garden, so a rerun of one run name stays in one garden.
- **Nothing is deleted.** The garden, plants and scans stay; the state file names the garden.

Usage (from the repository root, or from backend/ as `python3 ../ml/training/eval/e2e.py ...`):

    python3 ml/training/eval/e2e.py fetch --hf-env-file /srv/pdm/env/.env.models   # photos from pdm-models
    python3 ml/training/eval/e2e.py run --base-url https://pdm-cp.tailb3ce07.ts.net \\
        --env-file /srv/pdm/env/.env.api [--cases ID,ID] [--limit N] [--run-name j5-2026-10-25] [--score]
    python3 ml/training/eval/e2e.py run --dry-run --limit 8 --score     # a fake API in-process, no network

Standard library only. Neither a key nor a token is ever printed.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import http.server
import json
import os
import random
import re
import socketserver
import sys
import threading
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
MANIFEST = HERE / "manifest.jsonl"
PHOTOS = HERE / "photos"
RUNS = ROOT / "ml/training/runs/e2e"
SEED = ROOT / "ml/knowledge/seed"
SMOKE = ROOT / "backend/tools/smoke"
HF_REPO = "GreenLens/pdm-models"     # the evaluation photos are under eval/photos/ (private)
HF_PREFIX = "eval/photos/"

KIND_ORDER = ("whole_plant", "leaf_closeup", "leaf_underside", "flower_fruit", "soil_surface")
KIND_LIMIT = {"whole_plant": 1, "leaf_closeup": 2, "leaf_underside": 1, "soil_surface": 1, "flower_fruit": 1}
ANSWER_POLICIES = ("labels", "unsure", "oracle")
SPECIES_POLICIES = ("confirm", "name", "none")
DAILY_SCANS = 30                       # contracts 3, POST /scans
PHONE_BASE = 160                       # Clerk test numbers +1555555 0160..: free of the smoke scripts'
RECORD_VERSION = 1
CTYPES = {".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png", ".webp": "image/webp",
          ".heic": "image/heic"}

_print_lock = threading.Lock()


def info(msg: str = "") -> None:
    with _print_lock:
        print(msg, flush=True)


def now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def parse_ts(text: str | None) -> dt.datetime | None:
    if not text:
        return None
    m = re.match(r"(.*?T\d\d:\d\d:\d\d)(\.\d+)?(Z|[+-]\d\d:\d\d)$", text)
    if not m:
        return None
    frac = (m.group(2) or ".0")[:7]
    zone = "+00:00" if m.group(3) == "Z" else m.group(3)
    return dt.datetime.fromisoformat(m.group(1) + frac + zone)


def secs(a: str | None, b: str | None) -> float | None:
    ta, tb = parse_ts(a), parse_ts(b)
    return None if ta is None or tb is None else round((tb - ta).total_seconds(), 3)


def read_env_file(path: Path) -> dict[str, str]:
    """KEY=value pairs of a file, never printed."""
    out = {}
    if not path or not Path(path).is_file():
        return out
    for line in Path(path).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        out[k.strip().removeprefix("export ").strip()] = v.strip().strip('"').strip("'")
    return out


# --- cases ------------------------------------------------------------------------------------

def load_cases(manifest: Path, split: str, only: list[str] | None = None, limit: int | None = None) -> list[dict]:
    if split != "dev":
        raise SystemExit("e2e runs on the dev split only: the test split was scored once (M18) and is "
                         "never scored again (evaluation 4)")
    rows = [json.loads(line) for line in manifest.read_text().splitlines() if line.strip()]
    cases = [r for r in rows if r.get("split") == split]
    if only:
        by_id = {c["case_id"]: c for c in rows}
        unknown = [i for i in only if i not in by_id]
        if unknown:
            raise SystemExit(f"--cases: not in the manifest: {unknown}")
        wrong = [i for i in only if by_id[i].get("split") != split]
        if wrong:
            raise SystemExit(f"--cases: not dev cases: {wrong}")
        cases = [by_id[i] for i in only]
    return cases[:limit] if limit else cases


def uploads_of(case: dict, closeup_copy: bool = True) -> list[tuple[str, dict]]:
    """(kind, manifest image) to upload, in order, within the kinds' limits (contracts 3)."""
    shots = sorted(case["images"], key=lambda im: KIND_ORDER.index(im["kind"]) if im["kind"] in KIND_ORDER else 99)
    out, used = [], {}
    for im in shots:
        k = im["kind"]
        if used.get(k, 0) >= KIND_LIMIT.get(k, 1):
            continue
        used[k] = used.get(k, 0) + 1
        out.append((k, im))
    whole = next((im for k, im in out if k == "whole_plant"), None)
    if closeup_copy and whole and not used.get("leaf_closeup"):
        out.insert(1, ("leaf_closeup", {**whole, "copy_of": "whole_plant"}))
    return out


def file_sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def photo_state(cases: list[dict], photos: Path) -> tuple[list[dict], list[dict]]:
    """(missing, wrong hash) manifest images."""
    missing, wrong = [], []
    seen = set()
    for c in cases:
        for im in c["images"]:
            if im["file"] in seen:
                continue
            seen.add(im["file"])
            p = photos / im["file"]
            if not p.is_file():
                missing.append(im)
            elif file_sha(p) != im["sha256"]:
                wrong.append(im)
    return missing, wrong


def fetch_photos(images: list[dict], photos: Path, token: str) -> tuple[int, list[str]]:
    """Downloads manifest images from pdm-models' eval/photos/ and checks each SHA-256."""
    got, failed = 0, []
    for im in images:
        url = f"https://huggingface.co/{HF_REPO}/resolve/main/{HF_PREFIX}{urllib.parse.quote(im['file'])}"
        req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}", "User-Agent": "pdm-e2e/1"})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
        except urllib.error.HTTPError as e:
            failed.append(f"{im['file']}: HTTP {e.code}")
            continue
        except (urllib.error.URLError, OSError) as e:
            failed.append(f"{im['file']}: {e}")
            continue
        if hashlib.sha256(data).hexdigest() != im["sha256"]:
            failed.append(f"{im['file']}: SHA-256 differs from the manifest's")
            continue
        dest = photos / im["file"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
        got += 1
    return got, failed


# --- the knowledge base, for the oracle policy -----------------------------------------------------

def problem_indicators(seed: Path = SEED) -> dict[str, set[str]]:
    """problem id -> its indicators' slugs (the part after `<problem>.`)."""
    import yaml  # only the oracle policy needs it
    out = {}
    for f in sorted((seed / "problems").glob("*.yaml")):
        doc = yaml.safe_load(f.read_text()) or {}
        out[doc.get("id", f.stem)] = {i["id"].split(".", 1)[-1] for i in doc.get("indicators") or []}
    return out


def answer_for(policy: str, case: dict, question: dict, kb: dict[str, set[str]] | None) -> str:
    slug = question.get("indicator_id") or ""
    truth = case["truth"]
    if policy == "unsure" or not slug:
        return "unsure"
    if policy == "labels":
        labels = truth.get("indicators") or {}
        for key, value in labels.items():
            if value in ("yes", "no", "unsure") and (key == slug or key.endswith("." + slug)):
                return value
        return "unsure"
    # oracle: the label leaks into the answer (an upper bound)
    true_problems = truth.get("problems") or []
    return "yes" if any(slug in (kb or {}).get(p, set()) for p in true_problems) else "no"


# --- HTTP ----------------------------------------------------------------------------------------

class ApiError(Exception):
    def __init__(self, method, path, status, body):
        self.status, self.body = status, body
        super().__init__(f"{method} {path}: {status}: {json.dumps(body)[:300] if not isinstance(body, str) else body[:300]}")

    @property
    def code(self):
        """The ErrorBody's code (contracts 2: `{error: {code, ...}}`)."""
        return error_field(self.body, "code")


def error_field(body, name):
    if not isinstance(body, dict):
        return None
    inner = body.get("error") if isinstance(body.get("error"), dict) else body
    return inner.get(name)


class LockedClerk:
    """A Clerk token source shared by threads: one mint at a time, retried on a network error."""

    def __init__(self, clerk):
        self.clerk = clerk
        self.lock = threading.Lock()

    def token(self):
        with self.lock:
            for attempt in range(4):
                try:
                    return self.clerk.token()
                except SystemExit:          # smoke.http_json dies on a network error
                    if attempt == 3:
                        raise
                    time.sleep(2 * (attempt + 1))


class User:
    def __init__(self, index: int, email: str, clerk, base: str):
        import smoke  # noqa: PLC0415 (backend/tools/smoke on sys.path)
        self.index, self.email = index, email
        self.clerk = LockedClerk(clerk)
        self.api = smoke.Api(base, self.clerk)
        self.base = base.rstrip("/")
        self.scans = 0
        self.exhausted = False
        self.member = index == 0

    def call(self, method: str, path: str, body=None, expect=(200,), idem=False, raw=None, ctype=None,
             retries: int = 5):
        """One API call; retried on a network error, 429 rate_limited, 502, 503 and 504 with the same
        Idempotency-Key. Any other status outside `expect` raises ApiError."""
        key = str(uuid.uuid4()) if idem else None
        for attempt in range(retries + 1):
            h = {"Authorization": f"Bearer {self.clerk.token()}", "User-Agent": "pdm-e2e/1"}
            data = None
            if key:
                h["Idempotency-Key"] = key
            if raw is not None:
                h["Content-Type"], data = ctype, raw
            elif body is not None:
                h["Content-Type"], data = "application/json", json.dumps(body).encode()
            req = urllib.request.Request(self.base + path, data=data, method=method, headers=h)
            try:
                with urllib.request.urlopen(req, timeout=120) as r:
                    status, text = r.status, r.read()
            except urllib.error.HTTPError as e:
                status, text = e.code, e.read()
            except (urllib.error.URLError, OSError) as e:
                if attempt == retries:
                    raise ApiError(method, path, 0, str(e)) from e
                time.sleep(min(30, 2 ** attempt))
                continue
            try:
                out = json.loads(text) if text else None
            except ValueError:
                out = text.decode("utf-8", "replace")
            if status in expect:
                return out
            code = error_field(out, "code")
            if attempt < retries and (status in (502, 503, 504) or (status == 429 and code != "budget_exceeded")
                                      or code == "request_in_progress"):
                wait = error_field(out, "retry_after_s")
                time.sleep(float(wait or min(30, 2 ** attempt)))
                continue
            raise ApiError(method, path, status, out)
        raise AssertionError("unreachable")

    def upload(self, scan_id: str, kind: str, data: bytes, name: str):
        ctype = CTYPES.get(Path(name).suffix.lower(), "image/jpeg")
        boundary = "e2e" + uuid.uuid4().hex
        body = b"".join([
            f'--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\n{kind}\r\n'.encode(),
            (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\n'
             f"Content-Type: {ctype}\r\n\r\n").encode() + data + b"\r\n",
            f"--{boundary}--\r\n".encode(),
        ])
        return self.call("POST", f"/scans/{scan_id}/images", expect=(201,), idem=True, raw=body,
                         ctype=f"multipart/form-data; boundary={boundary}")


def smoke_imports():
    if str(SMOKE) not in sys.path:
        sys.path.insert(0, str(SMOKE))
    import s4  # noqa: F401,PLC0415
    import s6  # noqa: F401,PLC0415
    import smoke  # noqa: F401,PLC0415
    return s4, s6, smoke


class Users:
    """The run's Clerk test users: user 0 owns the garden; the next is invited when one reaches the
    daily scan limit."""

    def __init__(self, base: str, make_clerk, prefix: str, max_users: int):
        self.base, self.make_clerk, self.prefix, self.max_users = base, make_clerk, prefix, max_users
        self.users: list[User] = []
        self.lock = threading.Lock()
        self.garden = None

    def user(self, k: int) -> User:
        while len(self.users) <= k:
            i = len(self.users)
            email = f"{self.prefix}{i}+clerk_test@example.com"
            self.users.append(User(i, email, self.make_clerk(email, PHONE_BASE + i), self.base))
        return self.users[k]

    def owner(self) -> User:
        return self.user(0)

    def join(self, u: User) -> None:
        if u.member:
            return
        inv = self.owner().call("POST", f"/gardens/{self.garden}/invites", {"role": "editor"},
                                expect=(201,), idem=True)
        u.call("POST", f"/invites/{inv['code']}/accept")
        u.member = True
        info(f"user {u.index} joined the garden as an editor")

    def current(self) -> User:
        with self.lock:
            for k in range(self.max_users):
                u = self.user(k)
                if not u.exhausted and u.scans < DAILY_SCANS:
                    self.join(u)
                    return u
            raise RuntimeError(f"all {self.max_users} test users reached the daily scan limit")

    def spent(self, u: User) -> None:
        with self.lock:
            if not u.exhausted:
                u.exhausted = True
                info(f"user {u.index} reached the daily scan limit; the next user takes over")


# --- the run --------------------------------------------------------------------------------------

def strip_urls(value):
    if isinstance(value, dict):
        return {k: strip_urls(v) for k, v in value.items() if k not in ("url", "thumbnail_url")}
    if isinstance(value, list):
        return [strip_urls(v) for v in value]
    return value


def compact_candidate(c: dict) -> dict:
    ev = lambda xs: [{k: e.get(k) for k in ("indicator_id", "weight", "answer", "source")} for e in xs or []]  # noqa: E731
    return {"problem_id": c.get("problem_id"), "kind": c.get("kind"), "score": c.get("score"),
            "classifier_weight": c.get("classifier_weight"), "weather_fit": c.get("weather_fit"),
            "matched": ev(c.get("matched")), "contradicted": ev(c.get("contradicted")),
            "unsure": ev(c.get("unsure"))}


def compact_diagnosis(d: dict | None) -> dict | None:
    if not d:
        return None
    return {"id": d.get("id"), "run": d.get("run"), "supersedes": d.get("supersedes"),
            "status": d.get("status"), "low_evidence": d.get("low_evidence"),
            "primary": (d.get("primary") or {}).get("problem_id"),
            "candidates": [compact_candidate(c) for c in d.get("candidates") or []],
            "organisms": [{k: o.get(k) for k in ("organism_id", "name", "role")} for o in d.get("organisms") or []],
            "risks": d.get("risks"), "model_versions": d.get("model_versions"), "created_at": d.get("created_at")}


def compact_observation(o: dict, kinds: dict[str, str]) -> dict:
    region = o.get("region") or {}
    detail = {k: v for k, v in (o.get("detail") or {}).items() if k != "measurement"}
    return {"stage": o.get("stage"), "pipeline_stage": o.get("pipeline_stage"), "run": o.get("run"),
            "label": o.get("label"), "ref": o.get("ref"), "role": o.get("role"), "detail": detail or None,
            "confidence": o.get("confidence"), "source": o.get("source"),
            "model_version": o.get("model_version"),
            "image_kind": kinds.get(region.get("image_id")) if isinstance(region, dict) else None}


def vlm_path(mv: dict | None, observations: list[dict]) -> str:
    """Which worker served the VLM job, as far as the API shows it: the hosted model of the
    fallback worker (`llm_fallback`) or the rig's VLM (`vlm_model`). The API doesn't expose the
    worker id of a job, so two rig VLM workers can't be told apart."""
    mv = mv or {}
    sources = {o.get("source") for o in observations}
    if "llm_fallback" in mv or "llm_fallback" in sources:
        return "fallback"
    if "vlm_model" in mv or "vlm" in sources:
        return "rig_vlm"
    return "none"


def timing(events: list[dict]) -> dict:
    """Seconds from submit, by the events' own `ts`."""
    submit = next((e["ts"] for e in events if e["type"] == "status_changed" and e.get("to") == "queued"), None)
    submit = submit or next((e["ts"] for e in events if e["type"] == "run_started"), None)
    first = lambda pred: next((e["ts"] for e in events if pred(e)), None)  # noqa: E731
    diagnosed = first(lambda e: e["type"] == "status_changed" and e.get("to") == "diagnosed")
    wait = 0.0
    opened = None
    for e in events:
        if e["type"] != "status_changed":
            continue
        if e.get("to") == "awaiting_answer":
            opened = e["ts"]
        elif e.get("from") == "awaiting_answer" and opened:
            wait += secs(opened, e["ts"]) or 0.0
            opened = None
    out = {
        "submitted_at": submit,
        "first_question_s": secs(submit, first(lambda e: e["type"] == "question")),
        "first_diagnosis_s": secs(submit, first(lambda e: e["type"] == "diagnosis")),
        "diagnosed_s": secs(submit, diagnosed),
        "final_diagnosis_s": secs(submit, next((e["ts"] for e in reversed(events) if e["type"] == "diagnosis"), None)),
        "plan_ready_s": secs(submit, first(lambda e: e["type"] == "plan_ready")),
        "failed_s": secs(submit, first(lambda e: e["type"] == "failed")),
        "awaiting_answer_s": round(wait, 3),
    }
    # The time a user waits for the result: to the last diagnosis (a species change's rerun included),
    # without the time the scan waited on the user's answer.
    out["machine_s"] = (round(out["final_diagnosis_s"] - wait, 3) if out["final_diagnosis_s"] is not None else None)
    stages = {}
    for e in events:
        if e["type"] == "stage_completed" and e.get("duration_ms") is not None:
            stages[e["stage"]] = stages.get(e["stage"], 0) + e["duration_ms"]
    out["stage_ms"] = stages
    return out


class Harness:
    def __init__(self, args, users: Users, cases: list[dict]):
        self.args, self.users, self.cases = args, users, cases
        self.out = Path(args.out)
        self.state_path = self.out.with_suffix(".state.json")
        self.write_lock = threading.Lock()
        self.gpu = threading.BoundedSemaphore(args.concurrency)
        self.kb = problem_indicators() if args.answers == "oracle" else None
        self.state = json.loads(self.state_path.read_text()) if self.state_path.is_file() else {}
        self.s4, _, _ = smoke_imports()

    # state and output
    def save_state(self):
        tmp = self.state_path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.state, indent=1) + "\n")
        tmp.replace(self.state_path)

    def done_cases(self) -> set[str]:
        done = set()
        if self.out.is_file():
            for line in self.out.read_text().splitlines():
                if not line.strip():
                    continue
                r = json.loads(line)
                if r.get("outcome") != "error" or self.args.keep_errors:
                    done.add(r["case_id"])
        return done

    def append(self, record: dict) -> None:
        with self.write_lock:
            with self.out.open("a") as f:
                f.write(json.dumps(record, separators=(",", ":")) + "\n")
            # An error keeps the case's scan in flight, so the retry follows that scan again rather than
            # start (and pay for) another; a scan that was never submitted is replaced in _run_case.
            if record.get("outcome") != "error":
                self.state.setdefault("inflight", {}).pop(record["case_id"], None)
            self.save_state()

    def setup(self) -> None:
        a = self.args
        self.out.parent.mkdir(parents=True, exist_ok=True)
        st = self.state
        if st and (st.get("run_name") != a.run_name or (st.get("base_url") != a.base_url and not a.dry_run)):
            raise SystemExit(f"{self.state_path} belongs to run {st.get('run_name')} on {st.get('base_url')}; "
                             f"use another --out")
        st.update({"run_name": a.run_name, "base_url": a.base_url, "manifest": str(a.manifest),
                   "answers": a.answers, "closeup_copy": not a.no_closeup_copy, "species": a.species})
        st.setdefault("started_at", now_iso())
        owner = self.users.owner()
        if not st.get("garden_id"):
            g = owner.call("POST", "/gardens", {"name": a.run_name, "location": {"lat": a.lat, "lon": a.lon}},
                           expect=(201,), idem=True)
            st["garden_id"] = g["id"]
            info(f"garden {g['id']} ({a.run_name}) made for the run")
        else:
            info(f"garden {st['garden_id']} ({a.run_name}) from {self.state_path.name}")
        self.users.garden = st["garden_id"]
        for k in st.get("members", []):
            self.users.user(k).member = True
        self.save_state()

    # one case
    def run_case(self, case: dict) -> dict:
        t0 = time.monotonic()
        rec = {"v": RECORD_VERSION, "case_id": case["case_id"], "split": case["split"], "truth": case["truth"],
               "label_confidence": case.get("label_confidence"), "run_name": self.args.run_name,
               "base_url": self.args.base_url, "garden_id": self.users.garden,
               "policy": {"answers": self.args.answers, "closeup_copy": not self.args.no_closeup_copy,
                          "species": self.args.species},
               "started_at": now_iso(), "uploads": [], "questions": []}
        try:
            self._run_case(case, rec)
        except Exception as e:  # noqa: BLE001  a case's error is its record, not the run's end
            rec["outcome"] = "error"
            rec["error"] = f"{type(e).__name__}: {e}"
            rec["trace"] = traceback.format_exc(limit=4)[-1500:]
        except SystemExit as e:  # smoke's die(): a broken stream invariant
            rec["outcome"] = "error"
            rec["error"] = f"smoke.die: {e}"
        rec["harness_s"] = round(time.monotonic() - t0, 1)
        rec["finished_at"] = now_iso()
        return rec

    def _new_scan(self, case: dict, rec: dict) -> tuple[User, str]:
        a = self.args
        plant = {"name": f"j5 {case['case_id']}"[:80]}
        if a.species == "name" and case["truth"].get("species_id"):
            plant["species_id"] = case["truth"]["species_id"]
        while True:
            u = self.users.current()
            try:
                scan = u.call("POST", "/scans", {"garden_id": self.users.garden, "new_plant": plant},
                              expect=(201,), idem=True)
            except ApiError as e:
                if e.status == 429 and e.code == "budget_exceeded":
                    self.users.spent(u)
                    continue
                raise
            u.scans += 1
            rec["plant_id"] = scan.get("plant_id")
            return u, scan["id"]

    def _run_case(self, case: dict, rec: dict) -> None:
        a = self.args
        inflight = self.state.setdefault("inflight", {}).get(case["case_id"])
        u, sid = None, None
        if inflight:
            u = self.users.user(inflight["user"])
            u.member = True
            sid = inflight["scan_id"]
            try:
                scan = u.call("GET", f"/scans/{sid}")
            except ApiError as e:
                if e.status != 404:
                    raise
                scan = {"status": "gone"}
            rec["plant_id"] = scan.get("plant_id")
            if scan["status"] in ("pending", "gone"):
                sid = None                      # never submitted, or deleted: start again with a new scan
            else:
                rec["resumed"] = True
                rec["uploads"] = inflight.get("uploads", [])
                info(f"{case['case_id']}: following scan {sid} again ({scan['status']})")
        if sid is None:
            u, sid = self._new_scan(case, rec)
            rec["user"] = u.index
            rec["scan_id"] = sid
            for kind, im in uploads_of(case, not a.no_closeup_copy):
                path = Path(a.photos) / im["file"]
                entry = {"kind": kind, "file": im["file"], "copy_of": im.get("copy_of")}
                try:
                    out = u.upload(sid, kind, path.read_bytes(), path.name)
                    entry.update(status=201, quality=(out or {}).get("quality"),
                                 width=(out or {}).get("image", {}).get("width"),
                                 height=(out or {}).get("image", {}).get("height"))
                except ApiError as e:
                    if e.status != 422:
                        raise
                    entry.update(status=422, error=e.code, message=(e.body or {}).get("message")
                                 if isinstance(e.body, dict) else None)
                rec["uploads"].append(entry)
            if not any(x["kind"] == "whole_plant" and x["status"] == 201 for x in rec["uploads"]):
                rec["outcome"] = "rejected"
                rec["status"] = "pending"
                info(f"{case['case_id']}: whole_plant refused by the quality gate; not submitted")
                return
        rec["user"], rec["scan_id"] = u.index, sid
        with self.write_lock:
            self.state["inflight"][case["case_id"]] = {"scan_id": sid, "user": u.index, "uploads": rec["uploads"]}
            members = sorted({x.index for x in self.users.users if x.member})
            self.state["members"] = members
            self.save_state()

        self.gpu.acquire()
        held = [True]

        def release():
            if held[0]:
                held[0] = False
                self.gpu.release()

        try:
            rec["workers_at_submit"] = self.status_snapshot()
            if not rec.get("resumed"):
                u.call("POST", f"/scans/{sid}/submit", expect=(200, 202))
            rec["submitted_wall"] = now_iso()
            stream = self.s4.Stream(SimpleNamespace(api=u.api, args=SimpleNamespace(verbose=a.verbose)), sid)
            answered = set()
            target = [1]            # the run whose plan ends the case: a species change starts another

            def confirm_species(d):
                sp = case["truth"].get("species_id")
                if a.species != "confirm" or not sp or rec.get("species_answer") or not rec.get("plant_id"):
                    return
                working = d.get("working_species_id")
                entry = {"run": d.get("run"), "suggested": working, "answered": sp, "rerun": sp != working}
                # The card is answered while the run goes on. A step that commits in between makes the
                # new run's compare-and-set fail (409 invalid_state): the user taps again a moment later.
                for attempt in range(20):
                    try:
                        u.call("POST", f"/plants/{rec['plant_id']}/species", {"species_id": sp})
                        entry.pop("error", None)
                        break
                    except ApiError as e:
                        entry["error"] = f"{e.status} {e.code}: {error_field(e.body, 'message')}"
                        if e.status != 409:
                            break
                        time.sleep(3)
                entry["attempts"] = attempt + 1
                if entry.get("error"):
                    entry["rerun"] = False
                rec["species_answer"] = entry
                if entry["rerun"]:
                    target[0] = max(target[0], (d.get("run") or 1) + 1)

            def hook(_s, d):
                t = d["type"]
                if t == "run_started":
                    target[0] = max(target[0], d.get("run") or 1)
                # A failed scan ends the case at any run: it doesn't follow a species change (contracts,
                # POST /plants/{id}/species), so an answer that lands after the failure starts no run.
                if t == "failed" or (t in ("question", "diagnosis") and (d.get("run") or 1) >= target[0]):
                    release()
                if t == "species_suggested":
                    confirm_species(d)
                if t != "question":
                    return
                q = d["question"]
                entry = {k: q.get(k) for k in ("id", "run", "kind", "indicator_id", "photo_kind", "candidates",
                                               "text", "asked_at", "continue_at")}
                if q.get("kind") == "indicator" and q["id"] not in answered:
                    answer = answer_for(a.answers, case, q, self.kb)
                    try:
                        u.call("POST", f"/scans/{sid}/answers", {"question_id": q["id"], "option_id": answer},
                               expect=(202,), idem=True)
                        entry["answer"] = answer
                    except ApiError as e:
                        entry["answer_error"] = f"{e.status} {e.code}"
                    answered.add(q["id"])
                elif q.get("kind") != "indicator":
                    entry["answer"] = None      # a photo request: left to the scheduler
                rec["questions"].append(entry)

            stream.hooks.append(hook)
            def ended(s):
                return any(e["type"] == "failed" or (e["type"] in ("plan_ready", "plan_superseded")
                                                    and (e.get("run") or 1) >= target[0])
                           for e in s.events)

            stream.follow(ended, a.case_timeout, "plan_ready")
        finally:
            release()
        scan = u.call("GET", f"/scans/{sid}")
        self.fill(rec, stream.events, scan)

    def status_snapshot(self) -> list[str] | None:
        try:
            with urllib.request.urlopen(self.args.base_url.rstrip("/") + "/status", timeout=10) as r:
                st = json.loads(r.read())
            return sorted(f"{w['worker_id']}:{','.join(w.get('caps', []))}" for w in st.get("workers", [])
                          if w.get("healthy"))
        except (urllib.error.URLError, OSError, ValueError):
            return None

    def fill(self, rec: dict, events: list[dict], scan: dict) -> None:
        kinds = {im["id"]: im["kind"] for im in scan.get("images") or []}
        obs = [compact_observation(o, kinds) for o in scan.get("observations") or []]
        diag = scan.get("diagnosis")
        plant = scan.get("plant") or {}
        suggested = [e for e in events if e["type"] == "species_suggested"]
        selected = [e for e in events if e["type"] == "candidates_selected"]
        rec.update({
            "status": scan.get("status"),
            "outcome": "planned" if scan.get("status") == "planned" else
                       ("failed" if scan.get("status") == "failed" else f"stopped_{scan.get('status')}"),
            "failure": scan.get("failure"),
            "run": scan.get("run"),
            "plant_id": plant.get("id"),
            "events": [[e["seq"], e["type"], e["ts"]] + ([e.get("stage") or e.get("to")] if e.get("stage") or e.get("to") else [])
                       for e in events],
            "timing": timing(events),
            "diagnoses": [compact_diagnosis(e["diagnosis"]) for e in events if e["type"] == "diagnosis"],
            "diagnosis": compact_diagnosis(diag),
            "open_question": {k: (scan.get("open_question") or {}).get(k) for k in ("id", "kind", "indicator_id", "photo_kind")}
                             if scan.get("open_question") else None,
            "candidates_selected": [[{k: c.get(k) for k in ("problem_id", "source")} for c in e["candidates"]] for e in selected],
            "species": {
                "suggested": strip_urls({k: suggested[0].get(k) for k in ("run", "candidates", "working_species_id", "needs_confirmation")})
                             if suggested else None,
                "suggested_last": strip_urls({k: suggested[-1].get(k) for k in ("run", "candidates", "working_species_id", "needs_confirmation")})
                                  if len(suggested) > 1 else None,
                "plant_species": (plant.get("species") or {}).get("id"),
                "species_status": plant.get("species_status"),
                "plant_candidates": strip_urls(plant.get("species_candidates")),
            },
            "observations": obs,
            "vlm_path": vlm_path((diag or {}).get("model_versions"), obs),
            "plan": ({k: (scan.get("plan") or {}).get(k) for k in ("id", "version", "provisional", "llm_model", "prompt_version")}
                     | {"steps": len((scan.get("plan") or {}).get("steps") or [])}) if scan.get("plan") else None,
        })

    def run(self) -> int:
        a = self.args
        self.setup()
        done = self.done_cases()
        todo = [c for c in self.cases if c["case_id"] not in done]
        if not a.dry_run:
            missing, wrong = photo_state(todo, Path(a.photos))
            if missing or wrong:
                bad = {im["file"] for im in missing + wrong}
                skipped = [c["case_id"] for c in todo if any(im["file"] in bad for im in c["images"])]
                msg = (f"{len(missing)} photo(s) missing and {len(wrong)} with another SHA-256 under {a.photos}, "
                       f"in {len(skipped)} case(s); run `e2e.py fetch` first")
                if not a.skip_missing:
                    raise SystemExit(msg + ", or pass --skip-missing to leave those cases out of this run")
                info(msg + "; left out of this run (--skip-missing)")
                todo = [c for c in todo if c["case_id"] not in skipped]
        info(f"{len(self.cases)} cases, {len(done)} already in {self.out.name}, {len(todo)} to run; "
             f"{a.concurrency} at once on the GPU, {a.max_open} open")
        counts = {}
        with ThreadPoolExecutor(max_workers=a.max_open) as pool:
            futures = {pool.submit(self.run_case, c): c for c in todo}
            for i, fut in enumerate(as_completed(futures), 1):
                rec = fut.result()
                self.append(rec)
                counts[rec["outcome"]] = counts.get(rec["outcome"], 0) + 1
                d = rec.get("diagnosis") or {}
                tm = rec.get("timing") or {}
                info(f"[{i}/{len(todo)}] {rec['case_id']}: {rec['outcome']} {d.get('status') or ''} "
                     f"{d.get('primary') or ''} (truth {rec['truth']['outcome']} {','.join(rec['truth'].get('problems') or [])}) "
                     f"diagnosed {tm.get('diagnosed_s')} s, vlm {rec.get('vlm_path')}"
                     + (f"; {rec['error']}" if rec.get("error") else ""))
        self.state["finished_at"] = now_iso()
        self.save_state()
        info(f"done: {counts}; garden {self.users.garden}; results in {self.out}")
        return 0


# --- the fake API (--dry-run) ----------------------------------------------------------------------

class FakeApi:
    """A small in-process stand-in for the API's scan endpoints, for --dry-run: the client, the
    users and invites, the daily limit, the stream with questions, resuming and the JSONL all run
    without the network. Diagnoses are made up from the case's truth, seeded by the case id."""

    def __init__(self, cases: dict[str, dict], daily_limit: int = 3):
        self.cases, self.daily_limit = cases, daily_limit
        self.lock = threading.Lock()
        self.ids = 0
        self.scans: dict[str, dict] = {}
        self.members: dict[str, set[str]] = {}
        self.invites: dict[str, str] = {}
        self.per_user: dict[str, int] = {}

    def new_id(self, prefix):
        with self.lock:
            self.ids += 1
            return f"{prefix}{self.ids:06d}"

    def handle(self, method: str, path: str, user: str, body: dict | None):
        parts = [p for p in path.split("?")[0].split("/") if p]
        if path == "/status":
            return 200, {"workers": [{"worker_id": "fake-perception", "caps": ["perception"], "healthy": True},
                                     {"worker_id": "fake-vlm", "caps": ["vlm"], "healthy": True}]}
        if method == "POST" and parts == ["gardens"]:
            g = self.new_id("garden")
            self.members[g] = {user}
            return 201, {"id": g, "name": body["name"]}
        if len(parts) >= 2 and parts[0] == "gardens":
            self.members.setdefault(parts[1], {user})     # a garden of an earlier dry run's state file
        if method == "POST" and len(parts) == 3 and parts[0] == "gardens" and parts[2] == "invites":
            code = self.new_id("code")
            self.invites[code] = parts[1]
            return 201, {"id": code, "code": code, "role": body["role"]}
        if method == "POST" and parts[:1] == ["invites"]:
            self.members[self.invites[parts[1]]].add(user)
            return 200, {"id": self.invites[parts[1]]}
        if method == "POST" and parts == ["scans"]:
            self.members.setdefault(body["garden_id"], {user})
            if user not in self.members[body["garden_id"]]:
                return 404, {"error": {"code": "not_found", "message": "fake", "retryable": False}}
            if self.per_user.get(user, 0) >= self.daily_limit:
                return 429, {"error": {"code": "budget_exceeded", "message": "fake", "retryable": False}}
            self.per_user[user] = self.per_user.get(user, 0) + 1
            case_id = body["new_plant"]["name"].removeprefix("j5 ")
            sid = self.new_id("scan")
            self.scans[sid] = {"id": sid, "case": self.cases[case_id], "status": "pending", "images": [],
                               "events": [], "answered": threading.Event(), "question": None, "run": 0}
            return 201, {"id": sid, "plant_id": "plant" + sid, "status": "pending"}
        if method == "POST" and len(parts) == 3 and parts[0] == "plants" and parts[2] == "species":
            s = self.scans.get(parts[1].removeprefix("plant"))
            if s is None:
                return 404, {"error": {"code": "not_found", "message": "fake", "retryable": False}}
            s["confirmed"] = body["species_id"]
            return 200, {"id": parts[1], "species": {"id": body["species_id"]}, "species_status": "confirmed"}
        if len(parts) >= 2 and parts[0] == "scans":
            s = self.scans.get(parts[1])
            if s is None:
                return 404, {"error": {"code": "not_found", "message": "fake", "retryable": False}}
            if method == "POST" and parts[2:] == ["images"]:
                iid = self.new_id("img")
                s["images"].append({"id": iid, "kind": body["kind"], "width": 1600, "height": 1200})
                return 201, {"image": s["images"][-1], "quality": {"blur_score": 400.0, "exposure": 0.5, "passed": True}}
            if method == "POST" and parts[2:] == ["submit"]:
                if s["status"] == "pending":
                    s["status"] = "queued"
                    s["run"] = 1
                    threading.Thread(target=self.play, args=(s,), daemon=True).start()
                    return 202, {"id": s["id"], "status": "queued"}
                return 200, {"id": s["id"], "status": s["status"]}
            if method == "POST" and parts[2:] == ["answers"]:
                q = s["question"]
                if not q or q["id"] != body["question_id"] or s["answered"].is_set():
                    return 409, {"error": {"code": "invalid_state", "message": "fake", "retryable": False}}
                s["answer"] = body["option_id"]
                s["answered"].set()
                return 202, {"id": s["id"]}
            if method == "GET" and parts[2:] == []:
                return 200, self.detail(s)
        return 404, {"error": {"code": "not_found", "message": "fake", "retryable": False}}

    def emit(self, s, type_, **extra):
        with self.lock:
            seq = len(s["events"]) + 1
            ts = dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
            s["events"].append({"v": 1, "seq": seq, "run": s["run"], "scan_id": s["id"], "ts": ts, "type": type_, **extra})

    def play(self, s):
        """The run, scripted from the truth: a true tomato problem is found by the head; a question
        is asked on half the cases; non-tomato problems end uncertain with a photo request."""
        c = s["case"]
        t = c["truth"]
        rnd = random.Random(c["case_id"])
        e = lambda *a, **k: self.emit(s, *a, **k)  # noqa: E731
        e("run_started", reason="submit", rerun_stages=[])
        e("status_changed", **{"from": "pending", "to": "queued"})
        time.sleep(0.05)
        e("status_changed", **{"from": "queued", "to": "analysing"})
        for st in ("segmentation", "plant_check", "species", "disease", "image_embedding", "context", "retrieve"):
            e("stage_started", stage=st)
            e("stage_completed", stage=st, duration_ms=rnd.randint(50, 900))
        problems = t.get("problems") or []
        sp = t["species_id"] if rnd.random() < 0.7 else "potato"
        e("candidates_selected", candidates=[{"problem_id": p, "name": p, "kind": "fungal", "source": "classifier"} for p in problems])
        e("species_suggested", candidates=[{"species_id": sp, "name": sp, "score": 0.81, "source": "bioclip"},
                                           {"species_id": t["species_id"], "name": t["species_id"], "score": 0.12, "source": "bioclip"}],
          working_species_id=sp, needs_confirmation=True)
        time.sleep(0.3)                     # the client answers the card
        if s.get("confirmed") and s["confirmed"] != sp:
            s["run"] = 2                    # a species change: a new run (contracts 3)
            e("run_started", reason="species_changed", rerun_stages=["species", "disease", "vlm_findings"])
            e("status_changed", **{"from": "analysing", "to": "analysing"})
        for st in ("vlm_findings", "vlm_indicators", "scoring", "fusion"):
            e("stage_started", stage=st)
            e("stage_completed", stage=st, duration_ms=rnd.randint(200, 4000))
        lead = rnd.choice([0, 1, 2, 3, 4])
        if problems and rnd.random() < 0.5:
            q = {"id": self.new_id("q"), "run": 1, "kind": "indicator", "text": "Rings?", "indicator_id": "concentric_rings_bullseye",
                 "candidates": problems, "asked_at": "", "continue_at": ""}
            s["question"] = q
            e("question", question=q)
            e("status_changed", **{"from": "analysing", "to": "awaiting_answer"})
            s["answered"].wait(5)
            e("question_resolved", question_id=q["id"], by="user")
            e("status_changed", **{"from": "awaiting_answer", "to": "analysing"})
            s["question"] = None
            if s.get("answer") == "yes":
                lead += 2
        if not problems and t["outcome"] == "healthy":
            status, cands = "healthy", []
        else:
            top = problems[0] if problems and rnd.random() < 0.7 else "tomato_septoria_leaf_spot"
            top_score = 2 + lead
            cands = [{"problem_id": top, "name": top, "kind": "fungal", "score": top_score, "classifier_weight": 2,
                      "matched": [{"indicator_id": "x", "weight": 2, "answer": "yes", "source": "vlm"}] if lead else [],
                      "contradicted": [], "unsure": [], "weather_fit": "neutral"},
                     {"problem_id": "tomato_late_blight", "name": "Late blight", "kind": "fungal", "score": 2,
                      "classifier_weight": 0, "matched": [], "contradicted": [], "unsure": [], "weather_fit": "neutral"}]
            status = "confirmed" if lead >= 2 else "uncertain"
        diag = {"id": self.new_id("diag"), "scan_id": s["id"], "run": 1, "status": status, "low_evidence": False,
                "candidates": cands, "model_versions": {"classifier": "fake", "vlm_model": "fake-vlm"},
                **({"primary": cands[0]} if cands else {})}
        s["diagnosis"] = diag
        e("diagnosis", diagnosis=diag)
        e("status_changed", **{"from": "analysing", "to": "diagnosed"})
        e("stage_started", stage="plan")
        e("stage_completed", stage="plan", duration_ms=rnd.randint(1000, 3000))
        e("status_changed", **{"from": "diagnosed", "to": "planned"})
        s["status"] = "planned"
        e("plan_ready", plan_id="plan" + s["id"], provisional=status == "uncertain")

    def detail(self, s):
        sp = next((e for e in s["events"] if e["type"] == "species_suggested"), None)
        return {"id": s["id"], "status": s["status"], "run": s["run"], "images": s["images"],
                "plant": {"id": "plant" + s["id"], "species": {"id": sp["working_species_id"]} if sp else None,
                          "species_status": "suggested", "species_candidates": sp["candidates"] if sp else []},
                "observations": [{"stage": "plant_check", "pipeline_stage": "plant_check", "label": "Plant",
                                  "confidence": 0.97, "source": "segmentation", "model_version": "fake"}],
                "diagnosis": s.get("diagnosis"), "last_event_seq": len(s["events"]),
                "plan": {"id": "plan" + s["id"], "version": 1, "provisional": (s.get("diagnosis") or {}).get("status") == "uncertain",
                         "steps": [{}], "llm_model": "fake", "prompt_version": "fake"}}


def serve_fake(fake: FakeApi) -> tuple[str, socketserver.BaseServer]:
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *a):
            pass

        def _user(self):
            return (self.headers.get("Authorization") or "").removeprefix("Bearer ")

        def _send(self, status, body):
            data = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def _body(self):
            n = int(self.headers.get("Content-Length") or 0)
            raw = self.rfile.read(n) if n else b""
            ctype = self.headers.get("Content-Type") or ""
            if ctype.startswith("multipart/"):
                m = re.search(rb'name="kind"\r\n\r\n([a-z_]+)\r\n', raw)
                return {"kind": m.group(1).decode() if m else "whole_plant"}
            return json.loads(raw) if raw else None

        def do_POST(self):
            self._send(*fake.handle("POST", self.path, self._user(), self._body()))

        def do_GET(self):
            if self.path.endswith("/events"):
                return self.stream()
            self._send(*fake.handle("GET", self.path, self._user(), None))

        def stream(self):
            sid = self.path.split("/")[2]
            s = fake.scans.get(sid)
            if s is None:
                return self._send(404, {"code": "not_found"})
            last = int(self.headers.get("Last-Event-ID") or 0)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            deadline = time.monotonic() + 60
            try:
                while time.monotonic() < deadline:
                    evs = s["events"][last:]
                    for ev in evs:
                        self.wfile.write(f"id: {ev['seq']}\nevent: {ev['type']}\ndata: {json.dumps(ev)}\n\n".encode())
                        last = ev["seq"]
                    self.wfile.flush()
                    if s["status"] in ("planned", "failed") and last == len(s["events"]):
                        break
                    time.sleep(0.05)
            except (BrokenPipeError, ConnectionResetError):
                pass
            self.close_connection = True

    class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
        daemon_threads = True

    srv = Server(("127.0.0.1", 0), Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return f"http://127.0.0.1:{srv.server_address[1]}", srv


class FakeClerk:
    def __init__(self, email):
        self.email = email

    def token(self):
        return self.email


# --- CLI -----------------------------------------------------------------------------------------

def cmd_fetch(args) -> int:
    cases = load_cases(args.manifest, "dev", args.cases, args.limit)
    missing, wrong = photo_state(cases, args.photos)
    info(f"{len(cases)} dev cases: {len(missing)} photo(s) missing, {len(wrong)} with another SHA-256 under {args.photos}")
    todo = missing + wrong
    if not todo:
        return 0
    token = os.environ.get("HF_TOKEN") or read_env_file(args.hf_env_file).get("HF_TOKEN")
    if not token:
        raise SystemExit(f"HF_TOKEN not in the environment or {args.hf_env_file}")
    got, failed = fetch_photos(todo, args.photos, token)
    info(f"fetched {got} from {HF_REPO}/{HF_PREFIX}; {len(failed)} not there or not matching")
    for f in failed[:20]:
        info(f"  {f}")
    if len(failed) > 20:
        info(f"  ... and {len(failed) - 20} more")
    return 0 if not failed else 3


def cmd_run(args) -> int:
    if args.concurrency < 1 or args.max_open < args.concurrency:
        raise SystemExit("--concurrency must be 1 or more, and --max-open at least --concurrency")
    if args.answers == "oracle":
        info("--answers oracle: the label leaks into every answer; an upper bound, not a number of record")
    cases = load_cases(args.manifest, "dev", args.cases, args.limit)
    if args.dry_run:
        import tempfile
        tmp = Path(tempfile.mkdtemp(prefix="e2e-dry-"))
        for c in cases:
            for im in c["images"]:
                p = tmp / im["file"]
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(b"dry-run")
        args.photos = tmp
        fake = FakeApi({c["case_id"]: c for c in cases}, daily_limit=args.fake_daily_limit)
        args.base_url, _srv = serve_fake(fake)
        args.run_name = args.run_name or "j5-dry-run"
        if not args.out:
            args.out = str(tmp / f"{args.run_name}.jsonl")
        make_clerk = lambda email, _suffix: FakeClerk(email)  # noqa: E731
        info(f"dry run: fake API at {args.base_url}, photos are placeholders in {tmp}")
        smoke_imports()
    else:
        _, s6, smoke = smoke_imports()
        env_file = Path(args.env_file)
        secret = os.environ.get("CLERK_SECRET_KEY") or read_env_file(env_file).get("CLERK_SECRET_KEY")
        if not secret:
            raise SystemExit(f"CLERK_SECRET_KEY not in the environment or {env_file}")
        if not secret.startswith("sk_test_"):
            raise SystemExit("CLERK_SECRET_KEY must be a test instance's (sk_test_)")
        make_clerk = lambda email, suffix: s6.Clerk(secret, email, suffix)  # noqa: E731
        try:
            with urllib.request.urlopen(args.base_url.rstrip("/") + "/healthz", timeout=10) as r:
                ok = r.status == 200
        except (urllib.error.URLError, OSError):
            ok = False
        if not ok:
            raise SystemExit(f"{args.base_url}/healthz did not answer 200")
    args.run_name = args.run_name or f"j5-{dt.date.today().isoformat()}"
    args.out = args.out or str(RUNS / f"{args.run_name}.jsonl")
    users = Users(args.base_url, make_clerk, args.user_prefix, args.max_users)
    rc = Harness(args, users, cases).run()
    if args.score:
        from importlib import import_module
        sys.path.insert(0, str(ROOT))
        score = import_module("ml.training.eval.score")
        score_args = [args.out, "--manifest", str(args.manifest)]
        if args.dry_run:
            score_args += ["--reports", str(Path(args.out).parent), "--name", f"e2e-{args.run_name}"]
        rc = score.main(score_args) or rc
    return rc


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="J5: the dev cases through the real API (evaluation 9).")
    sub = ap.add_subparsers(dest="cmd", required=True)

    f = sub.add_parser("fetch", help="download missing case photos from pdm-models (HF_TOKEN)")
    r = sub.add_parser("run", help="run the dev cases through the API; one JSON line per case")
    for p in (f, r):
        p.add_argument("--manifest", type=Path, default=MANIFEST)
        p.add_argument("--photos", type=Path, default=PHOTOS, help="the evaluation photos (git-ignored)")
        p.add_argument("--cases", type=lambda s: [x.strip() for x in s.split(",") if x.strip()],
                       help="comma-separated case ids (dev only); default every dev case")
        p.add_argument("--limit", type=int, help="the first N cases only")
    f.add_argument("--hf-env-file", type=Path, default=Path("/srv/pdm/env/.env.models"),
                   help="holds HF_TOKEN, if the environment doesn't")

    r.add_argument("--base-url", default="https://pdm-cp.tailb3ce07.ts.net")
    r.add_argument("--env-file", default="/srv/pdm/env/.env.api",
                   help="holds CLERK_SECRET_KEY (sk_test_) of the Clerk instance the API trusts")
    r.add_argument("--run-name", help="the garden's name and the JSONL's stem; default j5-<today>")
    r.add_argument("--out", help="the JSONL; default ml/training/runs/e2e/<run-name>.jsonl")
    r.add_argument("--concurrency", type=int, default=2,
                   help="scans between submit and their first question or diagnosis (the GPU part); "
                        "default 2, the 5070 Ti's VLM_SLOTS")
    r.add_argument("--max-open", type=int, default=12,
                   help="scans open at once, waiting on a question or the plan writer included")
    r.add_argument("--answers", choices=ANSWER_POLICIES, default="labels",
                   help="labels: the case's indicator labels, else unsure (default); unsure; "
                        "oracle: yes for the true problems' indicators (an upper bound)")
    r.add_argument("--species", choices=SPECIES_POLICIES, default="confirm",
                   help="confirm: answer the species card with the true species (default); name: name it on "
                        "the new plant; none: never answer the card")
    r.add_argument("--no-closeup-copy", action="store_true",
                   help="don't send a case's whole_plant photo again as a leaf_closeup when it has none")
    r.add_argument("--case-timeout", type=int, default=1800,
                   help="seconds from submit to plan_ready (a photo request waits 10 minutes)")
    r.add_argument("--keep-errors", action="store_true", help="don't retry cases whose line is an error")
    r.add_argument("--skip-missing", action="store_true", help="leave out cases whose photos are missing")
    r.add_argument("--user-prefix", default="j5u", help="test users <prefix><k>+clerk_test@example.com")
    r.add_argument("--max-users", type=int, default=20,
                   help="test users in the pool (30 scans a day each; 209 cases need 7, more if they scanned today)")
    r.add_argument("--lat", type=float, default=43.65)
    r.add_argument("--lon", type=float, default=-79.38)
    r.add_argument("--score", action="store_true", help="run score.py on the JSONL afterwards")
    r.add_argument("--dry-run", action="store_true", help="a fake API in-process; no network, no photos")
    r.add_argument("--fake-daily-limit", type=int, default=DAILY_SCANS,
                   help="--dry-run: scans per fake user (default the API's 30; lower it to exercise the rotation)")
    r.add_argument("--verbose", action="store_true", help="print every event")
    args = ap.parse_args(argv)
    return cmd_fetch(args) if args.cmd == "fetch" else cmd_run(args)


if __name__ == "__main__":
    sys.exit(main())
