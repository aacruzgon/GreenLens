"""The ingestion's front half (training.md §10): fetch, chunk, tag, extract and discriminate.

No test touches the network or the hosted model: fetch runs on httpx's MockTransport, the model
steps on a fake client, and discriminate on a fake embedder. The sanitiser and the dose check are
the real Rust ones, through cargo, as in the loader's tests.
"""

import json
import shutil
from pathlib import Path
from types import SimpleNamespace

import httpx
import numpy as np
import pytest
import yaml

from ml.knowledge import chunk, discriminate, extract, fetch, llm, review, tag, validate
from ml.knowledge.seed import load

FIXTURE = Path(__file__).parent / "fixtures/knowledge_seed"


def no_dose(products, texts):
    return []


def identity(schema):
    return schema


@pytest.fixture
def seed_dir(tmp_path):
    d = tmp_path / "seed"
    shutil.copytree(FIXTURE, d)
    return d


# --- fetch -----------------------------------------------------------------------------------

def test_fetch_obeys_robots_names_itself_and_never_retries_a_refusal(seed_dir, tmp_path):
    seen = []

    def handler(request: httpx.Request) -> httpx.Response:
        seen.append((str(request.url), request.headers["user-agent"]))
        if request.url.path == "/robots.txt":
            return httpx.Response(200, text="User-agent: *\nDisallow: /private/\n")
        if request.url.path == "/blocked":
            return httpx.Response(403)
        return httpx.Response(200, text="<html><article><p>Leaf spots.</p></article></html>")

    s = yaml.safe_load((seed_dir / "sources.yaml").read_text())
    s += [{"id": "a_ok", "title": "A", "publisher": "P", "url": "https://ext.example/ok", "retrieved_at": "2026-10-19"},
          {"id": "b_private", "title": "B", "publisher": "P", "url": "https://ext.example/private/x", "retrieved_at": "2026-10-19"},
          {"id": "c_blocked", "title": "C", "publisher": "P", "url": "https://ext.example/blocked", "retrieved_at": "2026-10-19"}]
    (seed_dir / "sources.yaml").write_text(yaml.safe_dump(s))
    seed = load(seed_dir)
    seed.sources = [x for x in seed.sources if x["id"] in ("a_ok", "b_private", "c_blocked", "example_photos")]
    sleeps = []
    f = fetch.Fetcher(httpx.Client(transport=httpx.MockTransport(handler),
                                   headers={"User-Agent": fetch.USER_AGENT}),
                      sleep=sleeps.append, clock=lambda: 0.0)
    errors = fetch.run(seed, tmp_path / "raw", fetcher=f)
    assert sorted(errors) == ["b_private: robots.txt disallows https://ext.example/private/x",
                              "c_blocked: HTTP 403 for https://ext.example/blocked"]
    assert (tmp_path / "raw/a_ok.html").read_text().startswith("<html>")
    meta = json.loads((tmp_path / "raw/a_ok.json").read_text())
    assert meta["status"] == 200 and len(meta["sha256"]) == 64
    assert all(ua == fetch.USER_AGENT for _, ua in seen)
    # A server error on robots.txt, or no answer, disallows the whole host (RFC 9309).
    for answer in (httpx.Response(503), httpx.ConnectTimeout("slow")):
        def down(request, answer=answer):
            if isinstance(answer, Exception):
                raise answer
            return answer
        g = fetch.Fetcher(httpx.Client(transport=httpx.MockTransport(down)), sleep=sleeps.append, clock=lambda: 0.0)
        assert not g.allowed("https://down.example/page")
    assert [u for u, _ in seen].count("https://ext.example/blocked") == 1, "a refusal isn't retried"
    assert sleeps and all(x == fetch.DELAY_S for x in sleeps), "requests to one host are spaced"
    assert not any("example.org/photos" in u for u, _ in seen), "photo sources aren't fetched"
    # A second run fetches nothing new.
    seen.clear()
    fetch.run(seed, tmp_path / "raw", fetcher=f)
    assert [u for u, _ in seen if "ok" in u] == []


# --- chunk -----------------------------------------------------------------------------------

PAGE = """<html><body><nav><p>Home | Garden</p></nav>
<div class="row toc__wrapper"><p>On this page</p></div>
<article><div id="toc-end"><h1>Septoria Leaf Spot</h1>
<h2>Symptoms</h2>
<p>Small round spots with dark edges appear on the lower leaves.</p>
<figure><img src="x.jpg"><figcaption>Photo: someone</figcaption></figure>
<p>Spots have tiny black dots in their pale centres.</p>
<h2>Management</h2>
<table><tr><td>Product</td><td>2 tbsp per gallon</td></tr></table>
<ul><li>Remove infected lower leaves.</li><li>Water at the base.</li></ul>
<p>Links to this article are strongly encouraged, and this article may be republished.</p>
<p>https://example.org/</p>
<h2>References</h2><p>Someone, 2020.</p></div></article>
<footer><p>© University</p></footer></body></html>"""


def test_chunk_reads_the_main_content_and_never_crosses_a_heading():
    recs = chunk.chunks("s", chunk.blocks(PAGE))
    assert [r["id"] for r in recs] == ["s.1", "s.2"]
    assert recs[0]["heading"] == "Symptoms"
    assert recs[0]["text"] == ("Small round spots with dark edges appear on the lower leaves.\n"
                               "Spots have tiny black dots in their pale centres.")
    assert recs[1]["heading"] == "Management"
    assert recs[1]["text"] == "Remove infected lower leaves.\nWater at the base."
    text = " ".join(r["text"] for r in recs)
    for gone in ("Home", "On this page", "Photo:", "2 tbsp", "republished", "example.org", "Someone", "©"):
        assert gone not in text


