"""A fake VLM server for the vlm-worker: OpenAI-compatible, offline, answering with recorded
Qwen3-VL-8B replies to the worker's own requests (dev-roadmap §5, option 1; task M20).

    python3 -m ml.training.eval.fake_vlm                                        # replay
    python3 -m ml.training.eval.fake_vlm --record rec.json --upstream http://127.0.0.1:8000
    python3 -m ml.training.eval.fake_vlm --fault length --fault-every 3          # replay, faults

Point the worker at it with

    VLM_BASE_URL=http://127.0.0.1:8001 VLM_ADAPTER=vllm VLM_SERVED_NAME=vlm

(add VLM_API_KEY=… and --api-key … to test the key). Both body flavours are accepted: vLLM's
`max_completion_tokens`/`repetition_penalty` and llama.cpp's `max_tokens`/`repeat_penalty`.

Endpoints: GET /health (always open), GET /v1/models (lists --served-name), POST
/v1/chat/completions. With --api-key, /v1/* answers 401 unless `Authorization: Bearer <key>`.
In replay, a chat request whose `model` isn't --served-name answers 404, as vLLM does, so a wrong
VLM_SERVED_NAME shows offline (llama.cpp ignores the name; the fake is stricter there).

A request's key is the SHA-256 of its schema name, its schema as canonical JSON (sorted keys,
compact) and its number of images. The images themselves are not in the key. Two calls can share
a key: the worker's analyse `indicators` call and its `verify` call send the same schema and
differ only in their text.

Modes:

  replay (default): --recordings FILE (default: recordings/worker-qwen3vl8b-fp8-2026-10-16.json
    next to this file). A key hit answers with the recorded status and body unchanged, header
    `x-fake-vlm: replay`. With several recordings under one key, it takes the one whose message
    text (every text part, images left out) equals the request's, preferring status 200. When
    no recording under the key has the request's text (a prompt change that kept the schema), it
    still replays the first that answered 200 and logs a `text drift` line.
    A miss answers `x-fake-vlm: conformed`: the closest recording with the same schema name (most
    enum values shared with the request's schema, then the nearest image count, then the lowest
    key) has its JSON reply walked against the request's schema, keeping every value that fits
    (boxes, reasons, notes) and fixing the rest (see `conform`). With no recording of that kind
    (a `guided` call, or no file yet) the reply is built from the schema alone (`synthesize`).
    The same request always gets the same content.

  record: --record FILE --upstream URL. Each request goes unchanged to URL/v1/chat/completions,
    with `Authorization: Bearer $VLM_API_KEY` (--upstream-key-env names another variable; the key
    is never logged or written). The caller gets upstream's status and body unchanged, and FILE
    (`{"about", "models", "recordings": [...]}`) gets one entry per call with the key, the
    request without its image bytes, upstream's reply, the status and the wall time. FILE is
    rewritten atomically after each call, and an existing FILE is appended to. /v1/models is
    proxied, and recorded once.

Faults (replay only): --fault length (finish_reason `length`, content cut mid-JSON), http500,
http429 (with Retry-After: 1), bad-json (content that isn't JSON), slow (sleep --slow-seconds,
default 300, then answer). --fault-every N faults every Nth chat call (default 1: every call).

--log-requests DIR writes each chat request, without image bytes, as DIR/NNNN-<schema name>.json.

What it can't test: a prompt change (it replays the old answer and logs `text drift`),
real latency or throughput, and real image understanding (a conformed reply is valid, not
right). Standard library only (Python 3.11+).
"""

from __future__ import annotations

import argparse
import copy
import datetime as dt
import difflib
import hashlib
import json
import os
import re
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_RECORDINGS = HERE / "recordings/worker-qwen3vl8b-fp8-2026-10-16.json"
DEFAULT_PORT = 8001
SERVED_NAME = "vlm"
FAULTS = ["length", "http500", "http429", "bad-json", "slow"]
IMAGE_ID = re.compile(r"img(\d+)")


def log(message: str) -> None:
    print(f"fake_vlm: {message}", file=sys.stderr, flush=True)


# ---------------------------------------------------------------------------------------------
# Requests


def canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def json_schema(body: dict) -> tuple[str, dict]:
    """The request's `response_format.json_schema` name and schema."""
    spec = (body.get("response_format") or {}).get("json_schema") or {}
    return spec.get("name", ""), spec.get("schema") or {}


