"""The fake VLM server: replay, conform, synthesise, record through an in-process upstream,
the key, and the faults. Requests are shaped like the vlm-worker's
(backend/crates/integrations/src/openai_compat.rs; schemas from backend/bins/worker/src/vlm/schema.rs)."""

import json
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from ml.training.eval import fake_vlm as fv
from ml.training.eval import vlm_check as vc

RECORDED_SPECIES = ["tomato", "potato", "basil"]
RECORDED_ORGANISMS = ["aphid", "whitefly", "slug"]
RECORDED_INDICATORS = ["tomato_early_blight.concentric_rings",
                       "tomato_septoria_leaf_spot.small_spots_dark_border"]
FAKE_JPEG = "data:image/jpeg;base64," + "QUJD" * 500


# --- requests shaped like the worker's ------------------------------------------------------


def guided_schema(kind: str) -> dict:
    """schema.rs `guided`: only the branch for the image's kind."""
    certainty = {"type": "string", "enum": ["low", "medium", "high"]}
    if kind == "squeeze_test":
        return vc._obj({
            "kind": {"type": "string", "enum": ["squeeze_test"]},
            "result": {"type": "string", "enum": ["crumbles", "holds_ball", "sticky_ribbon", "unclear"]},
            "certainty": certainty,
        })
    return vc._obj({
        "kind": {"type": "string", "enum": ["jar_test"]},
        "settled": {"type": "boolean"},
        "texture": {"type": "string", "enum": ["sandy", "loamy", "silty", "clayey", "unclear"]},
        "certainty": certainty,
    })


def chat_body(name: str, schema: dict, n_images: int, flavour: str = "vllm") -> dict:
    parts = []
    for alias in vc.aliases(n_images):
        parts.append({"type": "text", "text": f"{alias}: whole plant"})
        parts.append({"type": "image_url", "image_url": {"url": FAKE_JPEG}})
    parts.append({"type": "text", "text": "Answer in JSON."})
    body = {
        "model": "vlm",
        "messages": [{"role": "system", "content": "You look at plants."},
                     {"role": "user", "content": parts}],
        "temperature": 0, "top_p": 1, "presence_penalty": 0, "seed": 0,
        "response_format": {"type": "json_schema",
                            "json_schema": {"name": name, "schema": schema, "strict": True}},
        "stream": False,
    }
    if flavour == "vllm":
        body |= {"max_completion_tokens": 1500, "repetition_penalty": 1.0}
    else:
        body |= {"max_tokens": 1500, "repeat_penalty": 1.0}
    return body


def recorded_findings() -> dict:
    return {
        "species_guess": {"species": "basil", "other_name": None, "certainty": "medium"},
        "soil": {
            "moisture": {"value": "moist", "image": "img3", "certainty": "medium"},
            "surface": None, "structure": None, "colour": None, "cover": None,
            "deposits": None, "erosion": None, "weeds": None,
        },
        "organisms": [{"label": "aphid", "other_name": None, "count": "few", "image": "img2",
                       "bbox_2d": [112, 233, 345, 467], "certainty": "low"}],
        "damage": [{"trace": "flea beetles", "pattern": "shot_holes", "other_description": None, "image": "img3",
                    "bbox_2d": [10, 20, 990, 980], "certainty": "high"}],
        "notes": "Small round holes on the upper leaves, and a few aphids under one leaf.",
    }


def recorded_answers() -> dict:
    return {
        "q1_concentric_rings": {"reason": "rings visible on two spots", "answer": "yes", "image": "img2"},
        "q2_small_spots_dark_border": {"reason": "spots are large, not small", "answer": "no", "image": None},
    }


def openai_reply(content: dict, finish: str = "stop") -> dict:
    return {"id": "chatcmpl-abc", "object": "chat.completion", "created": 1, "model": "vlm",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": json.dumps(content),
                                                 "reasoning_content": None},
                         "finish_reason": finish}],
            "usage": {"prompt_tokens": 2345, "completion_tokens": 123, "total_tokens": 2468}}