def test_chunk_drops_isus_byline():
    # ISU's byline block, as on its Septoria page: the label, the name and the date are separate divs.
    page = ('<article><h2>Septoria</h2><p>Small round spots on the lower leaves.</p>'
            '<div class="hhpn-authors-reviewed"><div>Authors:</div><div><a href="/p">Lina M Rodriguez Salamanca</a>'
            '</div><div>Last reviewed:</div><div>April 2022</div></div></article>')
    assert [r["text"] for r in chunk.chunks("s", chunk.blocks(page))] == ["Small round spots on the lower leaves."]


def test_chunk_sizes_stay_within_the_format():
    words = lambda n, w="word": " ".join([w] * n) + "."
    page = ("<article><h2>A</h2>" + "".join(f"<p>{words(70)}</p>" for _ in range(5))
            + f"<h2>B</h2><p>{'Long sentence here. ' * 120}</p></article>")
    recs = chunk.chunks("s", chunk.blocks(page))
    sizes = [len(r["text"].split()) for r in recs]
    assert all(s <= chunk.MAX_WORDS for s in sizes)
    a = [r for r in recs if r["heading"] == "A"]
    assert [len(r["text"].split()) for r in a] == [140, 140, 70], "paragraphs fill a chunk up to 200 words"
    assert len([r for r in recs if r["heading"] == "B"]) == 2, "a long paragraph splits between sentences"


def test_chunk_reads_an_edis_document_from_its_page_data():
    doc = "<h2>Symptoms</h2><p>Yellow stippling on the upper leaf surface.</p>"
    page = ('<html><body><div>menu</div><script id="__NEXT_DATA__" type="application/json">'
            + json.dumps({"props": {"pageProps": {"details": {"docContent": doc}}}}) + "</script></body></html>")
    assert chunk.chunks("e", chunk.blocks(page))[0]["text"] == "Yellow stippling on the upper leaf surface."


# --- the model, faked ------------------------------------------------------------------------

class FakeClient:
    """`messages.create` and `beta.messages.create`, answering with `reply(kwargs)`."""

    def __init__(self, reply, stop_reason="end_turn", served=None):
        self.calls = []
        self.reply, self.stop_reason = reply, stop_reason
        # The model that answers: the requested one, or `served(model)` for a fallback.
        self.served = served or (lambda model: model)
        self.messages = SimpleNamespace(create=lambda **kw: self._create(False, kw))
        self.beta = SimpleNamespace(messages=SimpleNamespace(create=lambda **kw: self._create(True, kw)))

    def _create(self, beta, kw):
        self.calls.append((beta, kw))
        usage = SimpleNamespace(input_tokens=1000, output_tokens=200, cache_read_input_tokens=0,
                                cache_creation_input_tokens=0)
        content = [SimpleNamespace(type="text", text=json.dumps(self.reply(kw)))]
        return SimpleNamespace(content=content, usage=usage, stop_reason=self.stop_reason, stop_details=None,
                               model=self.served(kw["model"]))


def test_llm_caches_replies_and_uses_the_fallback_on_opus(tmp_path):
    # Opus declines and the fallback answers.
    client = FakeClient(lambda kw: {"ok": True},
                        served=lambda model: "claude-haiku-5-5" if model == llm.EXTRACT_MODEL else model)
    m = llm.Llm(client, cache=tmp_path / "c.jsonl")
    args = dict(system="s", user="u", schema={"type": "object"}, effort="low", max_tokens=10)
    assert m.call(model=llm.TAG_MODEL, **args) == {"ok": True}
    assert m.call(model=llm.TAG_MODEL, **args) == {"ok": True}
    assert len(client.calls) == 1 and client.calls[0][0] is False
    assert client.calls[0][1]["output_config"]["format"]["type"] == "json_schema"
    assert m.call(model=llm.EXTRACT_MODEL, **args) == {"ok": True}
    beta, kw = client.calls[1]
    assert beta and kw["fallbacks"] == "default" and kw["betas"] == [llm.FALLBACK_BETA]
    # The fallback answered: the model that did is recorded, priced, and read back from the cache.
    assert m.served == "claude-haiku-5-5"
    again = llm.Llm(FakeClient(lambda kw: pytest.fail("not from the cache")), cache=tmp_path / "c.jsonl")
    assert again.call(model=llm.TAG_MODEL, **args) == {"ok": True} and again.served == llm.TAG_MODEL
    again.call(model=llm.EXTRACT_MODEL, **args)
    assert again.served == "claude-haiku-5-5"
    assert m.usage.calls == 3 and m.usage.cached == 1
    assert m.usage.cost() == pytest.approx((1000 * 2 + 200 * 10 + 1000 * 0.10 + 200 * 0.50) / 1e6)


def test_llm_prices_a_model_it_has_no_price_for_as_the_dearest(tmp_path):
    client = FakeClient(lambda kw: {"ok": True}, served=lambda model: "claude-opus-4-8")
    m = llm.Llm(client, cache=tmp_path / "c.jsonl")
    m.call(model=llm.EXTRACT_MODEL, system="s", user="u", schema={"type": "object"}, effort="low", max_tokens=10)
    assert m.usage.cost() == pytest.approx((1000 * 4.00 + 200 * 20.00) / 1e6)
    assert "claude-opus-4-8: 1000 in, 200 out, 0 cache read (not in PRICES: priced as the dearest model)" \
        in m.usage.report()