def image_count(body: dict) -> int:
    return sum(1 for m in body.get("messages", []) if isinstance(m.get("content"), list)
               for part in m["content"] if part.get("type") == "image_url")


def image_ids(n_images: int) -> list[str]:
    """The worker's aliases, `img1`…`imgN` (contracts 6)."""
    return [f"img{i}" for i in range(1, n_images + 1)]


def request_key(body: dict) -> str:
    """SHA-256 over the schema name, the canonical schema and the number of images."""
    name, schema = json_schema(body)
    text = f"{name}\n{canonical(schema)}\n{image_count(body)}"
    return hashlib.sha256(text.encode()).hexdigest()


def message_text(body: dict) -> str:
    """Each message's role and text parts, in order, without the images: what tells apart two
    requests under one key. A recording's request is redacted, so only text is compared."""
    out = []
    for m in body.get("messages", []):
        content = m.get("content")
        if isinstance(content, list):
            texts = [p.get("text", "") for p in content if p.get("type") == "text"]
        else:
            texts = [content or ""]
        out.append([m.get("role", ""), texts])
    return canonical(out)


def redact(body: dict) -> dict:
    """The request without its image bytes (as vlm_check.redact, which needs httpx to import)."""
    out = copy.deepcopy(body)
    for m in out.get("messages", []):
        if isinstance(m.get("content"), list):
            for part in m["content"]:
                if part.get("type") == "image_url":
                    url = part["image_url"]["url"]
                    part["image_url"]["url"] = f"<jpeg data url, {len(url)} chars>"
    return out


# ---------------------------------------------------------------------------------------------
# Building a valid instance from a schema


def types_of(schema: dict) -> list[str]:
    t = schema.get("type")
    if t is None:
        return []
    return t if isinstance(t, list) else [t]


def type_name(value) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "integer" if value.is_integer() else "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    return "object" if isinstance(value, dict) else "unknown"


def type_fits(value, schema: dict) -> bool:
    """Whether `value` has one of the schema's types (an integer is also a number)."""
    if "const" in schema:
        return value == schema["const"]
    if "enum" in schema and "type" not in schema:
        return value in schema["enum"]
    if "anyOf" in schema:
        return any(type_fits(value, b) for b in schema["anyOf"])
    types = types_of(schema)
    t = type_name(value)
    return not types or t in types or (t == "integer" and "number" in types)


def synthesize(schema: dict, image_ids: list[str] | None = None):
    """A minimal valid instance of `schema`: null where allowed, else the first enum value, the
    lowest number, the shortest string, the fewest items."""
    if "const" in schema:
        return schema["const"]
    if "enum" in schema:
        return schema["enum"][0]
    if "anyOf" in schema:
        branches = schema["anyOf"]
        nulls = [b for b in branches if "null" in types_of(b)]
        return None if nulls else synthesize(branches[0], image_ids)
    types = types_of(schema)
    if not types or "null" in types:
        return None
    t = types[0]
    if t == "object":
        props = schema.get("properties", {})
        return {k: synthesize(props.get(k, {}), image_ids) for k in required(schema)}
    if t == "array":
        return [synthesize(schema.get("items", {}), image_ids) for _ in range(schema.get("minItems", 0))]
    if t == "string":
        return "x" * schema.get("minLength", 0)
    if t in ("integer", "number"):
        low = schema.get("minimum", 0)
        high = schema.get("maximum")
        value = low if high is None or low <= high else high
        return int(value) if t == "integer" else value
    if t == "boolean":
        return False
    return None


def required(schema: dict) -> list[str]:
    """Required keys in the schema's property order (the generation order), then the rest."""
    req = schema.get("required", [])
    props = list(schema.get("properties", {}))
    return [k for k in props if k in req] + [k for k in req if k not in props]


def pick_enum(value, options: list, image_ids: list[str] | None) -> object:
    """A valid enum value for `value`: itself when listed; for an image id, the same position
    or the last image; else `other` when listed; else the closest string; else the first."""
    if value in options:
        return value
    if isinstance(value, str) and (m := IMAGE_ID.fullmatch(value)):
        ids = [o for o in options if isinstance(o, str) and IMAGE_ID.fullmatch(o)]
        if image_ids:
            ids = [o for o in ids if o in image_ids] or ids
        if ids:
            ids.sort(key=lambda o: int(o[3:]))
            n = int(m.group(1))
            return next((o for o in ids if int(o[3:]) == n), ids[-1])
    if "other" in options:
        return "other"
    strings = [o for o in options if isinstance(o, str)]
    if isinstance(value, str) and strings:
        close = difflib.get_close_matches(value, strings, n=1, cutoff=0.0)
        if close:
            return close[0]
    return options[0]


