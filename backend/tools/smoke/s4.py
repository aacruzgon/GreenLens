#!/usr/bin/env python3
"""Live checks for backend slice S4 (the diagnosis loop), in s5.py's style.

Signs in the way smoke.py does (a Clerk test instance's Backend API mints a session token), as
--email (default s4+clerk_test@example.com), and makes one `s4 …` garden with a tomato per scan.
Each scan uploads --photo as `whole_plant` and --leaf-photo as `leaf_closeup`, so the disease head
runs (it reads only leaf close-ups and undersides).

Checks, in this order (--only picks some); backend roadmap 8's exit checks:

  scan   one scan to `plan_ready`, answering indicator questions from --answers: `planned`, a
         diagnosis with a primary (--expect-problem), indicator evidence, a plan whose every
         step cites a source, the safety note on chemical steps, no amount in the plan's text,
         a model version on every result. Prints each candidate's score
  kill   a second scan: right after --kill-after's event, --api-kill-cmd, then --api-start-cmd
         (detached) and GET /healthz; the stream resumes with Last-Event-ID. The sweep must
         re-run the claimed step once it is stale (2 minutes; 5 for `plan`), and the scan must
         reach `planned` with seq 1.. unbroken, one `diagnosis`, one `plan_ready`, plan version 1
  late   a third scan whose question is left unanswered: the sweep continues it after 10
         minutes to a provisional plan. Then --late-answer: `question_resolved`, a diagnosis
         that supersedes the old one, `plan_superseded` (late_answer), plan version 2, and the
         old plan's open tasks dismissed. --late-scan-id reuses a scan already continued

The late scan is submitted first, before the other checks, so its 10 minutes overlap them; its
question is never answered until the late check runs. --out FILE writes every event of each
scan's stream and its final GET /scans/{id}, with the model versions it shows (vision J4).

usage (from backend/):
    python3 tools/smoke/s4.py --photo PLANT.jpg --leaf-photo LEAF.jpg
        [--base-url http://localhost:8080] [--env-file ../infra/dev/.env]
        [--email s4a+clerk_test@example.com --phone-suffix 104]
        [--answers concentric_rings_bullseye=yes,*=unsure] [--expect-problem tomato_early_blight]
        [--only scan,kill,late] [--kill-after diagnosis]
        [--api-kill-cmd CMD --api-start-cmd CMD] [--late-answer yes] [--late-scan-id ID]
        [--out s4-fixture.json] [--verbose]

Exit code 0 when no check failed. Neither a key nor a token is ever printed.
"""
import argparse
import http.client
import json
import re
import subprocess
import sys
import time
import urllib.parse
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import smoke  # noqa: E402  (Clerk, Api, Follower, sse_frames, http_json, load_env, die)
import s5  # noqa: E402  (run_check, call, upload_photos, the safety note and the amount pattern)
from s5 import Fail, Skip, info, need  # noqa: E402

CHECKS = ("scan", "kill", "late")
# --kill-after: the event the kill comes right after -> (the step holding the claim, its stage).
# Each step's progress events go out as it works, its outcome events only after its commit
# (backend flows 3), so these are the points where a claim is held and not yet committed.
KILL_POINTS = {
    "stage_started:context": ("retrieve", "context"),    # retrieve's first event (after measurement)
    "stage_started:retrieve": ("retrieve", "retrieve"),
    "stage_started:scoring": ("score", "scoring"),
    "stage_started:fusion": ("score", "scoring"),
    "diagnosis": ("plan", "plan"),                       # score committed, the claim set to `plan`
    "stage_started:plan": ("plan", "plan"),
}
# The sweep re-claims a step whose stamp is older than this (Pipeline::sweep, contracts 3); it
# ticks every 30 s, the first tick 10 s after start.
STALE_S = {"retrieve": 120, "score": 120, "plan": 300}
SWEEP_TICK_S = 30
SLACK_S = 30            # between a claim's stamp and the event the kill came after
CONTINUE_AFTER_S = 600  # CONTINUE_AFTER, domain::rules::questions
ANSWERS = ("yes", "no", "unsure")
# A number followed by a measure: the dose check's first rule (contracts 7) for digits, which no
# exemption covers. Number words and rates are left to s5.AMOUNT, which only warns: it is
# broader than the contract ("a cup" without "of" passes the server's check).
DOSE = re.compile(r"\b\d+(?:[.,/]\d+)?\s*(?:ml|millilit(?:re|er)s?|l|lit(?:re|er)s?|fl\s*oz|oz|"
                  r"ounces?|pints?|quarts?|gallons?|gal|g|grams?|kg|mg|lbs?|pounds?|tbsp|"
                  r"tablespoons?|tsp|teaspoons?|ppm)\b", re.I)