def test_llm_refuses_a_refusal_and_a_cut_off_reply(tmp_path):
    for stop in ("refusal", "max_tokens"):
        m = llm.Llm(FakeClient(lambda kw: {}, stop_reason=stop), cache=tmp_path / f"{stop}.jsonl")
        with pytest.raises(llm.LlmError):
            m.call(model=llm.TAG_MODEL, system="s", user="u", schema={}, effort="low", max_tokens=1)


@pytest.mark.skipif(shutil.which("cargo") is None, reason="needs cargo")
def test_the_extraction_schema_passes_the_real_sanitiser():
    sch, _ = validate.schemas()
    s = llm.sanitize(extract.problem_schema(sch, ["a.1", "a.2"], ["tomato_late_blight"], ["mulch"]))
    text = json.dumps(s)
    for refused in ('"oneOf"', '"maxLength"', '"minimum"', '"format": "double"', '"format": "uint32"'):
        assert refused not in text
    ind = s["$defs"]["IndicatorSeed"]["properties"]
    assert "discriminating" not in ind and '"pattern"' not in text


def test_extract_drops_a_citation_it_did_not_give():
    reply = {"indicators": [{"chunk_ids": ["a.1", "made.up"]}], "conditions_chunk_ids": ["a.2"]}
    assert extract._known(reply, {"a.1", "a.2"}) == ["made.up"]
    assert reply == {"indicators": [{"chunk_ids": ["a.1"]}], "conditions_chunk_ids": ["a.2"]}


# --- tag -------------------------------------------------------------------------------------

def test_tag_keeps_relevant_chunks_and_drops_the_rest(seed_dir, tmp_path):
    chunks_dir = tmp_path / "chunks"
    chunks_dir.mkdir()
    recs = [{"id": "src.1", "source_id": "src", "heading": "Early Blight", "text": "Target-like rings on old leaves."},
            {"id": "src.2", "source_id": "src", "heading": "", "text": "ISSN 1198-712X. Published 2005."},
            {"id": "src.3", "source_id": "src", "heading": "Control", "text": "Mix 2 tbsp per gallon of water."},
            {"id": "src.4", "source_id": "src", "heading": "Indoors", "text": "Vacuum up the wasps."}]
    (chunks_dir / "src.jsonl").write_text("".join(json.dumps(r) + "\n" for r in recs))
    no_grant = {"id": "nogrant.1", "source_id": "nogrant", "heading": "Leaf spots",
                "text": "Tell early blight from Septoria by the rings."}
    (chunks_dir / "nogrant.jsonl").write_text(json.dumps(no_grant) + "\n")
    plan = tmp_path / "plan.yaml"
    plan.write_text(yaml.safe_dump({"problems": [{"id": "tomato_late_blight", "name": "Late blight"}],
                                    "facts_only_sources": ["nogrant"],
                                    "withheld_chunks": {"src.4": "about wasps indoors"}}))

    def reply(kw):
        text = kw["messages"][0]["content"]
        assert "tomato_late_blight (Late blight)" in kw["system"][0]["text"], "the plan's ids are offered"
        assert kw["system"][0]["cache_control"] == {"type": "ephemeral"}
        assert kw["model"] == "claude-sonnet-5-5" and kw["output_config"]["effort"] == "medium"
        none = {"organism": None, "problems": [], "organisms": []}
        if "ISSN" in text:
            return {**none, "relevant": False, "crop": None, "problem": None, "topic": "care"}
        if "Tell" in text:
            return {**none, "relevant": True, "crop": "tomato", "problem": None, "topic": "symptoms",
                    "problems": ["tomato_early_blight", "tomato_septoria_leaf_spot"]}
        return {**none, "relevant": True, "crop": "tomato", "problem": "tomato_early_blight",
                "topic": "symptoms" if "rings" in text else "management"}

    dose = lambda products, texts: [{"id": t["id"], "reason": "a rate"} for t in texts if "tbsp" in t["text"]]
    summary = tag.run(seed_dir, plan, chunks_dir, tmp_path / "tags", llm.Llm(FakeClient(reply), tmp_path / "c.jsonl"),
                      dose=dose, sanitize=identity)
    assert summary["src"]["kept"] == 1 and summary["src"]["dropped"] == 1 and summary["src"]["withheld"] == 2
    assert len(summary["src"]["sample"]) == 1
    kept = [json.loads(l) for l in (seed_dir / "chunks/src.jsonl").read_text().splitlines()]
    assert kept == [{"id": "src.1", "source_id": "src", "crop": "tomato", "problem": "tomato_early_blight",
                     "organism": None, "topic": "symptoms", "text": "Target-like rings on old leaves."}]
    decided = {json.loads(l)["id"]: json.loads(l) for l in (tmp_path / "tags/src.jsonl").read_text().splitlines()}
    assert {i: (d["dropped"], d["withheld"]) for i, d in decided.items()} == {
        "src.1": (None, None), "src.2": ("not relevant", None), "src.3": (None, "dose check: a rate"),
        "src.4": (None, "withheld at review: about wasps indoors")}
    assert decided["src.1"]["problems"] == ["tomato_early_blight"], "the single tag is in the list"
    # A source with no reuse grant is tagged, with every problem it compares, but its wording
    # stays out of the seed.
    assert summary["nogrant"]["kept"] == 1 and not summary["nogrant"]["committed"]
    assert not (seed_dir / "chunks/nogrant.jsonl").exists()
    tagged = json.loads((tmp_path / "tags/nogrant.jsonl").read_text())
    assert tagged["problems"] == ["tomato_early_blight", "tomato_septoria_leaf_spot"] and not tagged["dropped"]


