"""`training.eval.commons` and the look-alike truth: licences, the case list, and the cases."""

import json

import pytest
import yaml

from ml.training.eval import commons, inat


@pytest.mark.parametrize("licence, ok", [
    ("CC0", True), ("CC0 1.0", True), ("Public domain", True), ("CC BY 2.0", True), ("CC BY 4.0", True),
    ("CC BY-SA 3.0", True), ("CC BY-SA 3.0 de", True), ("CC BY-NC 2.0", False), ("CC BY-NC-SA 4.0", False),
    ("CC BY-ND 4.0", False), ("GFDL", False), ("OGL 2", False), ("", False),
])
def test_only_licences_that_allow_reuse_with_credit(licence, ok):
    assert bool(commons.LICENCE.match(licence)) is ok


def test_the_case_list_is_valid_and_each_case_is_a_scan():
    cases = commons.load_cases()
    assert len(cases) == len({c["case"] for c in cases}) >= 15
    assert {c["look_alike"] for c in cases if c.get("look_alike")} <= set(inat.LOOK_ALIKES)


@pytest.mark.parametrize("outcome, problems, look_alike, needle", [
    ("diagnosed", [], None, "needs its problems, or a look_alike"),
    ("diagnosed", [], "sunburn", "look_alike must be one of"),
    ("healthy", [], "sunscald", "diagnosed with no problems"),
    ("diagnosed", ["tomato_early_blight"], "sunscald", "diagnosed with no problems"),
    ("sick", [], None, "outcome must be one of"),
])
def test_a_look_alike_is_diagnosed_with_no_problems(outcome, problems, look_alike, needle):
    with pytest.raises(ValueError, match=needle):
        inat.check_truth(outcome, problems, look_alike, "x")
    inat.check_truth("diagnosed", [], "sunscald", "x")
    assert inat.truth_of({"outcome": "diagnosed", "problems": [], "species_id": "pepper", "look_alike": "sunscald"}) == {
        "outcome": "diagnosed", "species_id": "pepper", "problems": [], "organisms": [], "damage": [],
        "look_alike": "sunscald"}


@pytest.mark.parametrize("raw, want", [
    ("2021-11-05 11:52:19", "2021-11-05"), ('<span>Taken on 27 July 2019</span>', "Taken on 27 July 2019"),
    ("10 March 2009 (original upload date)", "10 March 2009 (original upload date)"),
])
def test_taken_reads_a_date_when_commons_gives_one(raw, want):
    assert commons.taken(raw) == want


def test_a_kept_case_becomes_a_dev_case_with_its_credits(tmp_path):
    (tmp_path / "commons").mkdir()
    cands = {}
    for title, licence in (("Hail on tomato.jpg", "CC BY 2.0"), ("Hail on fruit.jpg", "CC BY-SA 4.0")):
        f = commons.photo_file(title)
        (tmp_path / f).write_bytes(b"photo " + title.encode())
        cands[title] = {"file": f, "page": f"https://commons.wikimedia.org/wiki/File:{title}", "licence": licence,
                        "artist": "Someone", "taken": "2008-07-03"}
    c = {"case": "tomato-hail", "species_id": "tomato", "outcome": "diagnosed", "problems": [],
         "look_alike": "hail_damage", "keep": True,
         "files": [{"title": "Hail on fruit.jpg", "kind": "flower_fruit"}, {"title": "Hail on tomato.jpg", "kind": "whole_plant"}]}
    out = commons.case(c, cands, tmp_path, "sam-michaels", {})
    assert (out["case_id"], out["split"], out["plant_key"]) == ("tomato-commons-tomato-hail", "dev", "commons-tomato-hail")
    assert [i["kind"] for i in out["images"]] == ["whole_plant", "flower_fruit"]
    assert out["truth"]["look_alike"] == "hail_damage" and out["truth"]["problems"] == []
    assert out["source"].endswith("File:Hail on tomato.jpg") and out["taken"] == "2008-07-03"
    assert out["licence"] == "CC BY 2.0 and CC BY-SA 4.0"
    assert out["credit"].startswith("Someone, via Wikimedia Commons, CC BY 2.0; re-encoded")
    with pytest.raises(ValueError, match="test split"):
        commons.case(c, cands, tmp_path, "x", {"commons-tomato-hail": "test"})


def test_add_appends_only_kept_cases_once(tmp_path):
    cases = tmp_path / "commons.yaml"
    entry = {"case": "pepper-sunscald", "species_id": "pepper", "outcome": "diagnosed", "problems": [],
             "look_alike": "sunscald", "files": [{"title": "Sunscald.jpg", "kind": "whole_plant"}]}
    cases.write_text(yaml.safe_dump({"cases": [{**entry, "keep": True}, {**entry, "case": "other", "keep": False}]}))
    (tmp_path / "commons").mkdir()
    (tmp_path / "commons/sunscald.jpg").write_bytes(b"x")
    (tmp_path / "commons/candidates.json").write_text(json.dumps({"Sunscald.jpg": {
        "file": "commons/sunscald.jpg", "page": "https://commons.wikimedia.org/wiki/File:Sunscald.jpg",
        "licence": "CC BY 3.0", "artist": "Don Ferrin", "taken": "2019-05-12"}}))
    manifest = tmp_path / "manifest.jsonl"
    args = ["add", "--labeller", "x", "--cases", str(cases), "--photos", str(tmp_path), "--manifest", str(manifest)]
    assert commons.main(args) == 0 and commons.main(args) == 0
    assert [json.loads(line)["case_id"] for line in manifest.read_text().splitlines()] == ["pepper-commons-pepper-sunscald"]


@pytest.mark.parametrize("short, stored", [
    ("CC0", "CC0 1.0"), ("CC0 1.0", "CC0 1.0"), ("Public domain", "Public domain"), ("CC BY-SA 4.0", "CC BY-SA 4.0"),
])
def test_cc0_is_stored_as_cc0_1_0_as_inaturalists_is(short, stored):
    assert commons.licence_name(short) == stored and commons.LICENCE.match(stored)
