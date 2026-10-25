#!/usr/bin/env python3
"""Live checks for backend slice S6 (background work and resilience), in s4.py's and s5.py's style.

Signs in the way smoke.py does (a Clerk test instance's Backend API mints session tokens), as
fresh test users: --email (default s6-<timestamp>+clerk_test@example.com) and, for the checks
that need more than one, the same address with a tag (s6-<timestamp>-b+clerk_test@...). Each
check makes its own `s6 <check> …` gardens. Prints PASS, FAIL or SKIP with the evidence for each
check, and exits 1 if any check failed. SKIP means the check couldn't prove its claim on this run
(an option missing, or the case didn't come up), never a pass.

Checks, in this order (--only picks some); backend roadmap 10's exit checks:

  tokens       POST /me/push-tokens: a well-formed ios token 204 twice, an unknown field 400, a
               bad platform 400, a malformed ios token 400; DELETE (URL-encoded) 204, an unknown
               token 204; with --db-check the push_token row appears and goes
  prune        A deletes a garden B is an editor of: the garden_deleted push reaches Expo, which
               answers DeviceNotRegistered for B's unknown ios token, and the row goes (DB, 60 s);
               B's web token stays. --real-token adds a real device's token (prune-real)
  alerts       two phases (--phase seed, then check after hourly sweeps): one frost alert per
               frost spell, never one per sweep; dismissed stays dismissed; DELETE is 409
  plant        DELETE /plants/{id} 202: the plant, its scan, image, conversation, chat image and
               every row naming them gone (DB), and their prefixes empty (RustFS)
  garden       DELETE /gardens/{id} 202: gardens/{G}/ empty and no row names the garden
  account      DELETE /me 202: G1 deleted, G2 transferred to B; the tombstone finishes; no user
               row nor any row naming the user, nothing of G1, no object of G1 or of A's chat in
               G2; Clerk's user 404; the old (still unexpired) token 401
  restart      the same, with RustFS stopped during the cascade: the failure is logged, the
               tombstone stays unfinished, G1 stays deleting, the Clerk user stays, the token is
               already 401; RustFS back, then the retry (or an api restart) finishes it
  orphans      two phases (--phase seed, [--phase fresh], --phase check after the daily sweep):
               stray objects under a real scan, an unknown scan, an unknown conversation and an
               unknown garden are gone; a fresh stray, every referenced key and knowledge/ stay
  rig-failure  an early-blight scan; on the first perception stage_started (--kill-at) the rig
               is killed: the fallback leases the job within 51 s of the dead worker's last
               heartbeat, the scan plans on the fallback alone, the queue drains, and a second
               scan submitted while the rig is down isn't refused with 503

Host access for what the API can't show (the control-plane host only; read from the files, never
printed): --db-check reads SurrealDB's HTTP /sql with the root user of --db-env; --rustfs-check
lists, heads and (orphans only, under gardens/) puts objects with --rustfs-env's keys;
--api-log-cmd prints api's log. Neither a key, a token nor a signed URL is ever printed.

usage (from backend/; on the control-plane host):
    python3 tools/smoke/s6.py [--base-url URL] [--env-file /srv/pdm/env/.env.api]
        [--email s6-…+clerk_test@example.com] [--phone-suffix N] [--only tokens,prune,...]
        [--phase seed|check|fresh] [--state s6-state.json] [--photo PLANT.jpg --leaf-photo LEAF.jpg]
        [--db-check [--db-env FILE] [--db-url URL]] [--rustfs-check [--rustfs-env FILE] [--rustfs-url URL]]
        [--api-log-cmd CMD] [--api-restart-cmd CMD] [--rustfs-stop-cmd CMD --rustfs-start-cmd CMD]
        [--rig-kill-cmd CMD --rig-start-cmd CMD] [--manual-kill] [--kill-at perception|vlm]
        [--answers concentric_rings_bullseye=yes,*=unsure] [--real-token TOKEN] [--out FILE]
        [--cleanup] [--verbose]

Exit code 0 when no check failed.
"""
import argparse
import base64
import datetime as _dt
import hashlib
import hmac
import json
import os
import re
import secrets
import string
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import smoke  # noqa: E402  (Clerk, Api, http_json, load_env, die, USER_AGENT, PERCEPTION_STAGES)
import s4  # noqa: E402  (Stream, trace, strip_signatures, parse_ts, parse_answers, answer_for)
import s5  # noqa: E402  (run_check, read_env_file, upload_photos)
from s5 import Fail, Skip, info, need  # noqa: E402

DEFAULT_BASE = "https://pdm-cp.tailb3ce07.ts.net"
CHECKS = ("tokens", "prune", "alerts", "plant", "garden", "account", "restart", "orphans",
          "rig-failure")
PHASED = ("alerts", "orphans")
SCHEMA = Path(__file__).resolve().parents[2] / "migrations" / "schema.surql"

# --- api's log lines: S6's messages, as `api` logs them. A pattern matches the line's message
# (JSON lines, a compose prefix allowed) or, for LOG_FORMAT=pretty, the whole line, so none is
# anchored; `account deleted` skips the quoted `error="account deleted"` of a refused request.
PUSH_PRUNED_LOG = re.compile(r"\bpush sent\b")                                    # api: Pusher::send
WEATHER_RUN_LOG = re.compile(r"\bweather sweep\b")                                # api: one line a weather run
ORPHAN_RUN_LOG = re.compile(r"\borphan sweep finished\b")                         # api: one line a sweep
DELETION_FAILED_LOG = re.compile(r"\bdeletion (?:failed; retrying|failed after its retries|stopped by shutdown)")
ACCOUNT_DELETED_LOG = re.compile(r"(?<![\"=])\baccount deleted\b(?!\")")          # api: after Clerk confirms
LEASE_LOG = re.compile(r"\bjob leased\b")                                         # api: broker, with worker_id

# --- timings (contracts 5, api scheduler)
STALE_S = 30            # a worker's heartbeat older than this: its jobs are requeued
REQUEUE_TICK_S = 5      # the `requeue` task's period
FALLBACK_DELAY_S = 15   # a job unclaimed this long after queued_at is the fallback's
POLL_SLACK_S = 1        # the fallback's long poll and the status poll
TAKEOVER_BOUND_S = STALE_S + REQUEUE_TICK_S + FALLBACK_DELAY_S + POLL_SLACK_S   # 51
TAKEOVER_TARGET_S = 45  # architecture 14: "within about 45 seconds" of the kill
WEATHER_EVERY_S = 3600  # the `weather` task: hourly, the first run 30 s after api starts
WEATHER_FIRST_S = 30
ORPHANS_FIRST_S = 3600  # the `orphans` task: daily, the first run 1 h after api starts
ORPHAN_MIN_AGE_S = 3600  # the sweep deletes only objects older than this
HORIZON_DAYS = 3        # forecast alerts look at the next 3 days (scheduler/triggers.rs)
FROST_TITLE = "Frost expected"
STATUS_POLL_S = 0.5
DEFAULT_DEAD_WORKER = {"perception": "rig-3070-perception-1", "vlm": "rig-5070ti-vlm-1"}
KILL_STAGES = {"perception": smoke.PERCEPTION_STAGES, "vlm": {"vlm_findings", "vlm_indicators"}}
# Observation sources by model: the rig's perception models, the rig's VLM, the hosted fallback.
RIG_PERCEPTION_SOURCES = {"segmentation", "bioclip", "classifier"}
RIG_VLM_SOURCES = {"vlm"}
FALLBACK_SOURCE = "llm_fallback"
EXPO_ALPHABET = string.ascii_letters + string.digits
CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
# A few bytes with WebP's RIFF header: the strays are never decoded, only listed and deleted.
STRAY_BYTES = b"RIFF\x0c\x00\x00\x00WEBPVP8 s6 stray"
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()


# --- pure helpers (unit-tested offline) -------------------------------------------------------

def new_ulid(ms=None, rand=None):
    """A ULID: 48 bits of Unix milliseconds, 80 random bits, Crockford base32, upper case."""
    ms = int(time.time() * 1000) if ms is None else ms
    rand = int.from_bytes(os.urandom(10), "big") if rand is None else rand
    n = (ms << 80) | rand
    return "".join(CROCKFORD[(n >> (5 * i)) & 31] for i in reversed(range(26)))


def ulid_ms(ulid):
    n = 0
    for ch in ulid[:10]:
        n = n * 32 + CROCKFORD.index(ch)
    return n


def expo_token(tag="s6"):
    """A well-formed Expo push token no device holds: Expo answers DeviceNotRegistered for it."""
    return f"ExponentPushToken[{tag}{''.join(secrets.choice(EXPO_ALPHABET) for _ in range(20))}]"


def token_path(token):
    """DELETE /me/push-tokens/{token}: the brackets URL-encoded (contracts 9)."""
    return "/me/push-tokens/" + urllib.parse.quote(token, safe="")


def jwt_exp(jwt):
    """The `exp` claim of a JWT (seconds since the epoch), read without verifying it."""
    payload = jwt.split(".")[1]
    payload += "=" * (-len(payload) % 4)
    return json.loads(base64.urlsafe_b64decode(payload))["exp"]


def to_epoch(text):
    """An RFC 3339 timestamp as epoch seconds, or None."""
    if not text:
        return None
    try:
        return s4.parse_ts(str(text)).timestamp()
    except (Fail, ValueError):
        return None


def iso(epoch):
    return datetime.fromtimestamp(epoch, timezone.utc).isoformat(timespec="milliseconds")


def spells(dates):
    """Runs of consecutive local dates: ["2026-10-24", "2026-10-25", "2026-10-27"] gives
    [["2026-10-24", "2026-10-25"], ["2026-10-27"]]. One alert per spell (S6's decision)."""
    days = sorted({_dt.date.fromisoformat(d) for d in dates})
    out = []
    for d in days:
        if out and (d - _dt.date.fromisoformat(out[-1][-1])).days == 1:
            out[-1].append(d.isoformat())
        else:
            out.append([d.isoformat()])
    return out


def close_pairs(alerts):
    """Pairs of alerts of the same trigger kind on the same or adjacent days: [(a, b)], each a
    dict with `kind` and `due`. Under one alert per spell there are none."""
    by_kind = {}
    for a in alerts:
        by_kind.setdefault(a["kind"], []).append(a)
    out = []
    for kind_alerts in by_kind.values():
        kind_alerts = sorted(kind_alerts, key=lambda a: a["due"])
        for a, b in zip(kind_alerts, kind_alerts[1:]):
            gap = (_dt.date.fromisoformat(b["due"]) - _dt.date.fromisoformat(a["due"])).days
            if gap <= 1:
                out.append((a, b))
    return out


