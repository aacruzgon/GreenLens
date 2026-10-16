"""The VLM day-1 check's pure parts (image preparation, schemas, boxes), and one offline run
against a mocked server."""

import json

import httpx
import jsonschema
import pytest
from PIL import Image

from ml.training.eval import vlm_check as vc

SPECIES = ["tomato", "potato", "basil"]
ORGANISMS = [s for s, _ in vc.STANDIN_ORGANISMS]
INDICATORS = [i for i, _, _ in vc.SAMPLE_INDICATORS]


def valid_findings() -> dict:
    return {
        "species_guess": {"species": "tomato", "other_name": None, "certainty": "medium"},
        "soil": {
            "moisture": {"value": "moist", "image": "img4", "certainty": "medium"},
            "surface": None, "structure": None, "colour": None, "cover": None,
            "deposits": None, "erosion": None, "weeds": None,
        },
        "organisms": [{"label": "aphid", "other_name": None, "count": "few", "image": "img3",
                       "bbox_2d": [100, 200, 300, 400], "certainty": "low"}],
        "damage": [
            {"pattern": "shot_holes", "other_description": None, "image": "img2",
             "bbox_2d": [0, 0, 1000, 1000], "certainty": "high"},
            {"pattern": "other", "other_description": "pale stippling", "image": "img2",
             "bbox_2d": None, "certainty": "low"},
        ],
        "notes": "",
    }


def valid_answers() -> dict:
    return {vc.indicator_key(i, ind): {"reason": "rings visible on two spots", "answer": "yes", "image": "img2"}
            for i, ind in enumerate(INDICATORS, 1)}


# --- image preparation -----------------------------------------------------------------------


def test_prepare_2048x1536():
    im = Image.new("RGB", (2048, 1536), (90, 140, 60))
    assert vc.prepare(im, 1_003_520, 32).size == (1152, 864)


def test_prepare_small_rounds_down_only():
    im = Image.new("RGB", (800, 600))
    assert vc.prepare(im, 1_003_520, 32).size == (800, 576)


@pytest.mark.parametrize("size", [(300, 200), (31, 900), (640, 480), (4032, 3024), (1000, 3000)])
def test_prepare_never_upscales_and_stays_in_budget(size):
    w, h = vc.target_size(size, 1_003_520, 32)
    assert w % 32 == 0 and h % 32 == 0
    assert w * h <= 1_003_520 or size[0] < 32 or size[1] < 32
    assert w <= max(size[0], 32) and h <= max(size[1], 32)


def test_data_url_is_jpeg():
    url = vc.to_data_url(Image.new("RGB", (64, 32)))
    assert url.startswith("data:image/jpeg;base64,/9j/")


# --- schemas ---------------------------------------------------------------------------------


def test_findings_schema_is_valid_and_accepts_example():
    schema = vc.findings_schema(4, SPECIES, ORGANISMS)
    jsonschema.Draft202012Validator.check_schema(schema)
    assert vc.validation_errors(schema, valid_findings()) == []
    assert vc.box_problems(valid_findings()) == []


def test_findings_schema_rejects_bad_box_and_alias():
    schema = vc.findings_schema(4, SPECIES, ORGANISMS)
    bad = valid_findings()
    bad["organisms"][0]["bbox_2d"] = [100, 200, 300, 1001]
    assert vc.validation_errors(schema, bad)
    bad = valid_findings()
    bad["organisms"][0]["bbox_2d"] = [100, 200, 300]
    assert vc.validation_errors(schema, bad)
    bad = valid_findings()
    bad["damage"][0]["image"] = "img5"
    assert vc.validation_errors(schema, bad)
    bad = valid_findings()
    bad["notes"] = "x" * 201
    assert vc.validation_errors(schema, bad)


def test_findings_schema_follows_the_grammar_rules():
    """Every object closed and fully required; no `pattern` next to `maxLength` (contracts 6)."""
    def walk(node):
        if isinstance(node, dict):
            if node.get("type") == "object":
                assert node["additionalProperties"] is False
                assert node["required"] == list(node["properties"])
            assert not ("maxLength" in node and ("pattern" in node or "format" in node))
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(vc.findings_schema(4, SPECIES, ORGANISMS))
    walk(vc.indicators_schema(INDICATORS))


def test_box_problems_finds_zero_area():
    f = valid_findings()
    f["organisms"][0]["bbox_2d"] = [300, 200, 300, 400]
    assert vc.box_problems(f) == ["organisms[0]: [300, 200, 300, 400] has no area"]


