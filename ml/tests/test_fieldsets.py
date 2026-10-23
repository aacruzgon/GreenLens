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
            assert bool(c["problems"]) == (c["outcome"] == "diagnosed"), folder


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
    out = fs.group_frames(cases)
    assert [c["plant_key"] for c in out[:3]] == ["s-a-leaf-7"] * 3
    assert {c["split"] for c in out[:3]} == {inat.split_of("s-a-leaf-7")}
    assert out[3]["plant_key"] == "s-a-leaf-14" and out[3]["split"] == inat.split_of("s-a-leaf-14")
    assert out[4]["plant_key"] == "s-b-leaf-9"
    assert out[5:] == cases[5:]


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
    manifest, args = add_beside(photo, monkeypatch, "fieldsets/maize-bd-2026/common-rust/img-203.jpg")
    assert inat.split_of("maize-bd-2026-common-rust-img-203") == inat.split_of(KEY_201)
    assert fs.main(args) == 0
    cases = [json.loads(line) for line in manifest.read_text().splitlines()]
    assert [(c["plant_key"], c["split"]) for c in cases] == [(KEY_201, inat.split_of(KEY_201))] * 2


def test_add_moves_a_case_already_there_to_another_split_only_when_told(photo, monkeypatch, capsys):
    there = "fieldsets/maize-bd-2026/common-rust/img-204.jpg"
    manifest, args = add_beside(photo, monkeypatch, there)
    was, now = inat.split_of(fs.own_key(there)), inat.split_of(KEY_201)
    assert was != now
    before = manifest.read_text()
    assert fs.main(args) == 1 and manifest.read_text() == before
    assert f"{there}: {was} -> {now}" in capsys.readouterr().err
    assert fs.main(args + ["--resplit"]) == 0
    assert [json.loads(line)["split"] for line in manifest.read_text().splitlines()] == [now, now]


def test_the_manifests_neighbouring_frames_share_a_plant_key():
    lines = inat.MANIFEST.read_text().splitlines()
    assert fs.regroup(lines) == lines, "run fieldsets.regroup on manifest.jsonl (evaluation.md §2, Splits)"