def entry(body: dict, content: dict) -> dict:
    return {"key": fv.request_key(body), "schema_name": body["response_format"]["json_schema"]["name"],
            "n_images": fv.image_count(body), "request": fv.redact(body),
            "response": openai_reply(content), "status": 200, "wall_s": 1.0}


FINDINGS_BODY = chat_body("findings", vc.findings_schema(3, RECORDED_SPECIES, RECORDED_ORGANISMS), 3)
INDICATORS_BODY = chat_body("indicators", vc.indicators_schema(RECORDED_INDICATORS, 3), 3)


@pytest.fixture
def recordings(tmp_path):
    path = tmp_path / "rec.json"
    path.write_text(json.dumps({"about": "synthetic", "recordings": [
        entry(FINDINGS_BODY, recorded_findings()),
        entry(INDICATORS_BODY, recorded_answers()),
    ]}))
    return path


# --- running servers -------------------------------------------------------------------------


def serve(server):
    thread = threading.Thread(target=server.serve_forever, args=(0.05,), daemon=True)
    thread.start()
    return f"http://127.0.0.1:{server.server_address[1]}"


@pytest.fixture
def start():
    servers = []

    def run(**options):
        server = fv.make_server(port=0, **options)
        servers.append(server)
        return serve(server)

    yield run
    for s in servers:
        s.shutdown()
        s.server_close()


