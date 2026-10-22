"""The knowledge loader: the seed checks of contracts §10 and the knowledge.surql it writes.

The fixture seed is ml/tests/fixtures/knowledge_seed. The migration it gives with FakeEmbedder is
committed as backend/crates/store/tests/embedded/knowledge_fixture.surql, which the store's
embedded-engine test loads; set UPDATE_KNOWLEDGE_FIXTURE=1 to rewrite it.
"""

import copy
import json
import os
import shutil
from pathlib import Path

import numpy as np
import pytest

from ml.knowledge import build_migration as bm
from ml.knowledge import embed, validate
from ml.knowledge.seed import ROOT, load

FIXTURE = Path(__file__).parent / "fixtures/knowledge_seed"
STORE_FIXTURE = ROOT / "backend/crates/store/tests/embedded/knowledge_fixture.surql"
REVISION = "2957b322090f9cb17ae72c71981c7218a28d81e0"


class FakeEmbedder:
    """One-hot vectors in chunk order, so the store test can search for each chunk."""
    name = "test-embed@0"

    def embed_documents(self, texts):
        out = np.zeros((len(texts), embed.DIMENSION), np.float32)
        out[np.arange(len(texts)), np.arange(len(texts))] = 1.0
        return out


def no_dose(products, texts):
    return []


def templates():
    return json.loads(bm.TEMPLATES.read_text())


def by_id(records, i):
    return next(r for r in records if r["id"] == i)


def errors_of(seed):
    return validate.check(seed, no_dose)


@pytest.fixture
def seed():
    return load(FIXTURE)


def test_the_fixture_passes(seed):
    assert errors_of(seed) == []
    assert [s["id"] for s in seed.species] == ["strawberry", "tomato"]
    assert seed.sources[0]["retrieved_at"] == "2026-10-15"  # a string, not a date


def test_yaml_reads_only_true_and_false_as_booleans(tmp_path):
    from ml.knowledge.seed import read_yaml
    p = tmp_path / "x.yaml"
    p.write_text("a: no\nb: yes\nc: true\nd: 2026-10-20\ne: False\n")
    assert read_yaml(p) == {"a": "no", "b": "yes", "c": True, "d": "2026-10-20", "e": False}


def test_label_set_version_matches_the_formats_vector():
    labels = [
        {"table": "species", "id": "strawberry", "label": "Fragaria × ananassa with common name strawberry"},
        {"table": "species", "id": "tomato", "label": "Solanum lycopersicum with common name tomato"},
        {"table": "organism", "id": "ladybug_larva", "label": "Coccinellidae larva with common name ladybug larva"},
    ]
    assert bm.label_set_version(labels, REVISION, templates()) == "129f0395e8d89bd5"
    assert bm.label_set_version(labels[::-1], REVISION, templates()) == "129f0395e8d89bd5"
    assert bm.bioclip_revision() == REVISION


@pytest.mark.parametrize("break_it, expected", [
    (lambda s: by_id(s.problems, 'tomato_early_blight').update(prevalence="usual"), "ProblemSeed at prevalence"),
    (lambda s: by_id(s.problems, 'tomato_early_blight').update(prevalance="common"), "Additional properties"),
    (lambda s: by_id(s.organisms, 'aphid').update(id="tomato"), "'tomato' is used 2 times"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["indicators"][3].update(discriminating=True),
     "4 discriminating indicators"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["treatments"].append("burn_it"), "treatment 'burn_it' is not a known"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["looks_like"].append(
        {"problem": "tomato_early_blight", "how_to_tell": "x"}), "is written 2 times"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["looks_like"].append(
        {"problem": "tomato_septoria_leaf_spot", "how_to_tell": "x"}), "can't look like itself"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["indicators"][0].update(image_kinds=["whole_plant"]),
     "which none of ['whole_plant'] can measure"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["indicators"][0]["measure"].update(min=3, max=1.5), "min >= max"),
    (lambda s: by_id(s.problems, 'tomato_septoria_leaf_spot')["indicators"][0].update(id="tomato_x.small_spots"),
     "must start with tomato_septoria_leaf_spot."),
    (lambda s: by_id(s.species, 'tomato').pop("care"), "a full species needs care"),
    (lambda s: by_id(s.species, 'tomato').update(classifier_supported=False),
     "classifier_supported is false but classifier_labels.yaml has class"),
    (lambda s: by_id(s.species, 'strawberry').update(classifier_supported=True), "has no class for it"),
    (lambda s: s.classifier_labels["labels"].append(
        {"index": 2, "label": "x", "species_id": "tomato", "problem_id": "tomato_rot"}),
     "unknown problem 'tomato_rot'"),
    (lambda s: by_id(s.organisms, 'aphid').update(role="invasive"), "needs reporting_url"),
    (lambda s: by_id(s.species, 'tomato')["reference_images"][0].update(file="species/none.png"), "is missing"),
    (lambda s: by_id(s.sources, 'example_photos').pop("licence"), "has no licence"),
    (lambda s: s.products["products"].append({"name": "Neem Oil", "broad_spectrum": True}),
     "'neem oil' is listed 2 times"),
    (lambda s: s.chunks[0].update(id="other_source.1"), "must be <source_id>.<n>"),
    (lambda s: by_id(s.species, 'tomato').update(sources=["nowhere"]), "source 'nowhere' is not a known source"),
])
def test_each_check_fails(seed, break_it, expected):
    break_it(seed)
    errors = errors_of(seed)
    assert any(expected in e for e in errors), errors