def cut(text: str, limit: int) -> str:
    """`text` cut to `limit` characters, on a word boundary when one is in the second half."""
    if len(text) <= limit:
        return text
    head = text[:limit]
    space = head.rfind(" ")
    if text[limit] != " " and space >= limit // 2:
        head = head[:space]
    return head.rstrip()


def conform_object(value: dict, schema: dict, image_ids):
    props = schema.get("properties", {})
    closed = schema.get("additionalProperties", True) is False
    unknown = [v for k, v in value.items() if k not in props]
    out = {}
    for k in required(schema):
        if k in value:
            out[k] = conform(value[k], props.get(k, {}), image_ids)
        elif unknown:
            # A renamed key (indicators' `q<n>_<slug>`): carry over the dropped values in order.
            out[k] = conform(unknown.pop(0), props.get(k, {}), image_ids)
        else:
            out[k] = synthesize(props.get(k, {}), image_ids)
    for k, v in value.items():
        if k in out:
            continue
        if k in props:
            out[k] = conform(v, props[k], image_ids)
        elif not closed:
            out[k] = v
    return out


def conform_array(value: list, schema: dict, image_ids):
    items = schema.get("items", {})
    out = [conform(v, items, image_ids) for v in value]
    if "maxItems" in schema:
        out = out[: schema["maxItems"]]
    while len(out) < schema.get("minItems", 0):
        out.append(synthesize(items, image_ids))
    return out


def conform_number(value, schema: dict, t: str):
    if "minimum" in schema:
        value = max(value, schema["minimum"])
    if "maximum" in schema:
        value = min(value, schema["maximum"])
    return int(round(value)) if t == "integer" else value


def conform_string(value: str, schema: dict):
    if "maxLength" in schema:
        value = cut(value, schema["maxLength"])
    if len(value) < schema.get("minLength", 0):
        value = value.ljust(schema["minLength"], ".")
    return value


def conform_any_of(value, schema: dict, image_ids):
    branches = schema["anyOf"]
    nullable = any("null" in types_of(b) for b in branches)
    if value is None and nullable:
        return None
    for b in branches:
        if value is not None and type_fits(value, b):
            return conform(value, b, image_ids)
    return None if nullable else conform(value, branches[0], image_ids)


def conform(value, schema: dict, image_ids: list[str] | None = None):
    """`value` made valid for `schema`, keeping whatever already fits.

    Walks `const`, `anyOf` (the branch the value's type fits; null when allowed and none does),
    `type` (a mismatch is replaced by `synthesize`), `enum` (see `pick_enum`), object
    `properties`/`required`/`additionalProperties: false` (unknown keys dropped, and their
    values carried over in order into missing required keys), array `items`/`minItems`/
    `maxItems`, string `maxLength`/`minLength` (cut on a word boundary) and number
    `minimum`/`maximum` (clamped; integers rounded)."""
    if "const" in schema:
        return schema["const"]
    if "anyOf" in schema:
        return conform_any_of(value, schema, image_ids)
    types = types_of(schema)
    t = type_name(value)
    if t == "integer" and "integer" not in types and "number" in types:
        t = "number"
    if types and t not in types:
        if "null" in types:
            return None
        if t == "number" and "integer" in types:
            t = "integer"
        else:
            return synthesize(schema, image_ids)
    if "enum" in schema:
        return pick_enum(value, schema["enum"], image_ids)
    if t == "object":
        return conform_object(value, schema, image_ids)
    if t == "array":
        return conform_array(value, schema, image_ids)
    if t in ("integer", "number"):
        return conform_number(value, schema, t)
    if t == "string":
        return conform_string(value, schema)
    return value


# ---------------------------------------------------------------------------------------------
# Recordings


def load_recordings(path: Path | None) -> dict:
    if path is None or not path.exists():
        return {"about": "", "recordings": []}
    return json.loads(path.read_text())