def test_tag_uses_the_plans_model_for_a_source_unless_one_is_given(seed_dir, tmp_path):
    chunks_dir = tmp_path / "chunks"
    chunks_dir.mkdir()
    for src in ("hard", "easy"):
        (chunks_dir / f"{src}.jsonl").write_text(json.dumps(
            {"id": f"{src}.1", "source_id": src, "heading": "", "text": f"Rings on old leaves ({src})."}) + "\n")
    plan = tmp_path / "plan.yaml"
    plan.write_text(yaml.safe_dump({"tag_models": {"hard": "claude-opus-5-5"}}))
    reply = lambda kw: {"relevant": True, "crop": "tomato", "problem": "tomato_early_blight", "organism": None,
                        "topic": "symptoms", "problems": [], "organisms": []}

    def models(**kw):
        client = FakeClient(reply)
        summary = tag.run(seed_dir, plan, chunks_dir, tmp_path / "tags", llm.Llm(client, tmp_path / "c.jsonl"),
                          dose=no_dose, sanitize=identity, **kw)
        assert {s: v["model"] for s, v in summary.items()} == {"easy": kw.get("model_id", llm.TAG_MODEL),
                                                               "hard": kw.get("model_id", "claude-opus-5-5")}
        return sorted(c[1]["model"] for c in client.calls)

    assert models() == ["claude-opus-5-5", "claude-sonnet-5-5"]
    assert models(model_id="claude-haiku-5-5") == ["claude-haiku-5-5", "claude-haiku-5-5"]


# --- extract ---------------------------------------------------------------------------------

def test_quote_links_what_the_seed_does_not_carry():
    chunks = {"a.1": {"source_id": "a", "text": "Rings on\nold leaves."},
              "a.2": {"source_id": "a", "text": "Spray at 2%.", "withheld": "dose check: a rate"},
              "a.3": {"source_id": "a", "text": "Vacuum them.", "withheld": "withheld at review: indoors"},
              "b.1": {"source_id": "b", "text": "Not ours."}}
    lines = extract._quote(chunks, ["b.1", "a.3", "a.2", "a.1", "zz.9"], {"b"}, {"a": "https://a"}).splitlines()
    assert lines[0] == "  - `a.1`: Rings on old leaves."
    assert "(https://a, it gives a rate: run fetch and chunk" in lines[1]
    assert "(https://a, withheld at review: indoors: run fetch and chunk" in lines[2]
    assert "(b, wording not committed: run fetch and chunk" in lines[3] and len(lines) == 4


PROBLEM_REPLY = {
    "kind": "fungal", "prevalence": "common", "damage_patterns": [],
    "indicators": [
        {"slug": "water_soaked_spots", "text": "Large water-soaked spots", "question": "Are there large dark, wet-looking patches on the leaves?",
         "weight": 3, "where": "upper_leaves", "image_kinds": ["leaf_closeup"], "measure": None,
         "chunk_ids": ["example_extension_tomato_diseases.1"]},
        {"slug": "white_growth", "text": "White growth under the leaf", "question": "Is there white fuzz under the leaves?",
         "weight": 3, "where": "leaf_underside", "image_kinds": ["leaf_underside"],
         "measure": {"quantity": "lesion_diameter_mm", "stat": "median", "min": 10, "max": None},
         "chunk_ids": ["example_extension_tomato_diseases.1"]},
        {"slug": "fruit_spot_size", "text": "Fruit spots up to 6 mm", "question": "Are the fruit spots no bigger than 6 mm?",
         "weight": 2, "where": "fruit", "image_kinds": ["flower_fruit"],
         "measure": {"quantity": "lesion_diameter_mm", "stat": "max", "min": None, "max": 6},
         "chunk_ids": ["example_extension_tomato_diseases.2"]}],
    "conditions": {"temp_c": {"min": 10, "max": 24}, "rh_pct_min": 90, "humid_hours_min": None, "notes": "Cool, wet weather."},
    "conditions_chunk_ids": ["example_extension_tomato_diseases.2"],
    "looks_like": [{"problem": "tomato_early_blight", "how_to_tell": "No target rings.",
                    "chunk_ids": ["example_extension_tomato_diseases.1"]}],
    "new_treatments": [{"id": "destroy_infected_plants", "kind": "cultural", "text": "Bag and remove infected plants.",
                        "chunk_ids": ["example_extension_tomato_diseases.3", "nogrant.1"]},
                       {"id": "mulch_soil_surface", "kind": "cultural", "text": "Mulch to stop soil splashing.",
                        "chunk_ids": ["example_extension_tomato_diseases.3"]}],
    "existing_treatments": ["avoid_overhead_watering"],
    "products": [{"name": "copper fungicide", "broad_spectrum": False, "chunk_ids": ["example_extension_tomato_diseases.3"]}],
    "notes": "No size is given for the spots.",
}


