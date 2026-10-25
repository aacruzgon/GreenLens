#!/usr/bin/env python3
"""Live checks for backend slice S9 (farm connectors and Era), in s4.py's and s5.py's style.

Signs in the way smoke.py does (a Clerk test instance's Backend API mints a session token), as
the demo garden's owner, demo+clerk_test@example.com (tools/demo/demo_garden.py's user; --email
for another). The demo garden is --garden-id, else ERA_DEMO_GARDEN_ID from the environment or
--env-file, else the user's own garden named as the Maple Row Farm fixture's garden.json.

What it writes. The plan check makes one scan of bed 3 (the fixture's P_BED3) in the demo garden
unless --scan-id names one, and the chat check opens one conversation on that scan (a garden
conversation without a scan). The disconnect check deletes the --connector-id connector, and
only that one. Nothing else is created or changed: era-guard's DELETE must be refused.
--cleanup sends DELETE /scans/{id} for the scans the script made, and nothing else; the API
deletes only a `pending` scan, so a planned scan stays (409) and the script says so.

Checks, in this order (--only picks some); backend roadmap 12's exit checks 1 and 4:

  connectors  GET /gardens/{id}/connectors: both `era` connectors, `active`, synced, Slack's
              sources #field-reports and #crew, Drive's the Farm records folder, records > 0;
              --sync-wait waits for the first sync
  plan        one scan of bed 3 to `planned` (--photo, --leaf-photo, --answers), or --scan-id:
              ActionPlan.farm_records holds the spray log, a step's farm_citations name it,
              every `era` record carries `text`, and no step holds a rate
  chat        --chat-question on that scan: the stored message's farm_citations hold a
              #field-reports record with a link, an `era` record's text, and no streamed
              sentence holds a rate; token usage
  era-guard   DELETE /connectors/{id} of each `era` connector answers 409 invalid_state, and the
              connector stays
  disconnect  with --connector-id (a real vendor connector, active and synced): DELETE, gone
              from the list; --db-check: no farm_record of it and no connector row left;
              --api-log FILE or --api-log-cmd CMD: api's "token revoked" line for it. SKIP
              for any part it can't prove

A rate is a match of the fixture's own `rate_pattern` (answers.toml [safety], the agent
evaluation's rule); s5.py's broader amount pattern only warns.

usage (from backend/):
    python3 tools/smoke/s9.py --photo PLANT.jpg --leaf-photo LEAF.jpg
        [--base-url http://localhost:8080] [--env-file ../infra/dev/.env] [--garden-id ID]
        [--email demo+clerk_test@example.com] [--phone-suffix 101] [--sync-wait 0]
        [--answers concentric_rings_bullseye=yes,*=unsure] [--scan-id ID]
        [--chat-question TEXT] [--only connectors,plan,chat,era-guard,disconnect]
        [--connector-id ID [--db-check] [--api-log FILE | --api-log-cmd CMD]]
        [--out s9.json] [--cleanup] [--verbose]

Exit code 0 when no check failed; SKIP is never a pass. Neither a key nor a token is ever
printed, and nor is the error text of api's revoke lines.
"""
import argparse
import base64
import json
import re
import subprocess
import sys
import time
import tomllib
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
BACKEND = HERE.parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent / "demo"))
import smoke  # noqa: E402  (Clerk, Api, http_json, load_env, die)
import s4  # noqa: E402  (Stream, answering, parse_answers, strip_signatures)
import s5  # noqa: E402  (run_check, call, chat, upload_photos, AMOUNT, prices, read_env_file)
from s5 import Fail, Skip, info, need  # noqa: E402
from demo_garden import DEMO_EMAIL, DEMO_USER_FIELDS, pages  # noqa: E402

CHECKS = ("connectors", "plan", "chat", "era-guard", "disconnect")
FIXTURE = BACKEND / "tests/fixtures/era/maple-row"
GARDEN = json.loads((FIXTURE / "garden.json").read_text())
DRIVE = json.loads((FIXTURE / "google-drive.json").read_text())
SLACK = json.loads((FIXTURE / "slack.json").read_text())
ANSWERS = tomllib.loads((FIXTURE / "answers.toml").read_text())