def call(base: str, path: str, body: dict | None = None, key: str | None = None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(base + path, data=data, method="POST" if data else "GET")
    req.add_header("Content-Type", "application/json")
    if key:
        req.add_header("Authorization", f"Bearer {key}")
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, dict(r.headers), r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()


def content_of(raw: bytes):
    return json.loads(json.loads(raw)["choices"][0]["message"]["content"])


# --- the key and the pure parts --------------------------------------------------------------


def test_request_key_ignores_images_and_key_order_but_not_their_count():
    other = json.loads(json.dumps(FINDINGS_BODY))
    other["messages"][1]["content"][1]["image_url"]["url"] = "data:image/jpeg;base64,ZZZZ"
    other["temperature"] = 0.5
    assert fv.request_key(other) == fv.request_key(FINDINGS_BODY)
    reordered = json.loads(json.dumps(FINDINGS_BODY))
    schema = reordered["response_format"]["json_schema"]["schema"]
    schema["properties"] = dict(reversed(schema["properties"].items()))
    assert fv.request_key(reordered) == fv.request_key(FINDINGS_BODY)
    fewer = chat_body("findings", vc.findings_schema(3, RECORDED_SPECIES, RECORDED_ORGANISMS), 2)
    assert fv.request_key(fewer) != fv.request_key(FINDINGS_BODY)


def test_conform_cuts_clamps_and_maps():
    assert fv.conform("one two three four", {"type": "string", "maxLength": 10}) == "one two"
    assert fv.conform("abcdefghijklmnop", {"type": "string", "maxLength": 5}) == "abcde"
    assert fv.conform(1500, {"type": "integer", "minimum": 0, "maximum": 1000}) == 1000
    assert fv.conform(3.6, {"type": "integer"}) == 4
    image = {"type": "string", "enum": ["img1", "img2"]}
    assert fv.conform("img4", image, ["img1", "img2"]) == "img2"
    assert fv.conform("img1", image, ["img1", "img2"]) == "img1"
    assert fv.conform("tomato", {"type": "string", "enum": ["pepper", "other"]}) == "other"
    nullable = {"anyOf": [{"type": "string", "maxLength": 3}, {"type": "null"}]}
    assert fv.conform(5, nullable) is None
    assert fv.conform("abcdef", nullable) == "abc"
    assert fv.conform("x", {"const": "y"}) == "y"


def test_synthesize_is_valid_for_every_worker_schema():
    for schema in [vc.findings_schema(2, ["tomato"], []), vc.indicators_schema(RECORDED_INDICATORS, 1),
                   guided_schema("squeeze_test"), guided_schema("jar_test")]:
        assert vc.validation_errors(schema, fv.synthesize(schema, ["img1", "img2"])) == []


# --- endpoints and auth ----------------------------------------------------------------------


def test_models_lists_the_served_name_and_health_is_open(start):
    base = start(recordings=None)
    status, _, raw = call(base, "/v1/models")
    assert status == 200
    assert [m["id"] for m in json.loads(raw)["data"]] == ["vlm"]
    assert call(base, "/health")[0] == 200


def test_api_key(start, recordings):
    base = start(recordings=recordings, api_key="s3cret")
    for key in (None, "wrong"):
        status, _, raw = call(base, "/v1/chat/completions", FINDINGS_BODY, key=key)
        assert status == 401 and json.loads(raw)["error"]["type"] == "authentication_error"
        assert call(base, "/v1/models", key=key)[0] == 401
    assert call(base, "/v1/chat/completions", FINDINGS_BODY, key="s3cret")[0] == 200
    assert call(base, "/v1/models", key="s3cret")[0] == 200
    assert call(base, "/health")[0] == 200


# --- replay, conform, synthesise -------------------------------------------------------------


@pytest.mark.parametrize("flavour", ["vllm", "llama_cpp"])
def test_replay_on_a_key_hit(start, recordings, flavour):
    base = start(recordings=recordings)
    body = chat_body("findings", vc.findings_schema(3, RECORDED_SPECIES, RECORDED_ORGANISMS), 3, flavour)
    status, headers, raw = call(base, "/v1/chat/completions", body)
    assert status == 200 and headers["x-fake-vlm"] == "replay"
    reply = json.loads(raw)
    assert reply == openai_reply(recorded_findings())
    assert reply["choices"][0]["finish_reason"] == "stop"


def with_text(body: dict, text: str) -> dict:
    """The body with its first text part changed, as the worker's verify call differs from the
    analyse indicators call: same schema and image count, other text."""
    out = json.loads(json.dumps(body))
    out["messages"][1]["content"][0]["text"] = text
    return out


def test_shared_key_replays_the_recording_with_the_same_text(start, tmp_path, capsys):
    analyse = with_text(INDICATORS_BODY, "img1: close-up of a leaf")
    verify = with_text(INDICATORS_BODY, "img1: new photo, taken to answer a question")
    assert fv.request_key(analyse) == fv.request_key(verify)
    verify_answers = {**recorded_answers(),
                      "q1_concentric_rings": {"reason": "no rings on the new photo", "answer": "no",
                                              "image": None}}
    path = tmp_path / "rec.json"
    path.write_text(json.dumps({"about": "synthetic", "recordings": [
        entry(analyse, recorded_answers()), entry(verify, verify_answers)]}))
    base = start(recordings=path)
    for body, want in ((verify, verify_answers), (analyse, recorded_answers())):
        status, headers, raw = call(base, "/v1/chat/completions", body)
        assert status == 200 and headers["x-fake-vlm"] == "replay"
        assert content_of(raw) == want
    assert "text drift" not in capsys.readouterr().err

    drifted = with_text(INDICATORS_BODY, "img1: a prompt neither recording has")
    status, headers, raw = call(base, "/v1/chat/completions", drifted)
    assert status == 200 and headers["x-fake-vlm"] == "replay"
    assert content_of(raw) == recorded_answers()
    assert "none of its 2 recordings has this request's text" in capsys.readouterr().err


def test_a_prompt_change_on_a_single_recording_is_replayed_and_logged(start, recordings, capsys):
    base = start(recordings=recordings)
    status, headers, raw = call(base, "/v1/chat/completions", with_text(FINDINGS_BODY, "img1: soil"))
    assert status == 200 and headers["x-fake-vlm"] == "replay"
    assert content_of(raw) == recorded_findings()
    assert "text drift: findings" in capsys.readouterr().err


@pytest.mark.parametrize("served", ["vlm", "qwen3-vl"])
def test_a_model_not_served_answers_404(start, recordings, served):
    base = start(recordings=recordings, served_name=served)
    status, _, raw = call(base, "/v1/chat/completions", {**FINDINGS_BODY, "model": "not-served"})
    assert status == 404
    assert json.loads(raw)["error"]["message"] == "The model `not-served` does not exist."
    assert call(base, "/v1/chat/completions", {**FINDINGS_BODY, "model": served})[0] == 200
    unnamed = {k: v for k, v in FINDINGS_BODY.items() if k != "model"}
    assert call(base, "/v1/chat/completions", unnamed)[0] == 200


def test_findings_with_new_enums_and_an_extra_image_are_conformed(start, recordings):
    base = start(recordings=recordings)
    schema = vc.findings_schema(4, ["pepper", "cucumber"], ["spider_mite", "aphid", "caterpillar"])
    status, headers, raw = call(base, "/v1/chat/completions", chat_body("findings", schema, 4))
    assert status == 200 and headers["x-fake-vlm"] == "conformed"
    reply = content_of(raw)
    assert vc.validation_errors(schema, reply) == []
    assert reply["species_guess"]["species"] == "other"
    assert reply["organisms"][0]["label"] == "aphid"
    assert reply["organisms"][0]["bbox_2d"] == [112, 233, 345, 467]
    assert reply["damage"][0]["bbox_2d"] == [10, 20, 990, 980]
    assert reply["notes"] == recorded_findings()["notes"]


def test_findings_with_fewer_images_map_image_ids(start, recordings):
    base = start(recordings=recordings)
    schema = vc.findings_schema(2, RECORDED_SPECIES, RECORDED_ORGANISMS)
    reply = content_of(call(base, "/v1/chat/completions", chat_body("findings", schema, 2))[2])
    assert vc.validation_errors(schema, reply) == []
    assert reply["soil"]["moisture"]["image"] == "img2"
    assert reply["species_guess"]["species"] == "basil"


def test_indicators_with_new_keys_are_conformed(start, recordings):
    base = start(recordings=recordings)
    ids = ["tomato_late_blight.water_soaked_patches", "aphids.honeydew", "powdery_mildew.white_powder"]
    schema = vc.indicators_schema(ids, 2, unsure_only=frozenset({"aphids.honeydew"}))
    status, headers, raw = call(base, "/v1/chat/completions", chat_body("indicators", schema, 2))
    assert headers["x-fake-vlm"] == "conformed"
    reply = content_of(raw)
    assert vc.validation_errors(schema, reply) == []
    assert list(reply) == ["q1_water_soaked_patches", "q2_honeydew", "q3_white_powder"]
    assert reply["q1_water_soaked_patches"]["reason"] == "rings visible on two spots"
    assert reply["q2_honeydew"]["answer"] == "unsure"


@pytest.mark.parametrize("kind", ["squeeze_test", "jar_test"])
def test_guided_without_a_recording_is_synthesised(start, recordings, kind):
    base = start(recordings=recordings)
    schema = guided_schema(kind)
    status, headers, raw = call(base, "/v1/chat/completions", chat_body("guided", schema, 1))
    assert status == 200 and headers["x-fake-vlm"] == "synthesised"
    assert vc.validation_errors(schema, content_of(raw)) == []


def test_guided_from_the_other_kinds_recording(start, tmp_path):
    jar = chat_body("guided", guided_schema("jar_test"), 1)
    path = tmp_path / "rec.json"
    path.write_text(json.dumps({"recordings": [entry(jar, {"kind": "jar_test", "settled": True,
                                                          "texture": "loamy", "certainty": "high"})]}))
    base = start(recordings=path)
    schema = guided_schema("squeeze_test")
    reply = content_of(call(base, "/v1/chat/completions", chat_body("guided", schema, 1))[2])
    assert vc.validation_errors(schema, reply) == []
    assert reply["kind"] == "squeeze_test" and reply["certainty"] == "high"


def test_missing_recordings_file_warns_and_synthesises(start, tmp_path, capsys):
    base = start(recordings=tmp_path / "none.json")
    assert "warning" in capsys.readouterr().err
    schema = vc.findings_schema(3, ["tomato"], ["aphid"])
    status, headers, raw = call(base, "/v1/chat/completions", chat_body("findings", schema, 3))
    assert headers["x-fake-vlm"] == "synthesised"
    assert vc.validation_errors(schema, content_of(raw)) == []


def test_same_request_same_content(start, recordings):
    base = start(recordings=recordings)
    body = chat_body("findings", vc.findings_schema(4, ["pepper"], ["mite"]), 4)
    first = json.loads(call(base, "/v1/chat/completions", body)[2])
    second = json.loads(call(base, "/v1/chat/completions", body)[2])
    assert first["choices"] == second["choices"] and first["usage"] == second["usage"]


def test_log_requests_writes_redacted_requests(start, recordings, tmp_path):
    base = start(recordings=recordings, log_requests=tmp_path / "log")
    call(base, "/v1/chat/completions", FINDINGS_BODY)
    call(base, "/v1/chat/completions", INDICATORS_BODY)
    files = sorted(p.name for p in (tmp_path / "log").iterdir())
    assert files == ["0001-findings.json", "0002-indicators.json"]
    assert "QUJD" not in (tmp_path / "log/0001-findings.json").read_text()


# --- faults ----------------------------------------------------------------------------------


def test_fault_length(start, recordings):
    base = start(recordings=recordings, fault="length")
    status, headers, raw = call(base, "/v1/chat/completions", FINDINGS_BODY)
    choice = json.loads(raw)["choices"][0]
    assert status == 200 and choice["finish_reason"] == "length"
    with pytest.raises(ValueError):
        json.loads(choice["message"]["content"])


@pytest.mark.parametrize("fault,status", [("http500", 500), ("http429", 429)])
def test_fault_status(start, recordings, fault, status):
    base = start(recordings=recordings, fault=fault)
    got, headers, raw = call(base, "/v1/chat/completions", FINDINGS_BODY)
    assert got == status and "error" in json.loads(raw)
    if status == 429:
        assert headers["Retry-After"] == "1"


def test_fault_bad_json(start, recordings):
    base = start(recordings=recordings, fault="bad-json")
    raw = call(base, "/v1/chat/completions", FINDINGS_BODY)[2]
    with pytest.raises(ValueError):
        json.loads(json.loads(raw)["choices"][0]["message"]["content"])


def test_fault_slow(start, recordings):
    base = start(recordings=recordings, fault="slow", slow_seconds=0.3)
    t = time.monotonic()
    status, headers, _ = call(base, "/v1/chat/completions", FINDINGS_BODY)
    assert status == 200 and time.monotonic() - t >= 0.3


def test_fault_every_counts_chat_calls(start, recordings):
    base = start(recordings=recordings, fault="http500", fault_every=2)
    call(base, "/v1/models")
    statuses = [call(base, "/v1/chat/completions", FINDINGS_BODY)[0] for _ in range(4)]
    assert statuses == [200, 500, 200, 500]


# --- record mode -----------------------------------------------------------------------------


class Upstream(BaseHTTPRequestHandler):
    """A stand-in for vLLM: needs `Bearer upstream-key`, answers findings."""

    protocol_version = "HTTP/1.1"
    seen: list = []

    def log_message(self, *args):
        pass

    def reply(self, status: int, obj) -> None:
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self.reply(200, {"object": "list", "data": [{"id": "vlm", "object": "model",
                                                      "max_model_len": 16384}]})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        Upstream.seen.append((self.headers.get("Authorization"), body))
        if self.headers.get("Authorization") != "Bearer upstream-key":
            self.reply(401, {"error": {"message": "no"}})
        else:
            self.reply(200, openai_reply(recorded_findings()))


def test_record_then_replay(start, tmp_path, monkeypatch):
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    upstream_url = serve(upstream)
    try:
        monkeypatch.setenv("VLM_API_KEY", "upstream-key")
        path = tmp_path / "rec.json"
        base = start(record=path, upstream=upstream_url + "/v1", api_key="worker-key")
        assert [m["id"] for m in json.loads(call(base, "/v1/models", key="worker-key")[2])["data"]] == ["vlm"]
        status, _, raw = call(base, "/v1/chat/completions", FINDINGS_BODY, key="worker-key")
    finally:
        upstream.shutdown()
        upstream.server_close()
    assert status == 200 and json.loads(raw) == openai_reply(recorded_findings())
    assert Upstream.seen[-1] == ("Bearer upstream-key", FINDINGS_BODY)
    text = path.read_text()
    assert "base64" not in text and "upstream-key" not in text and "worker-key" not in text
    doc = json.loads(text)
    assert doc["models"]["data"][0]["id"] == "vlm"
    (rec,) = doc["recordings"]
    assert rec["key"] == fv.request_key(FINDINGS_BODY)
    assert (rec["schema_name"], rec["n_images"], rec["status"]) == ("findings", 3, 200)
    assert rec["request"]["messages"][1]["content"][1]["image_url"]["url"].startswith("<jpeg data url, ")
    assert rec["response"] == openai_reply(recorded_findings())

    replay = start(recordings=path)
    status, headers, raw = call(replay, "/v1/chat/completions", FINDINGS_BODY)
    assert headers["x-fake-vlm"] == "replay"
    assert content_of(raw) == recorded_findings()


# --- the committed recordings -----------------------------------------------------------------


COMMITTED = sorted((fv.HERE / "recordings").glob("*.json"))


def recording_problems(doc: dict) -> list[str]:
    """What is wrong with a recordings file: a key that doesn't recompute from its redacted
    request, a schema name or image count that disagrees, or a 200 answer that isn't valid for
    its request's schema. `conform` keeps whatever already fits, so a valid answer comes back
    unchanged."""
    problems = []
    for i, r in enumerate(doc["recordings"]):
        name, schema = fv.json_schema(r["request"])
        n = fv.image_count(r["request"])
        if fv.request_key(r["request"]) != r["key"]:
            problems.append(f"{i}: key")
        if name != r["schema_name"] or n != r["n_images"]:
            problems.append(f"{i}: schema name or image count")
        if r["status"] == 200:
            content = fv.reply_content(r["response"])
            if content is None or fv.conform(content, schema, fv.image_ids(n)) != content:
                problems.append(f"{i}: answer")
    return problems


def test_the_default_recordings_are_committed():
    assert fv.DEFAULT_RECORDINGS in COMMITTED


@pytest.mark.parametrize("path", COMMITTED, ids=lambda p: p.name)
def test_each_committed_recording_keys_and_validates(path):
    doc = fv.load_recordings(path)
    assert doc["recordings"]
    assert recording_problems(doc) == []


def test_recording_problems_catches_a_stale_key_and_an_invalid_answer():
    doc = fv.load_recordings(fv.DEFAULT_RECORDINGS)
    first = doc["recordings"][0]
    first["key"] = "0" * 64
    answer = fv.reply_content(first["response"])
    answer.pop(next(iter(answer)))  # drop a required field
    first["response"]["choices"][0]["message"]["content"] = json.dumps(answer)
    assert recording_problems(doc) == ["0: key", "0: answer"]
