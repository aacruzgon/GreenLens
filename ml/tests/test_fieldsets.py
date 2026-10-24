"""`training.eval.fieldsets`: the set list, picking, copy and repeat removal, the review and the cases."""

import json
import re
from pathlib import Path

import numpy as np
import pytest
import yaml

from ml.training.eval import fieldsets as fs
from ml.training.eval import inat

ML = Path(__file__).resolve().parents[1]
SET = {"name": "maize-bd-2026", "dataset": "sb4wwjs8bz", "version": 1, "licence": "CC BY 4.0",
       "citation": "Someone (2026) Mendeley Data V1",
       "classes": {"m/RAW_Image/Common_Rust": {"species_id": "corn", "problems": ["corn_common_rust"],
                                               "outcome": "diagnosed"}}}


def test_no_field_set_is_a_training_source():
    train = (ML / "training/data/sources.yaml").read_text()
    for s in yaml.safe_load(fs.SETS.read_text())["sets"]:
        assert s["dataset"] not in train, s["name"]


def test_every_class_names_a_problem_the_training_mappings_know():
    known = set()
    for m in (ML / "training/data/mappings").glob("*.yaml"):
        known |= set(re.findall(r"problem_id:\s*([a-z0-9_]+)", m.read_text()))
    for s in yaml.safe_load(fs.SETS.read_text())["sets"]:
        assert s["licence"] == "CC BY 4.0", s["name"]
        for folder, c in s["classes"].items():
            assert c["outcome"] in ("diagnosed", "healthy"), folder
            assert set(c["problems"]) <= known, folder
            inat.check_truth(c["outcome"], c["problems"], c.get("look_alike"), folder)  # a look-alike names no problem
            assert bool(c["problems"]) == (c["outcome"] == "diagnosed" and not c.get("look_alike")), folder


def test_the_pick_order_is_stable_and_skips_non_images():
    rows = [{"path": f"m/RAW_Image/Common_Rust/IMG_{i}.jpeg"} for i in range(30)] + [{"path": "m/labels.csv"}]
    order = fs.pick_order("maize-bd-2026", rows)
    assert len(order) == 30
    assert order == fs.pick_order("maize-bd-2026", list(reversed(rows)))
    assert [r["path"] for r in order[:5]] != [r["path"] for r in rows[:5]]
    assert fs.photo_id("maize-bd-2026", "m/RAW_Image/Common_Rust", "m/RAW_Image/Common_Rust/IMG_201.jpeg") \
        == "maize-bd-2026/common-rust/img-201"


def unit(*v):
    a = np.zeros(8, np.float32)
    a[: len(v)] = v
    return a


def test_copies_of_training_images_and_repeats_of_one_plant_are_dropped():
    train = np.stack([unit(1), unit(0, 1)])
    emb = np.stack([
        unit(1, 0.01),       # 0: a copy of training image 0
        unit(0, 0, 1),       # 1: kept
        unit(0, 0, 1, 0.1),  # 2: the same plant as 1
        unit(0, 0, 0, 1),    # 3: kept
        unit(0, 0, 0, 0, 1), # 4: kept, but over the class's quota
        unit(0, 0, 0, 0, 0, 1),  # 5: another class, kept
    ])
    kept, copies = fs.select([[0, 1, 2, 3, 4], [5]], emb, train, per_class=2)
    assert kept == [1, 3, 5]
    assert list(copies) == [0] and copies[0][0] == 0 and copies[0][1] > 0.99


def test_a_rerun_keeps_the_verdicts_of_photos_still_picked():
    other = {**SET, "name": "tomato-bd-2025"}
    old = [{**fs.review_block(SET, "m/RAW_Image/Common_Rust", ["a", "b", "d"]), "photos": {"a": "sure", "b": "skip", "d": "probable"}},
           {**fs.review_block(other, "m/RAW_Image/Common_Rust", ["t"]), "photos": {"t": "sure"}}]
    new = [fs.review_block(SET, "m/RAW_Image/Common_Rust", ["a", "b", "c"])]
    merged, dropped = fs.merge_review(old, new)
    assert [b["photos"] for b in merged] == [{"a": "sure", "b": "skip", "c": "skip"}, {"t": "sure"}]
    assert dropped == 1  # d, no longer picked


@pytest.fixture
def photo(tmp_path):
    (tmp_path / "fieldsets/maize-bd-2026/common-rust").mkdir(parents=True)
    (tmp_path / "fieldsets/maize-bd-2026/common-rust/img-201.jpg").write_bytes(b"photo")
    pid = "maize-bd-2026/common-rust/img-201"
    cand = {"set": SET["name"], "class": "m/RAW_Image/Common_Rust", "path": "m/RAW_Image/Common_Rust/IMG_201.jpeg",
            "file": f"fieldsets/{pid}.jpg", "source": fs.source_url(SET, "m/RAW_Image/Common_Rust/IMG_201.jpeg")}
    return tmp_path, pid, cand