GARDEN_NAME = GARDEN["garden"]["name"]                                       # Maple Row Farm
BED3 = next(p["name"] for p in GARDEN["plants"] if p["key"] == "P_BED3")     # Bed 3
SPRAY_ID = "D_SPRAY"
SPRAY_LOG = next(f["name"] for f in DRIVE["files"] if f["id"] == SPRAY_ID)   # Spray log 2026
FOLDER = next(f for f in DRIVE["files"]
              if f["mimeType"] == "application/vnd.google-apps.folder")      # F_FARM, Farm records
CHANNELS = {c["id"]: c["name"] for c in SLACK["channels"]}                    # field-reports, crew
FIELD_REPORTS = "#field-reports"
# The evaluation's spray-rate rule: a mass or volume per volume or area, a spoon or fluid-ounce
# measure, a percentage of a product, or a 1:N ratio (connectors 9.3). Dates and bed numbers
# are not rates. Python reads the leading (?i) as Rust's regex does.
RATE = re.compile(ANSWERS["safety"]["rate_pattern"])
DEFAULT_QUESTION = "What has the crew reported about bed 3 in #field-reports?"
# api's log messages on a connector's deletion (bins/api/src/deletion.rs), longest first so a
# plain-text line matches the right one. Only the message, level and time are printed.
REVOKED = "token revoked"
LOG_OUTCOMES = (
    ("token revoke task failed; the records are deleted anyway", "FAIL"),
    ("token revoke failed; the records are deleted anyway", "FAIL"),
    ("couldn't check the account's other connectors; not revoked", "FAIL"),
    ("another connector holds this account's token; not revoked", "FAIL"),
    ("connectors off: the token can't be opened to revoke", "FAIL"),
    ("connectors off: no vendor to revoke with", "FAIL"),
    ("token not revoked: it doesn't open", "FAIL"),
    ("connector deletion stopped; it resumes on restart", "FAIL"),
    (REVOKED, "PASS"),
)
DELETED = "connector deleted"


def rates(text):
    """The sentences of `text` that hold a rate (the fixture's rate_pattern)."""
    return [s for s in sentences(text) if RATE.search(s)]


def sentences(text):
    """s5.py's dose check's sentence split."""
    return [s.strip() for s in re.split(r"(?<=[.!?])\s+|\n+", text or "") if s.strip()]


def record_parts(record_id):
    """A FarmRecordId's `<connector_id>.<n>.<vendor_id>` (domain::id), as (connector, vendor)."""
    connector, _, rest = record_id.partition(".")
    _, _, vendor = rest.partition(".")
    return connector, vendor


def log_events(text, connector_id):
    """api's deletion lines for one connector: [(message, verdict, level, timestamp)]. Reads
    the JSON lines `api` writes by default (flatten_event, a compose prefix allowed) and, for
    LOG_FORMAT=pretty, any line naming the connector and one of the messages."""
    out = []
    for line in text.splitlines():
        if connector_id not in line:
            continue
        msg = level = ts = None
        start = line.find("{")
        if start >= 0:
            try:
                obj = json.loads(line[start:])
            except ValueError:
                obj = None
            if isinstance(obj, dict):
                fields = obj.get("fields") if isinstance(obj.get("fields"), dict) else obj
                if fields.get("connector") != connector_id:
                    continue
                msg, level, ts = fields.get("message"), obj.get("level"), obj.get("timestamp")
        known = [m for m, _ in LOG_OUTCOMES] + [DELETED]
        if msg is None:
            msg = next((m for m in known if m in line), None)
        if msg in known:
            verdict = dict(LOG_OUTCOMES).get(msg, "INFO")
            out.append((msg, verdict, level, ts))
    return out