# The fixed product words of contracts 7's plan validation ("products make a step chemical");
# products.yaml adds more, which the script doesn't know.
PRODUCT = re.compile(r"\b(?:pesticides?|fungicides?|insecticides?|herbicides?|copper|sulfur|"
                     r"sulphur|neem|insecticidal soaps?|horticultural oils?|spinosad|"
                     r"pyrethrins?)\b|\b(?-i:Bt)\b", re.I)


def parse_ts(text):
    """An RFC 3339 timestamp as a UTC datetime (fractions cut to microseconds)."""
    m = re.match(r"(.*?T\d\d:\d\d:\d\d)(\.\d+)?(Z|[+-]\d\d:\d\d)$", text)
    if not m:
        raise Fail(f"not a timestamp: {text!r}")
    frac = (m.group(2) or ".0")[:7]
    zone = "+00:00" if m.group(3) == "Z" else m.group(3)
    return datetime.fromisoformat(m.group(1) + frac + zone).astimezone(timezone.utc)


def secs(a, b):
    """Seconds from event a to event b, by the server's own `ts`."""
    return (parse_ts(b["ts"]) - parse_ts(a["ts"])).total_seconds()


def parse_answers(text):
    rules = {}
    for part in text.split(","):
        key, sep, value = part.strip().partition("=")
        if not sep or not key.strip() or value.strip() not in ANSWERS:
            smoke.die(f"--answers {text!r}: a comma list of <indicator>=yes|no|unsure, *= the default")
        rules[key.strip()] = value.strip()
    rules.setdefault("*", "unsure")
    return rules


def answer_for(rules, slug):
    """The answer for an indicator question. Question.indicator_id is the indicator's own slug,
    without its problem (contracts 3): a key matches it exactly, as `<problem>.<slug>`, or as
    its suffix; otherwise `*`."""
    slug = slug or ""
    for key, value in rules.items():
        if key != "*" and slug and (key == slug or key.endswith("." + slug)):
            return value
    for key, value in rules.items():
        if key != "*" and slug and slug.endswith(key):
            return value
    return rules["*"]


def strip_signatures(value):
    """The fixture without signed URLs' query strings: they are short-lived credentials."""
    if isinstance(value, dict):
        return {k: strip_signatures(v) for k, v in value.items()}
    if isinstance(value, list):
        return [strip_signatures(v) for v in value]
    if isinstance(value, str) and "?" in value and ("sig=" in value or "X-Amz-" in value):
        return value.split("?", 1)[0] + "?<signature removed>"
    return value


def trace(scan):
    """Every model version GET /scans/{id} shows. Not exposed: which VLM call gave an indicator
    answer (IndicatorEvidence has no version; the VLM's is on its observations and in the
    diagnosis's model_versions), and the plan writer's retries."""
    if not scan:
        return None
    plan = scan.get("plan") or {}
    return {
        "observations": [{k: o.get(k) for k in ("id", "run", "stage", "pipeline_stage", "source",
                                                "label", "model_version")}
                         for o in scan.get("observations", [])],
        "diagnosis": (scan.get("diagnosis") or {}).get("model_versions"),
        "plan": {"llm_model": plan.get("llm_model"), "prompt_version": plan.get("prompt_version")},
    }


class Stream(smoke.Follower):
    """smoke.Follower's checks on every event (seq 1.. with no gap or repeat, data.seq == id,
    the SSE name == data.type), every event kept, and hooks run after each one. It follows the
    stream across reconnects (Last-Event-ID) until a condition holds, not to a resting point:
    a scan with an open question never rests."""

    def __init__(self, s4, scan_id):
        args = argparse.Namespace(timeout=0, drop_after=0, kill_cmd=None, restart_cmd=None,
                                  api_restart_cmd=None, verbose=s4.args.verbose,
                                  perception_only=False)
        super().__init__(s4.api, scan_id, args)
        self.events = []        # every event's data, in seq order
        self.hooks = []         # fn(stream, data) after each event
        self.reconnects = 0

    def handle(self, fid, ev, data, first):
        super().handle(fid, ev, data, first)
        d = json.loads(data)
        self.events.append(d)
        for hook in list(self.hooks):
            hook(self, d)

    def of(self, kind, after=0, run=None):
        return [d for d in self.events if d["type"] == kind and d["seq"] > after
                and (run is None or d["run"] == run)]

    def open(self):
        """GET /scans/{id}/events from the last seq; None while api is down or restarting
        (a refused connection, or 502/503 from the ingress)."""
        u = urllib.parse.urlparse(self.api.base)
        cls = http.client.HTTPSConnection if u.scheme == "https" else http.client.HTTPConnection
        conn = cls(u.hostname, u.port, timeout=30)
        h = self.api.headers({"Accept": "text/event-stream", "User-Agent": smoke.USER_AGENT})
        if self.last_seq:
            h["Last-Event-ID"] = str(self.last_seq)
        try:
            conn.request("GET", f"{u.path}/scans/{self.scan_id}/events", headers=h)
            resp = conn.getresponse()
        except (OSError, http.client.HTTPException):
            conn.close()
            return None, None
        if resp.status != 200:
            body = resp.read().decode("utf-8", "replace")[:300]
            conn.close()
            if resp.status in (502, 503, 504):
                return None, None
            raise Fail(f"GET /scans/{self.scan_id}/events (Last-Event-ID {self.last_seq}): "
                       f"{resp.status}: {body}")
        return conn, resp

    def follow(self, until, timeout, what):
        deadline = time.monotonic() + timeout
        while not until(self):
            if time.monotonic() > deadline:
                raise Fail(f"{what} not seen within {timeout} s (last seq {self.last_seq}, status "
                           f"{self.status}, events: {' '.join(self.types[-8:])})")
            conn, resp = self.open()
            if conn is None:
                time.sleep(2)
                continue
            first = self.last_seq > 0
            try:
                for frame in smoke.sse_frames(resp):
                    if frame is not None:
                        self.handle(*frame, first)
                        first = False
                    if until(self) or time.monotonic() > deadline:
                        break
            except (OSError, http.client.HTTPException) as e:
                info(f"  stream interrupted ({type(e).__name__}); resuming after seq {self.last_seq}")
            finally:
                conn.close()
            self.reconnects += 1
            if not until(self):
                time.sleep(1)  # closed at rest, or cut: reconnect with Last-Event-ID