def write_tags(seed_dir, tmp_path) -> Path:
    """build/tags as tag.py writes it: the fixture's chunks, tagged late blight, and one chunk of a
    facts-only source that compares late blight with early blight."""
    tags = tmp_path / "tags"
    tags.mkdir()
    path = seed_dir / "chunks/example_extension_tomato_diseases.jsonl"
    rows = [{**json.loads(l), "heading": "", "problems": ["tomato_late_blight"], "organisms": [], "dropped": None}
            for l in path.read_text().splitlines()]
    rows.append({**rows[0], "id": "example_extension_tomato_diseases.9", "dropped": "not relevant"})
    (tags / "example_extension_tomato_diseases.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
    (tags / "nogrant.jsonl").write_text(json.dumps(
        {"id": "nogrant.1", "source_id": "nogrant", "heading": "", "crop": "tomato", "problem": None,
         "organism": None, "problems": ["tomato_early_blight", "tomato_late_blight"], "organisms": [],
         "topic": "symptoms", "text": "Late blight spots have no rings; early blight's do.", "dropped": None}) + "\n")
    s = yaml.safe_load((seed_dir / "sources.yaml").read_text())
    s.append({"id": "nogrant", "title": "N", "publisher": "N", "url": "https://nogrant.example/tomato",
              "retrieved_at": "2026-10-19"})
    (seed_dir / "sources.yaml").write_text(yaml.safe_dump(s))
    # Early blight's file already holds the pair with late blight.
    eb = seed_dir / "problems/tomato_early_blight.yaml"
    d = yaml.safe_load(eb.read_text())
    d["looks_like"].append({"problem": "tomato_late_blight", "how_to_tell": "Rings."})
    eb.write_text(yaml.safe_dump(d, sort_keys=False))
    return tags


def test_extract_drafts_a_problem_with_its_citations(seed_dir, tmp_path):
    tags = write_tags(seed_dir, tmp_path)
    plan = tmp_path / "plan.yaml"
    plan.write_text(yaml.safe_dump({"problems": [{"id": "tomato_late_blight", "name": "Late blight", "affects": ["tomato"]}],
                                    "facts_only_sources": ["nogrant"]}))
    client = FakeClient(lambda kw: PROBLEM_REPLY, served=lambda model: "claude-opus-4-8")
    products = seed_dir / "products.yaml"
    products.write_text("# The broad_spectrum rule and its reasons.\n#\n" + products.read_text())
    ex = extract.Extractor(seed_dir, plan, llm.Llm(client, tmp_path / "c.jsonl"), sanitize=identity,
                           today=extract.date(2026, 10, 19), tags=tags)
    assert ex.run() == ["tomato_late_blight: 2 indicators, 3 treatments, 2 sources"]
    beta, kw = client.calls[0]
    assert kw["model"] == llm.EXTRACT_MODEL and kw["output_config"]["effort"] == "high"
    user = kw["messages"][0]["content"]
    assert "[example_extension_tomato_diseases.1] (symptoms)" in user
    assert "[nogrant.1] (symptoms)" in user, "a chunk listing several problems reaches each"
    assert "example_extension_tomato_diseases.9" not in user, "a dropped chunk doesn't"
    assert "lesion_diameter_mm from leaf_closeup, leaf_underside" in kw["system"][0]["text"]
    d = yaml.safe_load((seed_dir / "problems/tomato_late_blight.yaml").read_text())
    assert d["reviewed_by"] == "" and d["reviewed_at"] == ""
    # The fruit size can't be measured, so its size-only indicator goes too.
    assert [i["id"] for i in d["indicators"]] == ["tomato_late_blight.water_soaked_spots", "tomato_late_blight.white_growth"]
    assert all(i["discriminating"] is False for i in d["indicators"])
    assert d["indicators"][1]["measure"] == {"quantity": "lesion_diameter_mm", "stat": "median", "min": 10}
    assert d["conditions"] == {"temp_c": {"min": 10, "max": 24}, "rh_pct_min": 90, "notes": "Cool, wet weather."}
    # mulch_soil_surface exists with another text: the new one is renamed, not silently linked.
    assert d["treatments"] == ["avoid_overhead_watering", "destroy_infected_plants", "mulch_soil_surface_2"]
    assert yaml.safe_load((seed_dir / "treatments/mulch_soil_surface_2.yaml").read_text())["text"] == \
        "Mulch to stop soil splashing."
    assert d["sources"] == ["example_extension_tomato_diseases", "nogrant"]
    # Early blight's file holds the pair, so it isn't written twice.
    assert d["looks_like"] == []
    review = (seed_dir / "problems/tomato_late_blight.review.md").read_text()
    assert "by `claude-opus-4-8`" in review, "the model that answered, not the one asked"
    assert "No size is given for the spots." in review
    assert "Dropped fruit_spot_size" in review and "renamed mulch_soil_surface_2" in review
    assert "- **tomato_early_blight** (not written here: the pair is already in tomato_early_blight.yaml" in review
    full = next(json.loads(l) for l in (tags / "example_extension_tomato_diseases.jsonl").read_text().splitlines())
    assert f"`example_extension_tomato_diseases.1`: {full['text']}" in review, "excerpts are quoted whole"
    assert "`nogrant.1` (https://nogrant.example/tomato, wording not committed" in review
    assert "rings" not in review.split("`nogrant.1`")[1].split("\n")[0], "a facts-only excerpt isn't quoted"
    assert "**avoid_overhead_watering** (cultural, reused): " in review
    header = (seed_dir / "treatments/destroy_infected_plants.yaml").read_text()
    assert "reviewed, with its text, in the review file of" in header
    assert yaml.safe_load(header)["sources"] == ["example_extension_tomato_diseases", "nogrant"]
    assert {"name": "copper fungicide", "broad_spectrum": False} in yaml.safe_load(products.read_text())["products"]
    assert products.read_text().startswith("# The broad_spectrum rule and its reasons.\n#\n"), "the header stays"
    # A second run drafts nothing: the file exists.
    assert ex.run() == []

    # Until discriminate.py has run, a draft has no discriminating indicators; until a teammate
    # signs it, it isn't loaded. --drafts lifts only the second.
    needs_three = "problems/tomato_late_blight.yaml: 0 discriminating indicators; exactly 3 are needed"
    assert validate.check(load(seed_dir), no_dose) == [
        needs_three, "problems/tomato_late_blight.yaml: not reviewed yet (reviewed_by is empty)"]
    assert validate.check(load(seed_dir), no_dose, drafts=True) == [needs_three]
    # The reviewer signs both fields, or the check fails even on drafts.
    path = seed_dir / "problems/tomato_late_blight.yaml"
    path.write_text(path.read_text().replace('reviewed_by: ""', "reviewed_by: sara").replace("reviewed_by: ''", "reviewed_by: sara"))
    assert validate.check(load(seed_dir), no_dose, drafts=True) == [
        needs_three, "problems/tomato_late_blight.yaml: reviewed_by is set but reviewed_at is empty; sign both"]


def test_extract_skips_what_no_source_covers(seed_dir, tmp_path):
    plan = tmp_path / "plan.yaml"
    plan.write_text(yaml.safe_dump({"problems": [{"id": "tomato_leaf_mold", "name": "Leaf mold", "affects": ["tomato"]}]}))
    ex = extract.Extractor(seed_dir, plan, llm.Llm(FakeClient(lambda kw: pytest.fail("no call")), tmp_path / "c.jsonl"),
                           sanitize=identity)
    assert ex.run() == ["tomato_leaf_mold: no tagged excerpts; add a source or drop it from the plan"]


# --- validate: plan.yaml ----------------------------------------------------------------------

def test_validate_checks_the_plans_lists_against_the_seed_and_the_page(seed_dir, tmp_path):
    built = tmp_path / "chunks"
    built.mkdir()
    (built / "example_extension_tomato_diseases.jsonl").write_text(
        "".join(json.dumps({"id": f"example_extension_tomato_diseases.{n}"}) + "\n" for n in range(1, 6)))
    plan = {"facts_only_sources": ["gone"], "tag_models": {"typo_source": "claude-opus-5-5",
                                                           "example_photos": "claude-opus-5.5"},
            "withheld_chunks": {"example_extension_tomato_diseases.2": "committed by mistake",
                                "example_extension_tomato_diseases.5": "fine: on the page, not committed",
                                "example_extension_tomato_diseases.7": "a typo, or the page lost a paragraph",
                                "nosuch.1": "no such source"}}
    seed = load(seed_dir)
    assert validate.plan_errors(seed, plan, built) == [
        "plan.yaml: facts_only_sources names 'gone', which isn't in sources.yaml",
        "plan.yaml: tag_models names 'typo_source', which isn't in sources.yaml",
        "plan.yaml: tag_models gives 'example_photos' the model 'claude-opus-5.5', which llm.py doesn't know "
        "(claude-haiku-5-5, claude-opus-5-5, claude-sonnet-5-5)",
        "chunks/example_extension_tomato_diseases.jsonl: withheld chunk 'example_extension_tomato_diseases.2' "
        "is committed; run tag.py again",
        "plan.yaml: withheld chunk 'example_extension_tomato_diseases.7' isn't in "
        "build/chunks/example_extension_tomato_diseases.jsonl; check the id against the page",
        "plan.yaml: withheld chunk 'nosuch.1' names no source in sources.yaml"]
    # Without build/chunks, only the page check is skipped; a facts-only source's wording can't be in the seed.
    errors = validate.plan_errors(seed, {**plan, "facts_only_sources": ["example_extension_tomato_diseases"]})
    assert ("chunks/example_extension_tomato_diseases.jsonl: example_extension_tomato_diseases is a facts-only "
            "source, so its wording can't be committed") in errors
    assert not any("build/chunks" in e for e in errors)
    # check() reads plan.yaml only for the real seed, or when given one.
    assert validate.check(seed, no_dose) == []
    assert len(validate.check(seed, no_dose, plan=plan, built=built)) == 6


def test_validate_refuses_a_product_that_would_override_a_broad_spectrum_one(seed_dir):
    path = seed_dir / "products.yaml"
    products = yaml.safe_load(path.read_text())
    products["products"] += [{"name": "neem", "broad_spectrum": True},
                             {"name": "wettable sulfur", "broad_spectrum": False},
                             {"name": "sulfur", "broad_spectrum": True},
                             {"name": "neemish", "broad_spectrum": False}]   # a whole word only
    path.write_text(yaml.safe_dump(products))
    errors = [e for e in validate.check(load(seed_dir), no_dose) if e.startswith("products.yaml")]
    assert errors == [
        "products.yaml: 'neem oil' is broad_spectrum false but names 'neem', which is true; the longer name "
        "would win in a step, so mark it true",
        "products.yaml: 'wettable sulfur' is broad_spectrum false but names 'sulfur', which is true; the longer "
        "name would win in a step, so mark it true"]


# --- discriminate ----------------------------------------------------------------------------

def ind(i, weight, shots=("leaf_closeup",)):
    return {"id": f"p.{i}", "weight": weight, "image_kinds": list(shots)}


def test_choose_prefers_weight_then_difference_then_common_shots():
    inds = [ind("a", 3), ind("b", 3), ind("c", 2, ["leaf_underside"]), ind("d", 2, ["flower_fruit"]), ind("e", 1)]
    ov = {"p.a": (0.9, "q.x"), "p.b": (0.2, None), "p.c": (0.1, None), "p.d": (0.3, None), "p.e": (0.4, None)}
    picked, flagged = discriminate.choose(inds, ov)
    assert not flagged
    # a is shared; b, c, d are the top non-shared, but only b is in a common shot, so e replaces d.
    assert picked == ["p.b", "p.c", "p.e"]
    picked, flagged = discriminate.choose(inds[:3], ov)
    assert flagged and picked == ["p.b", "p.c", "p.a"]


class WordEmbedder:
    """One dimension per word, normalised: texts sharing words are similar."""
    name = "words@0"

    def embed_documents(self, texts):
        vocab = sorted({w for t in texts for w in t.lower().split()})
        out = np.zeros((len(texts), len(vocab)), np.float32)
        for n, t in enumerate(texts):
            for w in t.lower().split():
                out[n, vocab.index(w)] = 1
        return out / np.linalg.norm(out, axis=1, keepdims=True)


def test_discriminate_writes_the_flags_and_the_pair_table(seed_dir):
    p = seed_dir / "problems/tomato_septoria_leaf_spot.yaml"
    d = yaml.safe_load(p.read_text())
    d["looks_like"] = [{"problem": "tomato_early_blight", "how_to_tell": "Smaller spots, no rings."}]
    d["reviewed_by"] = ""
    p.write_text(yaml.safe_dump(d, sort_keys=False))
    lines = discriminate.run(seed_dir, WordEmbedder())
    assert len(lines) == 1 and lines[0].startswith("tomato_septoria_leaf_spot: ")
    after = yaml.safe_load(p.read_text())
    assert sum(i["discriminating"] for i in after["indicators"]) == 3
    review = p.with_suffix(".review.md").read_text()
    assert discriminate.SECTION in review and "| `tomato_septoria_leaf_spot.small_spots` |" in review
    # Running again replaces the section instead of adding a second one.
    discriminate.run(seed_dir, WordEmbedder())
    assert p.with_suffix(".review.md").read_text().count(discriminate.SECTION) == 1


def test_discriminate_can_refresh_the_table_and_keep_a_reviewers_flags(seed_dir):
    p = seed_dir / "problems/tomato_septoria_leaf_spot.yaml"
    d = yaml.safe_load(p.read_text())
    d["looks_like"] = [{"problem": "tomato_early_blight", "how_to_tell": "Smaller spots, no rings."}]
    d["reviewed_by"] = ""
    p.write_text(yaml.safe_dump(d, sort_keys=False))
    discriminate.run(seed_dir, WordEmbedder())
    review = p.with_suffix(".review.md")
    notes = ("**Override:** the reviewer's own set.\n\n## Changes after review\n\n| Point | Change |\n|---|---|\n"
             "| 3 | Swapped one indicator |\n\nThe last line.\n")
    review.write_text(review.read_text() + "\n" + notes)
    flags = [i["discriminating"] for i in yaml.safe_load(p.read_text())["indicators"]]
    chosen = [not f for f in flags[:3]] + flags[3:]   # a different set than the proposal
    d = yaml.safe_load(p.read_text())
    for i, f in zip(d["indicators"], chosen):
        i["discriminating"] = f
    p.write_text(yaml.safe_dump(d, sort_keys=False))
    before = p.read_text()
    discriminate.run(seed_dir, WordEmbedder(), keep_flags=True)
    assert p.read_text() == before, "the YAML is left alone"
    body = review.read_text()
    assert "| Proposed | In the YAML |" in body and body.count(discriminate.SECTION) == 1
    assert body.rstrip().endswith(notes.rstrip()), "the notes under the table stay, a table of their own included"


# --- review ----------------------------------------------------------------------------------

def test_review_rebuilds_the_record_sections_from_the_yaml(seed_dir):
    p = seed_dir / "problems/tomato_early_blight.yaml"
    old = "\n".join([
        "# Review: Early blight (`tomato_early_blight`)", "", "Draft of 2026-10-20 by `m`.", "",
        "## Model notes", "", "The model's note.", "",
        "## Indicators", "",
        "- **tomato_early_blight.concentric_rings** (weight 3, lower_leaves, leaf_closeup)",
        "  - Text: drafted text", "  - Question: drafted question",
        "  - `example_extension_tomato_diseases.1`: Early blight starts as ...", "",
        "## Treatments", "", "- **dropped_step** (cultural, new): gone", "  - `x.9`: gone", "",
        "## Changes after review (2026-10-20)", "", "- Point 1: kept as written.", "",
        discriminate.SECTION, "", "| table |", "", "**Override:** kept.", ""])
    p.with_suffix(".review.md").write_text(old)
    d = yaml.safe_load(p.read_text())
    d["indicators"][0]["text"] = "Rings like a target, edited by hand"
    d["indicators"].append({**d["indicators"][1], "id": "tomato_early_blight.added_by_hand",
                            "discriminating": False})
    p.write_text(yaml.safe_dump(d, sort_keys=False))
    assert review.run(seed_dir, all_records=True) == (["problems/tomato_early_blight.review.md"], [], [])
    body = p.with_suffix(".review.md").read_text()
    assert body.startswith("# Review: Early blight (`tomato_early_blight`)\n\nDraft of 2026-10-20 by `m`.")
    assert "## Model notes (as first drafted)\n\nThe model's note." in body
    ring = body.split("- **tomato_early_blight.concentric_rings**")[1].split("\n\n")[0]
    assert "(weight 3, discriminating," in ring and "Text: Rings like a target, edited by hand" in ring
    assert "`example_extension_tomato_diseases.1`: Early blight starts as small brown spots" in ring, \
        "the old citation is kept and quoted from the seed"
    added = body.split("- **tomato_early_blight.added_by_hand**")[1].split("\n\n")[0]
    assert "(no excerpt cited)" in added
    assert "dropped_step" not in body and "- **mulch_soil_surface** (" in body
    assert "- **tomato_septoria_leaf_spot**: Septoria spots are smaller" in body
    assert body.index("## Changes after review") < body.index(discriminate.SECTION)
    assert body.rstrip().endswith("**Override:** kept.")
    # A citation written by hand under a line is quoted on the next run, which changes nothing else.
    head, tail = body.split("- **tomato_early_blight.added_by_hand**")
    p.with_suffix(".review.md").write_text(head + "- **tomato_early_blight.added_by_hand**" + tail.replace(
        "  - (no excerpt cited)", "  - `example_extension_tomato_diseases.1`", 1))
    review.run(seed_dir, all_records=True)
    again = p.with_suffix(".review.md").read_text()
    added = again.split("- **tomato_early_blight.added_by_hand**")[1].split("\n\n")[0]
    assert "`example_extension_tomato_diseases.1`: Early blight starts" in added
    assert review.run(seed_dir, all_records=True) == ([], [], [])


def test_review_keeps_each_files_citations_and_sections_and_names_unknown_ids(seed_dir, tmp_path):
    eb, sep = (seed_dir / f"problems/{p}.review.md" for p in ("tomato_early_blight", "tomato_septoria_leaf_spot"))
    mulch = seed_dir / "treatments/mulch_soil_surface.yaml"
    mulch.write_text(mulch.read_text().replace("onto the leaves.", "onto the leaves, then spray neem oil."))
    eb.write_text("\n".join([
        "# Review: Early blight", "", "## Treatments", "",
        # The reviewer removed .2 here; Septoria's file still cites it.
        "- **remove_lower_infected_leaves** (cultural): x", "  - `example_extension_tomato_diseases.3`", "",
        "- **mulch_soil_surface** (cultural): x", "  - `example_extension_tomato_diseases.99`", "",
        "## Products", "", "- **neem oil**, broad-spectrum: False", "  - `example_extension_tomato_diseases.3`", "",
        "## Open questions", "", "- Is the mulch step needed?", "",
        "## Treatments to check", "", "- Ask about straw mulch.", ""]))
    sep.write_text("\n".join([
        "# Review: Septoria", "", "## Treatments", "",
        "- **remove_lower_infected_leaves** (cultural): x",
        "  - `example_extension_tomato_diseases.2`", "  - `example_extension_tomato_diseases.3`", "",
        "- **avoid_overhead_watering** (cultural): x", "  - `example_extension_tomato_diseases.1`", "",
        "## Products", "", "- **neem oil**, broad-spectrum: False", "  - `example_extension_tomato_diseases.2`", ""]))
    tags = tmp_path / "tags"
    tags.mkdir()
    (tags / "example_extension_tomato_diseases.jsonl").write_text("".join(
        json.dumps({"id": f"example_extension_tomato_diseases.{n}", "dropped": None, "withheld": None}) + "\n"
        for n in (1, 2, 3)))
    done, errors, unchecked = review.run(seed_dir, tmp_path / "no_plan.yaml", all_records=True, tags=tags)
    assert errors == ["problems/tomato_early_blight.review.md: cites `example_extension_tomato_diseases.99`, an "
                      "unknown chunk id: tag.py never saw it (build/tags/example_extension_tomato_diseases.jsonl)"]
    assert unchecked == []
    body = eb.read_text()
    item = lambda name: body.split(f"- **{name}**")[1].split("\n\n")[0]
    assert "diseases.3`" in item("remove_lower_infected_leaves")
    assert "diseases.2`" not in item("remove_lower_infected_leaves"), "a removed citation stays removed"
    assert "diseases.1`" in item("avoid_overhead_watering"), "a step new to this file starts from the others'"
    assert "diseases.3`" in item("neem oil") and "diseases.2`" not in item("neem oil"), "a product keeps its own too"
    assert "`example_extension_tomato_diseases.99`: **unknown chunk id**: tag.py never saw it" in item("mulch_soil_surface")
    assert "## Open questions\n\n- Is the mulch step needed?" in body, "a reviewer's own section stays"
    assert "## Treatments to check\n\n- Ask about straw mulch." in body, "even one named like ours"



def test_review_checks_every_uncommitted_id_it_can_and_writes_the_same_line_on_any_machine(seed_dir, tmp_path):
    seed = load(seed_dir)
    plan = {"facts_only_sources": ["example_extension_tomato_diseases"]}
    ids = ["example_extension_tomato_diseases.7", "example_extension_tomato_diseases.999", "nosuch.1"]
    row = lambda n, **kw: json.dumps({"id": f"example_extension_tomato_diseases.{n}", **kw}) + "\n"
    tags, chunks = tmp_path / "tags", tmp_path / "chunks"
    tags.mkdir(), chunks.mkdir()
    (tags / "example_extension_tomato_diseases.jsonl").write_text(row(7, dropped=None, withheld=None))
    (chunks / "example_extension_tomato_diseases.jsonl").write_text(row(7))
    # A facts-only source's chunks aren't committed, so a typo among them is caught only through
    # build/tags or build/chunks; with neither, the id is listed as unchecked.
    seen = {}
    for name, kw in (("tags", {"tags": tags}), ("chunks", {"chunks": chunks}), ("neither", {})):
        quote, unknown, unchecked = review.quoter(seed, plan, **kw)
        seen[name] = quote(ids)
        assert [i for i, _ in unknown] == (["nosuch.1"] if name == "neither"
                                           else ["example_extension_tomato_diseases.999", "nosuch.1"])
        assert unchecked == (ids[:2] if name == "neither" else [])
    line = lambda text: next(l for l in text.splitlines() if "diseases.7`" in l)
    assert line(seen["tags"]) == line(seen["chunks"]) == line(seen["neither"])
    assert "wording not committed" in line(seen["tags"])
    # A chunk the dose check withheld reads the same whether build/tags says so or not.
    (tags / "example_extension_tomato_diseases.jsonl").write_text(row(7, dropped=None, withheld="dose check: 2%"))
    with_tags = review.quoter(seed, {}, tags=tags)[0](["example_extension_tomato_diseases.7"])
    without = review.quoter(seed, {})[0](["example_extension_tomato_diseases.7"])
    assert with_tags == without and "it gives a rate" in without