class S9:
    call = s5.S5.call            # Api.call, a wrong status a Fail of the running check
    run_check = s5.S5.run_check  # PASS, FAIL or SKIP with the evidence
    chat = s5.S5.chat            # one chat turn, read to message_completed
    answering = s4.S4.answering  # answers indicator questions from --answers

    def __init__(self, args):
        self.args = args
        secret = smoke.load_env(args.env_file, "CLERK_SECRET_KEY")
        if not secret:
            smoke.die(f"CLERK_SECRET_KEY not in the environment or {args.env_file}")
        if args.email == DEMO_EMAIL:
            fields = DEMO_USER_FIELDS
        else:
            local = args.email.split("@")[0].split("+")[0]
            fields = {"username": f"pdm_{local}"[:30],
                      "phone_number": [f"+1555555{args.phone_suffix:04d}"]}
        self.api = smoke.Api(args.base_url, smoke.Clerk(secret, args.email, fields))
        self.results = []       # (check, verdict, evidence lines)
        self.usage = {"tokens_in": 0, "tokens_out": 0}
        self.turns = 0
        self.made = []          # scans the script made, for --cleanup
        self.garden = self.bed3 = self.scan_id = None
        self.out = {"connectors": None, "scan": None, "events": [], "chat": None}

    # --- plumbing ---------------------------------------------------------------------------

    def setup(self):
        a = self.args
        if not self.api.healthy():
            smoke.die(f"{a.base_url}/healthz did not answer 200")
        gid = a.garden_id or smoke.load_env(a.env_file, "ERA_DEMO_GARDEN_ID")
        if gid:
            g = self.api.call("GET", f"/gardens/{gid}")
            how = "--garden-id" if a.garden_id else "ERA_DEMO_GARDEN_ID"
        else:
            owned = [g for g in pages(self.api, "/gardens")
                     if g["name"] == GARDEN_NAME and g["my_role"] == "owner"]
            if len(owned) != 1:
                smoke.die(f"{a.email} owns {len(owned)} gardens named {GARDEN_NAME!r}; give "
                          f"--garden-id (or run tools/demo/demo_garden.py)")
            g, how = owned[0], f"the user's garden named {GARDEN_NAME!r}"
        self.garden = g["id"]
        info(f"demo garden {self.garden} ({g['name']!r}, {g['my_role']}; from {how})")
        if g["my_role"] != "owner":
            info(f"note: {a.email} is {g['my_role']}, not owner: era-guard's 409 would be a 403")
        bed = [p for p in pages(self.api, "/plants", {"garden_id": self.garden})
               if p["name"] == BED3]
        if len(bed) == 1:
            self.bed3 = bed[0]
            sp = (self.bed3.get("species") or {}).get("id")
            info(f"{BED3}: plant {self.bed3['id']}, species {sp}, {self.bed3['status']}")
        else:
            info(f"note: the garden has {len(bed)} plants named {BED3!r}; the plan check needs one")

    def connectors(self):
        out = self.call("GET", f"/gardens/{self.garden}/connectors")
        self.out["connectors"] = out
        return out

    def era_ids(self):
        return {c["id"] for c in self.connectors()["connectors"] if c["endpoint"] == "era"}

    @staticmethod
    def describe(c):
        acct = c.get("account") or {}
        who = acct.get("team_name") or ("a Google account" if acct.get("kind") == "google_drive"
                                        else "no account")
        return (f"{c['kind']} {c['id']}: {c['status']}, endpoint {c['endpoint']}, {who}, sources "
                f"{[s['name'] for s in c['sources']]}, synced_at {c.get('synced_at')}, "
                f"record_count {c['record_count']}")

    def cleanup(self):
        for sid in self.made:
            status, _, _ = smoke.http_json("DELETE", f"{self.api.base}/scans/{sid}",
                                           self.api.headers())
            note = " (only a pending scan can be deleted; this one stays)" if status == 409 else ""
            info(f"cleanup: DELETE /scans/{sid} -> {status}{note}")

    # --- checks -----------------------------------------------------------------------------

    def check_connectors(self, ev):
        a = self.args
        t0 = time.monotonic()
        while True:
            problems = self.era_problems()
            if not problems or time.monotonic() - t0 >= a.sync_wait:
                break
            info(f"  waiting for the first sync ({problems[0]})")
            time.sleep(5)
        lst = self.out["connectors"]
        for c in lst["connectors"]:
            ev.append(self.describe(c))
        if a.sync_wait and time.monotonic() - t0 > 1:
            ev.append(f"waited {time.monotonic() - t0:.0f} s (--sync-wait {a.sync_wait})")
        need(not problems, "; ".join(problems))
        ev.append("both era connectors active and synced, with the fixture's sources")

    def era_problems(self):
        """What keeps the demo garden's era connectors from S9's state: [] when none."""
        lst = self.connectors()
        if not lst["available"]:
            return ["available: false (api has no CONNECTOR_KEY)"]
        era = [c for c in lst["connectors"] if c["endpoint"] == "era"]
        problems = []
        for kind, want in (("slack", {f"#{n}" for n in CHANNELS.values()}),
                           ("google_drive", {FOLDER["name"]})):
            mine = [c for c in era if c["kind"] == kind]
            if len(mine) != 1:
                problems.append(f"{len(mine)} era {kind} connectors, expected 1 (api seeds them "
                                f"when ERA_SLACK_BASE, ERA_GOOGLE_BASE, CONNECTOR_KEY and "
                                f"ERA_DEMO_GARDEN_ID are set)")
                continue
            c = mine[0]
            names = {("#" + s["name"].lstrip("#")) if kind == "slack" else s["name"]
                     for s in c["sources"]}
            if c["status"] != "active":
                problems.append(f"era {kind} is {c['status']}, not active")
            if not c.get("synced_at"):
                problems.append(f"era {kind} has no synced_at: not synced yet")
            if c["record_count"] <= 0:
                problems.append(f"era {kind} holds {c['record_count']} records")
            if not want <= names:
                problems.append(f"era {kind} sources {sorted(names)}, expected {sorted(want)}")
            if kind == "google_drive" and FOLDER["id"] not in {s["id"] for s in c["sources"]}:
                problems.append(f"era google_drive's folder isn't {FOLDER['id']}")
        return problems

    def check_plan(self, ev):
        a = self.args
        era = self.era_ids()
        if a.scan_id:
            sid = a.scan_id
            scan = self.call("GET", f"/scans/{sid}")
            ev.append(f"--scan-id {sid}: {scan['status']}")
        else:
            need(self.bed3, f"no single plant named {BED3!r} in the demo garden")
            sid = self.call("POST", "/scans", {"garden_id": self.garden,
                            "plant_id": self.bed3["id"]}, expect=(201,), idem=True)["id"]
            self.made.append(sid)
            info(f"  scan {sid} of {BED3}")
            s5.upload_photos(self.api, sid, a.photo, a.leaf_photo)
            self.call("POST", f"/scans/{sid}/submit", expect=(202,))
            scan = {"status": "queued"}
        self.scan_id = sid
        if scan["status"] != "planned":
            st = s4.Stream(self, sid)
            self.out["events"] = st.events
            st.hooks.append(self.answering(sid))
            t0 = time.monotonic()
            st.follow(lambda s: s.of("plan_ready") or s.of("failed"), a.timeout, "plan_ready")
            ev.append(f"scan {sid}: {st.last_seq} events in {time.monotonic() - t0:.0f} s, "
                      f"{len(st.of('question'))} question(s)")
            need(not st.of("failed"),
                 f"the scan failed: {(st.of('failed') or [{}])[0].get('failure')}")
            scan = self.call("GET", f"/scans/{sid}")
        self.out["scan"] = scan
        need(scan["status"] == "planned", f"status {scan['status']}, not planned")
        if self.bed3:
            need(scan["plant"]["id"] == self.bed3["id"],
                 f"scan {sid} is of {scan['plant'].get('name')!r}, not {BED3}")
        d, plan = scan.get("diagnosis") or {}, scan.get("plan")
        ev.append(f"diagnosis {d.get('status')}, primary "
                  f"{(d.get('primary') or {}).get('problem_id')}")
        need(plan, "GET /scans/{id} has no plan")
        records = plan.get("farm_records") or []
        ev.append(f"plan {plan['id']} v{plan['version']}: {len(plan['steps'])} steps, "
                  f"{len(records)} farm records: "
                  f"{[(r['system'], r['title'], record_parts(r['record_id'])[1]) for r in records]}")
        need(records, "ActionPlan.farm_records is empty: the plan cites no farm record")
        spray = [r for r in records if r["system"] == "google_drive" and r["title"] == SPRAY_LOG]
        need(spray, f"no google_drive record titled {SPRAY_LOG!r} in farm_records")
        vendor_ids = {record_parts(r["record_id"])[1] for r in spray}
        if SPRAY_ID not in vendor_ids:
            ev.append(f"note: the spray log's Drive id is {sorted(vendor_ids)}, not the "
                      f"fixture's {SPRAY_ID}")
        spray_ids = {r["record_id"] for r in spray}
        known = {r["record_id"] for r in records}
        citing = []
        for s in plan["steps"]:
            fc = s.get("farm_citations") or []
            missing = [x for x in fc if x not in known]
            need(not missing, f"step {s['id']} cites {missing}, not in farm_records (contracts 7)")
            if spray_ids & set(fc):
                citing.append(s)
        need(citing, f"farm_records holds {SPRAY_LOG!r} but no step's farm_citations names it")
        for s in citing:
            ev.append(f"cites the spray log: step {s['order']} [{s['category']}] {s['title']!r} "
                      f"(farm_citations {s.get('farm_citations')}, sources {s['citations']})")
        era_records = [r for r in records if record_parts(r["record_id"])[0] in era]
        bare = [r["record_id"] for r in era_records if not (r.get("text") or "").strip()]
        need(era_records, "no farm record of an era connector in the plan")
        need(not bare, f"era records without text: {bare} (EON decision 10)")
        ev.append(f"{len(era_records)} era record(s), each with text ("
                  f"{', '.join(str(len(r['text'])) for r in era_records)} chars)")
        texts = [plan["summary"], *plan["watch_for"]] + [f"{s['title']}. {s['detail']}"
                                                         for s in plan["steps"]]
        found = [r for t in texts for r in rates(t)]
        need(not found, f"a rate in the plan's text: {found}")
        ev.append(f"no rate in the summary, {len(plan['watch_for'])} watch_for item(s) or "
                  f"{len(plan['steps'])} steps")
        rough = [t[:120] for t in texts if s5.AMOUNT.search(t)]
        if rough:
            ev.append(f"warning: s5's broader amount pattern matched (read them): {rough}")

    def check_chat(self, ev):
        a = self.args
        era = self.era_ids()
        if self.scan_id:
            conv = self.call("POST", "/conversations", {"garden_id": self.garden, "anchor": {
                "type": "scan", "id": self.scan_id}}, expect=(201,), idem=True)["id"]
            ev.append(f"conversation {conv} on scan {self.scan_id}")
        else:
            conv = self.call("POST", "/conversations", {"garden_id": self.garden, "anchor": {
                "type": "garden", "id": self.garden}}, expect=(201,), idem=True)["id"]
            ev.append(f"conversation {conv} on the garden (no scan: --scan-id or the plan check)")
        t = self.chat(conv, a.chat_question)
        ev.append(f"turn: {t.summary()}")
        ev.append(f"answer: {t.text.strip()[:400]!r}")
        tools = [name for name, _ in t.tools]
        msgs = self.call("GET", f"/conversations/{conv}")["messages"]["items"]   # newest first
        done = t.of("message_completed")[-1]
        msg = next((m for m in msgs if m["id"] == done.get("message_id")),
                   next((m for m in msgs if m["role"] == "assistant"), None))
        need(msg, "no stored assistant message")
        cites = msg.get("farm_citations") or []
        self.out["chat"] = {"conversation": conv, "question": a.chat_question, "text": t.text,
                            "route": t.route, "tools": tools, "usage": t.usage,
                            "farm_citations": cites}
        ev.append(f"stored message {msg['id']}: {len(cites)} farm citation(s): "
                  f"{[(c['system'], c['title']) for c in cites]}")
        need(cites, f"the message cites no farm record (tools {tools}; farm_citations are the "
             f"records search_farm_records returned)")
        field = [c for c in cites if c["system"] == "slack" and c["title"] == FIELD_REPORTS]
        need(field, f"no slack citation titled {FIELD_REPORTS!r}")
        linked = [c for c in field if (c.get("link") or "").strip()]
        need(linked, f"no {FIELD_REPORTS} citation has a link")
        c = linked[0]
        ev.append(f"{FIELD_REPORTS}: {len(field)} record(s); first {c['record_id']} by "
                  f"{c.get('author')} at {c['at']}, link {c['link']}, text "
                  f"{(c.get('text') or '')[:120]!r}")
        era_cites = [x for x in cites if record_parts(x["record_id"])[0] in era]
        with_text = [x for x in era_cites if (x.get("text") or "").strip()]
        need(with_text, f"no era citation carries text ({len(era_cites)} era citations): the "
             f"app's sheet would be empty (EON decision 10)")
        need(len(with_text) == len(era_cites),
             f"era citations without text: {[x['record_id'] for x in era_cites if x not in with_text]}")
        ev.append(f"{len(era_cites)} era citation(s), each with text")
        found = rates(t.text)
        n = len(sentences(t.text))
        need(not found, f"streamed sentences with a rate: {found}")
        ev.append(f"{n} streamed sentences, none with a rate")
        rough = [s for s in sentences(t.text) if s5.AMOUNT.search(s)]
        if rough:
            ev.append(f"warning: s5's broader amount pattern matched (read them): {rough}")

    def check_era_guard(self, ev):
        era = [c for c in self.connectors()["connectors"] if c["endpoint"] == "era"]
        if not era:
            raise Skip("the demo garden has no era connector to try")
        for c in era:
            status, _, body = smoke.http_json("DELETE", f"{self.api.base}/connectors/{c['id']}",
                                              self.api.headers())
            code = ((body or {}).get("error") or {}).get("code") if isinstance(body, dict) else None
            ev.append(f"DELETE /connectors/{c['id']} ({c['kind']}): {status} {code}")
            need(status != 202, f"the {c['kind']} demo connector was accepted for deletion: it is "
                 f"being deleted now (api's next start seeds it again)")
            need(status == 409 and code == "invalid_state",
                 f"expected 409 invalid_state, got {status}: {json.dumps(body)[:300]}")
        after = {c["id"]: c for c in self.connectors()["connectors"]}
        for c in era:
            need(c["id"] in after, f"the {c['kind']} demo connector is gone from the list")
            need(after[c["id"]]["status"] == c["status"],
                 f"the {c['kind']} demo connector went {c['status']} -> {after[c['id']]['status']}")
        ev.append(f"both stay listed, {', '.join(after[c['id']]['status'] for c in era)}")

    def check_disconnect(self, ev):
        a = self.args
        cid = a.connector_id
        if not cid:
            raise Skip("needs --connector-id (a real Slack or Drive connector, active and synced)")
        garden, c = self.find_connector(cid)
        need(c, f"connector {cid} isn't listed in any garden of {a.email} (gone, deleting, or "
             f"another user's)")
        ev.append(f"in garden {garden}: {self.describe(c)}")
        need(c["endpoint"] != "era", "an era connector: owners can't delete it (409 invalid_state, "
             "EON decision 11); exit check 4 needs a real vendor connector")
        need(c["status"] == "active" and c.get("synced_at") and c["record_count"] > 0,
             "exit check 4 needs a connector that connected and synced: active, synced_at set "
             "and records > 0")
        if a.db_check:
            n0 = self.db_records(cid)
            ev.append(f"SurrealDB before: {n0} farm_record rows of {cid} (record_count "
                      f"{c['record_count']})")
        t0 = time.monotonic()
        self.call("DELETE", f"/connectors/{cid}", expect=(202,))
        ev.append("DELETE /connectors/{id}: 202")
        while any(x["id"] == cid for x in self.call("GET", f"/gardens/{garden}/connectors")
                  ["connectors"]):
            need(time.monotonic() - t0 < 60, "still listed 60 s after the DELETE")
            time.sleep(1)
        ev.append(f"gone from the list after {time.monotonic() - t0:.0f} s (marked deleting; the "
                  f"list skips it)")
        proved, unproved = ["listed no more"], []
        deadline = t0 + a.delete_timeout
        if a.db_check:
            while True:
                n, row = self.db_records(cid), self.db_connector(cid)
                if (n == 0 and not row) or time.monotonic() > deadline:
                    break
                time.sleep(2)
            ev.append(f"SurrealDB after {time.monotonic() - t0:.0f} s: {n} farm_record rows, "
                      f"connector row {'present (' + str(row) + ')' if row else 'gone'}")
            need(n == 0, f"{n} farm_record rows of the connector remain")
            need(not row, f"the connector row remains: {row}")
            proved.append("no farm_record and no connector row")
        else:
            unproved.append("no farm_record left (needs --db-check)")
        if a.api_log or a.api_log_cmd:
            seen = []
            while True:
                seen = log_events(self.read_log(), cid)
                if any(m == DELETED for m, *_ in seen) or time.monotonic() > deadline:
                    break
                time.sleep(2)
            for m, verdict, level, ts in seen:
                ev.append(f"api log: {ts} {level} {m!r}")
            bad = [m for m, verdict, *_ in seen if verdict == "FAIL"]
            need(not bad, f"api didn't revoke the token: {bad}")
            need(any(m == REVOKED for m, *_ in seen),
                 f"no {REVOKED!r} line for {cid} in api's log within {a.delete_timeout} s")
            need(any(m == DELETED for m, *_ in seen),
                 f"no {DELETED!r} line for {cid}: the deletion hasn't finished")
            proved.append("token revoked (api's log: the vendor's revoke call succeeded)")
        else:
            unproved.append("token revoked (needs --api-log or --api-log-cmd)")
        ev.append(f"proved: {', '.join(proved)}")
        if unproved:
            raise Skip(f"not proved: {'; '.join(unproved)}")

    def find_connector(self, cid):
        """(garden id, Connector) of `cid` among the user's gardens, the demo garden first."""
        gardens = [self.garden] + [g["id"] for g in pages(self.api, "/gardens")
                                   if g["id"] != self.garden]
        for g in gardens:
            for c in self.call("GET", f"/gardens/{g}/connectors")["connectors"]:
                if c["id"] == cid:
                    return g, c
        return None, None

    def read_log(self):
        a = self.args
        if a.api_log_cmd:
            r = subprocess.run(a.api_log_cmd, shell=True, timeout=120, capture_output=True,
                               text=True, errors="replace")
            need(r.returncode == 0, f"--api-log-cmd exited {r.returncode}")
            return r.stdout + r.stderr
        p = Path(a.api_log)
        need(p.is_file(), f"--api-log {p}: no such file")
        return p.read_text(errors="replace")

    # --- optional database reads ------------------------------------------------------------

    def db_query(self, sql):
        """One statement's result over SurrealDB's HTTP API with --env-file's root user, as
        s5.py's --db-check: a dev-compose check only."""
        env = s5.read_env_file(self.args.env_file)
        url, user, pw = env.get("SURREAL_URL"), env.get("SURREAL_ROOT_USER"), env.get("SURREAL_ROOT_PASS")
        if not (url and user and pw):
            raise Fail("--db-check: SURREAL_URL or the root user missing from --env-file")
        http_url = re.sub(r"^ws", "http", url).rstrip("/") + "/sql"
        auth = base64.b64encode(f"{user}:{pw}".encode()).decode()
        status, _, out = smoke.http_json(
            "POST", http_url, {"Authorization": f"Basic {auth}", "Accept": "application/json",
                               "surreal-ns": "pdm", "surreal-db": "main"}, sql.encode())
        if status != 200 or not isinstance(out, list) or not out:
            raise Fail(f"SurrealDB {status}: {json.dumps(out)[:200]}")
        if out[0].get("status") != "OK":
            raise Fail(f"SurrealDB: {json.dumps(out[0])[:200]}")
        return out[0].get("result")

    def db_records(self, cid):
        res = self.db_query(f"SELECT count() AS n FROM farm_record WHERE connector = "
                            f"connector:`{cid}` GROUP ALL;")
        return res[0]["n"] if res else 0

    def db_connector(self, cid):
        res = self.db_query(f"SELECT VALUE status FROM connector:`{cid}`;")
        return res[0] if res else None

    # --- report -----------------------------------------------------------------------------

    def write_out(self):
        path = self.args.out
        if not path:
            return
        doc = {"script": "backend/tools/smoke/s9.py", "base_url": self.args.base_url,
               "written_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
               "garden_id": self.garden, "scan_id": self.scan_id,
               "results": {n: v for n, v, _ in self.results},
               "evidence": {n: e for n, _, e in self.results},
               "connectors": self.out["connectors"], "scan": self.out["scan"],
               "events": [{"type": d["type"], "seq": d["seq"], "data": d}
                          for d in self.out["events"]],
               "chat": self.out["chat"], "usage": self.usage}
        Path(path).write_text(json.dumps(s4.strip_signatures(doc), indent=1) + "\n")
        info(f"wrote {path}")

    def report(self):
        t_in, t_out = self.usage["tokens_in"], self.usage["tokens_out"]
        out_cost = t_out * s5.PRICE_OUT / 1e6
        low, high = t_in * s5.PRICE_READ / 1e6 + out_cost, t_in * s5.PRICE_WRITE / 1e6 + out_cost
        info("\n=== summary")
        for name, verdict, _ in self.results:
            info(f"  {verdict:4} {name}")
        if self.turns:
            info(f"  {self.turns} chat turn(s); usage {self.usage}; between US${low:.2f} (all "
                 f"input read from cache) and US${high:.2f} (all written to it) on Opus 5.5; the "
                 f"chat turn only, not the scan's plan and VLM calls")
        return 1 if any(v == "FAIL" for _, v, _ in self.results) else 0


