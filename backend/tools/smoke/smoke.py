#!/usr/bin/env python3
"""Smoke test for the PDM API: drives one scan through the API and asserts on its event stream.

Signs in with no frontend: a Clerk *test* instance's Backend API mints a session token for a
fixed test user (smoke+clerk_test@example.com). Then POST /gardens, POST /scans, an upload,
POST /submit, and GET /scans/{id}/events, parsed by hand. It asserts seq 1.. with no gaps and
data.seq == id, started stages completed, a resting point, and a resume with Last-Event-ID
(always, after --drop-after events). Optional modes: --kill-cmd / --restart-cmd (worker dies
mid-stage), --api-restart-cmd (api restarts during the stream), --face-photo (face blurring).

usage (from backend/):
    python3 tools/smoke/smoke.py --photo ../ml/some_leaf.jpg [--base-url http://localhost:8080]
        [--env-file ../infra/dev/.env] [--timeout 300] [--drop-after 3] [--verbose]
        [--kill-cmd CMD] [--restart-cmd CMD] [--api-restart-cmd CMD]
        [--face-photo PATH] [--out-dir DIR] [--allow-failed]

CLERK_SECRET_KEY comes from the environment or --env-file; it must start with sk_test_.
Neither the key nor a token is ever printed. Exit code 0 on pass, 1 on the first failed assert.
"""
import argparse
import http.client
import json
import os
import secrets
import socket
import string
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

CLERK_API = "https://api.clerk.com/v1"
TEST_EMAIL = "smoke+clerk_test@example.com"
TOKEN_MAX_AGE_S = 45
# Sign-up fields an instance may require. 555-0100 to 555-0199 are Clerk's test phone numbers,
# which never send an SMS.
TEST_USER_FIELDS = {"username": "pdm_smoke", "phone_number": ["+15555550100"]}
# Cloudflare in front of api.clerk.com refuses urllib's default User-Agent (error 1010).
USER_AGENT = "pdm-smoke/1"
RESTING_TYPES = {"plan_ready", "failed"}
PERCEPTION_STAGES = {"plant_check", "segmentation", "species", "disease", "image_embedding"}
LEASE_NOTE = (
    "contracts 5: a job lease lasts 2 minutes (heartbeats extend it); the broker requeues a job "
    "when its worker's heartbeat is older than 30 s or the lease expires"
)