def test_a_reviewed_photo_becomes_a_one_photo_case(photo):
    root, pid, cand = photo
    block = fs.review_block(SET, cand["class"], [pid])
    c = fs.case(block, pid, "sure", cand, SET, root, "sam-michaels")
    assert c["case_id"] == "corn-maize-bd-2026-common-rust-img-201"
    assert [i["kind"] for i in c["images"]] == ["whole_plant"] and len(c["images"][0]["sha256"]) == 64
    assert c["truth"]["problems"] == ["corn_common_rust"] and c["label_confidence"] == "sure"
    assert c["source"] == "https://data.mendeley.com/datasets/sb4wwjs8bz/1#m/RAW_Image/Common_Rust/IMG_201.jpeg"
    assert c["licence"] == "CC BY 4.0" and c["credit"].endswith("re-encoded, metadata removed")
    with pytest.raises(ValueError, match="verdict"):
        fs.case(block, pid, "maybe", cand, SET, root, "x")
    with pytest.raises(ValueError, match="needs its problems"):
        fs.case({**block, "problems": []}, pid, "sure", cand, SET, root, "x")


def test_add_writes_reviewed_photos_once(photo, monkeypatch):
    root, pid, cand = photo
    monkeypatch.setattr(fs, "load_sets", lambda only: [SET])
    (root / "fieldsets/candidates.json").write_text(json.dumps({pid: cand}))
    block = {**fs.review_block(SET, cand["class"], [pid]), "photos": {pid: "probable"}}
    (root / "fieldsets/review.yaml").write_text(yaml.safe_dump([block]))
    manifest = root / "manifest.jsonl"
    args = ["add", "--labeller", "x", "--photos", str(root), "--manifest", str(manifest)]
    assert fs.main(args) == 0 and fs.main(args) == 0
    lines = manifest.read_text().splitlines()
    assert len(lines) == 1 and json.loads(lines[0])["label_confidence"] == "probable"
    (root / "fieldsets/review.yaml").write_text(yaml.safe_dump([{**block, "photos": {pid: "skip"}}]))
    manifest.unlink()
    assert fs.main(args) == 0 and not manifest.exists()


def one_photo(file, plant_key=None, split="dev"):
    return {"case_id": file, "split": split, "plant_key": plant_key or fs.own_key(file), "images": [{"file": file}]}


def test_a_frame_is_the_first_number_in_the_file_name():
    assert fs.frame("fieldsets/t/healthy/img-3472-02d.jpg") == ("fieldsets/t/healthy", 3472)
    assert fs.frame("fieldsets/c/septoria/septoria-leaf-spot1910-ko9m8s.jpg") == ("fieldsets/c/septoria", 1910)
    assert fs.frame("fieldsets/c/septoria/leaf.jpg") == ("fieldsets/c/septoria", None)


def test_neighbouring_frames_share_the_lowest_frames_plant_key_and_split():
    files = ["fieldsets/s/a/leaf-10.jpg", "fieldsets/s/a/leaf-7.jpg", "fieldsets/s/a/leaf-8.jpg",
             "fieldsets/s/a/leaf-14.jpg", "fieldsets/s/b/leaf-9.jpg", "fieldsets/s/a/leaf.jpg"]
    cases = [one_photo(f, split="test") for f in files] + [one_photo("inat/1/2.jpg", "inat-1", "test")]
    cases[0] = {**cases[0], "split": "dev"}
    out = fs.group_frames(cases)
    assert [c["plant_key"] for c in out[:3]] == ["s-a-leaf-7"] * 3
    assert {c["split"] for c in out[:3]} == {"test"}  # one of its cases is in test, so the plant is
    assert out[3]["plant_key"] == "s-a-leaf-14" and out[3]["split"] == "test"
    assert out[4]["plant_key"] == "s-b-leaf-9"
    assert out[5:] == cases[5:]
    before = fs.group_frames(cases, frozen=False)  # the rule before M18: the key's split
    assert {c["split"] for c in before[:3]} == {inat.split_of("s-a-leaf-7")} == {"dev"}


def test_a_new_plant_goes_to_dev_since_the_test_split_is_frozen():
    files = ["fieldsets/s/a/leaf-1.jpg", "fieldsets/s/a/leaf-2.jpg", "fieldsets/s/c/leaf-5.jpg"]
    out = fs.group_frames([one_photo(f) for f in files] + [one_photo("fieldsets/s/c/leaf-6.jpg", split="test")])
    assert [c["split"] for c in out] == ["dev", "dev", "test", "test"]


def add_beside(photo, monkeypatch, there: str):
    """The fixture's img-201 reviewed, with the case `there` already in the manifest on its own key."""
    root, pid, cand = photo
    monkeypatch.setattr(fs, "load_sets", lambda only: [SET])
    (root / "fieldsets/candidates.json").write_text(json.dumps({pid: cand}))
    block = {**fs.review_block(SET, cand["class"], [pid]), "photos": {pid: "sure"}}
    (root / "fieldsets/review.yaml").write_text(yaml.safe_dump([block]))
    manifest = root / "manifest.jsonl"
    case = {**one_photo(there, split=inat.split_of(fs.own_key(there))), "source": "elsewhere"}
    manifest.write_text(json.dumps(case) + "\n")
    return manifest, ["add", "--labeller", "x", "--photos", str(root), "--manifest", str(manifest)]