def write_atomic(path: Path, doc: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp")
    with os.fdopen(fd, "w") as f:
        json.dump(doc, f, indent=1, ensure_ascii=False)
        f.write("\n")
    os.replace(tmp, path)


def enum_values(schema) -> set[str]:
    """Every enum value in a schema, as JSON text."""
    out = set()
    if isinstance(schema, dict):
        for k, v in schema.items():
            if k == "enum" and isinstance(v, list):
                out.update(canonical(x) for x in v)
            else:
                out |= enum_values(v)
    elif isinstance(schema, list):
        for v in schema:
            out |= enum_values(v)
    return out


def reply_content(response) -> object:
    """The recorded reply's parsed JSON content, or None when it has none."""
    try:
        return json.loads(response["choices"][0]["message"]["content"])
    except (KeyError, IndexError, TypeError, ValueError):
        return None


def closest(recordings: list[dict], name: str, schema: dict, n_images: int) -> dict | None:
    """The usable recording of the same kind most like the request (see the module doc)."""
    wanted = enum_values(schema)
    usable = [r for r in recordings if r.get("schema_name") == name and r.get("status") == 200
              and reply_content(r.get("response")) is not None]
    if not usable:
        return None

    def score(r):
        theirs = enum_values(json_schema(r.get("request", {}))[1])
        return (-len(wanted & theirs), abs(r.get("n_images", 0) - n_images), r.get("key", ""))

    return min(usable, key=score)


# ---------------------------------------------------------------------------------------------
# The server


class Reply:
    def __init__(self, status: int, body: bytes, headers: dict | None = None):
        self.status = status
        self.body = body
        self.headers = headers or {}


def json_reply(status: int, obj, headers: dict | None = None) -> Reply:
    return Reply(status, json.dumps(obj).encode(), headers)


def error_reply(status: int, message: str, kind: str, headers: dict | None = None) -> Reply:
    return json_reply(status, {"error": {"message": message, "type": kind, "param": None,
                                         "code": status}}, headers)


def completion(model: str, content: str, finish_reason: str, prompt_tokens: int, n: int) -> dict:
    completion_tokens = max(1, len(content) // 4)
    return {
        "id": f"chatcmpl-fake-{n:06d}",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": model,
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
                     "finish_reason": finish_reason}],
        "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens,
                  "total_tokens": prompt_tokens + completion_tokens},
    }