def die(msg):
    print(f"FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


def info(msg):
    print(msg, flush=True)


def load_env(path, name):
    if os.environ.get(name):
        return os.environ[name]
    p = Path(path)
    if p.is_file():
        for line in p.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            k = k.strip()
            if k.startswith("export "):
                k = k[7:].strip()
            if k == name:
                return v.strip().strip('"').strip("'")
    return None


def http_json(method, url, headers=None, body=None, timeout=30):
    """Returns (status, headers, parsed-or-text). Never raises on an HTTP error status."""
    data = None
    h = {"User-Agent": USER_AGENT, **(headers or {})}
    if isinstance(body, (dict, list)):
        data = json.dumps(body).encode()
        h["Content-Type"] = "application/json"
    elif isinstance(body, bytes):
        data = body
    req = urllib.request.Request(url, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            status, rh, raw = r.status, r.headers, r.read()
    except urllib.error.HTTPError as e:
        status, rh, raw = e.code, e.headers, e.read()
    except (urllib.error.URLError, socket.timeout, ConnectionError) as e:
        die(f"{method} {url}: {e}")
    try:
        parsed = json.loads(raw) if raw else None
    except ValueError:
        parsed = raw.decode("utf-8", "replace")
    return status, rh, parsed


class Clerk:
    """Mints session tokens for the test user through Clerk's Backend API."""

    def __init__(self, secret):
        if not secret.startswith("sk_test_"):
            die("CLERK_SECRET_KEY must start with sk_test_ (never run against production)")
        self.auth = {"Authorization": f"Bearer {secret}"}
        self.session_id = None
        self._jwt = None
        self._at = 0.0

    def _call(self, method, path, body=None):
        status, _, out = http_json(method, CLERK_API + path, self.auth, body)
        if status >= 300:
            die(f"Clerk {method} {path} -> {status}: {json.dumps(out)[:400]}")
        return out

    def _ensure_session(self):
        if self.session_id:
            return
        users = self._call("GET", "/users?" + urllib.parse.urlencode({"email_address": TEST_EMAIL}))
        if users:
            user_id = users[0]["id"]
        else:
            alphabet = string.ascii_letters + string.digits
            pw = "".join(secrets.choice(alphabet) for _ in range(24))
            body = {"email_address": [TEST_EMAIL], "password": pw, "skip_password_checks": True}
            status, _, out = http_json("POST", CLERK_API + "/users", self.auth, body)
            if status == 422:
                # The instance may require more sign-up fields; Clerk names them in the error.
                missing = {p for e in out.get("errors", []) if e.get("code") == "form_data_missing"
                           for p in e.get("meta", {}).get("param_names", [])}
                for name in missing & TEST_USER_FIELDS.keys():
                    body[name] = TEST_USER_FIELDS[name]
                if missing:
                    info(f"instance requires {sorted(missing)}; retrying with test values")
                    status, _, out = http_json("POST", CLERK_API + "/users", self.auth, body)
            if status >= 300:
                die(f"Clerk POST /users -> {status}: {json.dumps(out)[:400]}")
            user_id = out["id"]
            info("created Clerk test user")
        self.session_id = self._call("POST", "/sessions", {"user_id": user_id})["id"]

    def token(self):
        """A session JWT at most 45 s old (tokens live 60 s)."""
        if self._jwt is None or time.monotonic() - self._at > TOKEN_MAX_AGE_S:
            self._ensure_session()
            out = self._call("POST", f"/sessions/{self.session_id}/tokens", {})
            self._jwt, self._at = out["jwt"], time.monotonic()
        return self._jwt


class Api:
    def __init__(self, base, clerk):
        self.base = base.rstrip("/")
        self.clerk = clerk

    def headers(self, extra=None):
        h = {"Authorization": f"Bearer {self.clerk.token()}"}
        h.update(extra or {})
        return h

    def call(self, method, path, body=None, expect=(200,), idem=False, raw=None, ctype=None):
        h = self.headers()
        if idem:
            h["Idempotency-Key"] = str(uuid.uuid4())
        if raw is not None:
            h["Content-Type"] = ctype
            body = raw
        status, _, out = http_json(method, self.base + path, h, body)
        if status not in expect:
            die(f"{method} {path}: expected {'/'.join(map(str, expect))}, got {status}: "
                f"{json.dumps(out)[:500]}")
        return out

    def healthy(self):
        try:
            with urllib.request.urlopen(self.base + "/healthz", timeout=3) as r:
                return r.status == 200
        except (urllib.error.URLError, socket.timeout, ConnectionError, OSError):
            return False

    def upload(self, scan_id, kind, path):
        p = Path(path)
        if not p.is_file():
            die(f"photo not found: {path}")
        ext = p.suffix.lower()
        ctype = {".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png",
                 ".webp": "image/webp", ".heic": "image/heic"}.get(ext)
        if not ctype:
            die(f"{path}: accepted media types are jpeg, png, webp and heic")
        boundary = "smoke" + uuid.uuid4().hex
        parts = [
            f'--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\n{kind}\r\n'.encode(),
            (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{p.name}"\r\n'
             f"Content-Type: {ctype}\r\n\r\n").encode() + p.read_bytes() + b"\r\n",
            f"--{boundary}--\r\n".encode(),
        ]
        return self.call("POST", f"/scans/{scan_id}/images", expect=(201,), idem=True,
                         raw=b"".join(parts), ctype=f"multipart/form-data; boundary={boundary}")

    def open_stream(self, scan_id, last_id, timeout=30):
        u = urllib.parse.urlparse(self.base)
        cls = http.client.HTTPSConnection if u.scheme == "https" else http.client.HTTPConnection
        conn = cls(u.hostname, u.port, timeout=timeout)
        h = self.headers({"Accept": "text/event-stream", "User-Agent": USER_AGENT})
        if last_id:
            h["Last-Event-ID"] = str(last_id)
        conn.request("GET", f"{u.path}/scans/{scan_id}/events", headers=h)
        resp = conn.getresponse()
        if resp.status != 200:
            body = resp.read().decode("utf-8", "replace")[:400]
            conn.close()
            die(f"GET /scans/{scan_id}/events (Last-Event-ID {last_id}): {resp.status}: {body}")
        return conn, resp


def sse_frames(resp):
    """Yields (id, event, data-text) per frame, and None per comment line (': keepalive'), so
    the caller can check its deadline while the stream is idle. Returns when the server closes
    the stream."""
    fid = ev = None
    data = []
    while True:
        line = resp.readline()
        if not line:
            return
        line = line.decode("utf-8").rstrip("\r\n")
        if line == "":
            if data or ev or fid:
                yield fid, ev, "\n".join(data)
            fid = ev = None
            data = []
        elif line.startswith(":"):
            yield None
        else:
            k, _, v = line.partition(":")
            v = v[1:] if v.startswith(" ") else v
            if k == "id":
                fid = v
            elif k == "event":
                ev = v
            elif k == "data":
                data.append(v)


class Follower:
    """Follows a scan's events across connections and keeps the asserts' state."""

    def __init__(self, api, scan_id, args):
        self.api, self.scan_id, self.args = api, scan_id, args
        self.last_seq = 0
        self.seen = set()
        self.types = []
        self.started = {}      # stage -> seq of stage_started
        self.completed = {}    # stage -> seq of the last stage_completed
        self.status = None
        self.killed_at = None  # seq of the event the kill came after
        self.kill_stage = None
        self.starts = {}       # stage -> seq of every stage_started (a requeued job starts again)
        self.kill_done = not args.kill_cmd
        self.resume_checked = False
        self.dropped = False
        self.resting = False
        self.deadline = time.monotonic() + args.timeout

    def handle(self, fid, ev, data, first_after_resume):
        try:
            d = json.loads(data)
        except ValueError:
            die(f"event data is not JSON: {data[:200]}")
        try:
            fid_n = int(fid)
        except (TypeError, ValueError):
            die(f"event id is not an integer: {fid!r}")
        seq = d.get("seq")
        if seq != fid_n:
            die(f"data.seq {seq!r} != id {fid_n}")
        if seq in self.seen:
            die(f"seq {seq} seen twice (last seq {self.last_seq})")
        if first_after_resume and seq != self.last_seq + 1:
            die(f"after resume with Last-Event-ID {self.last_seq}, first event was {seq}, "
                f"expected {self.last_seq + 1}")
        if seq != self.last_seq + 1:
            die(f"seq gap or reorder: got {seq} after {self.last_seq}")
        if d.get("type") != ev:
            die(f"seq {seq}: SSE event name {ev!r} != data.type {d.get('type')!r}")
        self.seen.add(seq)
        self.last_seq = seq
        t = d["type"]
        self.types.append(t)
        if self.args.verbose:
            info(f"  [{seq}] {t} {json.dumps({k: v for k, v in d.items() if k not in ('v', 'seq', 'scan_id', 'type')})[:300]}")
        if t == "stage_started":
            self.started[d["stage"]] = seq
            self.starts.setdefault(d["stage"], []).append(seq)
            if self.args.perception_only and not self.resting and d["stage"] == "context":
                # The perception job completed. Since roadmap gap 9 was fixed, `complete` sends
                # any stage events the worker couldn't deliver before this, so the open-stage
                # check below fails only on a regression.
                info("the context step started (--perception-only): stopping here")
                self.resting = True
            if not self.kill_done and d["stage"] in PERCEPTION_STAGES:
                self.run_kill(seq, d["stage"])
        elif t == "stage_completed":
            self.completed[d["stage"]] = seq
            if (self.args.perception_only and not self.resting
                    and PERCEPTION_STAGES <= self.completed.keys()):
                info("every perception stage completed (--perception-only): stopping here")
                self.resting = True
        elif t == "status_changed":
            self.status = d["to"]
        elif t == "failed":
            self.status = "failed"
        if t in RESTING_TYPES:
            self.resting = True

    def run_kill(self, seq, stage):
        self.kill_done = True
        self.killed_at, self.kill_stage = seq, stage
        info(f"stage_started {stage} at seq {seq}: running --kill-cmd")
        r = subprocess.run(self.args.kill_cmd, shell=True, timeout=120)
        if r.returncode != 0:
            die(f"--kill-cmd exited {r.returncode}: nothing was killed, the check would prove nothing")
        if self.args.restart_cmd:
            info("running --restart-cmd (detached)")
            subprocess.Popen(self.args.restart_cmd, shell=True, start_new_session=True,
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
        info(f"  {LEASE_NOTE}")

    def run(self):
        a = self.args
        reconnects = 0
        while True:
            if time.monotonic() > self.deadline:
                die(f"timeout after {a.timeout}s without a resting point (last seq {self.last_seq}, "
                    f"status {self.status}, events: {' '.join(self.types[-8:])})")
            try:
                conn, resp = self.api.open_stream(self.scan_id, self.last_seq)
            except (OSError, http.client.HTTPException) as e:
                if time.monotonic() > self.deadline:
                    die(f"cannot open the stream: {e}")
                time.sleep(2)
                continue
            first = self.last_seq > 0 and reconnects > 0
            drop_now = False
            try:
                for frame in sse_frames(resp):
                    if time.monotonic() > self.deadline:
                        die(f"timeout after {a.timeout}s without a resting point (last seq "
                            f"{self.last_seq}, status {self.status}, events: "
                            f"{' '.join(self.types[-8:])})")
                    if frame is None:
                        continue
                    self.handle(*frame, first)
                    first = False
                    if self.resting and a.perception_only:
                        break
                    if (not self.dropped and a.drop_after > 0 and len(self.seen) >= a.drop_after
                            and not self.resting):
                        drop_now = True
                        break
            except (OSError, http.client.HTTPException) as e:
                # a stall (no keepalive for 30 s) or a cut connection: resume
                info(f"stream interrupted ({type(e).__name__}); resuming from seq {self.last_seq}")
            finally:
                conn.close()
            reconnects += 1
            if drop_now:
                self.dropped = True
                info(f"dropped the stream after seq {self.last_seq}")
                if a.api_restart_cmd:
                    self.restart_api()
                else:
                    time.sleep(1)
                self.resume_checked = True
                continue
            if self.resting:
                return
            time.sleep(1)  # closed without a resting point: reconnect and keep following

    def restart_api(self):
        info("running --api-restart-cmd")
        r = subprocess.run(self.args.api_restart_cmd, shell=True, timeout=300)
        if r.returncode != 0:
            die(f"--api-restart-cmd exited {r.returncode}")
        t0 = time.monotonic()
        while not self.api.healthy():
            if time.monotonic() - t0 > 120:
                die("api did not answer GET /healthz with 200 within 120 s of the restart")
            time.sleep(1)
        info(f"api healthy again after {time.monotonic() - t0:.0f}s; resuming")


def face_step(api, args, garden_id, out_dir):
    scan = api.call("POST", "/scans", {"garden_id": garden_id, "new_plant": {"name": "smoke face"}},
                    expect=(201,), idem=True)
    res = api.upload(scan["id"], "whole_plant", args.face_photo)
    img = res.get("image", {})
    info(f"face photo stored: image {img.get('id')} {img.get('width')}x{img.get('height')}, "
         f"quality {res.get('quality')}")
    faces = {k: v for k, v in res.items() if "face" in k.lower()}
    faces.update({k: v for k, v in img.items() if "face" in k.lower()})
    if faces:
        info(f"faces reported: {faces}")
    url = img.get("url")
    if not url:
        info("no image url in the upload response; nothing to download")
        return
    url = urllib.parse.urljoin(api.base + "/", url)
    status, hdr, body = None, None, None
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            status, body = r.status, r.read()
    except urllib.error.HTTPError as e:
        die(f"download of the stored face image: {e.code}")
    if status != 200 or not body:
        die(f"download of the stored face image: status {status}")
    out = Path(out_dir) / f"face-{img.get('id', 'image')}.webp"
    out.write_bytes(body)
    info(f"stored (blurred) image saved to {out}: check the face by eye")


def main():
    ap = argparse.ArgumentParser(description="Smoke test: one scan through the PDM API.")
    ap.add_argument("--base-url", default="http://localhost:8080")
    ap.add_argument("--photo", required=True, help="plant photo (jpeg, png, webp or heic)")
    ap.add_argument("--env-file", default="../infra/dev/.env")
    ap.add_argument("--lat", type=float, default=43.65)
    ap.add_argument("--lon", type=float, default=-79.38)
    ap.add_argument("--timeout", type=int, default=None,
                    help="seconds to reach a resting point (default 300; 600 with --kill-cmd)")
    ap.add_argument("--drop-after", type=int, default=3,
                    help="drop the stream after N events and resume (0 turns the resume check off)")
    ap.add_argument("--kill-cmd", help="shell command run right after the first perception stage_started")
    ap.add_argument("--restart-cmd", help="shell command run (detached) right after the kill")
    ap.add_argument("--api-restart-cmd", help="shell command that restarts api, run while the stream is dropped")
    ap.add_argument("--face-photo", help="extra step: upload a photo with a face to a second scan")
    ap.add_argument("--out-dir", help="where the stored face image is saved (default a temp dir)")
    ap.add_argument("--allow-failed", action="store_true", help="exit 0 when the scan ends failed")
    ap.add_argument("--perception-only", action="store_true",
                    help="pass once every perception stage completed, without waiting for a resting "
                         "point (no VLM or fallback worker running yet)")
    ap.add_argument("--verbose", action="store_true", help="print every event")
    args = ap.parse_args()
    if args.timeout is None:
        args.timeout = 600 if args.kill_cmd else 300

    secret = load_env(args.env_file, "CLERK_SECRET_KEY")
    if not secret:
        die(f"CLERK_SECRET_KEY not in the environment or {args.env_file}")
    api = Api(args.base_url, Clerk(secret))
    if not Path(args.photo).is_file():
        die(f"photo not found: {args.photo}")
    if args.kill_cmd:
        info(LEASE_NOTE)
    if not api.healthy():
        die(f"{args.base_url}/healthz did not answer 200")

    stamp = time.strftime("%Y-%m-%d %H:%M:%S")
    garden = api.call("POST", "/gardens", {"name": f"smoke {stamp}",
                      "location": {"lat": args.lat, "lon": args.lon}}, expect=(201,), idem=True)
    info(f"garden {garden['id']}")
    scan = api.call("POST", "/scans", {"garden_id": garden["id"], "new_plant": {"name": "smoke plant"}},
                    expect=(201,), idem=True)
    sid = scan["id"]
    info(f"scan {sid}")
    up = api.upload(sid, "whole_plant", args.photo)
    info(f"uploaded {args.photo}: quality {up.get('quality')}")
    api.call("POST", f"/scans/{sid}/submit", expect=(202,))
    info("submitted; following events")

    f = Follower(api, sid, args)
    f.run()

    # asserts on the whole run
    open_stages = sorted(s for s in f.started if s not in f.completed
                         and (s in PERCEPTION_STAGES or not args.perception_only))
    final = f.status
    if f.seen != set(range(1, f.last_seq + 1)):
        die("seq numbers are not 1..N")
    if open_stages:
        msg = f"stages started but never completed: {open_stages}"
        if final == "failed":
            info(f"warning (scan failed): {msg}")
        else:
            die(msg)
    if args.drop_after > 0 and not f.dropped:
        info(f"note: the stream closed before {args.drop_after} events, so the resume check did not run")
    if args.kill_cmd:
        if f.killed_at is None:
            die("--kill-cmd never ran: no perception stage_started was seen")
        # A stage_completed after the kill proves nothing on its own: the worker may have finished
        # the stage before the kill landed. The job is requeued only if the stage starts again.
        again = [q for q in f.starts.get(f.kill_stage, []) if q > f.killed_at]
        if not again:
            die(f"stage {f.kill_stage} never started again after the kill at seq {f.killed_at}: the "
                f"job was not requeued (the kill may have landed after the stage finished; give the "
                f"stub a longer STUB_STAGE_DELAY_MS)")
        done = f.completed.get(f.kill_stage)
        if done is None or done <= f.killed_at:
            die(f"stage {f.kill_stage} did not complete after the kill (started at seq {f.killed_at}, "
                f"completed at {done})")
        info(f"kill check: {f.kill_stage} started at seq {f.killed_at}, started again at seq "
             f"{again[0]} after the requeue, and completed at seq {done}")
    if final == "failed" and not args.allow_failed:
        die("the scan ended failed (use --allow-failed if that is expected)")

    if args.face_photo:
        out_dir = args.out_dir or tempfile.mkdtemp(prefix="pdm-smoke-")
        Path(out_dir).mkdir(parents=True, exist_ok=True)
        face_step(api, args, garden["id"], out_dir)

    info("")
    info("PASS")
    info(f"  last status: {final}; events: {f.last_seq} (seq 1..{f.last_seq}, no gaps, no repeats)")
    counts = {}
    for t in f.types:
        counts[t] = counts.get(t, 0) + 1
    info("  types: " + ", ".join(f"{k} x{v}" for k, v in counts.items()))
    if f.dropped:
        info(f"  resume: dropped after {args.drop_after}, Last-Event-ID resume gave seq+1 and no repeats"
             + ("; api restarted in between" if args.api_restart_cmd else ""))


if __name__ == "__main__":
    main()