KEY_201 = "maize-bd-2026-common-rust-img-201"


def test_add_gives_a_case_already_there_its_new_neighbours_key(photo, monkeypatch):
    there = "fieldsets/maize-bd-2026/common-rust/img-204.jpg"
    manifest, args = add_beside(photo, monkeypatch, there)
    assert json.loads(manifest.read_text())["split"] == inat.split_of(fs.own_key(there)) == "dev"
    assert fs.main(args) == 0
    cases = [json.loads(line) for line in manifest.read_text().splitlines()]
    assert [(c["plant_key"], c["split"]) for c in cases] == [(KEY_201, "dev")] * 2


def test_add_leaves_out_a_new_neighbour_of_a_test_case(photo, monkeypatch, capsys):
    there = "fieldsets/maize-bd-2026/common-rust/img-203.jpg"
    manifest, args = add_beside(photo, monkeypatch, there)
    before = manifest.read_text()
    assert json.loads(before)["split"] == "test"
    assert fs.main(args) == 1 and manifest.read_text() == before  # nor is the test case rekeyed
    assert "a neighbouring frame of a test case" in capsys.readouterr().err


def test_a_key_set_by_hand_is_kept_and_its_run_takes_it():
    a = "fieldsets/s/a/leaf-"
    cases = [one_photo(a + "40.jpg"), one_photo(a + "42.jpg"), one_photo(a + "47.jpg", "s-a-leaf-42"),
             one_photo(a + "80.jpg", "plant-x"), one_photo("fieldsets/s/b/leaf-3.jpg", "plant-x"), one_photo(a + "60.jpg")]
    out = fs.group_frames(cases)
    assert [c["plant_key"] for c in out] == ["s-a-leaf-42"] * 3 + ["plant-x"] * 2 + ["s-a-leaf-60"]
    assert {c["split"] for c in out} == {"dev"}
    assert fs.group_frames(out) == out
    before = fs.group_frames(cases, frozen=False)
    assert {c["split"] for c in before[3:5]} == {inat.split_of("plant-x")} == {"test"}


def test_a_key_grouping_set_follows_its_run_when_a_lower_frame_joins():
    a = "fieldsets/s/a/leaf-"
    out = fs.group_frames([one_photo(a + "30.jpg"), one_photo(a + "31.jpg", "s-a-leaf-30"), one_photo(a + "27.jpg")])
    assert [c["plant_key"] for c in out] == ["s-a-leaf-27"] * 3


def test_one_plant_with_two_keys_set_by_hand_is_refused():
    a = "fieldsets/s/a/leaf-"
    with pytest.raises(ValueError, match="two keys set by hand"):
        fs.group_frames([one_photo(a + "40.jpg", "plant-x"), one_photo(a + "42.jpg", "plant-y")])


def test_regroup_groups_the_manifest_and_moves_a_case_only_when_told(tmp_path):
    def manifest_of(*files):
        m = tmp_path / "manifest.jsonl"
        m.write_text("".join(json.dumps(one_photo(f, split=inat.split_of(fs.own_key(f)))) + "\n" for f in files))
        return m, ["regroup", "--manifest", str(m)]

    m, args = manifest_of("fieldsets/maize-bd-2026/common-rust/img-201.jpg", "fieldsets/maize-bd-2026/common-rust/img-203.jpg")
    assert fs.main(args) == 0
    assert {json.loads(line)["plant_key"] for line in m.read_text().splitlines()} == {KEY_201}
    m, args = manifest_of("fieldsets/maize-bd-2026/common-rust/img-201.jpg", "fieldsets/maize-bd-2026/common-rust/img-204.jpg")
    before = m.read_text()
    assert fs.main(args) == 1 and m.read_text() == before
    assert fs.main(args + ["--resplit"]) == 0
    assert {json.loads(line)["split"] for line in m.read_text().splitlines()} == {inat.split_of(KEY_201)}


def test_the_manifests_neighbouring_frames_share_a_plant_key():
    lines = inat.MANIFEST.read_text().splitlines()
    assert fs.regroup(lines) == lines, "run `python -m ml.training.eval.fieldsets regroup` (evaluation.md §2, Splits)"


def test_the_copy_check_skips_sources_the_leakage_check_skips(tmp_path, monkeypatch):
    (tmp_path / "raw/plantdoc_cls/a").mkdir(parents=True)
    (tmp_path / "raw/plantdoc_cls/a/1.jpg").write_bytes(b"x")
    sources = tmp_path / "sources.yaml"
    sources.write_text(yaml.safe_dump({"datasets": [{"name": "plantdoc_cls"}, {"name": "coco_val2017"},
                                                    {"name": "plantseg", "leakage": False},
                                                    {"name": "openimages_v7_faces"}]}))
    monkeypatch.setattr(fs, "SOURCES", sources)
    monkeypatch.setattr(fs, "RAW", tmp_path / "raw")
    assert fs.training_images() == [tmp_path / "raw/plantdoc_cls/a/1.jpg"]  # plantseg and the face tables aren't on disk, and aren't needed
