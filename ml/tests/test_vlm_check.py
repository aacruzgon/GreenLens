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
            {"trace": "flea beetles", "pattern": "shot_holes", "other_description": None, "image": "img2",
             "bbox_2d": [0, 0, 1000, 1000], "certainty": "high"},
            {"trace": "mites under the leaf", "pattern": "other", "other_description": "pale stippling", "image": "img2",
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


# --- checks f and g ---------------------------------------------------------------------------


def one_image_findings() -> dict:
    f = valid_findings()
    f["soil"]["moisture"]["image"] = "img1"
    f["organisms"][0]["image"] = "img1"
    f["damage"] = []
    return f


def put(obj, path, value):
    for p in path[:-1]:
        obj = obj[p]
    obj[path[-1]] = value


@pytest.mark.parametrize("path,value", vc.OUT_OF_SCHEMA_DEMANDS)
def test_every_demand_is_outside_the_schema(path, value):
    """Check g proves something only if each demanded value fails validation."""
    schema = vc.findings_schema(1, SPECIES, ORGANISMS)
    f = one_image_findings()
    f["soil"]["weeds"] = {"value": "few", "image": "img1", "certainty": "low"}
    f["soil"]["colour"] = {"value": "dark", "image": "img1", "certainty": "low"}
    assert vc.validation_errors(schema, f) == []
    put(f, path, value)
    assert vc.validation_errors(schema, f)


def test_path_name_and_get_path():
    f = one_image_findings()
    assert vc.path_name(("organisms", 0, "bbox_2d")) == "organisms[0].bbox_2d"
    assert vc.path_name(("soil", "moisture", "value")) == "soil.moisture.value"
    assert vc.get_path(f, ("organisms", 0, "count")) == "few"
    assert vc.get_path(f, ("soil", "weeds", "value")) is vc._MISSING  # weeds is null
    assert vc.get_path(f, ("organisms", 3, "count")) is vc._MISSING
    assert vc.get_path(None, ("soil",)) is vc._MISSING


def test_demand_outcomes():
    f = one_image_findings()
    f["soil"]["moisture"]["value"] = "saturated"
    out = {o["field"]: o for o in vc.demand_outcomes(f)}
    assert out["soil.moisture.value"]["obeyed"] is True
    assert out["soil.weeds.value"] == {"field": "soil.weeds.value", "demanded": "some",
                                       "returned": "<missing>", "obeyed": False}
    assert out["organisms[0].count"]["returned"] == "few" and not out["organisms[0].count"]["obeyed"]


def test_demands_in_text():
    f = one_image_findings()
    f["species_guess"]["certainty"] = "certain"
    f["organisms"][0]["bbox_2d"] = [0, 0, 1500, 1500]
    f["organisms"][0]["count"] = "25"  # a string, not the demanded number
    fenced = "```json\n" + json.dumps(f, indent=2) + "\n```"
    assert vc.demands_in_text(fenced) == ["species_guess.certainty", "organisms[0].bbox_2d"]
    reshaped = '{"soil": {"moisture": "saturated", "weeds": "some"}, "count": 25}'  # shape changed too
    assert vc.demands_in_text(reshaped) == ["soil.moisture.value", "soil.weeds.value", "organisms[0].count"]
    broken = '{"certainty": "certain", "bbox_2d": [0, 0,\n 1500, 1500], "count": "25", "x": 250'
    assert vc.demands_in_text(broken) == ["species_guess.certainty", "organisms[0].bbox_2d"]
    assert vc.demands_in_text("") == [] and vc.demands_in_text(None) == []


def test_out_of_schema_prompt_names_every_demand():
    text = vc.out_of_schema_prompt("BASE")
    assert text.startswith("BASE\n")
    assert 'soil.moisture.value must be exactly "saturated".' in text
    assert "organisms[0].bbox_2d must be exactly [0, 0, 1500, 1500]." in text
    assert "organisms[0].count must be exactly 25." in text


def test_soil_reading():
    f = one_image_findings()
    assert vc.soil_reading(f) == {"soil_null": 7, "soil_not_null": {"moisture": f["soil"]["moisture"]}}
    assert vc.soil_reading(None) == {"soil_null": 0, "soil_not_null": {}}


# --- an offline run against a mocked server --------------------------------------------------


def mock_server(request: httpx.Request) -> httpx.Response:
    if request.url.path.endswith("/models"):
        return httpx.Response(200, json={"data": [{"id": "vlm", "root": "/models/Qwen3-VL-8B-Instruct-FP8"}]})
    body = json.loads(request.content)
    assert body["max_completion_tokens"] and body["temperature"] == 0 and "max_tokens" not in body
    if "response_format" not in body:  # check g's control: the model obeys the overrides
        text = '{"soil": {"moisture": {"value": "saturated"}}, "organisms": [{"bbox_2d": [0, 0, 1500, 1500]}]}'
        return httpx.Response(200, json={"choices": [{"message": {"content": text}, "finish_reason": "stop"}]})
    name = body["response_format"]["json_schema"]["name"]
    if name == "findings":
        content = valid_findings()
        parts = body["messages"][1]["content"]
        if sum(p["type"] == "image_url" for p in parts) == 1:
            content["organisms"][0]["image"] = "img1"
            content["damage"] = []
            if parts[0]["text"].endswith(vc.SOIL_LABEL) and vc.SOIL_NUDGE in parts[-1]["text"]:
                content["soil"]["moisture"]["image"] = "img1"
            else:  # the leaf close-up, and the soil photos with check d's request
                content["soil"] = {k: None for k in content["soil"]}
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
    for i, size in enumerate([(2048, 1536), (800, 600), (1200, 1600), (640, 480), (493, 493), (485, 485)]):
        p = tmp_path / f"p{i}.jpg"
        Image.new("RGB", size, (60 + i * 30, 120, 50)).save(p)
        paths.append(str(p))
    out = tmp_path / "reports"
    args = vc.argparse.Namespace(base_url="http://vlm.test/v1", api_key="k", images=paths[:4],
                                 soil_images=paths[4:], max_pixels=1_003_520, label=None, out=str(out))
    assert vc.run(args, transport=httpx.MockTransport(mock_server)) == 0
    [report] = out.glob("*.json")
    r = json.loads(report.read_text())
    assert r["label"].startswith("Qwen3-VL-8B-Instruct-FP8-")
    assert [c["name"][0] for c in r["checks"]] == list("abcdefg")
    assert [c["pass"] for c in r["checks"]] == [True] * 7
    assert r["images"][0]["prepared"] == [1152, 864]
    assert r["checks"][3]["details"]["soil_null"] == 8
    f, g = r["checks"][5]["details"], r["checks"][6]["details"]
    assert f["soil_not_null_total"] == 2 and [p["soil_null"] for p in f["photos"]] == [8, 7, 8, 7]
    assert [p["variant"] for p in f["photos"]] == ["plain", "nudged"] * 2
    assert g["obeyed"] == [] and g["control_wrote"] == ["soil.moisture.value", "organisms[0].bbox_2d"]
    assert "response_format" not in r["calls"]["findings_out_of_schema_control"]["request"]
    text = report.read_text()
    assert "base64" not in text and '"k"' not in text
    assert (out / f"{r['label']}.md").exists() and len(r["drawn"]) == 9


def test_server_down_is_a_clear_error(capsys):
    def refuse(request):
        raise httpx.ConnectError("connection refused", request=request)

    args = vc.argparse.Namespace(base_url="http://vlm.test/v1", api_key=None, images=None,
                                 max_pixels=1_003_520, label=None, out="unused")
    with pytest.raises(vc.ServerDown):
        vc.run(args, transport=httpx.MockTransport(refuse))