def test_a_file_must_be_named_after_its_id(tmp_path):
    shutil.copytree(FIXTURE, tmp_path / "seed")
    (tmp_path / "seed/organisms/aphid.yaml").rename(tmp_path / "seed/organisms/greenfly.yaml")
    assert any("must be named aphid.yaml" in e for e in errors_of(load(tmp_path / "seed")))


def test_dose_failures_are_reported(seed):
    def dose(products, texts):
        assert {"name": "neem oil", "broad_spectrum": False} in products["products"]
        return [{"id": t["id"], "rule": "Measure", "matched": "2 tbsp", "reason": "r"}
                for t in texts if "aphids off" in t["text"]]
    errors = validate.check(seed, dose)
    assert errors == ["treatments/spray_aphids_off_with_water.yaml: text: fails the dose check "
                      "(Measure): r"]


@pytest.mark.skipif(shutil.which("cargo") is None, reason="needs cargo")
def test_the_real_dose_check_runs(seed):
    by_id(seed.treatments, 'avoid_overhead_watering')["text"] = "Mix 2 tbsp of neem oil into the sprayer."
    errors = validate.check(seed, validate.cargo_dose_check)
    assert len(errors) == 1 and "avoid_overhead_watering" in errors[0] and "2 tbsp" in errors[0]


def build(seed):
    return bm.build(seed, FakeEmbedder(), REVISION, templates())


def test_the_migration_is_deterministic_and_complete(seed):
    text, summary, photos = build(seed)
    assert text == build(load(FIXTURE))[0]
    lines = text.splitlines()
    assert lines[0].startswith("-- Generated by ml/knowledge/build_migration.py")
    assert lines[1].startswith("UPSERT knowledge_meta:`current` CONTENT { classifier_labels_version: "
                               "'fixture_v1', embedding_dimension: 384, embedding_model: "
                               "'test-embed@0', label_set_version: ")
    for bad in ("USE ", "BEGIN", "COMMIT", "DEFINE "):
        assert not any(l.startswith(bad) for l in lines)
    writes = [l for l in lines[1:] if not l.startswith("--")]
    assert all(l.endswith("RETURN NONE;") for l in writes)
    assert "DELETE species WHERE id NOT IN [species:`strawberry`, species:`tomato`] RETURN NONE;" in lines
    assert "RELATE organism:`ladybug_larva`->preys_on->organism:`aphid` RETURN NONE;" in lines
    assert "RELATE disease:`tomato_aphids`->caused_by->organism:`aphid` RETURN NONE;" in lines
    looks = [l for l in lines if l.startswith("INSERT RELATION INTO looks_like")]
    assert len(looks) == 2, "written both ways"
    # edges are deleted before the tables and written after them
    first_upsert = next(i for i, l in enumerate(lines) if l.startswith("UPSERT source"))
    assert lines.index("DELETE looks_like RETURN NONE;") < first_upsert
    assert min(i for i, l in enumerate(lines) if l.startswith("RELATE")) > max(
        i for i, l in enumerate(lines) if l.startswith("UPSERT"))
    # floats in the shortest f32 form, ints as ints, text escaped
    assert "rh_pct_min: 85.5" in text and "temp_c: { max: 29, min: 24 }" in text
    assert "it\\'s an \\'easy\\' first step" in text
    assert "mulch the soil.\\nRemove infected leaves." in text
    assert "classifier_label:['fixture_v1', 0] CONTENT { label: 'Tomato___healthy', species: species:`tomato` }" in text
    assert "soil_condition:['moisture', 'waterlogged']" in text
    assert "UPSERT product:`neem_oil` CONTENT { broad_spectrum: false, name: 'neem oil' }" in text
    assert summary["species_options"] == [{"id": "strawberry", "name": "Strawberry"},
                                           {"id": "tomato", "name": "Tomato"}]
    assert summary["label_set_version"] == bm.label_set_version(
        bm.bioclip_labels(seed), REVISION, templates())
    (key, data), = photos.items()
    assert key.startswith("knowledge/species/tomato/") and key.endswith(".webp")
    assert f"storage_key: '{key}'" in text
    from PIL import Image
    import io
    assert max(Image.open(io.BytesIO(data)).size) == 1024