class S4:
    call = s5.S5.call            # Api.call, a wrong status a Fail of the running check
    run_check = s5.S5.run_check  # PASS, FAIL or SKIP with the evidence

    def __init__(self, args):
        self.args = args
        secret = smoke.load_env(args.env_file, "CLERK_SECRET_KEY")
        if not secret:
            smoke.die(f"CLERK_SECRET_KEY not in the environment or {args.env_file}")
        local = args.email.split("@")[0].split("+")[0]
        fields = {"username": f"pdm_{local}"[:30],
                  "phone_number": [f"+1555555{args.phone_suffix:04d}"]}
        self.api = smoke.Api(args.base_url, smoke.Clerk(secret, args.email, fields))
        self.results = []
        self.fixture = {}       # check -> {scan_id, events, scan}
        self.garden = None
        self.late_sid = None

    # --- plumbing ---------------------------------------------------------------------------

    def setup(self):
        a = self.args
        if not self.api.healthy():
            smoke.die(f"{a.base_url}/healthz did not answer 200")
        stamp = time.strftime("%Y-%m-%d %H:%M:%S")
        self.garden = self.api.call("POST", "/gardens", {"name": f"s4 {stamp}", "location": {
            "lat": a.lat, "lon": a.lon}}, expect=(201,), idem=True)["id"]
        info(f"garden {self.garden}")
        if "late" in a.only and not a.late_scan_id:
            self.late_sid = self.new_scan("late")
            self.late_t0 = time.monotonic()
            info(f"late scan {self.late_sid} submitted first; its question stays unanswered")

    def new_scan(self, label):
        a = self.args
        sid = self.api.call("POST", "/scans", {"garden_id": self.garden, "new_plant": {
            "name": f"s4 {label} tomato", "species_id": "tomato"}}, expect=(201,), idem=True)["id"]
        info(f"{label} scan {sid}")
        s5.upload_photos(self.api, sid, a.photo, a.leaf_photo)
        self.api.call("POST", f"/scans/{sid}/submit", expect=(202,))
        return sid

    def stream(self, check, sid):
        st = Stream(self, sid)
        self.fixture[check] = {"scan_id": sid, "events": st.events, "scan": None}
        return st

    def get_scan(self, check, sid):
        scan = self.call("GET", f"/scans/{sid}")
        self.fixture[check]["scan"] = scan
        return scan

    def answering(self, sid):
        """A hook that answers each indicator question from --answers, as the user would in the
        app. A photo request is left to the scheduler (it continues after 10 minutes)."""
        rules = self.args.answers

        def hook(_stream, d):
            if d["type"] != "question":
                return
            q = d["question"]
            if q.get("kind") == "indicator":
                answer = answer_for(rules, q.get("indicator_id"))
                self.call("POST", f"/scans/{sid}/answers",
                          {"question_id": q["id"], "option_id": answer}, expect=(202, 409))
                info(f"  answered {q.get('indicator_id')} {q.get('text')!r}: {answer}")
            else:
                info(f"  photo request {q.get('text')!r} ({q.get('photo_kind')}): not answered, "
                     f"the scheduler continues at {q.get('continue_at')}")
        return hook

    def plan_tasks(self, garden, plan_id):
        """The garden's plan tasks of one plan version, any state (contracts 9: GET /tasks)."""
        out, cursor = [], None
        while True:
            q = {"garden_id": garden, "source": "plan", "limit": 100}
            if cursor:
                q["cursor"] = cursor
            page = self.call("GET", "/tasks?" + urllib.parse.urlencode(q))
            out += [t for t in page["items"] if t.get("plan_id") == plan_id]
            cursor = page.get("next_cursor")
            if not cursor:
                return out

    def wait_healthy(self, ev, t0):
        limit = self.args.health_timeout
        while not self.api.healthy():
            need(time.monotonic() - t0 < limit, f"api not healthy within {limit} s of the kill")
            time.sleep(1)
        ev.append(f"api healthy {time.monotonic() - t0:.0f} s after the kill")

    # --- checks -----------------------------------------------------------------------------

    def check_scan(self, ev):
        a = self.args
        sid = self.new_scan("scan")
        st = self.stream("scan", sid)
        st.hooks.append(self.answering(sid))
        t0 = time.monotonic()
        st.follow(lambda s: s.of("plan_ready") or s.of("failed"), a.timeout, "plan_ready")
        ev.append(f"scan {sid}: {st.last_seq} events in {time.monotonic() - t0:.0f} s, "
                  f"{len(st.of('question'))} question(s)")
        need(not st.of("failed"), f"the scan failed: {(st.of('failed') or [{}])[0].get('failure')}")
        scan = self.get_scan("scan", sid)
        need(scan["status"] == "planned", f"status {scan['status']}, not planned")
        d, plan = scan.get("diagnosis"), scan.get("plan")
        need(d, "GET /scans/{id} has no diagnosis")
        self.print_diagnosis(d, ev)
        primary = d.get("primary")
        need(primary, f"the diagnosis has no primary (status {d['status']})")
        if a.expect_problem:
            need(primary["problem_id"] == a.expect_problem,
                 f"--expect-problem {a.expect_problem}: the primary is {primary['problem_id']}")
        evidence = sum(len(c[k]) for c in d["candidates"] for k in ("matched", "contradicted", "unsure"))
        need(evidence, "no indicator evidence on any candidate (matched, contradicted, unsure)")
        ev.append(f"indicator evidence: {evidence} entries across {len(d['candidates'])} candidates")
        need(plan, "GET /scans/{id} has no plan")
        ready = st.of("plan_ready")[-1]
        need(ready["plan_id"] == plan["id"], f"plan_ready names {ready['plan_id']}, the scan {plan['id']}")
        need(plan["diagnosis_id"] == d["id"], "the plan wasn't written from the current diagnosis")
        need(plan["provisional"] == (d["status"] == "uncertain"),
             f"provisional {plan['provisional']} with a {d['status']} diagnosis (contracts 7)")
        self.check_plan(plan, ev)
        self.check_versions(scan, ev)

    def print_diagnosis(self, d, ev):
        p = d.get("primary") or {}
        ev.append(f"diagnosis {d['id']}: {d['status']}, low_evidence {d['low_evidence']}, primary "
                  f"{p.get('problem_id')} score {p.get('score')}")
        for c in d["candidates"]:
            weight = (f", classifier_weight {c['classifier_weight']}"
                      if "classifier_weight" in c else "")
            ev.append(f"  {c['problem_id']}: score {c['score']}{weight}, weather {c['weather_fit']}, "
                      f"matched {[e['indicator_id'] for e in c['matched']]}, contradicted "
                      f"{[e['indicator_id'] for e in c['contradicted']]}, unsure {len(c['unsure'])}")

    def check_plan(self, plan, ev):
        """Contracts 7's plan validation, as far as the stored plan shows it. Not observable: the
        steps validation dropped (the plan is stored without them, and nothing on the wire says
        so), the dose check's verdicts, and products.yaml's names."""
        steps = plan["steps"]
        sources = {c["source_id"] for c in plan["citations"]}
        ev.append(f"plan {plan['id']} v{plan['version']}, provisional {plan['provisional']}, "
                  f"{len(steps)} steps, {len(plan['citations'])} sources, {len(plan['task_ids'])} "
                  f"tasks, {plan['llm_model']} {plan['prompt_version']}")
        need(steps, "the plan has no step")
        for s in steps:
            ev.append(f"  {s['order']}. [{s['category']}] {s['title']!r}: cites {s['citations']}"
                      + (f", safety note {s['safety_note']!r}" if s.get("safety_note") else ""))
            need(s["citations"], f"step {s['id']} cites no source")
            missing = [c for c in s["citations"] if c not in sources]
            need(not missing, f"step {s['id']} cites {missing}, not in the plan's citations")
        need(all(c.get("url") for c in plan["citations"]), "a plan citation has no url")
        chemical = [s for s in steps if s["category"] == "chemical"]
        for s in chemical:
            need(s.get("safety_note") == s5.SAFETY_NOTE,
                 f"chemical step {s['id']} without the fixed safety note: {s.get('safety_note')!r}")
        ev.append(f"{len(chemical)} chemical step(s), each with the safety note" if chemical else
                  "no chemical step, so the safety note isn't observable on this plan")
        # When a step names a treatment, its category is the treatment's kind; whether the
        # product rule still applies then is not stated, so only steps without one are checked.
        for s in steps:
            word = PRODUCT.search(f"{s['title']} {s['detail']}")
            if word and not s.get("treatment_id"):
                need(s["category"] == "chemical",
                     f"step {s['id']} names {word.group(0)!r} but is {s['category']}, not chemical")
        texts = [plan["summary"], *plan["watch_for"]] + [f"{s['title']}. {s['detail']}" for s in steps]
        doses = [m.group(0) for t in texts for m in [DOSE.search(t)] if m]
        need(not doses, f"amounts in the plan's text: {doses}")
        rough = [t[:120] for t in texts if s5.AMOUNT.search(t)]
        if rough:
            ev.append(f"warning: s5's broader amount pattern matched (not the contract's check, "
                      f"read them): {rough}")

    def check_versions(self, scan, ev):
        """A model version on every result GET /scans/{id} shows (vision J4's trace)."""
        obs = scan.get("observations", [])
        unversioned = [f"{o['stage']}/{o['source']}" for o in obs
                       if o["source"] != "user" and not o.get("model_version")]
        need(not unversioned, f"observations without model_version: {unversioned}")
        versions = sorted({f"{o['pipeline_stage']}/{o['source']}: {o['model_version']}" for o in obs})
        ev.append(f"observation model versions: {versions}")
        mv = scan["diagnosis"].get("model_versions") or {}
        ev.append(f"diagnosis model_versions: {mv}")
        need(mv, "the diagnosis has no model_versions")
        plan = scan["plan"]
        need(plan.get("llm_model") and plan.get("prompt_version"),
             "the plan has no llm_model or prompt_version")

    def check_kill(self, ev):
        a = self.args
        if not (a.api_kill_cmd and a.api_start_cmd):
            raise Skip("needs --api-kill-cmd and --api-start-cmd")
        step, stage = KILL_POINTS[a.kill_after]
        kind, _, kill_stage = a.kill_after.partition(":")
        sid = self.new_scan("kill")
        st = self.stream("kill", sid)
        st.hooks.append(self.answering(sid))
        kill = {}

        def killer(_stream, d):
            if kill or d["type"] != kind or (kill_stage and d.get("stage") != kill_stage):
                return
            kill["event"] = d
            info(f"  {a.kill_after} at seq {d['seq']}: running --api-kill-cmd ({step} holds the claim)")
            t0 = time.monotonic()
            r = subprocess.run(a.api_kill_cmd, shell=True, timeout=120)
            need(r.returncode == 0, f"--api-kill-cmd exited {r.returncode}: nothing was killed, "
                 f"so the check would prove nothing")
            kill["down"] = not self.api.healthy()
            info("  running --api-start-cmd (detached)")
            subprocess.Popen(a.api_start_cmd, shell=True, start_new_session=True,
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
            self.wait_healthy(ev, t0)
            info(f"  api back; the sweep re-claims {step} once its stamp is {STALE_S[step]} s old")

        st.hooks.append(killer)
        t0 = time.monotonic()
        st.follow(lambda s: s.of("plan_ready") or s.of("failed"),
                  a.timeout + STALE_S[step] + 2 * SWEEP_TICK_S + a.health_timeout, "plan_ready")
        need(kill, f"--kill-after {a.kill_after} never arrived before the run ended "
             f"(events: {' '.join(st.types)})")
        k = kill["event"]
        ev.append(f"scan {sid}: killed after {a.kill_after} (seq {k['seq']}); api "
                  f"{'refused' if kill['down'] else 'still answered'} /healthz right after the kill; "
                  f"{st.last_seq} events in {time.monotonic() - t0:.0f} s, {st.reconnects} reconnects")
        need(not st.of("failed"), f"the scan failed: {(st.of('failed') or [{}])[0].get('failure')}")
        scan = self.get_scan("kill", sid)
        need(scan["status"] == "planned", f"status {scan['status']}, not planned")
        need(st.seen == set(range(1, st.last_seq + 1)), "seq is not 1..N")
        run = scan["run"]
        # The re-run: the step's stage started after the kill, once the claim was stale. Without
        # it the step committed before the kill landed, and nothing was recovered.
        starts = [d for d in st.of("stage_started", after=k["seq"], run=run) if d["stage"] == stage]
        for d in starts:
            ev.append(f"  stage_started {stage} at seq {d['seq']}, {secs(k, d):.0f} s after the "
                      f"kill's event")
        swept = [d for d in starts if secs(k, d) >= STALE_S[step] - SLACK_S]
        need(swept, f"no {stage} stage started {STALE_S[step] - SLACK_S} s or more after the kill's "
             f"event: the {step} step committed before the kill landed, or was re-run by "
             f"something other than the sweep")
        need(len(swept) == 1, f"the sweep re-ran {stage} {len(swept)} times, not once")
        commits = {"retrieve": "candidates_selected", "score": "diagnosis", "plan": "plan_ready"}
        done = st.of(commits[step], after=k["seq"], run=run)
        need(done, f"no {commits[step]} after the kill: the recovered {step} step didn't commit")
        ev.append(f"recovered: {step} re-run at seq {swept[0]['seq']} ({secs(k, swept[0]):.0f} s "
                  f"after the kill's event), committed at seq {done[0]['seq']} ({commits[step]})")
        # Committed exactly once: outcome events go out only after a commit (backend flows 3).
        diagnoses, ready = st.of("diagnosis", run=run), st.of("plan_ready", run=run)
        vlm = next((d["seq"] for d in st.of("stage_started", run=run) if d["stage"] == "vlm_findings"),
                   st.last_seq + 1)
        selected = [d for d in st.of("candidates_selected", run=run) if d["seq"] < vlm]
        ev.append(f"run {run}: {len(diagnoses)} diagnosis, {len(ready)} plan_ready, {len(selected)} "
                  f"candidates_selected before the VLM stage")
        need(len(diagnoses) == 1, f"{len(diagnoses)} diagnosis events in run {run}")
        need(len(ready) == 1, f"{len(ready)} plan_ready events in run {run}")
        need(len(selected) == 1, f"{len(selected)} candidates_selected before the VLM stage")
        last = {}
        for d in st.events:
            if d["type"] in ("stage_started", "stage_completed"):
                last[d["stage"]] = d["type"]
        open_stages = sorted(s for s, t in last.items() if t == "stage_started")
        need(not open_stages, f"stages started and never completed: {open_stages}")
        plan, d = scan.get("plan") or {}, scan.get("diagnosis") or {}
        need(plan.get("version") == 1 and not plan.get("supersedes"),
             f"plan version {plan.get('version')}, supersedes {plan.get('supersedes')}: not version 1")
        need(plan["id"] == ready[0]["plan_id"], "the scan's plan is not the one plan_ready named")
        need(d.get("id") == diagnoses[0]["diagnosis"]["id"] and not d.get("supersedes"),
             "the scan's diagnosis is not the run's one diagnosis")
        ev.append(f"plan {plan['id']} version 1, diagnosis {d['id']} ({d['status']}); seq "
                  f"1..{st.last_seq} with no gap or repeat across the restart")

    def check_late(self, ev):
        a = self.args
        if a.late_scan_id:
            sid = a.late_scan_id
            st = self.stream("late", sid)
            scan = self.get_scan("late", sid)
            need(scan["status"] == "planned" and scan.get("open_question") and scan.get("plan"),
                 f"--late-scan-id {sid} is {scan['status']}, open question "
                 f"{bool(scan.get('open_question'))}: it must be planned with its question open")
            stored = scan["last_event_seq"]
            st.follow(lambda s: s.last_seq >= stored, 120, f"the stored events up to seq {stored}")
            ev.append(f"reused scan {sid}: replayed its {stored} events")
        else:
            sid = self.late_sid
            st = self.stream("late", sid)
            budget = a.timeout + CONTINUE_AFTER_S + 2 * SWEEP_TICK_S
            st.follow(lambda s: s.of("plan_ready") or s.of("failed"), budget, "plan_ready")
            ev.append(f"scan {sid}: plan_ready {time.monotonic() - self.late_t0:.0f} s after submit")
            need(not st.of("failed"), f"the scan failed: {(st.of('failed') or [{}])[0].get('failure')}")
            scan = self.get_scan("late", sid)
        asked = st.of("question")
        if not asked:
            raise Skip(f"scan {sid} asked no question, so there is nothing to answer late")
        q = scan.get("open_question")
        need(q, "the plan is ready but no question is open: it was answered or superseded")
        ask = next((d for d in asked if d["question"]["id"] == q["id"]), None)
        ready = (st.of("plan_ready") or [None])[-1]
        need(ask, "the open question was never sent as a question event")
        need(ready, "no plan_ready in the scan's events: the continuation wrote no plan")
        ev.append(f"question {q['id']} ({q['kind']}, {q.get('indicator_id') or q.get('photo_kind')}) "
                  f"{q['text']!r} at seq {ask['seq']}, continue_at {q['continue_at']}; plan_ready at "
                  f"seq {ready['seq']}, {secs(ask, ready):.0f} s after it (10 minutes, then the "
                  f"sweep's next tick and the plan writer)")
        need(q["status"] == "open", f"the question is {q['status']}, not open")
        continued = [d for d in st.of("status_changed") if d["from"] == "awaiting_answer"
                     and d["to"] == "diagnosed"]
        need(continued, "no status_changed awaiting_answer → diagnosed: the scan wasn't continued")
        old_plan, old_diag = scan["plan"], scan["diagnosis"]
        need(old_plan["provisional"] and old_diag["status"] == "uncertain",
             f"the continued plan: provisional {old_plan['provisional']}, diagnosis "
             f"{old_diag['status']} (expected provisional and uncertain)")
        need((old_plan.get("open_question") or {}).get("id") == q["id"],
             "the provisional plan doesn't hold the open question")
        garden = scan["garden_id"]
        old_tasks = self.plan_tasks(garden, old_plan["id"])
        open_before = [t["id"] for t in old_tasks if not t.get("done_at") and not t.get("dismissed_at")]
        ev.append(f"provisional plan {old_plan['id']} v{old_plan['version']}: task_ids "
                  f"{len(old_plan['task_ids'])}, {len(old_tasks)} plan tasks listed, "
                  f"{len(open_before)} open")

        mark = st.last_seq
        t0 = time.monotonic()
        if q["kind"] == "indicator":
            self.call("POST", f"/scans/{sid}/answers",
                      {"question_id": q["id"], "option_id": a.late_answer}, expect=(202,))
            by = "user"
            ev.append(f"answered late: {a.late_answer}")
        else:
            need(a.leaf_photo, "a photo request is open: a late photo needs --leaf-photo")
            kind = q.get("photo_kind") or "leaf_closeup"
            self.api.upload(sid, kind, a.leaf_photo,
                            extra_parts=(("question_id", None, "text/plain", q["id"].encode()),))
            by = "photo"
            ev.append(f"answered late with --leaf-photo as {kind}")

        def superseded(s):
            return s.of("plan_superseded", after=mark) or s.of("failed", after=mark)
        st.follow(superseded, a.timeout, "plan_superseded")
        ev.append(f"re-planned {time.monotonic() - t0:.0f} s after the answer")
        need(not st.of("failed", after=mark), "the scan failed after the late answer")
        after = [d for d in st.events if d["seq"] > mark]
        ev.append(f"events after the answer: {' '.join(d['type'] for d in after)}")
        resolved = [d for d in after if d["type"] == "question_resolved" and d["question_id"] == q["id"]]
        need(resolved, "no question_resolved for the question")
        need(resolved[0]["by"] == by, f"question_resolved by {resolved[0]['by']}, expected {by}")
        diag = next((d for d in after if d["type"] == "diagnosis"
                     and d["seq"] > resolved[0]["seq"]), None)
        need(diag, "no diagnosis after question_resolved")
        new_diag = diag["diagnosis"]
        need(new_diag.get("supersedes") == old_diag["id"],
             f"the new diagnosis supersedes {new_diag.get('supersedes')}, not {old_diag['id']}")
        sup = st.of("plan_superseded", after=mark)[0]
        need(sup["seq"] > diag["seq"], "plan_superseded came before the new diagnosis")
        need(sup["old_plan_id"] == old_plan["id"] and sup["reason"] == "late_answer",
             f"plan_superseded {sup['old_plan_id']} ({sup['reason']}), expected {old_plan['id']} "
             f"(late_answer)")
        # contracts 4 lists question_resolved, diagnosis and plan_superseded for a late answer; the
        # plan step sends plan_ready only on a `diagnosed` scan, so none is expected here.
        extra = st.of("plan_ready", after=mark)
        ev.append(f"seq {resolved[0]['seq']} question_resolved ({by}), {diag['seq']} diagnosis "
                  f"{new_diag['id']} ({new_diag['status']}, supersedes the old), {sup['seq']} "
                  f"plan_superseded late_answer -> {sup['new_plan_id']}"
                  + (f"; plan_ready at seq {extra[0]['seq']}" if extra else "; no plan_ready"))
        scan2 = self.get_scan("late", sid)
        plan2 = scan2.get("plan") or {}
        need(scan2["status"] == "planned", f"status {scan2['status']}: late work never moves it")
        need(plan2.get("id") == sup["new_plan_id"], "GET /scans/{id} doesn't show the new plan")
        need(plan2["version"] == old_plan["version"] + 1 and plan2.get("supersedes") == old_plan["id"],
             f"plan version {plan2['version']} supersedes {plan2.get('supersedes')}: not version "
             f"{old_plan['version'] + 1} superseding {old_plan['id']}")
        need(plan2["diagnosis_id"] == new_diag["id"] and scan2["diagnosis"]["id"] == new_diag["id"],
             "the scan's diagnosis or the new plan's isn't the new diagnosis")
        need(plan2["provisional"] == (new_diag["status"] == "uncertain"),
             f"provisional {plan2['provisional']} with a {new_diag['status']} diagnosis")
        if by == "user":
            need(not scan2.get("open_question") and not plan2.get("open_question"),
                 "the answered question is still open")
        tasks = {t["id"]: t for t in self.plan_tasks(garden, old_plan["id"])}
        kept = [i for i in open_before if not (tasks.get(i, {}).get("dismissed_at")
                and tasks[i].get("dismissed_reason") == "plan_superseded")]
        need(not kept, f"the old plan's tasks not dismissed as plan_superseded: {kept}")
        new_tasks = self.plan_tasks(garden, plan2["id"])
        ev.append(f"plan {plan2['id']} v{plan2['version']} (provisional {plan2['provisional']}); "
                  f"{len(open_before)} old open task(s) dismissed (plan_superseded); the new plan "
                  f"has {len(new_tasks)} task(s)")
        if not open_before:
            raise Skip("everything else passed, but the provisional plan had no open task, so "
                       "the dismissal wasn't observable. " + " ".join(ev[-1:]))

    # --- report -----------------------------------------------------------------------------

    def write_out(self):
        path = self.args.out
        if not path:
            return
        doc = {"script": "backend/tools/smoke/s4.py", "base_url": self.args.base_url,
               "written_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
               "results": {n: v for n, v, _ in self.results}, "checks": {}}
        for check, f in self.fixture.items():
            doc["checks"][check] = {
                "scan_id": f["scan_id"],
                "events": [{"type": d["type"], "seq": d["seq"], "data": d} for d in f["events"]],
                "scan": f["scan"],
                "model_versions": trace(f["scan"]),
            }
        Path(path).write_text(json.dumps(strip_signatures(doc), indent=1) + "\n")
        info(f"wrote {path} ({sum(len(f['events']) for f in self.fixture.values())} events)")

    def report(self):
        info("\n=== summary")
        for name, verdict, _ in self.results:
            info(f"  {verdict:4} {name}")
        return 1 if any(v == "FAIL" for _, v, _ in self.results) else 0


def main():
    ap = argparse.ArgumentParser(description="S4 live checks: the early-blight scan, the api kill "
                                 "test, a late answer.")
    ap.add_argument("--base-url", default="http://localhost:8080")
    ap.add_argument("--env-file", default="../infra/dev/.env",
                    help="holds CLERK_SECRET_KEY (sk_test_) of the Clerk development instance "
                         "the api trusts, also on the control plane")
    ap.add_argument("--email", default="s4+clerk_test@example.com",
                    help="the Clerk test user (a +clerk_test address); 3 scans of the daily 30")
    ap.add_argument("--phone-suffix", type=int, default=104,
                    help="the user's Clerk test phone number +1555555<0100-0199>, if the "
                         "instance asks for one")
    ap.add_argument("--photo", required=True, help="whole-plant photo, uploaded as whole_plant")
    ap.add_argument("--leaf-photo", help="leaf close-up, uploaded as leaf_closeup (the disease head "
                    "reads only these); also the late photo when the open question asks for one")
    ap.add_argument("--lat", type=float, default=43.65)
    ap.add_argument("--lon", type=float, default=-79.38)
    ap.add_argument("--answers", default="*=unsure",
                    help="answers to indicator questions in the scan and kill checks: a comma "
                         "list of <indicator id or its suffix>=yes|no|unsure, * the default, e.g. "
                         "concentric_rings_bullseye=yes,*=unsure")
    ap.add_argument("--expect-problem", help="fail the scan check unless the primary problem is "
                    "this, e.g. tomato_early_blight")
    ap.add_argument("--timeout", type=int, default=900,
                    help="seconds for one scan to reach plan_ready (the kill and late checks add "
                         "the sweep's and the continuation's waits to it)")
    ap.add_argument("--only", default=",".join(CHECKS),
                    help=f"comma-separated checks; they run in this order: {','.join(CHECKS)}")
    ap.add_argument("--kill-after", default="diagnosis", choices=sorted(KILL_POINTS),
                    help="the event the kill comes right after; default diagnosis (score has "
                         "committed and handed its claim to plan, whose LLM call is the widest "
                         "window)")
    ap.add_argument("--api-kill-cmd", help="shell command that kills api at once (SIGKILL, not a "
                    "graceful stop, which would drain the step); must exit 0")
    ap.add_argument("--api-start-cmd", help="shell command, run detached, that starts api again")
    ap.add_argument("--health-timeout", type=int, default=300,
                    help="seconds for api to answer /healthz after the kill")
    ap.add_argument("--late-answer", default="yes", choices=ANSWERS,
                    help="the late answer to the continued indicator question")
    ap.add_argument("--late-scan-id", help="a scan already continued past its question (planned, "
                    "question open), to skip the 10 minutes; the test user must own it")
    ap.add_argument("--out", help="write each scan's events and final GET /scans/{id} to this "
                    "JSON file (signed URLs' signatures removed)")
    ap.add_argument("--verbose", action="store_true", help="print every event")
    args = ap.parse_args()
    args.only = {c.strip() for c in args.only.split(",") if c.strip()}
    unknown = args.only - set(CHECKS)
    if unknown:
        smoke.die(f"unknown checks: {sorted(unknown)}")
    for p in (args.photo, args.leaf_photo):
        if p and not Path(p).is_file():
            smoke.die(f"photo not found: {p}")
    if not args.leaf_photo:
        info("note: no --leaf-photo, so the disease head never runs (no leaf_closeup shot)")
    args.answers = parse_answers(args.answers)

    s = S4(args)
    s.setup()
    try:
        for name in CHECKS:
            s.run_check(name, getattr(s, f"check_{name}"))
    finally:
        s.write_out()
    sys.exit(s.report())


if __name__ == "__main__":
    main()
