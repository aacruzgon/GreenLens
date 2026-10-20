"""`training.eval.inat`: taxon matching, licences, metadata stripping, the review and the cases."""

import io

import pytest
from PIL import Image

from ml.training.eval import inat

TARGET = {"taxon": "Venturia inaequalis", "species_id": "apple", "problems": ["apple_scab"],
          "outcome": "diagnosed", "quality": "any"}


def obs(taxon_id=7, ancestors=(1, 2), licences=("cc-by", "cc-by-nc", "cc0", None)):
    return {"id": 42, "uri": "https://www.inaturalist.org/observations/42", "quality_grade": "research",
            "observed_on": "2026-06-01",
            "taxon": {"id": taxon_id, "name": "Venturia inaequalis", "ancestor_ids": list(ancestors)},
            "photos": [{"id": 100 + i, "license_code": code, "attribution": "(c) someone, some rights reserved (CC BY)",
                        "url": f"https://inaturalist-open-data.s3.amazonaws.com/photos/{100 + i}/square.jpg"}
                       for i, code in enumerate(licences)]}


def test_taxa_match_exactly_unless_descendants_are_asked_for():
    assert inat.matches(obs(taxon_id=7), 7)
    assert not inat.matches(obs(taxon_id=8, ancestors=(7,)), 7)
    assert inat.matches(obs(taxon_id=8, ancestors=(7,)), 7, "descendants")
    assert not inat.matches(obs(taxon_id=8, ancestors=(3,)), 7, "descendants")


def test_only_cc0_and_cc_by_photos_are_kept_at_full_size():
    c = inat.candidate(obs(), TARGET)
    assert [(p["id"], p["licence"]) for p in c["photos"]] == [(100, "CC BY"), (102, "CC0")]
    assert c["photos"][0]["url"].endswith("/photos/100/original.jpg")
    assert c["photos"][0]["file"] == "inat/42-100.jpg"


def test_strip_drops_the_metadata_and_applies_the_orientation():
    exif = Image.Exif()
    exif[0x0112] = 6  # rotated 90°
    exif[0x8825] = {1: "N", 2: (43.0, 39.0, 0.0)}  # GPS
    buf = io.BytesIO()
    Image.new("RGB", (40, 20), "green").save(buf, "JPEG", exif=exif)
    with Image.open(io.BytesIO(inat.strip(buf.getvalue()))) as out:
        assert out.size == (20, 40)
        assert not out.getexif()
        assert "exif" not in out.info


def test_a_rerun_keeps_the_review_edits():
    edited = {**inat.review_entry(inat.candidate(obs(), TARGET), TARGET), "keep": True}
    other = {**edited, "obs": 43, "keep": False}
    merged = inat.merge_review([edited], [inat.review_entry(inat.candidate(obs(), TARGET), TARGET), other])
    assert merged == [edited, other]


@pytest.fixture
def photos(tmp_path):
    (tmp_path / "inat").mkdir()
    for pid in (100, 102):
        (tmp_path / "inat" / f"42-{pid}.jpg").write_bytes(b"photo %d" % pid)
    return tmp_path


def kept(images, **over):
    cand = inat.candidate(obs(), TARGET)
    entry = {**inat.review_entry(cand, TARGET), "keep": True, "images": images, **over}
    return entry, cand


def test_a_kept_observation_becomes_a_case(photos):
    entry, cand = kept({100: "leaf_closeup", 102: "whole_plant"})
    c = inat.case(entry, cand, photos, "sam-michaels")
    assert c["case_id"] == "apple-inat-42" and c["plant_key"] == "inat-42"
    assert [i["kind"] for i in c["images"]] == ["whole_plant", "leaf_closeup"]
    assert c["images"][0]["file"] == "inat/42-102.jpg" and len(c["images"][0]["sha256"]) == 64
    assert c["truth"] == {"outcome": "diagnosed", "species_id": "apple", "problems": ["apple_scab"],
                          "organisms": [], "damage": []}
    assert (c["label_basis"], c["label_confidence"], c["licence"]) == ("source_caption", "probable", "CC BY and CC0")
    assert c["source"] == "https://www.inaturalist.org/observations/42" and c["reviewed_by"] == ""
    assert c["split"] == inat.split_of("inat-42") and c["split"] in ("dev", "test")
    assert inat.case({**entry, "split": "test"}, cand, photos, "x")["split"] == "test"


@pytest.mark.parametrize("images, over, needle", [
    ({100: "leaf_closeup", 102: "skip"}, {}, "exactly one whole_plant"),
    ({100: "whole_plant", 102: "whole_plant"}, {}, "at most 1"),
    ({100: "whole_plant", 102: "close-up"}, {}, "kind 'close-up'"),
    ({100: "whole_plant"}, {"outcome": "sick"}, "outcome"),
    ({100: "whole_plant"}, {"problems": []}, "needs its problems"),
    ({100: "whole_plant"}, {"confidence": "maybe"}, "confidence"),
])
def test_a_case_that_isnt_a_valid_scan_is_refused(photos, images, over, needle):
    entry, cand = kept(images, **over)
    with pytest.raises(ValueError, match=needle):
        inat.case(entry, cand, photos, "x")


def test_the_split_is_stable_and_mostly_dev():
    splits = [inat.split_of(f"inat-{i}") for i in range(1000)]
    assert splits == [inat.split_of(f"inat-{i}") for i in range(1000)]
    assert 0.55 < splits.count("dev") / 1000 < 0.65


def test_fetch_refuses_photos_from_before_2026(tmp_path):
    with pytest.raises(SystemExit, match="2026-01-01"):
        inat.main(["fetch", "--since", "2025-06-01", "--photos", str(tmp_path)])


def test_a_name_shared_by_a_complex_and_a_species_picks_the_species():
    results = [{"id": 1, "name": "Podosphaera xanthii", "rank": "complex"},
               {"id": 2, "name": "Podosphaera xanthii", "rank": "species"},
               {"id": 3, "name": "Podosphaera", "rank": "genus"}]
    assert inat.pick_taxon(results, "Podosphaera xanthii") == 2
    assert inat.pick_taxon(results, "Podosphaera xanthii", "complex") == 1
    assert inat.pick_taxon(results, "Podosphaera") == 3
    with pytest.raises(SystemExit):
        inat.pick_taxon(results, "Erysiphe necator")