def test_reference_photos_are_turned_upright_and_lose_their_metadata(tmp_path):
    from PIL import Image
    import io
    # Stored landscape, red on the left and blue on the right, with Orientation 6 (rotate 90°
    # clockwise to show it) and a GPS tag: shown upright it is portrait, red on top.
    im = Image.new("RGB", (40, 20), "blue")
    im.paste("red", (0, 0, 20, 20))
    exif = Image.Exif()
    exif[0x0112] = 6
    exif[0x8825] = {1: "N", 2: (52.0, 0.0, 0.0)}
    root = tmp_path / "seed"
    (root / "images").mkdir(parents=True)
    im.save(root / "images/side.jpg", exif=exif.tobytes(), quality=95)
    _, data = bm.reference_image(load(root), "tomato", {"file": "side.jpg"})
    out = Image.open(io.BytesIO(data))
    assert out.size == (20, 40)
    r, g, b = out.convert("RGB").getpixel((10, 5))
    assert r > 200 and b < 60, "red is on top"
    r, g, b = out.convert("RGB").getpixel((10, 35))
    assert b > 200 and r < 60, "blue is at the bottom"
    assert not out.getexif(), "no metadata survives"


def test_the_header_names_the_seed_it_came_from(seed):
    text, _, _ = build(seed)
    assert text.startswith("-- Generated by ml/knowledge/build_migration.py from the seed in "
                           "ml/tests/fixtures/knowledge_seed. Do not edit.\n")


def test_a_relative_seed_path_gives_the_same_bytes(seed, monkeypatch):
    monkeypatch.chdir(ROOT / "ml")
    relative = load(Path("tests/fixtures/knowledge_seed"))
    assert not relative.root.is_absolute()
    assert build(relative)[0] == build(seed)[0]


def test_removing_a_record_deletes_it(seed):
    seed.organisms = [o for o in seed.organisms if o["id"] != "ladybug_larva"]
    text, _, _ = build(seed)
    assert "DELETE organism WHERE id NOT IN [organism:`aphid`] RETURN NONE;" in text
    assert "preys_on->" not in text
    seed.chunks = []
    assert "DELETE knowledge_chunk RETURN NONE;" in build(seed)[0]


def test_values_are_written_safely():
    assert bm.f32(0.1) == "0.1" and bm.f32(24.0) == "24.0" and bm.f32(1 / 3) == "0.33333334"
    assert bm.string("a'b\\c") == "'a\\'b\\\\c'"
    with pytest.raises(bm.BuildError):
        bm.string("bell\x07")
    with pytest.raises(bm.BuildError):
        bm.f32(float("nan"))


def test_the_store_fixture_is_current(seed):
    text, _, _ = build(seed)
    if os.environ.get("UPDATE_KNOWLEDGE_FIXTURE"):
        STORE_FIXTURE.write_text(text, encoding="utf-8", newline="\n")
    assert STORE_FIXTURE.read_text(encoding="utf-8") == text, (
        "run UPDATE_KNOWLEDGE_FIXTURE=1 pytest ml/tests/test_knowledge.py")


def test_the_cache_embeds_each_text_once(tmp_path):
    calls = []

    class Counting(FakeEmbedder):
        def embed_documents(self, texts):
            calls.append(list(texts))
            return super().embed_documents(texts)

    c = embed.CachedEmbedder(make=Counting, cache=tmp_path / "c.jsonl")
    a = c.embed_documents(["x", "y", "x"])
    assert a[0] == a[2] and len(calls) == 1 and sorted(calls[0]) == ["x", "y"]
    again = embed.CachedEmbedder(make=Counting, cache=tmp_path / "c.jsonl", name="test-embed@0")
    assert again.embed_documents(["y"]) == [a[1]] and len(calls) == 1


# The golden test of models.md §10: 20 fixed strings (python -m ml.tests.make_text_golden), the
# first 8 values within 1e-4 and a norm of 1 within 1e-5, from the pinned files. The Rust embedder
# checks the same values.
GOLDEN = json.loads((Path(__file__).parent / "fixtures/text_embedder_golden.json").read_text(encoding="utf-8"))


@pytest.mark.skipif(not embed.default_manifest().is_file(),
                    reason="needs python3 -m ml.models.fetch --for ingest")
def test_the_embedder_matches_the_golden_values():
    e = embed.Embedder()
    assert e.name == "snowflake-arctic-embed-xs@d8c86521"
    texts = [g["input"] for g in GOLDEN["cases"]]
    assert len(texts) == 20
    batch = e.embed_documents(texts)
    for g, v in zip(GOLDEN["cases"], batch):
        assert abs(np.linalg.norm(v) - 1) < 1e-5
        assert np.allclose(v[:8], g["first_8"], atol=1e-4), g["input"]
    alone = e.embed_documents(texts[:1])[0]
    assert float(alone @ batch[0]) > 0.99999, "a vector doesn't depend on its batch in fp32"