def main():
    ap = argparse.ArgumentParser(description="S9 live checks: the demo garden's era connectors, a "
                                 "bed 3 plan citing the spray log, a chat citing #field-reports, "
                                 "the era guard, a real connector's disconnect.")
    ap.add_argument("--base-url", default="http://localhost:8080")
    ap.add_argument("--env-file", default="../infra/dev/.env",
                    help="holds CLERK_SECRET_KEY (sk_test_) of the Clerk development instance "
                         "the api trusts, also on the control plane; ERA_DEMO_GARDEN_ID when "
                         "not in the environment; --db-check reads SURREAL_*")
    ap.add_argument("--email", default=DEMO_EMAIL,
                    help=f"the Clerk test user; default the demo garden's owner, {DEMO_EMAIL}")
    ap.add_argument("--phone-suffix", type=int, default=101,
                    help="with another --email: its Clerk test phone number +1555555<0100-0199>, "
                         "if the instance asks for one")
    ap.add_argument("--garden-id", help="the demo garden; default ERA_DEMO_GARDEN_ID, else the "
                    f"user's garden named {GARDEN_NAME!r}")
    ap.add_argument("--sync-wait", type=int, default=0,
                    help="seconds the connectors check waits for the era connectors' first sync")
    ap.add_argument("--photo", help="whole-plant photo of the bed 3 scan (without --scan-id)")
    ap.add_argument("--leaf-photo", help="leaf close-up, uploaded as leaf_closeup (the disease head "
                    "reads only these)")
    ap.add_argument("--answers", default="*=unsure",
                    help="answers to the scan's indicator questions, as s4.py's: a comma list of "
                         "<indicator id or its suffix>=yes|no|unsure, * the default")
    ap.add_argument("--scan-id", help="a scan of bed 3 in the demo garden to use instead of "
                    "making one; the user must own it")
    ap.add_argument("--timeout", type=int, default=900,
                    help="seconds for the scan to reach plan_ready")
    ap.add_argument("--chat-question", default=DEFAULT_QUESTION)
    ap.add_argument("--turn-timeout", type=int, default=300, help="seconds for the chat turn")
    ap.add_argument("--connector-id", help="the disconnect check's real (vendor) connector, "
                    "active and synced; it is deleted")
    ap.add_argument("--delete-timeout", type=int, default=300,
                    help="seconds for the disconnect's revoke and record deletion to show")
    ap.add_argument("--db-check", action="store_true",
                    help="count the connector's farm_record rows in SurrealDB (dev compose)")
    ap.add_argument("--api-log", help="api's log file, read for the revoke line")
    ap.add_argument("--api-log-cmd", help="shell command printing api's log (instead of "
                    "--api-log), e.g. ssh <rig> '<compose> logs --no-color api'")
    ap.add_argument("--only", default=",".join(CHECKS),
                    help=f"comma-separated checks; they run in this order: {','.join(CHECKS)}")
    ap.add_argument("--out", help="write the results, the connectors, the scan and its events and "
                    "the chat turn to this JSON file (signed URLs' signatures removed)")
    ap.add_argument("--cleanup", action="store_true",
                    help="DELETE /scans/{id} for the scans the script made (only a pending scan "
                         "can be deleted, so a planned one stays)")
    ap.add_argument("--verbose", action="store_true", help="print every scan and chat event")
    args = ap.parse_args()
    args.only = {c.strip() for c in args.only.split(",") if c.strip()}
    unknown = args.only - set(CHECKS)
    if unknown:
        smoke.die(f"unknown checks: {sorted(unknown)}")
    if "plan" in args.only and not args.scan_id and not args.photo:
        smoke.die("the plan check needs --photo (or --scan-id)")
    for p in (args.photo, args.leaf_photo):
        if p and not Path(p).is_file():
            smoke.die(f"photo not found: {p}")
    if args.api_log and args.api_log_cmd:
        smoke.die("--api-log and --api-log-cmd: give one")
    if "plan" in args.only and args.photo and not args.leaf_photo and not args.scan_id:
        info("note: no --leaf-photo, so the disease head never runs (no leaf_closeup shot)")
    args.answers = s4.parse_answers(args.answers)

    s = S9(args)
    s.setup()
    try:
        for name in CHECKS:
            s.run_check(name, getattr(s, f"check_{name.replace('-', '_')}"))
    finally:
        if args.cleanup:
            s.cleanup()
        s.write_out()
    sys.exit(s.report())


if __name__ == "__main__":
    main()