class State:
    """Everything the handlers share. `lock` guards the counter and the recordings."""

    def __init__(self, served_name=SERVED_NAME, api_key=None, recordings=None, record=None,
                 upstream=None, upstream_key_env="VLM_API_KEY", fault=None, fault_every=1,
                 slow_seconds=300.0, log_requests=None):
        self.served_name = served_name
        self.api_key = api_key
        self.record = Path(record) if record else None
        self.upstream = upstream.rstrip("/").removesuffix("/v1") if upstream else None
        self.upstream_key_env = upstream_key_env
        self.fault = fault
        self.fault_every = max(1, fault_every)
        self.slow_seconds = slow_seconds
        self.log_requests = Path(log_requests) if log_requests else None
        self.lock = threading.Lock()
        self.calls = 0
        source = self.record if self.record else (Path(recordings) if recordings else None)
        self.doc = load_recordings(source)
        if self.record:
            self.doc.setdefault("recordings", [])
            self.doc["about"] = (f"Recorded by ml/training/eval/fake_vlm.py from {self.upstream}; "
                                 "requests without image bytes, replies as the server sent them.")
        elif source is None or not source.exists():
            log(f"warning: no recordings at {source}; every reply is built from the schema alone")

    # --- record mode ---

    def forward(self, method: str, path: str, data: bytes | None) -> tuple[int, bytes]:
        req = urllib.request.Request(f"{self.upstream}{path}", data=data, method=method)
        if data is not None:
            req.add_header("Content-Type", "application/json")
        key = os.environ.get(self.upstream_key_env)
        if key:
            req.add_header("Authorization", f"Bearer {key}")
        try:
            with urllib.request.urlopen(req, timeout=600) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def save(self, entry_key: str, entry) -> None:
        with self.lock:
            if entry_key == "models":
                self.doc["models"] = entry
            else:
                self.doc["recordings"].append(entry)
            write_atomic(self.record, self.doc)

    def record_chat(self, raw: bytes, body: dict) -> Reply:
        start = time.monotonic()
        try:
            status, data = self.forward("POST", "/v1/chat/completions", raw)
        except (urllib.error.URLError, OSError) as e:
            return error_reply(502, f"upstream unreachable: {e}", "upstream_error")
        try:
            response = json.loads(data)
        except ValueError:
            response = data.decode(errors="replace")
        name, _ = json_schema(body)
        self.save("chat", {
            "key": request_key(body), "schema_name": name, "n_images": image_count(body),
            "request": redact(body), "response": response, "status": status,
            "wall_s": round(time.monotonic() - start, 3),
        })
        log(f"recorded {name} ({image_count(body)} images): {status}")
        return Reply(status, data, {"Content-Type": "application/json"})

    def models(self) -> Reply:
        if not self.record:
            return json_reply(200, {"object": "list", "data": [
                {"id": self.served_name, "object": "model", "created": 0, "owned_by": "fake_vlm"}]})
        try:
            status, data = self.forward("GET", "/v1/models", None)
        except (urllib.error.URLError, OSError) as e:
            return error_reply(502, f"upstream unreachable: {e}", "upstream_error")
        if status == 200 and "models" not in self.doc:
            self.save("models", json.loads(data))
        return Reply(status, data, {"Content-Type": "application/json"})

    # --- replay mode ---

    def replay(self, body: dict, n: int) -> tuple[Reply, str]:
        """The reply and how it was made: `replay`, `conformed` or `synthesised`."""
        key = request_key(body)
        name, schema = json_schema(body)
        hits = [r for r in self.doc.get("recordings", []) if r.get("key") == key]
        if hits:
            text = message_text(body)
            same = [r for r in hits if message_text(r.get("request") or {}) == text]
            if not same:
                log(f"text drift: {name} matches key {key[:12]}, but none of its {len(hits)} "
                    "recordings has this request's text; replaying the first")
            pool = same or hits
            hit = next((r for r in pool if r.get("status") == 200), pool[0])
            response = hit["response"]
            data = response.encode() if isinstance(response, str) else json.dumps(response).encode()
            return Reply(hit.get("status", 200), data), "replay"
        n_images = image_count(body)
        ids = image_ids(n_images)
        base = closest(self.doc.get("recordings", []), name, schema, n_images)
        if base is None:
            value, how, prompt_tokens = synthesize(schema, ids), "synthesised", 0
            log(f"synthesised {name} ({n_images} images): no recording of this kind")
        else:
            value = conform(reply_content(base["response"]), schema, ids)
            how = "conformed"
            prompt_tokens = (base["response"].get("usage") or {}).get("prompt_tokens", 0)
            log(f"conformed {name} ({n_images} images) from recording {base['key'][:12]}")
        content = json.dumps(value, ensure_ascii=False)
        model = body.get("model", self.served_name)
        return json_reply(200, completion(model, content, "stop", prompt_tokens, n)), how

    def faulted(self, body: dict, n: int) -> Reply | None:
        """The fault's reply for call `n`, or None to answer normally."""
        if not self.fault or n % self.fault_every:
            return None
        if self.fault == "http500":
            return error_reply(500, "fake_vlm: injected fault", "server_error")
        if self.fault == "http429":
            return error_reply(429, "fake_vlm: injected fault", "rate_limit", {"Retry-After": "1"})
        if self.fault == "slow":
            time.sleep(self.slow_seconds)
            return None
        model = body.get("model", self.served_name)
        if self.fault == "bad-json":
            return json_reply(200, completion(model, "Sure! Here is the JSON you asked for: {",
                                              "stop", 0, n))
        reply, _ = self.replay(body, n)  # length
        content = json.loads(reply.body)["choices"][0]["message"]["content"]
        return json_reply(200, completion(model, content[: max(1, len(content) // 2)],
                                          "length", 0, n))

    def chat(self, raw: bytes) -> Reply:
        try:
            body = json.loads(raw)
        except ValueError:
            return error_reply(400, "the body is not JSON", "invalid_request_error")
        model = body.get("model")
        if not self.record and model is not None and model != self.served_name:
            return error_reply(404, f"The model `{model}` does not exist.", "NotFoundError")
        name, schema = json_schema(body)
        if not name or not schema:
            return error_reply(400, "fake_vlm needs response_format.json_schema with a name and "
                                    "a schema", "invalid_request_error")
        with self.lock:
            self.calls += 1
            n = self.calls
        if self.log_requests:
            self.log_requests.mkdir(parents=True, exist_ok=True)
            safe = re.sub(r"[^A-Za-z0-9_.-]", "_", name)
            (self.log_requests / f"{n:04d}-{safe}.json").write_text(
                json.dumps(redact(body), indent=1, ensure_ascii=False) + "\n")
        if self.record:
            return self.record_chat(raw, body)
        fault = self.faulted(body, n)
        if fault is not None:
            log(f"fault {self.fault} on call {n} ({name})")
            fault.headers["x-fake-vlm"] = f"fault-{self.fault}"
            return fault
        reply, how = self.replay(body, n)
        reply.headers["x-fake-vlm"] = how
        return reply


class Handler(BaseHTTPRequestHandler):
    server_version = "fake_vlm/1"
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):  # noqa: A002 (the base class's name)
        pass

    @property
    def state(self) -> State:
        return self.server.state

    def send(self, reply: Reply) -> None:
        self.send_response(reply.status)
        headers = {"Content-Type": "application/json", **reply.headers}
        for k, v in headers.items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(reply.body)))
        self.end_headers()
        self.wfile.write(reply.body)

    def authorised(self) -> bool:
        if self.state.api_key is None:
            return True
        if self.headers.get("Authorization") == f"Bearer {self.state.api_key}":
            return True
        self.send(error_reply(401, "missing or wrong API key", "authentication_error"))
        return False

    def do_GET(self):
        path = self.path.split("?")[0].rstrip("/")
        if path == "/health":
            self.send(Reply(200, b""))
        elif path == "/v1/models":
            if self.authorised():
                self.send(self.state.models())
        else:
            self.send(error_reply(404, f"no route {self.path}", "not_found"))

    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        if self.path.split("?")[0].rstrip("/") != "/v1/chat/completions":
            self.send(error_reply(404, f"no route {self.path}", "not_found"))
        elif self.authorised():
            self.send(self.state.chat(raw))


class FakeVlmServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address: tuple[str, int], state: State):
        super().__init__(address, Handler)
        self.state = state


def make_server(host: str = "127.0.0.1", port: int = DEFAULT_PORT, **options) -> FakeVlmServer:
    """A server bound to `host:port` (0 picks a free port), not yet serving. `options` are
    `State`'s: served_name, api_key, recordings, record, upstream, upstream_key_env, fault,
    fault_every, slow_seconds, log_requests."""
    return FakeVlmServer((host, port), State(**options))


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                 formatter_class=argparse.RawDescriptionHelpFormatter,
                                 epilog="See the module docstring (ml/training/eval/fake_vlm.py).")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=DEFAULT_PORT)
    ap.add_argument("--served-name", default=SERVED_NAME)
    ap.add_argument("--api-key", default=None, help="require this Bearer key on /v1/*")
    ap.add_argument("--recordings", default=str(DEFAULT_RECORDINGS), help="replay from this file")
    ap.add_argument("--record", default=None, help="record mode: append calls to this file")
    ap.add_argument("--upstream", default=None, help="record mode: the real server's base URL")
    ap.add_argument("--upstream-key-env", default="VLM_API_KEY",
                    help="environment variable holding upstream's key (default VLM_API_KEY)")
    ap.add_argument("--fault", choices=FAULTS, default=None)
    ap.add_argument("--fault-every", type=int, default=1, help="fault every Nth chat call")
    ap.add_argument("--slow-seconds", type=float, default=300.0)
    ap.add_argument("--log-requests", default=None, help="write each chat request to this folder")
    args = ap.parse_args(argv)
    if bool(args.record) != bool(args.upstream):
        ap.error("--record and --upstream go together")
    if args.record and args.fault:
        ap.error("--fault works in replay mode only")
    server = make_server(
        args.host, args.port, served_name=args.served_name, api_key=args.api_key,
        recordings=None if args.record else args.recordings, record=args.record,
        upstream=args.upstream, upstream_key_env=args.upstream_key_env, fault=args.fault,
        fault_every=args.fault_every, slow_seconds=args.slow_seconds,
        log_requests=args.log_requests)
    mode = f"recording to {args.record} from {args.upstream}" if args.record else \
        f"replaying {args.recordings}"
    log(f"listening on http://{args.host}:{server.server_address[1]}, {mode}, "
        f"served name {args.served_name}, {dt.datetime.now().isoformat(timespec='seconds')}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