def test_indicators_schema():
    schema = vc.indicators_schema(INDICATORS, 4)
    jsonschema.Draft202012Validator.check_schema(schema)
    assert list(schema["properties"]) == ["q1_concentric_rings", "q2_small_spots_dark_border",
                                          "q3_water_soaked_patches"]
    assert vc.validation_errors(schema, valid_answers()) == []
    missing = valid_answers()
    missing.pop("q2_small_spots_dark_border")
    assert vc.validation_errors(schema, missing)
    long_reason = valid_answers()
    long_reason["q1_concentric_rings"]["reason"] = "x" * 61
    assert vc.validation_errors(schema, long_reason)


def test_indicator_key_cuts_slug_to_24():
    assert vc.indicator_key(7, "x.abcdefghijklmnopqrstuvwxyz") == "q7_abcdefghijklmnopqrstuvwx"


def test_unsure_only_indicator():
    schema = vc.indicators_schema(INDICATORS, 4, unsure_only=frozenset({INDICATORS[0]}))
    assert vc.validation_errors(schema, valid_answers())  # "yes" not allowed for q1


def test_species_options_from_standin_labels():
    opts = dict(vc.species_options())
    assert opts["tomato"] == "Tomato" and "aphid" not in opts


# --- boxes -----------------------------------------------------------------------------------


def test_norm1000_to_px():
    assert vc.norm1000_to_px([100, 200, 500, 1000], 1152, 864) == pytest.approx((115.2, 172.8, 576.0, 864.0))
    assert vc.norm1000_to_px([0, 0, 1000, 1000], 800, 576) == (0, 0, 800, 576)


# --- an offline run against a mocked server --------------------------------------------------


def mock_server(request: httpx.Request) -> httpx.Response:
    if request.url.path.endswith("/models"):
        return httpx.Response(200, json={"data": [{"id": "vlm", "root": "/models/Qwen3-VL-8B-Instruct-FP8"}]})
    body = json.loads(request.content)
    name = body["response_format"]["json_schema"]["name"]
    assert body["max_completion_tokens"] and body["temperature"] == 0 and "max_tokens" not in body
    if name == "findings":
        content = valid_findings()
        n = sum(p["type"] == "image_url" for p in body["messages"][1]["content"])
        if n == 1:
            content["soil"] = {k: None for k in content["soil"]}
            content["organisms"][0]["image"] = "img1"
            content["damage"] = []
    elif name == "indicators":
        content = valid_answers()
    else:
        content = {"answer": "Tomatoes g"}
    text = json.dumps(content)
    usage = {"prompt_tokens": 4200, "completion_tokens": 120, "total_tokens": 4320}
    if body.get("stream"):
        chunks = [{"choices": [{"delta": {"content": text[i:i + 40]}, "finish_reason": None}]}
                  for i in range(0, len(text), 40)]
        chunks[-1]["choices"][0]["finish_reason"] = "stop"
        chunks.append({"choices": [], "usage": usage})
        sse = "".join(f"data: {json.dumps(c)}\n\n" for c in chunks) + "data: [DONE]\n\n"
        return httpx.Response(200, text=sse, headers={"content-type": "text/event-stream"})
    return httpx.Response(200, json={"choices": [{"message": {"content": text}, "finish_reason": "stop"}],
                                     "usage": usage})


def test_offline_run(tmp_path):
    paths = []
    for i, size in enumerate([(2048, 1536), (800, 600), (1200, 1600), (640, 480)]):
        p = tmp_path / f"p{i}.jpg"
        Image.new("RGB", size, (60 + i * 30, 120, 50)).save(p)
        paths.append(str(p))
    out = tmp_path / "reports"
    args = vc.argparse.Namespace(base_url="http://vlm.test/v1", api_key="k", images=paths,
                                 max_pixels=1_003_520, label=None, out=str(out))
    assert vc.run(args, transport=httpx.MockTransport(mock_server)) == 0
    [report] = out.glob("*.json")
    r = json.loads(report.read_text())
    assert r["label"].startswith("Qwen3-VL-8B-Instruct-FP8-")
    assert [c["pass"] for c in r["checks"]] == [True] * 5
    assert r["images"][0]["prepared"] == [1152, 864]
    assert r["checks"][3]["details"]["soil_null"] == 8
    assert "base64" not in report.read_text()
    assert (out / f"{r['label']}.md").exists() and len(r["drawn"]) == 5


def test_server_down_is_a_clear_error(capsys):
    def refuse(request):
        raise httpx.ConnectError("connection refused", request=request)

    args = vc.argparse.Namespace(base_url="http://vlm.test/v1", api_key=None, images=None,
                                 max_pixels=1_003_520, label=None, out="unused")
    with pytest.raises(vc.ServerDown):
        vc.run(args, transport=httpx.MockTransport(refuse))