def guaranteed_sweeps(elapsed_s, restarted):
    """Weather sweeps certain to have run `elapsed_s` after the seed: after a restart at the
    seed, one at +30 s and then hourly; otherwise one per full hour (api's start is unknown)."""
    if elapsed_s < 0:
        return 0
    if restarted:
        return 0 if elapsed_s < WEATHER_FIRST_S else 1 + int((elapsed_s - WEATHER_FIRST_S) // WEATHER_EVERY_S)
    return int(elapsed_s // WEATHER_EVERY_S)


def count_runs(stamps, gap_s=300):
    """Log lines of one scheduled run come together: stamps more than `gap_s` apart are runs."""
    stamps = sorted(stamps)
    return sum(1 for i, t in enumerate(stamps) if i == 0 or t - stamps[i - 1] > gap_s)


def takeover(last_heartbeat, lease_at, kill_at):
    """The rig-failure check's numbers: the fallback's lease against the dead worker's last
    heartbeat (the bound: 30 s stale + 5 s requeue tick + 15 s fallback delay + 1 s poll) and
    against the kill (architecture 14's 45 s target, reported)."""
    after_hb = lease_at - last_heartbeat
    return {"after_last_heartbeat_s": round(after_hb, 1),
            "after_kill_s": round(lease_at - kill_at, 1),
            "bound_s": TAKEOVER_BOUND_S, "target_s": TAKEOVER_TARGET_S,
            "within_bound": after_hb <= TAKEOVER_BOUND_S,
            "within_target": lease_at - kill_at <= TAKEOVER_TARGET_S}


def infer_lease(samples, after):
    """From /status samples [(t, status)]: the first poll after `after` at which `leased` rose
    and `queued` fell, i.e. a queued job was leased (by the fallback, while the rig is dead).
    The lease happened between that poll and the one before it."""
    prev = None
    for t, st in samples:
        q = (st or {}).get("queue") or {}
        if prev is not None and t > after:
            pq = (prev or {}).get("queue") or {}
            if q.get("leased", 0) > pq.get("leased", 0) and q.get("queued", 0) < pq.get("queued", 0):
                return t
        prev = st
    return None


def last_heartbeat(samples, worker):
    """The newest `last_heartbeat` /status showed for `worker`, as epoch seconds, or None."""
    best = None
    for _, st in samples:
        for w in (st or {}).get("workers", []):
            if w.get("worker_id") == worker:
                t = to_epoch(w.get("last_heartbeat"))
                if t is not None and (best is None or t > best):
                    best = t
    return best


def _strip_comments(text):
    return "\n".join(line.split("--", 1)[0] for line in text.splitlines())


def _split_top(text):
    """`a, b<c, d>, e` split on the commas outside angle and square brackets."""
    out, depth, cur = [], 0, ""
    for ch in text:
        if ch in "<[(":
            depth += 1
        elif ch in ">])":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def schema_refs(text, target):
    """Every field of schema.surql that links to a `target` record: [(table, condition)], the
    condition with `$r` for the record. `id` arrays give `record::id(id)[i] = $r`, arrays of
    links `$r IN field`, anything else `field = $r` (the store's own query forms)."""
    out = []
    pattern = re.compile(r"DEFINE FIELD (?:OVERWRITE |IF NOT EXISTS )?([\w.]+)\s+ON (?:TABLE )?(\w+)"
                         r"\s+TYPE\s+(.*?)(?=\s+(?:REFERENCE|DEFAULT|VALUE|ASSERT|READONLY|"
                         r"PERMISSIONS|COMMENT|FLEXIBLE)\b|;)", re.S)
    link = re.compile(rf"record<[^>]*\b{re.escape(target)}\b[^>]*>")
    for field, table, ftype in pattern.findall(_strip_comments(text)):
        ftype = " ".join(ftype.split())
        if not link.search(ftype) or "." in field:
            continue
        if field == "id" and ftype.startswith("["):
            for i, part in enumerate(_split_top(ftype[1:-1])):
                if link.search(part):
                    out.append((table, f"record::id(id)[{i}] = $r"))
        elif re.search(rf"array<\s*(?:option<\s*)?record<[^>]*\b{re.escape(target)}\b", ftype):
            out.append((table, f"$r IN {field}"))
        else:
            out.append((table, f"{field} = $r"))
    return out


def rid(table, key):
    """A record literal: a ULID key in backticks, an array key as JSON strings."""
    if isinstance(key, list):
        return f"{table}:[{', '.join(json.dumps(k) for k in key)}]"
    return f"{table}:`{key}`"


def log_entries(text):
    """api's log as [{msg, level, ts, fields}]: the JSON lines api writes by default
    (flatten_event; a compose or docker prefix allowed) and, for LOG_FORMAT=pretty, the line."""
    out = []
    for line in text.splitlines():
        obj, start = None, line.find("{")
        if start >= 0:
            try:
                obj = json.loads(line[start:])
            except ValueError:
                obj = None
        if isinstance(obj, dict):
            fields = obj.get("fields") if isinstance(obj.get("fields"), dict) else {}
            merged = {**obj, **fields}
            msg = str(fields.get("message") or obj.get("message") or "")
            level = str(obj.get("level") or "").upper()
            ts = to_epoch(obj.get("timestamp"))
        else:
            merged, msg = {}, line
            m = re.search(r"\b(TRACE|DEBUG|INFO|WARN|ERROR)\b", line)
            level = m.group(1) if m else ""
            m = re.search(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)", line)
            ts = to_epoch(m.group(0)) if m else None
        out.append({"msg": msg, "level": level, "ts": ts, "fields": merged})
    return out


def find_log(entries, pattern, after=None, info_only=False):
    return [e for e in entries if pattern.search(e["msg"])
            and (not info_only or e["level"] in ("INFO", ""))
            and (after is None or e["ts"] is None or e["ts"] >= after)]


def short(e):
    keep = {k: v for k, v in e["fields"].items() if k not in ("message", "timestamp", "level",
                                                              "fields", "target", "span", "spans")}
    when = iso(e["ts"]) if e["ts"] else "?"
    return f"{when} {e['level']} {e['msg'][:120]!r} {json.dumps(keep)[:200]}"


# --- RustFS: a stdlib S3 client (SigV4, path-style), from ml/knowledge/upload.py --------------

def _hmac(key, msg):
    return hmac.new(key, msg.encode(), hashlib.sha256).digest()


def sigv4(method, path, query, headers, payload_hash, amz_date, access_key, secret_key, region):
    """The Authorization header of AWS Signature Version 4 for S3 (ml/knowledge/upload.py's
    `sign`). `headers` are lower-case and include host, x-amz-content-sha256 and x-amz-date; all
    are signed. `path` and `query` are already URI-encoded and canonical."""
    names = sorted(headers)
    canonical = "\n".join([method, path, query, *(f"{k}:{headers[k].strip()}" for k in names),
                           "", ";".join(names), payload_hash])
    scope = f"{amz_date[:8]}/{region}/s3/aws4_request"
    to_sign = "\n".join(["AWS4-HMAC-SHA256", amz_date, scope,
                         hashlib.sha256(canonical.encode()).hexdigest()])
    key = _hmac(("AWS4" + secret_key).encode(), amz_date[:8])
    for part in (region, "s3", "aws4_request"):
        key = _hmac(key, part)
    signature = hmac.new(key, to_sign.encode(), hashlib.sha256).hexdigest()
    return (f"AWS4-HMAC-SHA256 Credential={access_key}/{scope}, "
            f"SignedHeaders={';'.join(names)}, Signature={signature}")


def canonical_query(query):
    return "&".join(f"{urllib.parse.quote(k, safe='-_.~')}={urllib.parse.quote(v, safe='-_.~')}"
                    for k, v in sorted((query or {}).items()))


class S3:
    NS = "{http://s3.amazonaws.com/doc/2006-03-01/}"

    def __init__(self, endpoint, bucket, access_key, secret_key, region="us-east-1", timeout=30.0):
        u = urllib.parse.urlsplit(endpoint)
        if u.scheme not in ("http", "https") or not u.netloc or u.path not in ("", "/"):
            smoke.die(f"--rustfs-url {endpoint!r} isn't http(s)://host:port")
        self.base, self.host = f"{u.scheme}://{u.netloc}", u.netloc
        self.bucket, self.region, self.timeout = bucket, region, timeout
        self._ak, self._sk = access_key, secret_key

    def request(self, method, key=None, query=None, body=b"", headers=None):
        """(status, headers, body). Retries a connection error or a 5xx twice."""
        path = "/" + self.bucket + ("/" + urllib.parse.quote(key, safe="/-_.~") if key else "")
        q = canonical_query(query)
        for attempt in range(1, 4):
            amz_date = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
            payload = hashlib.sha256(body).hexdigest()
            hdrs = {"host": self.host, "x-amz-content-sha256": payload, "x-amz-date": amz_date,
                    **{k.lower(): v for k, v in (headers or {}).items()}}
            req = urllib.request.Request(self.base + path + (f"?{q}" if q else ""),
                                         data=body if method == "PUT" else None, method=method)
            for k, v in hdrs.items():
                if k != "host":
                    req.add_header(k, v)
            req.add_header("Authorization", sigv4(method, path, q, hdrs, payload, amz_date,
                                                  self._ak, self._sk, self.region))
            try:
                with urllib.request.urlopen(req, timeout=self.timeout) as r:
                    return r.status, r.headers, r.read()
            except urllib.error.HTTPError as e:
                if e.code < 500 or attempt == 3:
                    return e.code, e.headers, e.read()
            except (urllib.error.URLError, OSError) as e:
                if attempt == 3:
                    raise Fail(f"RustFS {method} {path}: {getattr(e, 'reason', e)}") from e
            time.sleep(attempt)
        raise AssertionError("unreachable")

    def list(self, prefix):
        """Every object under `prefix`: [{key, last_modified, etag, size}] (ListObjectsV2)."""
        out, token = [], None
        while True:
            query = {"list-type": "2", "prefix": prefix}
            if token:
                query["continuation-token"] = token
            status, _, body = self.request("GET", query=query)
            if status != 200:
                code = re.search(rb"<Code>([^<]*)</Code>", body or b"")
                raise Fail(f"RustFS list {prefix}: HTTP {status}"
                           + (f" {code.group(1).decode(errors='replace')}" if code else ""))
            root = ET.fromstring(body)
            for c in root.findall(f"{self.NS}Contents"):
                out.append({"key": c.findtext(f"{self.NS}Key"),
                            "last_modified": c.findtext(f"{self.NS}LastModified"),
                            "etag": c.findtext(f"{self.NS}ETag"),
                            "size": int(c.findtext(f"{self.NS}Size") or 0)})
            if root.findtext(f"{self.NS}IsTruncated") != "true":
                return out
            token = root.findtext(f"{self.NS}NextContinuationToken")

    def head(self, key):
        return self.request("HEAD", key)[0]

    def put(self, key, data, content_type="image/webp"):
        """Only under gardens/: the script never writes elsewhere (knowledge/ is the loader's)."""
        if not key.startswith("gardens/") or ".." in key:
            raise Fail(f"refusing to write {key!r}: strays go under gardens/ only")
        status, _, body = self.request("PUT", key, body=data, headers={"content-type": content_type})
        need(status == 200, f"RustFS put {key}: HTTP {status}")


# --- SurrealDB over HTTP ----------------------------------------------------------------------

class Db:
    def __init__(self, url, user, password, ns="pdm", db="main"):
        self.url = re.sub(r"^ws", "http", url).rstrip("/") + "/sql"
        self.auth = base64.b64encode(f"{user}:{password}".encode()).decode()
        self.ns, self.db = ns, db

    def run(self, sql):
        """Each statement's result, in order; a failed statement fails the check."""
        try:
            status, _, out = smoke.http_json(
                "POST", self.url, {"Authorization": f"Basic {self.auth}", "Accept": "application/json",
                                   "surreal-ns": self.ns, "surreal-db": self.db}, sql.encode(), timeout=60)
        except SystemExit as e:
            raise Fail("SurrealDB unreachable (see stderr)") from e
        if status != 200 or not isinstance(out, list):
            raise Fail(f"SurrealDB {status}: {json.dumps(out)[:200]}")
        bad = [r for r in out if r.get("status") != "OK"]
        if bad:
            raise Fail(f"SurrealDB: {json.dumps(bad[0])[:300]}")
        return [r.get("result") for r in out]

    def one(self, sql):
        return self.run(sql)[-1]


# --- Clerk: fresh test users, long-lived tokens, the user's existence --------------------------

def phone_number(suffix):
    """Clerk's test numbers +1 555 555 0100 to 0199, which never send an SMS."""
    return f"+1555555{100 + (suffix - 100) % 100:04d}"


class Clerk(smoke.Clerk):
    def __init__(self, secret, email, phone_suffix):
        local = re.sub(r"[^a-z0-9]+", "_", email.split("@")[0].split("+")[0].lower()).strip("_")
        super().__init__(secret, email, {"username": f"pdm_{local}"[:60],
                                         "phone_number": [phone_number(phone_suffix)]})
        self.phone_suffix = phone_suffix
        self.user_id = None

    def _ensure_session(self):
        if self.session_id:
            return
        users = self._call("GET", "/users?" + urllib.parse.urlencode({"email_address": self.email}))
        self.user_id = users[0]["id"] if users else self._create()
        self.session_id = self._call("POST", "/sessions", {"user_id": self.user_id})["id"]

    def _create(self):
        pw = "".join(secrets.choice(string.ascii_letters + string.digits) for _ in range(24))
        body = {"email_address": [self.email], "password": pw, "skip_password_checks": True}
        for _ in range(60):
            status, _, out = smoke.http_json("POST", smoke.CLERK_API + "/users", self.auth, body)
            if status < 300:
                info(f"created Clerk test user {self.email}")
                return out["id"]
            errors = out.get("errors", []) if isinstance(out, dict) else []
            missing = {p for e in errors if e.get("code") == "form_data_missing"
                       for p in e.get("meta", {}).get("param_names", [])}
            taken = {e.get("meta", {}).get("param_name") for e in errors
                     if e.get("code") != "form_data_missing"}
            if status == 422 and missing:
                unknown = missing - self.user_fields.keys()
                if unknown:
                    smoke.die(f"Clerk requires {sorted(unknown)}, which the script can't fill")
                for name in missing:
                    body[name] = self.user_fields[name]
            elif status == 422 and "phone_number" in taken:
                self.phone_suffix += 1      # a test number another user holds: the next one
                body["phone_number"] = [phone_number(self.phone_suffix)]
            elif status == 422 and "username" in taken:
                body["username"] = f"{self.user_fields['username'][:50]}_{secrets.token_hex(3)}"
            else:
                smoke.die(f"Clerk POST /users -> {status}: {json.dumps(out)[:400]}")
        smoke.die("Clerk POST /users: no free test phone number after 60 tries")

    def long_token(self, seconds):
        """A session JWT that lives `seconds` (Clerk's expires_in_seconds), not cached."""
        self._ensure_session()
        return self._call("POST", f"/sessions/{self.session_id}/tokens",
                          {"expires_in_seconds": seconds})["jwt"]

    def user_status(self):
        """GET /v1/users/{id}: 200 while the user exists, 404 once deleted. Never creates one."""
        if self.user_id is None:
            raise Fail("the Clerk user's id isn't known yet")
        status, _, _ = smoke.http_json("GET", f"{smoke.CLERK_API}/users/{self.user_id}", self.auth)
        return status


class User:
    def __init__(self, secret, base, email, phone_suffix):
        self.email = email
        self.clerk = Clerk(secret, email, phone_suffix)
        self.api = smoke.Api(base, self.clerk)
        self._id = None
        self.deleted = False

    @property
    def id(self):
        if self._id is None:
            self._id = self.api.call("GET", "/me")["id"]
            self.clerk._ensure_session()
        return self._id


class StatusPoller(threading.Thread):
    """GET /status (no sign-in) every 0.5 s, kept as (epoch, status) samples."""

    def __init__(self, base):
        super().__init__(daemon=True)
        self.url = base.rstrip("/") + "/status"
        self.samples = []
        self.stop = threading.Event()

    def run(self):
        while not self.stop.is_set():
            t = time.time()
            try:
                req = urllib.request.Request(self.url, headers={"User-Agent": smoke.USER_AGENT})
                with urllib.request.urlopen(req, timeout=5) as r:
                    self.samples.append((t, json.loads(r.read())))
            except (urllib.error.URLError, OSError, ValueError):
                self.samples.append((t, None))
            self.stop.wait(STATUS_POLL_S)

    def changes(self):
        """The samples where the status changed, for the fixture."""
        out, prev = [], object()
        for t, st in self.samples:
            if st != prev:
                out.append({"at": iso(t), "status": st})
                prev = st
        return out


# --- the checks -------------------------------------------------------------------------------

class S6:
    run_check = s5.S5.run_check  # PASS, FAIL or SKIP with the evidence

    def __init__(self, args):
        self.args = args
        self.secret = smoke.load_env(args.env_file, "CLERK_SECRET_KEY")
        if not self.secret:
            smoke.die(f"CLERK_SECRET_KEY not in the environment or {args.env_file}")
        self.results = []
        self.fixture = {}            # check -> data for --out
        self.made = []               # (user, garden) the script made, for --cleanup
        self.users = []              # helper users the script made, for --cleanup
        self.state = self.load_state()
        self.next_phone = args.phone_suffix
        main = self.state.get("user") if args.phase != "seed" else None
        if main:
            info(f"main user from {args.state}: {main['email']}")
            self.main = User(self.secret, args.base_url, main["email"], main["phone_suffix"])
        else:
            self.main = self.new_user(None)
        self.db = self.open_db() if args.db_check else None
        self.s3 = self.open_s3() if args.rustfs_check else None
        self.restart_wanted = []     # seeds that want api restarted at the end of the seed phase
        self._log_cache = None

    # --- plumbing ---------------------------------------------------------------------------

    def new_user(self, tag):
        local, _, domain = self.args.email.partition("@")
        name, plus, rest = local.partition("+")
        email = f"{name}-{tag}{plus}{rest}@{domain}" if tag else self.args.email
        u = User(self.secret, self.args.base_url, email, self.next_phone)
        self.next_phone += 1
        if tag:
            self.users.append(u)
        return u

    def open_db(self):
        a = self.args
        p = Path(a.db_env)
        if not p.is_file():
            smoke.die(f"--db-check: {p} not found")
        env = s5.read_env_file(p)
        user = env.get("SURREAL_USER") or env.get("SURREAL_ROOT_USER")
        pw = env.get("SURREAL_PASS") or env.get("SURREAL_ROOT_PASS")
        if not (user and pw):
            smoke.die(f"--db-check: {p} has neither SURREAL_USER/SURREAL_PASS nor "
                      f"SURREAL_ROOT_USER/SURREAL_ROOT_PASS")
        url = a.db_url or env.get("SURREAL_URL") or "http://127.0.0.1:8000"
        return Db(url, user, pw, a.db_ns, a.db_name)

    def open_s3(self):
        a = self.args
        p = Path(a.rustfs_env)
        if not p.is_file():
            smoke.die(f"--rustfs-check: {p} not found")
        env = s5.read_env_file(p)
        ak, sk = env.get("RUSTFS_ACCESS_KEY"), env.get("RUSTFS_SECRET_KEY")
        if not (ak and sk):
            smoke.die(f"--rustfs-check: {p} lacks RUSTFS_ACCESS_KEY or RUSTFS_SECRET_KEY")
        api_env = s5.read_env_file(a.env_file) if Path(a.env_file).is_file() else {}
        bucket = a.rustfs_bucket or env.get("RUSTFS_BUCKET") or api_env.get("RUSTFS_BUCKET") or "pdm-images"
        return S3(a.rustfs_url, bucket, ak, sk, env.get("RUSTFS_REGION") or "us-east-1")

    def load_state(self):
        p = Path(self.args.state)
        if self.args.phase == "seed" or not p.is_file():
            return {}
        return json.loads(p.read_text())

    def save_state(self):
        if not self.state:
            return
        self.state.setdefault("user", {"email": self.main.email,
                                       "phone_suffix": self.main.clerk.phone_suffix})
        Path(self.args.state).write_text(json.dumps(self.state, indent=1) + "\n")
        info(f"state written to {self.args.state}")

    def raw(self, user, method, path, body=None, idem=False, token=None, data=None, ctype=None):
        h = {"Authorization": f"Bearer {token or user.clerk.token()}"}
        if idem:
            h["Idempotency-Key"] = str(uuid.uuid4())
        if data is not None:
            h["Content-Type"] = ctype
            body = data
        status, _, out = smoke.http_json(method, user.api.base + path, h, body)
        return status, out

    def call(self, user, method, path, body=None, expect=(200,), **kw):
        status, out = self.raw(user, method, path, body, **kw)
        if status not in expect:
            raise Fail(f"{method} {path}: expected {'/'.join(map(str, expect))}, got {status}: "
                       f"{json.dumps(out)[:400]}")
        return out

    def page(self, user, path, query):
        out, cursor = [], None
        while True:
            q = dict(query, limit=100, **({"cursor": cursor} if cursor else {}))
            pg = self.call(user, "GET", f"{path}?{urllib.parse.urlencode(q)}")
            out += pg["items"]
            cursor = pg.get("next_cursor")
            if not cursor:
                return out

    def new_garden(self, user, kind, lat=None, lon=None):
        a = self.args
        stamp = time.strftime("%Y-%m-%d %H:%M:%S")
        g = self.call(user, "POST", "/gardens", {"name": f"s6 {kind} {stamp}", "location": {
            "lat": a.lat if lat is None else lat, "lon": a.lon if lon is None else lon}},
            expect=(201,), idem=True)["id"]
        self.made.append((user, g))
        return g

    def upload_chat_image(self, user, conv, path):
        p = Path(path)
        ctype = {".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png",
                 ".webp": "image/webp", ".heic": "image/heic"}.get(p.suffix.lower(), "image/jpeg")
        boundary = "s6" + uuid.uuid4().hex
        data = ((f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{p.name}"'
                 f"\r\nContent-Type: {ctype}\r\n\r\n").encode() + p.read_bytes()
                + f"\r\n--{boundary}--\r\n".encode())
        return self.call(user, "POST", f"/conversations/{conv}/images", expect=(201,), idem=True,
                         data=data, ctype=f"multipart/form-data; boundary={boundary}")["image_id"]

    def fixture_garden(self, user, garden, label, anchor="plant", chat=True):
        """A pending scan with a real whole_plant upload (its plant made by new_plant) and, with
        `chat`, a conversation anchored on the plant (or the garden) with a chat image. Returns
        the ids and the object keys (docs/storage 3)."""
        need(self.args.photo, "needs --photo (real uploads)")
        scan = self.call(user, "POST", "/scans", {"garden_id": garden, "new_plant": {
            "name": f"s6 {label} tomato", "species_id": "tomato"}}, expect=(201,), idem=True)
        sid, pid = scan["id"], scan["plant_id"]
        img = user.api.upload(sid, "whole_plant", self.args.photo)["image"]["id"]
        fx = {"garden": garden, "plant": pid, "scan": sid, "image": img,
              "keys": [f"gardens/{garden}/scans/{sid}/{img}.webp"],
              "prefixes": [f"gardens/{garden}/scans/{sid}/"]}
        if chat:
            target = pid if anchor == "plant" else garden
            conv = self.call(user, "POST", "/conversations", {"garden_id": garden, "anchor": {
                "type": anchor, "id": target}}, expect=(201,), idem=True)["id"]
            ci = self.upload_chat_image(user, conv, self.args.photo)
            fx.update(conversation=conv, chat_image=ci)
            fx["keys"].append(f"gardens/{garden}/chat/{conv}/{ci}.webp")
            fx["prefixes"].append(f"gardens/{garden}/chat/{conv}/")
        return fx

    def objects_present(self, keys, ev):
        if not self.s3:
            return
        missing = [k for k in keys if self.s3.head(k) != 200]
        need(not missing, f"objects missing right after upload, so their absence later would prove "
             f"nothing: {missing}")
        ev.append(f"RustFS: {len(keys)} object(s) present before the delete")

    def wait_until(self, pred, timeout, period=2.0):
        """(True, seconds, last value) once pred() is truthy, or (False, timeout, last value)."""
        t0 = time.monotonic()
        while True:
            last = pred()
            if last:
                return True, time.monotonic() - t0, last
            if time.monotonic() - t0 > timeout:
                return False, time.monotonic() - t0, last
            time.sleep(period)

    def leftovers(self, targets):
        """For each (table, key): the record itself and every row of a table whose field links
        to it (schema.surql). {"table:key": ["<table> WHERE <cond>: n", ...]}, empty when gone."""
        text = SCHEMA.read_text() if SCHEMA.is_file() else ""
        need(text, f"{SCHEMA} not found: the linked tables come from it")
        stmts, labels = [], []
        for table, key in targets:
            lit = rid(table, key)
            stmts.append(f"SELECT VALUE id FROM {lit};")
            labels.append((f"{table}:{key}", "the record itself"))
            for t, cond in schema_refs(text, table):
                stmts.append(f"SELECT count() AS n FROM {t} WHERE {cond.replace('$r', lit)} GROUP ALL;")
                labels.append((f"{table}:{key}", f"{t} WHERE {cond}"))
        results = self.db.run("\n".join(stmts))
        out = {f"{t}:{k}": [] for t, k in targets}
        for (target, what), res in zip(labels, results):
            n = len(res) if what == "the record itself" else (res[0]["n"] if res else 0)
            if n:
                out[target].append(f"{what}: {n}")
        return out

    def wait_db_gone(self, targets, ev, timeout=None):
        timeout = timeout or self.args.delete_timeout
        def gone():
            left = self.leftovers(targets)
            return not any(left.values())
        ok, dt, _ = self.wait_until(gone, timeout)
        if ok:
            ev.append(f"DB: {', '.join(f'{t}:{k}' for t, k in targets)} and every row linking to "
                      f"them gone after {dt:.0f} s")
            return True
        rest = {k: v for k, v in self.leftovers(targets).items() if v}
        raise Fail(f"DB rows left after {timeout} s: {json.dumps(rest)[:900]}")

    def wait_prefixes_empty(self, prefixes, ev, timeout=None):
        timeout = timeout or self.args.delete_timeout

        def empty():
            left = {p: len(self.s3.list(p)) for p in prefixes}
            return left if not any(left.values()) else None
        ok, dt, _ = self.wait_until(empty, timeout)
        if not ok:
            left = {p: [o["key"] for o in self.s3.list(p)][:5] for p in prefixes}
            raise Fail(f"RustFS objects left after {timeout} s: {json.dumps(left)[:600]}")
        ev.append(f"RustFS: {', '.join(prefixes)} empty after {dt:.0f} s")

    def read_log(self, fresh=True):
        a = self.args
        if not a.api_log_cmd:
            return None
        if fresh or self._log_cache is None:
            r = subprocess.run(a.api_log_cmd, shell=True, timeout=180, capture_output=True,
                               text=True, errors="replace")
            need(r.returncode == 0, f"--api-log-cmd exited {r.returncode}")
            self._log_cache = log_entries(r.stdout + r.stderr)
        return self._log_cache

    def restart_api(self, ev):
        a = self.args
        info("running --api-restart-cmd")
        r = subprocess.run(a.api_restart_cmd, shell=True, timeout=400)
        need(r.returncode == 0, f"--api-restart-cmd exited {r.returncode}")
        t0 = time.monotonic()
        while not self.main.api.healthy():
            need(time.monotonic() - t0 < 300, "api not healthy within 300 s of the restart")
            time.sleep(1)
        ev.append(f"api restarted, healthy {time.monotonic() - t0:.0f} s later")
        return time.time()

    def shell(self, opt, cmd, timeout=300):
        info(f"running {opt}")
        r = subprocess.run(cmd, shell=True, timeout=timeout)
        need(r.returncode == 0, f"{opt} exited {r.returncode}")

    def db_part(self, missing):
        """The parts left unchecked for want of an option, for a SKIP's message."""
        if not self.db:
            missing.append("the DB rows (--db-check)")
        if not self.s3:
            missing.append("the objects (--rustfs-check)")
        return missing

    def finish(self, missing, ev):
        if missing:
            raise Skip("everything checked passed, but not " + " and ".join(missing) + ". "
                       + " ".join(ev[-1:]))

    # --- tokens -----------------------------------------------------------------------------

    def check_tokens(self, ev):
        u = self.main
        tok = expo_token()
        body = {"token": tok, "platform": "ios"}
        for i in (1, 2):
            status, out = self.raw(u, "POST", "/me/push-tokens", body)
            need(status == 204, f"POST /me/push-tokens (ios, try {i}): {status} {json.dumps(out)[:200]}")
        ev.append("a well-formed ios token: 204, and 204 again (a no-op)")
        if self.db:
            row = self.db.one(f"SELECT platform, record::id(user) AS user FROM {rid('push_token', [tok])};")
            need(row and row[0]["platform"] == "ios" and row[0]["user"] == u.id,
                 f"push_token row after POST: {row}")
            ev.append("DB: push_token row with platform ios and the user")
        cases = [("an unknown field", {"token": expo_token(), "platform": "ios", "extra": 1}, None),
                 ("a bad platform", {"token": expo_token(), "platform": "windows"}, None),
                 ("a malformed ios token", {"token": "not-an-expo-token", "platform": "ios"},
                  "the ios token validation is part of S6's Rust changes: this FAILs until deployed")]
        notes = []
        for label, b, note in cases:
            status, out = self.raw(u, "POST", "/me/push-tokens", b)
            code = (out or {}).get("error", {}).get("code") if isinstance(out, dict) else None
            ev.append(f"{label}: {status} {code or ''}".rstrip())
            if status != 400:
                if status == 204:
                    self.raw(u, "DELETE", token_path(b["token"]))
                notes.append(f"{label} gave {status}, not 400" + (f" ({note})" if note else ""))
        status, _ = self.raw(u, "DELETE", token_path(tok))
        need(status == 204, f"DELETE {token_path('<token>')}: {status}")
        status, _ = self.raw(u, "DELETE", token_path(expo_token("unknown")))
        need(status == 204, f"DELETE of an unknown token: {status}")
        ev.append("DELETE /me/push-tokens/{token} (brackets URL-encoded): 204; an unknown token: 204")
        if self.db:
            row = self.db.one(f"SELECT * FROM {rid('push_token', [tok])};")
            need(not row, f"push_token row still there after DELETE: {row}")
            ev.append("DB: the row is gone after DELETE")
        else:
            ev.append("the rows weren't read (no --db-check)")
        need(not notes, "; ".join(notes))

    # --- prune ------------------------------------------------------------------------------

    def check_prune(self, ev):
        a = self.args
        owner, b = self.main, self.new_user("b")
        g = self.new_garden(owner, "prune")
        inv = self.call(owner, "POST", f"/gardens/{g}/invites", {"role": "editor"}, expect=(201,),
                        idem=True)
        self.call(b, "POST", f"/invites/{inv['code']}/accept")
        ev.append(f"garden {g}; B joined as editor with an invite")
        ios, web = expo_token(), f"s6-web-{secrets.token_hex(8)}"
        self.call(b, "POST", "/me/push-tokens", {"token": ios, "platform": "ios"}, expect=(204,))
        self.call(b, "POST", "/me/push-tokens", {"token": web, "platform": "web"}, expect=(204,))
        if a.real_token:
            self.call(b, "POST", "/me/push-tokens", {"token": a.real_token, "platform": "ios"},
                      expect=(204,))
        if not self.db:
            self.call(owner, "DELETE", f"/gardens/{g}", expect=(202,))
            self.results.append(("prune-real", "SKIP", ["needs --db-check"]))
            raise Skip("needs --db-check: no route shows a user's push tokens (the garden was deleted)")
        rows = self.db.one(f"SELECT VALUE record::id(id)[0] FROM push_token WHERE user = {rid('user', b.id)};")
        need(ios in rows and web in rows, f"B's tokens not stored: {len(rows)} rows")
        t0 = time.time()
        self.call(owner, "DELETE", f"/gardens/{g}", expect=(202,))
        ev.append("DELETE /gardens/{id}: 202, so the deletion runner pushes garden_deleted to B")

        def pruned():
            left = self.db.one(f"SELECT VALUE record::id(id)[0] FROM push_token WHERE user = "
                               f"{rid('user', b.id)};")
            return left if ios not in left else None
        ok, dt, left = self.wait_until(pruned, a.prune_timeout, 1.0)
        need(ok, f"B's unknown ios token still stored {a.prune_timeout} s after the delete: Expo's "
             f"DeviceNotRegistered didn't remove it (or no push was sent)")
        ev.append(f"DB: the unknown ios token was removed {time.time() - t0:.0f} s after the delete "
                  f"(DeviceNotRegistered)")
        need(web in left, "B's web token was removed too: web tokens are stored, never sent")
        ev.append("DB: B's web token stays (Expo Push doesn't deliver to the web)")
        log = self.read_log()
        if log is not None:
            hits = find_log(log, PUSH_PRUNED_LOG, after=t0 - 5)
            ev.append(f"api log: {len(hits)} line(s) matching {PUSH_PRUNED_LOG.pattern!r}"
                      + (f"; first: {short(hits[0])}" if hits else ""))
            need(hits, "no push log line naming the unregistered token (Pusher::send's info line)")
        else:
            ev.append("api log not read (no --api-log-cmd)")
        ev.append("proves the push up to Expo's push service; a delivered notification needs a "
                  "real device (--real-token)")
        if a.real_token:
            kept = a.real_token in left
            self.results.append(("prune-real", "PASS" if kept else "FAIL", [
                f"the real device's token {'stays' if kept else 'was removed'} after the push; "
                f"check the phone for the 'garden deleted' notification, and the receipts task "
                f"(every 15 min) keeps it unless Expo reports it unregistered"]))
        else:
            self.results.append(("prune-real", "SKIP", ["no --real-token (a development build's "
                                 "Expo token): a real receipt needs a real device"]))

    # --- alerts (two phases) ----------------------------------------------------------------

    def alert_tasks(self, garden):
        return self.page(self.main, "/tasks", {"garden_id": garden, "source": "forecast_alert"})

    @staticmethod
    def horizon_frost(weather):
        days = weather.get("forecast", [])[:HORIZON_DAYS]
        return [d["date"] for d in days if d.get("frost_risk")]

    @staticmethod
    def forecast_line(weather):
        return ", ".join(f"{d['date']} {d['t_min_c']}/{d['t_max_c']} °C"
                         + (" frost" if d.get("frost_risk") else "")
                         for d in weather.get("forecast", [])[:HORIZON_DAYS + 1])

    def check_alerts(self, ev):
        a = self.args
        if a.phase == "seed":
            return self.alerts_seed(ev)
        if a.phase == "check":
            return self.alerts_check(ev)
        raise Skip("--phase fresh is the orphans check's")

    def alerts_seed(self, ev):
        a = self.args
        g = self.new_garden(self.main, "alerts", a.frost_lat, a.frost_lon)
        w = self.call(self.main, "GET", f"/gardens/{g}/weather")["weather"]
        frost = self.horizon_frost(w)
        now = time.time()
        self.state["alerts"] = {"garden_id": g, "seeded_at": now, "restarted_at": None,
                                "frost_seen": {d: now for d in frost},
                                "forecasts": [[iso(now), self.forecast_line(w)]], "checks": []}
        ev.append(f"garden {g} at {a.frost_lat}, {a.frost_lon}: forecast {self.forecast_line(w)}")
        ev.append(f"frost spells in the {HORIZON_DAYS}-day horizon: {spells(frost)}")
        if a.api_restart_cmd:
            self.restart_wanted.append("alerts")
            ev.append("api is restarted at the end of the seed phase, so a sweep runs 30 s later")
        if not frost:
            raise Skip(f"no frost in the {HORIZON_DAYS}-day horizon at the frost garden; the state "
                       f"is kept, and --phase check reads the forecast again")
        ev.append(f"next: --phase check after at least one hourly sweep, then once more after "
                  f"another (state {a.state})")

    def sweeps_since(self, st, ev):
        """(count, how): the weather sweeps run since the seed, from api's log (lines within
        5 minutes of each other count as one run) or, without it, by the clock."""
        log = self.read_log()
        if log is not None:
            runs = find_log(log, WEATHER_RUN_LOG, after=st["seeded_at"], info_only=True)
            n = count_runs([e["ts"] for e in runs if e["ts"]])
            ev.append(f"api log: {n} weather run(s) since the seed ({WEATHER_RUN_LOG.pattern!r})"
                      + (f", the last at {iso(max(e['ts'] for e in runs if e['ts']))}" if n else ""))
            return n, "log"
        since = st.get("restarted_at") or st["seeded_at"]
        n = guaranteed_sweeps(time.time() - since, bool(st.get("restarted_at")))
        ev.append(f"no --api-log-cmd: at least {n} sweep(s) by the clock since "
                  f"{'the restart' if st.get('restarted_at') else 'the seed'}")
        return n, "clock"

    def alerts_check(self, ev):
        st = self.state.get("alerts")
        if not st:
            raise Skip(f"no alerts seed in {self.args.state}: run --phase seed first")
        g, now = st["garden_id"], time.time()
        w = self.call(self.main, "GET", f"/gardens/{g}/weather")["weather"]
        now_frost = self.horizon_frost(w)
        for d in now_frost:
            st["frost_seen"].setdefault(d, now)
        st["forecasts"].append([iso(now), self.forecast_line(w)])
        ev.append(f"garden {g}: forecast now {self.forecast_line(w)}")
        alerts = [{"id": t["id"], "kind": (t.get("trigger") or {}).get("kind"), "due": t.get("due"),
                   "dismissed": bool(t.get("dismissed_at")), "task": t} for t in self.alert_tasks(g)]
        ev.append(f"alert inbox: {[(x['kind'], x['due'], 'dismissed' if x['dismissed'] else 'open') for x in alerts]}")
        problems = []
        for x in alerts:
            t, trig = x["task"], x["task"].get("trigger") or {}
            if t.get("plant_id") or t.get("source") != "forecast_alert" or not x["kind"]:
                problems.append(f"task {x['id']}: plant {t.get('plant_id')}, source {t.get('source')}, "
                                f"trigger {trig}")
            if x["due"] != trig.get("date"):
                problems.append(f"task {x['id']}: due {x['due']} != trigger date {trig.get('date')}")
            if x["kind"] == "frost" and t.get("title") != FROST_TITLE:
                problems.append(f"frost task {x['id']} titled {t.get('title')!r}, not {FROST_TITLE!r}")
        for p, q in close_pairs(alerts):
            problems.append(f"two {p['kind']} alerts on {p['due']} and {q['due']} (same or "
                            f"adjacent days): one spell alerted twice")
        sweeps, how = self.sweeps_since(st, ev)
        frost = [x for x in alerts if x["kind"] == "frost"]
        union = set(st["frost_seen"]) | {x["due"] for x in frost}
        for sp in spells(union):
            mine = [x for x in frost if x["due"] in sp]
            line = f"frost spell {sp[0]}..{sp[-1]}: {len(mine)} alert(s) {[x['due'] for x in mine]}"
            ev.append(line)
            if len(mine) > 1:
                problems.append(line + ": more than one")
            elif mine and mine[0]["due"] != sp[0]:
                ev.append(f"  note: due {mine[0]['due']}, the spell's first day seen here {sp[0]} "
                          f"(a day the sweeps never saw, or a forecast change)")
            elif not mine and set(sp) & set(now_frost):
                # A spell this script saw over an hour ago has been through an hourly sweep.
                first = min(st["frost_seen"].get(d, now) for d in sp)
                if now - first > WEATHER_EVERY_S + 300 and sweeps >= 1:
                    problems.append(f"no frost alert for the spell {sp[0]}..{sp[-1]}, in the horizon "
                                    f"since {iso(first)}, after {sweeps} sweep(s) ({how})")
                else:
                    ev.append(f"  note: no alert yet; the spell was first seen {iso(first)}")
        ids = {x["id"] for x in alerts}
        prev = st["checks"][-1] if st["checks"] else None
        if prev:
            gone = [i for i in prev["task_ids"] if i not in ids]
            if gone:
                problems.append(f"alerts listed at the previous check are gone: {gone}")
            new = sorted(ids - set(prev["task_ids"]))
            ev.append(f"since the previous check ({iso(prev['at'])}): {len(ids & set(prev['task_ids']))} "
                      f"same task id(s), new {[(x['kind'], x['due']) for x in alerts if x['id'] in new]}")
        dismissed = next((c["dismissed"] for c in reversed(st["checks"]) if c.get("dismissed")), None)
        verified = False
        if dismissed:
            d = next((x for x in alerts if x["id"] == dismissed), None)
            if not d or not d["dismissed"]:
                problems.append(f"the alert dismissed at an earlier check ({dismissed}) isn't "
                                f"dismissed now: {d and d['task'].get('dismissed_at')}")
            else:
                verified = True
                ev.append(f"the {d['kind']} alert of {d['due']} dismissed earlier is still dismissed, "
                          f"and its spell has no second alert")
        if alerts:
            target = (frost or alerts)[0]
            status, out = self.raw(self.main, "DELETE", f"/tasks/{target['id']}")
            code = (out or {}).get("error", {}).get("code") if isinstance(out, dict) else None
            ev.append(f"DELETE /tasks/{{id}} of a {target['kind']} alert: {status} {code or ''}".rstrip())
            if status != 409:
                problems.append(f"DELETE of a forecast alert gave {status}, not 409 invalid_state")
            if not dismissed:
                self.call(self.main, "PATCH", f"/tasks/{target['id']}", {"dismissed": True})
                dismissed = target["id"]
                ev.append(f"dismissed the {target['kind']} alert of {target['due']}; a later check must "
                          f"find it dismissed and not recreated")
        st["checks"].append({"at": now, "task_ids": sorted(ids), "dismissed": dismissed,
                             "sweeps": sweeps, "how": how})
        need(not problems, "; ".join(problems))
        if not union:
            raise Skip("no frost seen in the horizon at the seed or now, and no frost alert")
        if not frost:
            raise Skip(f"no frost alert yet after {sweeps} sweep(s) ({how}); run --phase check again")
        if sweeps < 2:
            raise Skip(f"one alert per spell so far, but only {sweeps} sweep(s) since the seed "
                       f"({how}): 'not one per sweep' needs two; run --phase check again after the "
                       f"next hourly sweep")
        if not (prev and verified):
            raise Skip("one alert per spell across the sweeps so far; run --phase check once more, "
                       "after another sweep, for the same ids and the dismissed alert")

    # --- plant and garden -------------------------------------------------------------------

    def check_plant(self, ev):
        u = self.main
        g = self.new_garden(u, "plant")
        fx = self.fixture_garden(u, g, "plant")
        mem = self.call(u, "POST", "/memories", {"garden_id": g, "plant_id": fx["plant"],
                        "text": "s6: watered every morning"}, expect=(201,), idem=True)["id"]
        task = self.call(u, "POST", "/tasks", {"garden_id": g, "plant_id": fx["plant"],
                         "title": "s6: check the lower leaves"}, expect=(201,), idem=True)["id"]
        ev.append(f"garden {g}: plant {fx['plant']}, scan {fx['scan']} with image {fx['image']}, "
                  f"conversation {fx['conversation']} with chat image {fx['chat_image']}, a memory "
                  f"and a task about the plant")
        self.objects_present(fx["keys"], ev)
        self.call(u, "DELETE", f"/plants/{fx['plant']}", expect=(202,))
        ev.append("DELETE /plants/{id}: 202")
        ok, dt, _ = self.wait_until(lambda: self.raw(u, "GET", f"/plants/{fx['plant']}")[0] == 404,
                                    self.args.delete_timeout)
        need(ok, "GET /plants/{id} still answers after the delete")
        for path in (f"/scans/{fx['scan']}", f"/conversations/{fx['conversation']}"):
            status, _ = self.raw(u, "GET", path)
            need(status == 404, f"GET {path}: {status}, not 404")
        need(self.raw(u, "GET", f"/gardens/{g}")[0] == 200, "the garden went with the plant")
        ev.append("API: the plant, its scan and the conversation answer 404; the garden stays")
        missing = self.db_part([])
        if self.db:
            self.wait_db_gone([("plant", fx["plant"]), ("scan", fx["scan"]),
                               ("scan_image", fx["image"]), ("conversation", fx["conversation"]),
                               ("chat_image", fx["chat_image"]), ("memory", mem), ("task", task)], ev)
        if self.s3:
            self.wait_prefixes_empty(fx["prefixes"], ev)
        self.finish(missing, ev)

    def check_garden(self, ev):
        u = self.main
        g = self.new_garden(u, "garden")
        fx = self.fixture_garden(u, g, "garden", anchor="garden")
        self.call(u, "POST", "/memories", {"garden_id": g, "text": "s6: garden note"},
                  expect=(201,), idem=True)
        self.call(u, "POST", "/tasks", {"garden_id": g, "title": "s6: garden task"},
                  expect=(201,), idem=True)
        self.call(u, "POST", f"/gardens/{g}/invites", {"role": "viewer"}, expect=(201,), idem=True)
        ev.append(f"garden {g}: plant, scan with image, a garden conversation with a chat image, a "
                  f"memory, a task, an invite")
        self.objects_present(fx["keys"], ev)
        self.call(u, "DELETE", f"/gardens/{g}", expect=(202,))
        ev.append("DELETE /gardens/{id}: 202")
        ok, _, _ = self.wait_until(lambda: self.raw(u, "GET", f"/gardens/{g}")[0] == 404,
                                   self.args.delete_timeout)
        need(ok, "GET /gardens/{id} still answers after the delete")
        ev.append("API: GET /gardens/{id} answers 404")
        missing = self.db_part([])
        if self.db:
            self.wait_db_gone([("garden", g), ("plant", fx["plant"]), ("scan", fx["scan"]),
                               ("conversation", fx["conversation"])], ev)
        if self.s3:
            self.wait_prefixes_empty([f"gardens/{g}/"], ev)
        self.finish(missing, ev)

    # --- account and restart ----------------------------------------------------------------

    def account_fixture(self, tag, ev):
        """A with G1 (sole owner: a pending scan with an image, a conversation with a chat image,
        a memory, a push token) and G2 shared with B as editor (A's conversation there, with a
        chat image)."""
        a_user, b_user = self.new_user(f"{tag}-a"), self.new_user(f"{tag}-b")
        g1 = self.new_garden(a_user, f"{tag} G1")
        fx1 = self.fixture_garden(a_user, g1, f"{tag} G1")
        mem = self.call(a_user, "POST", "/memories", {"garden_id": g1, "text": "s6: A's note"},
                        expect=(201,), idem=True)["id"]
        push = expo_token()
        self.call(a_user, "POST", "/me/push-tokens", {"token": push, "platform": "ios"}, expect=(204,))
        g2 = self.new_garden(a_user, f"{tag} G2")
        inv = self.call(a_user, "POST", f"/gardens/{g2}/invites", {"role": "editor"},
                        expect=(201,), idem=True)
        self.call(b_user, "POST", f"/invites/{inv['code']}/accept")
        self.call(b_user, "POST", "/plants", {"garden_id": g2, "name": "s6 B's tomato",
                  "species_id": "tomato"}, expect=(201,), idem=True)
        c2 = self.call(a_user, "POST", "/conversations", {"garden_id": g2, "anchor": {
            "type": "garden", "id": g2}}, expect=(201,), idem=True)["id"]
        ci2 = self.upload_chat_image(a_user, c2, self.args.photo)
        self.call(a_user, "POST", "/memories", {"garden_id": g2, "text": "s6: A's note in G2"},
                  expect=(201,), idem=True)
        fx = SimpleNamespace(a=a_user, b=b_user, a_id=a_user.id, b_id=b_user.id,
                             auth_id=a_user.clerk.user_id, g1=g1, g2=g2, fx1=fx1, mem=mem,
                             push=push, c2=c2, ci2=ci2,
                             keys=fx1["keys"] + [f"gardens/{g2}/chat/{c2}/{ci2}.webp"])
        ev.append(f"A {fx.a_id}: G1 {g1} (plant, pending scan with an image, a conversation with a "
                  f"chat image, a memory, a push token); G2 {g2} shared with B {fx.b_id} as editor, "
                  f"with A's conversation {c2} and its chat image")
        self.objects_present(fx.keys, ev)
        return fx

    def long_token(self, fx, ev):
        a = self.args
        tok = fx.a.clerk.long_token(a.token_seconds)
        exp = jwt_exp(tok)
        need(exp - time.time() > 60, f"the long token expires in {exp - time.time():.0f} s")
        ev.append(f"A's session token minted for {a.token_seconds} s (exp {iso(exp)}), so its later "
                  f"401 can't be an expiry")
        return tok, exp

    def old_token_refused(self, fx, tok, exp, ev, when):
        need(time.time() < exp - 5, f"the token expired before the {when} check, so a 401 proves nothing")
        status, out = self.raw(fx.a, "GET", "/me", token=tok)
        code = (out or {}).get("error", {}).get("code") if isinstance(out, dict) else None
        need(status == 401 and code == "unauthenticated",
             f"GET /me with A's old token {when}: {status} {code}, not 401 unauthenticated")
        ev.append(f"GET /me with A's old token {when}: 401 unauthenticated, {exp - time.time():.0f} s "
                  f"before it expires")

    def delete_me(self, fx, tok, ev):
        res = self.call(fx.a, "DELETE", "/me", expect=(202,), token=tok)
        fx.a.deleted = True
        ev.append(f"DELETE /me: 202 {json.dumps(res)}")
        need(fx.g1 in res.get("deleted_gardens", []), f"deleted_gardens lacks G1: {res}")
        need(any(t.get("garden_id") == fx.g2 and t.get("new_owner_user_id") == fx.b_id
                 for t in res.get("transferred_gardens", [])),
             f"transferred_gardens lacks G2 → B: {res}")
        return time.time()

    def tombstone(self, fx):
        rows = self.db.one(f"SELECT user, started_at, clerk_deleted_at FROM "
                           f"{rid('deleted_user', [fx.auth_id])};")
        return rows[0] if rows else None

    def wait_account_done(self, fx, timeout, ev):
        """Until the tombstone has clerk_deleted_at (DB), or Clerk's user is 404 (no DB)."""
        if self.db:
            ok, dt, _ = self.wait_until(lambda: (self.tombstone(fx) or {}).get("clerk_deleted_at"),
                                        timeout, 3.0)
            if ok:
                ev.append(f"DB: the tombstone has clerk_deleted_at {dt:.0f} s later")
            return ok, dt
        ok, dt, _ = self.wait_until(lambda: fx.a.clerk.user_status() == 404, timeout, 3.0)
        if ok:
            ev.append(f"Clerk: the user is gone {dt:.0f} s later (no --db-check: the tombstone "
                      f"isn't read)")
        return ok, dt

    def account_gone(self, fx, tok, exp, ev, t_delete):
        status = fx.a.clerk.user_status()
        need(status == 404, f"Clerk GET /v1/users/{{id}}: {status}, not 404")
        ev.append("Clerk: GET /v1/users/{id} 404")
        self.old_token_refused(fx, tok, exp, ev, "after the cascade")
        g2 = self.call(fx.b, "GET", f"/gardens/{fx.g2}")
        members = self.call(fx.b, "GET", f"/gardens/{fx.g2}/members")["items"]
        roles = {m["user_id"]: m["role"] for m in members}
        need(roles.get(fx.b_id) == "owner" and fx.a_id not in roles,
             f"G2's members: {roles}, expected B owner and A gone")
        ev.append(f"G2 {g2['id']} stays, B its owner, A not a member")
        need(self.raw(fx.b, "GET", f"/conversations/{fx.c2}")[0] == 404,
             "A's conversation in G2 still answers")
        missing = self.db_part([])
        if self.db:
            tomb = self.tombstone(fx)
            need(tomb and tomb.get("clerk_deleted_at"), f"the tombstone: {tomb}")
            ev.append(f"DB: the tombstone deleted_user:[{fx.auth_id}] remains, the only row allowed")
            self.wait_db_gone([("user", fx.a_id), ("garden", fx.g1), ("plant", fx.fx1["plant"]),
                               ("scan", fx.fx1["scan"]), ("conversation", fx.fx1["conversation"]),
                               ("conversation", fx.c2), ("chat_image", fx.ci2), ("memory", fx.mem)],
                              ev, timeout=60)
            rows = self.db.one(f"SELECT * FROM {rid('push_token', [fx.push])};")
            need(not rows, "A's push token remains")
        if self.s3:
            self.wait_prefixes_empty([f"gardens/{fx.g1}/", f"gardens/{fx.g2}/chat/{fx.c2}/"], ev,
                                     timeout=60)
        log = self.read_log()
        if log is not None:
            hits = [e for e in find_log(log, ACCOUNT_DELETED_LOG, after=t_delete - 5)]
            ev.append(f"api log: {len(hits)} 'account deleted' line(s)"
                      + (f"; {short(hits[-1])}" if hits else ""))
            need(hits, f"no log line matching {ACCOUNT_DELETED_LOG.pattern!r} after DELETE /me")
        return missing

    def check_account(self, ev):
        need(self.args.photo, "needs --photo (real uploads)")
        fx = self.account_fixture("acct", ev)
        tok, exp = self.long_token(fx, ev)
        t_delete = self.delete_me(fx, tok, ev)
        self.old_token_refused(fx, tok, exp, ev, "right after DELETE /me")
        ok, _ = self.wait_account_done(fx, self.args.account_timeout, ev)
        need(ok, f"the account cascade didn't finish within {self.args.account_timeout} s")
        self.finish(self.account_gone(fx, tok, exp, ev, t_delete), ev)

    def check_restart(self, ev):
        a = self.args
        if not (a.rustfs_stop_cmd and a.rustfs_start_cmd):
            raise Skip("needs --rustfs-stop-cmd and --rustfs-start-cmd")
        need(a.photo, "needs --photo (real uploads)")
        fx = self.account_fixture("rst", ev)
        tok, exp = self.long_token(fx, ev)
        stopped = False
        try:
            self.shell("--rustfs-stop-cmd", a.rustfs_stop_cmd)
            stopped, t_stop = True, time.time()
            ev.append("RustFS stopped")
            t_delete = self.delete_me(fx, tok, ev)
            log = self.read_log()
            if log is not None:
                def failed():
                    return find_log(self.read_log(), DELETION_FAILED_LOG, after=t_stop - 5)
                ok, dt, hits = self.wait_until(failed, a.failure_timeout, 5.0)
                need(ok, f"no deletion-failure line ({DELETION_FAILED_LOG.pattern!r}) within "
                     f"{a.failure_timeout} s of DELETE /me with RustFS down")
                ev.append(f"api log, {dt:.0f} s after DELETE /me: {short(hits[0])}")
            else:
                time.sleep(20)
                ev.append("no --api-log-cmd: waited 20 s instead of the failure line")
            self.old_token_refused(fx, tok, exp, ev, "while the cascade is stuck")
            need(fx.a.clerk.user_status() == 200, "the Clerk user is already gone, before the "
                 "cascade could finish: the order is wrong")
            ev.append("Clerk: the user still exists (it is deleted last)")
            if self.db:
                tomb = self.tombstone(fx)
                need(tomb and not tomb.get("clerk_deleted_at"), f"the tombstone: {tomb}, expected "
                     f"present and unfinished")
                g1 = self.db.one(f"SELECT deleting_at FROM {rid('garden', fx.g1)};")
                need(g1 and g1[0].get("deleting_at"), f"G1 isn't marked deleting: {g1}")
                ev.append("DB: the tombstone is unfinished and G1 is marked deleting")
            self.shell("--rustfs-start-cmd", a.rustfs_start_cmd)
            stopped, t_start = False, time.time()
            ok, _, _ = self.wait_until(lambda: self.main.api.healthy(), 180, 2.0)
            need(ok, "api's /healthz not 200 within 180 s of RustFS starting")
            ev.append(f"RustFS started {t_start - t_stop:.0f} s after it stopped; api healthy again")
            ok, dt = self.wait_account_done(fx, a.retry_wait, ev)
            restarted = False
            if not ok:
                if not a.api_restart_cmd:
                    raise Fail(f"the cascade didn't resume within {a.retry_wait} s of RustFS "
                               f"starting, and there is no --api-restart-cmd to resume it")
                ev.append(f"no in-process retry within {a.retry_wait} s: restarting api")
                self.restart_api(ev)
                restarted = True
                ok, dt = self.wait_account_done(fx, a.account_timeout, ev)
                need(ok, f"the cascade didn't finish within {a.account_timeout} s of the restart")
            took = time.time() - t_start
            ev.append(f"finished {took:.0f} s after RustFS came back, "
                      f"{'after an api restart' if restarted else 'by the in-process retry, no restart'}")
            self.fixture["restart"] = {"resumed_after_s": round(took, 1), "api_restarted": restarted}
            self.finish(self.account_gone(fx, tok, exp, ev, t_delete), ev)
        finally:
            if stopped:
                info("starting RustFS again (--rustfs-start-cmd)")
                subprocess.run(a.rustfs_start_cmd, shell=True, timeout=300)

    # --- orphans (two or three phases) --------------------------------------------------------

    def check_orphans(self, ev):
        if not self.s3:
            raise Skip("needs --rustfs-check: the strays are put in RustFS directly")
        return {"seed": self.orphans_seed, "fresh": self.orphans_fresh,
                "check": self.orphans_check}[self.args.phase](ev)

    def orphans_seed(self, ev):
        u = self.main
        g = self.new_garden(u, "orphans")
        fx = self.fixture_garden(u, g, "orphans")
        self.objects_present(fx["keys"], ev)
        s = fx["scan"]
        strays = {
            "under the real scan": f"gardens/{g}/scans/{s}/{new_ulid()}.webp",
            "under a scan with no record": f"gardens/{g}/scans/{new_ulid()}/{new_ulid()}.webp",
            "under a conversation with no record": f"gardens/{g}/chat/{new_ulid()}/{new_ulid()}.webp",
            "under a garden with no record": f"gardens/{new_ulid()}/scans/{new_ulid()}/{new_ulid()}.webp",
        }
        for key in strays.values():
            self.s3.put(key, STRAY_BYTES)
        put_at = time.time()
        need(all(self.s3.head(k) == 200 for k in strays.values()), "a stray isn't readable back")
        knowledge = {o["key"]: o["etag"] for o in self.s3.list("knowledge/")}
        self.state["orphans"] = {"garden_id": g, "scan_id": s, "conversation_id": fx["conversation"],
                                 "referenced": fx["keys"], "strays": strays, "put_at": put_at,
                                 "knowledge": knowledge, "restarted_at": None, "fresh": None}
        ev.append(f"garden {g}, scan {s}, conversation {fx['conversation']}: {len(fx['keys'])} "
                  f"referenced object(s)")
        for label, key in strays.items():
            ev.append(f"stray {label}: {key}")
        ev.append(f"knowledge/: {len(knowledge)} object(s) recorded")
        if self.args.api_restart_cmd:
            self.restart_wanted.append("orphans")
            ev.append("api is restarted at the end of the seed phase, so the sweep runs 1 h later")
        else:
            ev.append("no --api-restart-cmd: the sweep runs 1 h after api's last start, then daily")
        ev.append(f"next: --phase fresh in 30 to 50 min, then --phase check after the sweep "
                  f"(state {self.args.state})")

    def orphans_fresh(self, ev):
        st = self.state.get("orphans")
        if not st:
            raise Skip(f"no orphans seed in {self.args.state}")
        key = f"gardens/{st['garden_id']}/scans/{st['scan_id']}/{new_ulid()}.webp"
        self.s3.put(key, STRAY_BYTES)
        st["fresh"] = {"key": key, "put_at": time.time()}
        age = time.time() - st["put_at"]
        ev.append(f"fresh stray {key}, {age / 60:.0f} min after the seed's strays")
        if age < 1800:
            ev.append("note: less than 30 min after the seed; it may be over an hour old at the sweep")

    def orphans_check(self, ev):
        st = self.state.get("orphans")
        if not st:
            raise Skip(f"no orphans seed in {self.args.state}: run --phase seed first")
        put_at = st["put_at"]
        ready = put_at + ORPHAN_MIN_AGE_S
        log = self.read_log()
        sweep_at, how = None, None
        if log is not None:
            runs = find_log(log, ORPHAN_RUN_LOG, after=ready, info_only=True)
            ev.append(f"api log: {len(runs)} orphan sweep line(s) after the strays were an hour old "
                      f"({ORPHAN_RUN_LOG.pattern!r})" + (f"; {short(runs[0])}" if runs else ""))
            if runs:
                sweep_at, how = runs[0]["ts"] or time.time(), "log"
        elif st.get("restarted_at") and time.time() > st["restarted_at"] + ORPHANS_FIRST_S + 120:
            sweep_at, how = st["restarted_at"] + ORPHANS_FIRST_S, "clock (api restarted at the seed)"
            ev.append(f"no --api-log-cmd: the sweep ran at about {iso(sweep_at)} by the clock")
        problems, notes = [], []
        status = {label: self.s3.head(k) for label, k in st["strays"].items()}
        for label, code in status.items():
            ev.append(f"stray {label}: {'gone' if code == 404 else f'HTTP {code}'}")
        left = [label for label, code in status.items() if code != 404]
        if st.get("fresh"):
            fresh = st["fresh"]
            code = self.s3.head(fresh["key"])
            young = sweep_at is None or fresh["put_at"] > sweep_at - ORPHAN_MIN_AGE_S
            ev.append(f"fresh stray (put {iso(fresh['put_at'])}): {'present' if code == 200 else f'HTTP {code}'}")
            if code != 200 and young:
                problems.append("the fresh stray, under an hour old at the sweep, was deleted")
        else:
            key = f"gardens/{st['garden_id']}/scans/{st['scan_id']}/{new_ulid()}.webp"
            self.s3.put(key, STRAY_BYTES)
            st["fresh"] = {"key": key, "put_at": time.time()}
            notes.append("no --phase fresh ran: a fresh stray was put now, so its survival shows "
                         "nothing yet")
        missing = [k for k in st["referenced"] if self.s3.head(k) != 200]
        if missing:
            problems.append(f"referenced objects deleted: {missing}")
        else:
            ev.append(f"all {len(st['referenced'])} referenced object(s) present")
        scan = self.call(self.main, "GET", f"/scans/{st['scan_id']}")
        url = urllib.parse.urljoin(self.main.api.base + "/", scan["images"][0]["url"])
        try:
            req = urllib.request.Request(url, headers={"User-Agent": smoke.USER_AGENT})
            with urllib.request.urlopen(req, timeout=30) as r:
                served = (r.status, len(r.read()))
        except urllib.error.HTTPError as e:
            served = (e.code, 0)
        if served[0] != 200 or not served[1]:
            problems.append(f"the scan image's signed URL answered {served[0]}")
        else:
            ev.append(f"GET of the scan image through its signed URL: 200, {served[1]} bytes")
        now = {o["key"]: o["etag"] for o in self.s3.list("knowledge/")}
        changed = [k for k, e in st["knowledge"].items() if now.get(k) != e]
        added = sorted(set(now) - set(st["knowledge"]))
        if changed:
            problems.append(f"knowledge/ objects deleted or changed: {changed[:5]}")
        ev.append(f"knowledge/: {len(st['knowledge'])} object(s) untouched"
                  + (f", {len(added)} added since (a seed upload)" if added else ""))
        if left:
            if sweep_at is not None:
                msg = f"strays left after the sweep ({how}): {left}"
                if "under a garden with no record" in left:
                    msg += " (the pass over deleted gardens is part of S6's Rust changes)"
                problems.append(msg)
            elif not problems:
                raise Skip(f"strays still present ({left}) and no sign that the sweep ran after "
                           f"{iso(ready)}, when they turned an hour old; run --phase check later")
        elif sweep_at is None:
            notes.append("every stray is gone, though no sweep run was seen "
                         + ("(no --api-log-cmd)" if log is None else
                            f"(no line matched {ORPHAN_RUN_LOG.pattern!r})"))
        ev.extend(notes)
        need(not problems, "; ".join(problems))

    # --- rig-failure ------------------------------------------------------------------------

    def check_rig_failure(self, ev):
        a = self.args
        if not (a.rig_kill_cmd or a.manual_kill):
            raise Skip("needs --rig-kill-cmd or --manual-kill")
        need(a.photo, "needs --photo")
        u = self.main
        dead = a.dead_worker or DEFAULT_DEAD_WORKER[a.kill_at]
        g = self.new_garden(u, "rig-failure")
        poller = StatusPoller(a.base_url)
        poller.start()
        data = {"garden_id": g, "dead_worker": dead, "kill_at": a.kill_at}
        self.fixture["rig-failure"] = data
        kill = {}
        try:
            sid = self.call(u, "POST", "/scans", {"garden_id": g, "new_plant": {
                "name": "s6 rig-failure tomato", "species_id": "tomato"}}, expect=(201,), idem=True)["id"]
            s5.upload_photos(u.api, sid, a.photo, a.leaf_photo)
            self.call(u, "POST", f"/scans/{sid}/submit", expect=(202,))
            ev.append(f"scan {sid} submitted; killing at the first {a.kill_at} stage_started")
            st = s4.Stream(SimpleNamespace(api=u.api, args=a), sid)
            data.update(scan_id=sid, events=st.events)
            st.hooks.append(self.answering(u, sid))

            def killer(_stream, d):
                if kill or d["type"] != "stage_started" or d.get("stage") not in KILL_STAGES[a.kill_at]:
                    return
                kill.update(event=d, at=time.time())
                info(f"  stage_started {d['stage']} at seq {d['seq']}")
                if a.rig_kill_cmd:
                    self.shell("--rig-kill-cmd", a.rig_kill_cmd, 120)
                if a.manual_kill:
                    print(f"\n>>> {datetime.now(timezone.utc):%H:%M:%S} UTC: kill vlm-worker-nv on "
                          f"sam-linux now (docker compose kill vlm-worker-nv), then press Enter",
                          flush=True)
                    input()
                    kill["manual_at"] = time.time()
                kill["done_at"] = time.time()
            st.hooks.append(killer)
            st.follow(lambda s: s.of("plan_ready") or s.of("failed"), a.timeout, "plan_ready")
            need(kill, f"no {a.kill_at} stage_started before the scan rested: nothing was killed")
            scan = self.call(u, "GET", f"/scans/{sid}")
            data["scan"] = scan
            data["model_versions"] = s4.trace(scan)
            problems = []
            # (a) the takeover
            hb = last_heartbeat(poller.samples, dead)
            need(hb is not None, f"/status never showed worker {dead} (--dead-worker)")
            lease_at, source = None, None
            log = self.read_log()
            if log is not None:
                def by_fallback(e):
                    f = e["fields"]
                    worker = f.get("worker") or f.get("worker_id")
                    return ((worker is None or str(worker) == a.fallback_worker)
                            and f.get("kind", a.kill_at) == a.kill_at)
                leases = [e for e in find_log(log, LEASE_LOG, after=hb, info_only=True)
                          if by_fallback(e)]
                if leases and leases[0]["ts"]:
                    lease_at, source = leases[0]["ts"], f"api log: {short(leases[0])}"
            if lease_at is None:
                lease_at = infer_lease(poller.samples, hb + STALE_S)
                source = "/status: a queued job leased (±0.5 s)" if lease_at else None
            if lease_at is None:
                again = [d for d in st.events if d["type"] == "stage_started"
                         and d["seq"] > kill["event"]["seq"] and d.get("stage") in KILL_STAGES[a.kill_at]]
                if again:
                    lease_at, source = to_epoch(again[0]["ts"]), "the stage's restart event (an upper bound)"
            need(lease_at, "no sign of the fallback's lease: not in the log, /status or the events")
            t = takeover(hb, lease_at, kill["at"])
            data["takeover"] = dict(t, last_heartbeat=iso(hb), lease_at=iso(lease_at), kill_at=iso(kill["at"]),
                                    lease_source=source)
            ev.append(f"(a) {dead}'s last heartbeat {iso(hb)}; the fallback's lease at {iso(lease_at)} "
                      f"({source}): {t['after_last_heartbeat_s']} s after the heartbeat (bound "
                      f"{TAKEOVER_BOUND_S} s), {t['after_kill_s']} s after the kill (target "
                      f"{TAKEOVER_TARGET_S} s)")
            if not t["within_bound"]:
                problems.append(f"the takeover took {t['after_last_heartbeat_s']} s after the last "
                                f"heartbeat, over {TAKEOVER_BOUND_S} s")
            # (b) the scan
            run = scan["run"]
            if st.of("failed"):
                problems.append(f"the scan failed: {st.of('failed')[0].get('failure')}")
            if scan["status"] != "planned":
                problems.append(f"status {scan['status']}, not planned")
            if st.seen != set(range(1, st.last_seq + 1)):
                problems.append("seq isn't 1..N")
            last = {}
            for d in st.events:
                if d["type"] in ("stage_started", "stage_completed"):
                    last[d["stage"]] = d["type"]
            open_stages = sorted(s for s, typ in last.items() if typ == "stage_started")
            n_diag, n_ready = len(st.of("diagnosis", run=run)), len(st.of("plan_ready", run=run))
            ev.append(f"(b) {scan['status']}, seq 1..{st.last_seq} over {st.reconnects} reconnect(s), "
                      f"{n_diag} diagnosis, {n_ready} plan_ready, open stages {open_stages}")
            if open_stages or n_diag != 1 or n_ready != 1:
                problems.append(f"open stages {open_stages}, {n_diag} diagnosis, {n_ready} plan_ready")
            # (c) only the fallback after the kill
            killed = set(RIG_PERCEPTION_SOURCES if a.kill_at == "perception" else RIG_VLM_SOURCES)
            if a.manual_kill:
                killed |= RIG_VLM_SOURCES
            obs = scan.get("observations", [])
            before = [o for o in obs if (to_epoch(o.get("created_at")) or 0) < kill["at"]]
            after = [o for o in obs if (to_epoch(o.get("created_at")) or 0) >= kill["at"]]
            rig_versions = {o["model_version"] for o in before if o["source"] in
                            RIG_PERCEPTION_SOURCES | RIG_VLM_SOURCES} | set(a.rig_model_versions)
            def is_fallback(o):
                return (o["source"] == FALLBACK_SOURCE
                        or a.fallback_model in str(o.get("model_version") or ""))

            def name(o):
                return f"{o['stage']}/{o['source']}: {o['model_version']}"
            # With only one rig worker killed, the other's results are allowed; the model
            # versions then count only on the killed sources.
            rig_after = [name(o) for o in after if not is_fallback(o) and (
                o["source"] in killed or (a.manual_kill and o.get("model_version") in rig_versions))]
            fallback = sorted({name(o) for o in after if is_fallback(o)})
            others = sorted({name(o) for o in after if not is_fallback(o)
                             and o["source"] not in killed | {"user", "measurement"}})
            mv = (scan.get("diagnosis") or {}).get("model_versions") or {}
            ev.append(f"(c) after the kill: fallback {fallback}; from the killed models {rig_after}"
                      + (f"; from workers not killed {others}" if others else "")
                      + f"; diagnosis model_versions {mv}; plan {(scan.get('plan') or {}).get('llm_model')}")
            if rig_after:
                problems.append(f"observations from the killed rig models after the kill: {rig_after}")
            if not fallback:
                problems.append("no llm_fallback observation after the kill")
            if a.manual_kill and others:
                problems.append(f"with both rig workers killed, observations from other models: {others}")
            if not a.manual_kill and a.kill_at == "perception":
                ev.append("    the 5070 Ti's VLM worker wasn't killed (no --manual-kill), so its "
                          "observations are allowed")
            if a.manual_kill and any(v in rig_versions for v in mv.values()):
                problems.append(f"the diagnosis names a rig model version: {mv}")
            # (d) the queue drains
            def drained():
                latest = poller.samples[-1][1] if poller.samples else None
                return bool(latest) and latest["queue"] == {"queued": 0, "leased": 0}
            ok, dt, _ = self.wait_until(drained, 60, 1.0)
            last_q = (poller.samples[-1][1] or {}).get("queue") if poller.samples else None
            ev.append(f"(d) /status queue {last_q}" + (f", empty {dt:.0f} s after the plan" if ok else ""))
            if not ok:
                problems.append(f"the queue didn't drain within 60 s: {last_q}")
            # (e) a second scan while the rig is down
            sid2 = self.call(u, "POST", "/scans", {"garden_id": g, "new_plant": {
                "name": "s6 rig-down tomato", "species_id": "tomato"}}, expect=(201,), idem=True)["id"]
            u.api.upload(sid2, "whole_plant", a.photo)
            status, out = self.raw(u, "POST", f"/scans/{sid2}/submit")
            data["second_scan"] = {"scan_id": sid2, "submit_status": status}
            ev.append(f"(e) second scan {sid2} submitted while the rig is down: {status}")
            if status == 503:
                problems.append("a scan submitted while the rig is down got 503 upstream_unavailable "
                                "(the fix is part of S6's Rust changes)")
            elif status != 202:
                problems.append(f"the second submit answered {status}: {json.dumps(out)[:200]}")
            need(not problems, "; ".join(problems))
        finally:
            poller.stop.set()
            data["status_samples"] = poller.changes()
            if kill and a.rig_start_cmd:
                info("running --rig-start-cmd")
                subprocess.run(a.rig_start_cmd, shell=True, timeout=300)
            if kill and a.manual_kill:
                print(">>> the rig-failure check is done: start vlm-worker-nv on sam-linux again",
                      flush=True)

    def answering(self, user, sid):
        rules = self.args.answers

        def hook(_stream, d):
            if d["type"] != "question":
                return
            q = d["question"]
            if q.get("kind") == "indicator":
                answer = s4.answer_for(rules, q.get("indicator_id"))
                self.call(user, "POST", f"/scans/{sid}/answers",
                          {"question_id": q["id"], "option_id": answer}, expect=(202, 409))
                info(f"  answered {q.get('indicator_id')}: {answer}")
            else:
                info(f"  photo request ({q.get('photo_kind')}): left to the scheduler")
        return hook

    # --- the end of a run -------------------------------------------------------------------

    def after_seed(self):
        """One api restart for every seed that asked for one: a weather sweep 30 s later, the
        orphan sweep an hour later."""
        if not (self.restart_wanted and self.args.api_restart_cmd and self.args.phase == "seed"):
            return
        ev = []
        try:
            at = self.restart_api(ev)
        except Fail as e:
            info(f"FAIL: the seed's api restart: {e}")
            self.results.append(("seed-restart", "FAIL", [str(e)]))
            return
        for name in self.restart_wanted:
            self.state[name]["restarted_at"] = at
        if "alerts" in self.restart_wanted:
            time.sleep(WEATHER_FIRST_S + 20)
            st = self.state["alerts"]
            alerts = [((t.get("trigger") or {}).get("kind"), t.get("due"))
                      for t in self.alert_tasks(st["garden_id"])]
            ev.append(f"alerts {WEATHER_FIRST_S + 20} s after the restart: {alerts}")
        info("\n=== seed restart")
        for line in ev:
            info(f"  {line}")

    def cleanup(self):
        keep = set()
        if self.args.phase == "seed":
            keep = {self.state[k]["garden_id"] for k in PHASED if self.state.get(k)}
        for user, g in self.made:
            if g in keep or user.deleted:
                continue
            status, _ = self.raw(user, "DELETE", f"/gardens/{g}")
            info(f"cleanup: DELETE /gardens/{g} -> {status}")
        if self.args.phase == "check":
            for k in PHASED:
                if self.state.get(k):
                    status, _ = self.raw(self.main, "DELETE", f"/gardens/{self.state[k]['garden_id']}")
                    info(f"cleanup: DELETE /gardens/{self.state[k]['garden_id']} ({k}) -> {status}")
        for user in self.users:
            if not user.deleted:
                status, _ = self.raw(user, "DELETE", "/me")
                info(f"cleanup: DELETE /me as {user.email} -> {status}")

    def write_out(self):
        path = self.args.out
        if not path:
            return
        doc = {"script": "backend/tools/smoke/s6.py", "base_url": self.args.base_url,
               "written_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
               "phase": self.args.phase, "results": {n: v for n, v, _ in self.results},
               "evidence": {n: e for n, _, e in self.results}, "checks": {}}
        for check, data in self.fixture.items():
            d = dict(data)
            if "events" in d:
                d["events"] = [{"type": e["type"], "seq": e["seq"], "data": e} for e in d["events"]]
            doc["checks"][check] = d
        Path(path).write_text(json.dumps(s4.strip_signatures(doc), indent=1) + "\n")
        info(f"wrote {path}")

    def report(self):
        info("\n=== summary")
        for name, verdict, _ in self.results:
            info(f"  {verdict:4} {name}")
        return 1 if any(v == "FAIL" for _, v, _ in self.results) else 0


def main():
    stamp = time.strftime("%Y%m%d%H%M%S")
    ap = argparse.ArgumentParser(description="S6 live checks: push tokens and pruning, forecast "
                                 "alerts, deletion cascades, the orphan sweep, the rig failure.")
    ap.add_argument("--base-url", default=DEFAULT_BASE)
    ap.add_argument("--env-file", default="/srv/pdm/env/.env.api",
                    help="holds CLERK_SECRET_KEY (sk_test_) of the Clerk development instance the "
                         "api trusts (the environment wins); also RUSTFS_BUCKET")
    ap.add_argument("--email", default=f"s6-{stamp}+clerk_test@example.com",
                    help="the main test user; helper users get a tag before the +. In --phase "
                         "check and fresh, the state file's user is used")
    ap.add_argument("--phone-suffix", type=int, default=100 + int(stamp[-4:]) % 100,
                    help="the first Clerk test phone number +1555555<0100-0199>, if the instance "
                         "asks for one; each new user takes the next free one")
    ap.add_argument("--only", help=f"comma-separated checks, run in this order: {','.join(CHECKS)}; "
                    f"default every check in --phase seed, {','.join(PHASED)} otherwise")
    ap.add_argument("--phase", default="seed", choices=("seed", "check", "fresh"),
                    help="alerts and orphans run in phases: seed, (orphans) fresh, check")
    ap.add_argument("--state", default="s6-state.json", help="the phases' state file")
    ap.add_argument("--photo", help="whole-plant photo for the real uploads (plant, garden, account, "
                    "restart, orphans, rig-failure)")
    ap.add_argument("--leaf-photo", help="leaf close-up, uploaded as leaf_closeup in rig-failure")
    ap.add_argument("--answers", default="*=unsure",
                    help="rig-failure's indicator answers, as s4.py's: "
                         "concentric_rings_bullseye=yes,*=unsure")
    ap.add_argument("--lat", type=float, default=43.65)
    ap.add_argument("--lon", type=float, default=-79.38)
    ap.add_argument("--frost-lat", type=float, default=62.45,
                    help="the alerts garden's latitude (default Yellowknife)")
    ap.add_argument("--frost-lon", type=float, default=-114.37)
    ap.add_argument("--db-check", action="store_true", help="read SurrealDB (the root user of --db-env)")
    ap.add_argument("--db-env", default="/srv/pdm/env/.env.surrealdb",
                    help="SURREAL_USER/SURREAL_PASS (or SURREAL_ROOT_USER/SURREAL_ROOT_PASS)")
    ap.add_argument("--db-url", help="SurrealDB's HTTP base (default SURREAL_URL, else "
                    "http://127.0.0.1:8000)")
    ap.add_argument("--db-ns", default="pdm")
    ap.add_argument("--db-name", default="main")
    ap.add_argument("--rustfs-check", action="store_true",
                    help="list, head and (orphans) put objects in RustFS with --rustfs-env's keys")
    ap.add_argument("--rustfs-env", default="/srv/pdm/env/.env.rustfs",
                    help="RUSTFS_ACCESS_KEY and RUSTFS_SECRET_KEY")
    ap.add_argument("--rustfs-url", default="http://127.0.0.1:9000")
    ap.add_argument("--rustfs-bucket", help="default RUSTFS_BUCKET from --rustfs-env or --env-file, "
                    "else pdm-images")
    ap.add_argument("--api-log-cmd", help="shell command printing api's log (e.g. docker compose … "
                    "logs --no-color --since 3h api)")
    ap.add_argument("--api-restart-cmd", help="shell command that restarts api gracefully (seed "
                    "phase: a sweep right away; restart: resume the cascade)")
    ap.add_argument("--rustfs-stop-cmd", help="restart: stops RustFS")
    ap.add_argument("--rustfs-start-cmd", help="restart: starts RustFS (also run if the check fails)")
    ap.add_argument("--rig-kill-cmd", help="rig-failure: kills the worker holding the scan's job")
    ap.add_argument("--rig-start-cmd", help="rig-failure: starts it again at the end")
    ap.add_argument("--manual-kill", action="store_true",
                    help="rig-failure: also prompt for vlm-worker-nv's kill on sam-linux, and wait "
                         "for Enter; every observation after the kill must then be the fallback's")
    ap.add_argument("--kill-at", default="perception", choices=sorted(KILL_STAGES),
                    help="rig-failure: the first stage_started of these stages triggers the kill")
    ap.add_argument("--dead-worker", help="the killed worker's id (default rig-3070-perception-1, "
                    "or rig-5070ti-vlm-1 with --kill-at vlm)")
    ap.add_argument("--fallback-worker", default="cp-fallback-1",
                    help="rig-failure: the fallback worker's id, which must take the job over")
    ap.add_argument("--fallback-model", default="claude-opus-5-5",
                    help="rig-failure: the hosted model; a result naming it is the fallback's")
    ap.add_argument("--rig-model-versions", default="",
                    help="comma-separated model versions of the rig that must not appear after the kill")
    ap.add_argument("--real-token", help="prune: a real device's Expo token, registered for B")
    ap.add_argument("--token-seconds", type=int, default=600,
                    help="account, restart: the lifetime of A's minted session token")
    ap.add_argument("--delete-timeout", type=int, default=120)
    ap.add_argument("--prune-timeout", type=int, default=60)
    ap.add_argument("--account-timeout", type=int, default=300)
    ap.add_argument("--failure-timeout", type=int, default=300,
                    help="restart: seconds to wait for the deletion-failure log line")
    ap.add_argument("--retry-wait", type=int, default=180,
                    help="restart: seconds for the in-process retry before --api-restart-cmd")
    ap.add_argument("--timeout", type=int, default=900, help="rig-failure: seconds to plan_ready")
    ap.add_argument("--out", help="write the verdicts, evidence and the rig-failure scan's events, "
                    "final GET /scans/{id}, /status samples and takeover numbers as JSON")
    ap.add_argument("--cleanup", action="store_true",
                    help="delete the gardens the run made (not those a later phase needs; in "
                         "--phase check also the state's) and DELETE /me the helper users")
    ap.add_argument("--verbose", action="store_true", help="print every scan event")
    args = ap.parse_args()
    only = args.only or {"seed": ",".join(CHECKS), "check": ",".join(PHASED),
                         "fresh": "orphans"}[args.phase]
    args.only = {c.strip() for c in only.split(",") if c.strip()}
    unknown = args.only - set(CHECKS)
    if unknown:
        smoke.die(f"unknown checks: {sorted(unknown)}")
    for p in (args.photo, args.leaf_photo):
        if p and not Path(p).is_file():
            smoke.die(f"photo not found: {p}")
    args.answers = s4.parse_answers(args.answers)
    args.rig_model_versions = [v.strip() for v in args.rig_model_versions.split(",") if v.strip()]

    s = S6(args)
    if not s.main.api.healthy():
        smoke.die(f"{args.base_url}/healthz did not answer 200")
    try:
        for name in CHECKS:
            s.run_check(name, getattr(s, f"check_{name.replace('-', '_')}"))
        s.after_seed()
        if args.cleanup:
            s.cleanup()
    finally:
        if s.state:
            s.save_state()
        s.write_out()
    sys.exit(s.report())


if __name__ == "__main__":
    main()
