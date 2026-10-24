"""The face check's scoring (ml/training/eval/face_check.py): coverage of a face by the grown
boxes, size buckets, upload variants, and a whole `score` run on fake example output."""

from __future__ import annotations

import json
import math

import numpy as np
import pytest
from PIL import Image

from ml.training.data import faces as F
from ml.training.eval import face_check as fc


def det(x, y, w, h, score=0.9) -> dict:
    return {"x": x, "y": y, "w": w, "h": h, "score": score}


def test_growth_matches_the_rust_facebox():
    # FaceBox::grown's own test: (0, 0.4, 0.2, 0.2) grown 25% is x 0..0.25, y 0.35..0.65
    assert fc.grown(det(0.0, 0.4, 0.2, 0.2)) == pytest.approx((0.0, 0.35, 0.25, 0.65))
    assert fc.grown(det(0.9, 0.9, 0.1, 0.1))[2:] == (1.0, 1.0)


def test_coverage_is_the_union_without_double_counting():
    face = (0.0, 0.0, 1.0, 1.0)
    assert fc.covered_share(face, []) == 0.0
    assert fc.covered_share(face, [(-1, -1, 2, 2)]) == pytest.approx(1.0)
    assert fc.covered_share(face, [(0, 0, 0.5, 1), (0.25, 0, 0.75, 1)]) == pytest.approx(0.75)
    assert fc.covered_share(face, [(0, 0, 0.5, 0.5), (0.5, 0.5, 1, 1)]) == pytest.approx(0.5)
    assert fc.covered_share(face, [(2, 2, 3, 3)]) == 0.0
    assert fc.covered_share((0.2, 0.2, 0.2, 0.4), [(0, 0, 1, 1)]) == 0.0     # an empty face


def test_union_merge_matches_the_rust_test():
    # faces::tests::union_merge_unites_overlaps_and_containment
    merged = fc.union_merge([det(0.1, 0.1, 0.2, 0.2), det(0.15, 0.15, 0.2, 0.2), det(0.12, 0.12, 0.05, 0.05),
                             det(0.7, 0.7, 0.1, 0.1)], 0.2)
    assert len(merged) == 2
    big = next(b for b in merged if b["x"] < 0.5)
    assert big["w"] == pytest.approx(0.25)
    assert fc.union_merge([det(0.1, 0.1, 0.1, 0.1), det(0.18, 0.1, 0.1, 0.1)], 0.2) == [
        det(0.1, 0.1, 0.1, 0.1), det(0.18, 0.1, 0.1, 0.1)]      # IoU 0.11: kept apart


def test_blurred_share_is_the_grown_union():
    assert fc.blurred_share([]) == 0.0
    assert fc.blurred_share([det(0.4, 0.4, 0.2, 0.2)], 0.25) == pytest.approx(0.09)    # 0.3 x 0.3
    assert fc.blurred_share([det(0.4, 0.4, 0.2, 0.2)] * 2, 0.0) == pytest.approx(0.04)


def test_size_buckets():
    assert [fc.size_bucket(px) for px in (0, 19.99, 20, 39.9, 40, 159.9, 160, 4000)] == [
        "0-20 px", "0-20 px", "20-40 px", "20-40 px", "40-80 px", "80-160 px", "160+ px", "160+ px"]
    assert fc.SIZE_NAMES == ["0-20 px", "20-40 px", "40-80 px", "80-160 px", "160+ px"]


def face(x0, x1, y0, y1, occluded=0, truncated=0) -> dict:
    return {"x0": x0, "x1": x1, "y0": y0, "y1": y1, "occluded": occluded, "truncated": truncated,
            "group": 0, "depiction": 0}


def test_a_face_is_caught_when_the_grown_boxes_cover_90_percent():
    # 4000 x 3000 photo, stored at 1600 x 1200: a face 0.02 wide is 32 px stored and 80 px full.
    f = face(0.50, 0.52, 0.50, 0.53)
    tight = det(0.502, 0.502, 0.016, 0.026)     # 80% of the face's box, but grown 25% it covers all
    [s] = fc.score_photo({"faces": [tight]}, [f], (4000, 3000))
    assert s["stored_px"] == 32.0 and s["full_px"] == 80.0
    assert s["covered"] == pytest.approx(1.0) and s["caught"]
    half = det(0.50, 0.50, 0.008, 0.03)         # the left 40%, grown: 50% of the face
    [s] = fc.score_photo({"faces": [half]}, [f], (4000, 3000))
    assert s["covered"] == pytest.approx(0.5, abs=0.01) and not s["caught"]
    assert not fc.score_photo({"faces": []}, [f], (4000, 3000))[0]["caught"]


def test_a_box_matches_a_face_by_centre_or_overlap():
    f = face(0.4, 0.5, 0.4, 0.5)
    assert fc.matches_a_face(det(0.41, 0.41, 0.08, 0.08), [f])
    assert fc.matches_a_face(det(0.3, 0.3, 0.3, 0.3), [f])        # holds the whole face
    assert not fc.matches_a_face(det(0.8, 0.8, 0.05, 0.05), [f])
    assert not fc.matches_a_face(det(0.4, 0.4, 0.1, 0.1), [])


def test_variants_downscale_with_lanczos_at_quality_85_and_never_upscale(tmp_path):
    src = tmp_path / "p.jpg"
    Image.new("RGB", (4000, 2500), (90, 140, 60)).save(src, quality=95)
    assert fc.make_variant(src, tmp_path / "o.jpg", None) == src.stat().st_size
    n = fc.make_variant(src, tmp_path / "v3200.jpg", 3200)
    assert n == (tmp_path / "v3200.jpg").stat().st_size
    assert Image.open(tmp_path / "v3200.jpg").size == (3200, 2000)
    small = tmp_path / "s.jpg"
    Image.new("RGB", (2100, 1400), (90, 140, 60)).save(small, quality=95)
    fc.make_variant(small, tmp_path / "s3200.jpg", 3200)
    assert Image.open(tmp_path / "s3200.jpg").size == (2100, 1400)


def test_dev_photos_never_include_the_test_split(tmp_path):
    m = tmp_path / "manifest.jsonl"
    m.write_text(
        json.dumps({"case_id": "a", "split": "dev", "images": [{"file": "x/1.jpg", "kind": "whole_plant"},
                                                               {"file": "x/2.jpg", "kind": "leaf_closeup"}]}) + "\n"
        + json.dumps({"case_id": "b", "split": "test", "images": [{"file": "x/3.jpg", "kind": "whole_plant"}]}) + "\n")
    photos = fc.dev_photos(m, tmp_path)
    assert [p["key"] for p in photos] == ["a#0", "a#1"]
    assert photos[1]["path"] == tmp_path / "x/2.jpg"


def test_the_test_photos_run_only_when_asked_for(tmp_path):
    m = tmp_path / "manifest.jsonl"
    m.write_text(json.dumps({"case_id": "a", "split": "dev", "images": [{"file": "x/1.jpg", "kind": "whole_plant"}]}) + "\n"
                 + json.dumps({"case_id": "b", "split": "test", "images": [{"file": "x/3.jpg", "kind": "whole_plant"}]}) + "\n")
    assert [p["key"] for p in fc.split_photos("test", m, tmp_path)] == ["b#0"]
    assert "test" in fc.RUN_SETS and "test" not in fc.SETS     # `score` never reads them


def test_score_writes_the_report_from_the_example_output(tmp_path, monkeypatch):
    """One Open Images photo with two faces and one dev photo, through `score`."""
    faces_dir, tables = tmp_path / "faces", tmp_path / "tables"
    (faces_dir / "photos").mkdir(parents=True)
    tables.mkdir()
    Image.new("RGB", (4000, 3000), (120, 120, 120)).save(faces_dir / "photos/oi1.jpg", quality=95)
    index = tmp_path / "faces_openimages.tsv"
    index.write_text("\t".join(F.COLUMNS) + "\n" + "\t".join([
        "oi1", "test", "photos/oi1.jpg", "4000", "3000", "3000000", "CC BY 2.0", "A", "https://x", "2",
        "32.0", "80.0", "1", "0", "1.0"]) + "\n")
    (tables / F.SPLITS["test"][0]).write_text(
        "ImageID,Source,LabelName,Confidence,XMin,XMax,YMin,YMax,IsOccluded,IsTruncated,IsGroupOf,IsDepiction,IsInside\n"
        "oi1,xclick,/m/0dzct,1,0.50,0.52,0.50,0.53,0,0,0,0,0\n"       # 32 px stored, 80 px full
        "oi1,xclick,/m/0dzct,1,0.10,0.1095,0.10,0.115,1,0,0,0,0\n")   # 15 px stored, 38 px full
    photos = tmp_path / "photos"
    (photos / "x").mkdir(parents=True)
    Image.new("RGB", (1000, 800), (40, 160, 40)).save(photos / "x/leaf.jpg", quality=95)
    manifest = tmp_path / "manifest.jsonl"
    manifest.write_text(json.dumps({"case_id": "c", "split": "dev",
                                    "images": [{"file": "x/leaf.jpg", "kind": "leaf_closeup"}]}) + "\n")
    runs = tmp_path / "runs"
    for name, value in [("INDEX", index), ("FACES", faces_dir), ("TABLES", tables)]:
        monkeypatch.setattr(F, name, value)
    for name, value in [("RUNS", runs), ("CROPS", runs / "crops"), ("MANIFEST", manifest),
                        ("EVAL_PHOTOS", photos), ("REPORTS", tmp_path / "reports")]:
        monkeypatch.setattr(fc, name, value)

    def line(path, size, boxes, blur=500.0) -> str:
        return json.dumps({"path": str(path), "width": size[0], "height": size[1], "blur_score": blur,
                           "exposure": 0.4, "dark_share": 0.0, "bright_share": 0.0, "verdict": None,
                           "faces": boxes, "stored": {"width": 1600, "height": 1200, "bytes": 200_000},
                           "ms": {"decode": 50.0, "quality": 9.0, "detect": 900.0, "store": 200.0, "total": 1160.0}})

    oi = fc.openimages_photos()
    dev = fc.dev_photos()
    files = {"openimages": fc.variants_of("openimages", oi), "dev": fc.variants_of("dev", dev)}
    sizes = {"original": (4000, 3000), "3200": (3200, 2400), "2048": (2048, 1536)}
    dev_sizes = {"original": (1000, 800), "3200": (1000, 800), "2048": (1000, 800)}
    for t in (0.3, 0.2):
        for v in fc.VARIANTS:
            boxes = [det(0.502, 0.502, 0.016, 0.026)]
            if t == 0.2:
                boxes.append(det(0.101, 0.101, 0.008, 0.013))         # the small face too
                boxes.append(det(0.8, 0.2, 0.02, 0.03, 0.21))          # and a box on nothing
            out = fc.raw_path("openimages", v, t)
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(line(files["openimages"][v]["oi1"][0], sizes[v], boxes, 500.0 if v == "original" else 520.0) + "\n")
            dev_boxes = [det(0.5, 0.5, 0.05, 0.05, 0.25)] if t == 0.2 else []
            fc.raw_path("dev", v, t).write_text(line(files["dev"][v]["c#0"][0], dev_sizes[v], dev_boxes) + "\n")

    # an extra pass at 640 px that finds the second face and a box on the leaf
    fc.raw_path("openimages", "original", 0.3, 640).write_text(
        line(files["openimages"]["original"]["oi1"][0], sizes["original"], [det(0.101, 0.101, 0.008, 0.013)]) + "\n")
    fc.raw_path("dev", "original", 0.3, 640).write_text(
        line(files["dev"]["original"]["c#0"][0], dev_sizes["original"], [det(0.1, 0.1, 0.2, 0.2, 0.4)]) + "\n")

    assert fc.main(["score", "--date", "2026-10-22", "--out", "faces-test", "--crop-thresholds", "0.2"]) == 0
    report = json.loads((tmp_path / "reports/faces-test.json").read_text())
    assert report["faces"] == 2 and report["thresholds"] == [0.2, 0.3]
    r = report["recall"]["original@0.3"]
    assert r["all"] == {"faces": 1, "caught": 1, "recall": 1.0}            # the 16 px face doesn't count
    assert r["every_face"] == {"faces": 2, "caught": 1, "recall": 0.5}
    assert r["by_stored_size"]["0-20 px"] == {"faces": 1, "caught": 0, "recall": 0.0}
    assert r["under_40_full_all"]["faces"] == 1 and r["stored_20_40"]["faces"] == 1
    assert report["recall"]["2048@0.2"]["every_face"]["recall"] == 1.0
    assert report["unmatched_boxes"]["original@0.2"]["boxes"] == 1
    assert report["unmatched_boxes"]["original@0.3"]["boxes"] == 0
    assert report["dev_false_faces"]["original@0.3"]["with_a_box"] == 0
    assert report["dev_false_faces"]["original@0.2"] | {"list": []} == {"photos": 1, "with_a_box": 1, "share": 1.0, "list": []}
    drift = report["quality"]["openimages"]["drift_from_original"]["3200"]
    assert drift["resized_photos"] == 1 and drift["blur_ratio"]["median"] == pytest.approx(1.04)
    assert report["quality"]["dev"]["drift_from_original"]["2048"]["resized_photos"] == 0
    assert report["cost"]["openimages"]["original"]["detect_ms"]["median"] == 900
    shipped, extra = report["proposals"][0], report["proposals"][len(fc.GROWS)]
    assert (shipped["setup"], shipped["grow"], shipped["dev_with_a_box"]) == ("shipped", 0.25, 0)
    assert extra["setup"] == "extra pass at 640 px" and extra["dev_with_a_box"] == 1
    assert extra["dev_blurred_share_mean"] == pytest.approx(0.09)        # 0.3 x 0.3 of the one photo
    md = (tmp_path / "reports/faces-test.md").read_text()
    assert "| extra pass at 640 px | 25% | 100.0% (1/1) | 1/1 | 9.0% (9%) | 0 |" in md
    assert "original 100.0% (1/1)" in md and "| original | 0.2 | 1/1 | 100.0% | `c#0` |" in md
    crops = {p.name for p in (runs / "crops").glob("*.jpg")}
    assert "unmatched-original-0.2-oi1-2.jpg" in crops and "dev-original-0.2-c_0-0.jpg" in crops
    assert "sheet-dev-original-0.2-01.jpg" in crops
    assert not any("-0.3-" in c for c in crops)                    # only the thresholds asked for
    # boxes under 12 px in the stored image dropped: the dev box (0.05 x 1000 px) stays, and at 16
    # px the small face's box (0.008 x 1600 = 12.8 px) goes, which doesn't change counted recall
    tradeoff = report["min_box"]["original@0.2"]
    assert tradeoff["12"]["dev_with_a_box"] == 1 and tradeoff["16"]["recall"]["recall"] == 1.0
    assert tradeoff["0"]["dev_share"] == 1.0


# --- the landmark filter ------------------------------------------------------------------------

def member(points, box=(100.0, 100.0, 100.0, 120.0), score=0.9, pass_="whole") -> dict:
    """A pass box as the example's `--landmarks` writes it: pixels of the upright image."""
    x, y, w, h = box
    return {"pass": pass_, "x": x, "y": y, "w": w, "h": h, "score": score, "kps": [list(p) for p in points]}


# right eye, left eye, nose tip, right and left mouth corners: an upright face in (100, 100, 100, 120)
FRONTAL = [(130, 150), (170, 150), (150, 175), (135, 200), (165, 200)]
PROFILE_RIGHT = [(165, 150), (175, 151), (188, 176), (166, 200), (173, 200)]   # turned: points to one side


def rotated(points, degrees, centre=(150, 175)):
    a = math.radians(degrees)
    cx, cy = centre
    return [(cx + (x - cx) * math.cos(a) - (y - cy) * math.sin(a), cy + (x - cx) * math.sin(a) + (y - cy) * math.cos(a))
            for x, y in points]


def test_landmark_features_of_an_upright_frontal_face():
    f = fc.landmark_features(member(FRONTAL))
    size = (100 * 120) ** 0.5
    assert f["outside"] == 0.0 and f["roll"] == pytest.approx(0.0)
    assert f["nose_height"] == pytest.approx(0.5)            # halfway from the mouth line to the eye line
    assert f["eye_mouth"] == pytest.approx(50 / size)
    assert f["eye_side"] == pytest.approx(40 / 50) and f["mouth_side"] == pytest.approx(30 / 50)
    assert f["eye_width"] == pytest.approx(40 / size)
    assert f["mouth_eye"] == pytest.approx(0.75) and f["yaw"] == pytest.approx(0.8)


def test_only_roll_changes_when_the_face_tilts():
    upright = fc.landmark_features(member(FRONTAL))
    for degrees in (30, -75, 180):
        f = fc.landmark_features(member(rotated(FRONTAL, degrees)))
        assert f["roll"] == pytest.approx(abs(degrees))
        for k in ("nose_height", "eye_mouth", "eye_side", "mouth_side", "eye_width", "mouth_eye"):
            assert f[k] == pytest.approx(upright[k]), k


def test_points_outside_the_box_are_measured_by_the_box_side():
    points = FRONTAL[:4] + [(215, 230)]          # a mouth corner 15 px right of a 100 px wide box
    assert fc.landmark_features(member(points))["outside"] == pytest.approx(0.15)
    points = FRONTAL[:4] + [(165, 250)]          # 30 px below a 120 px tall box
    assert fc.landmark_features(member(points))["outside"] == pytest.approx(0.25)


BOUNDS = {"outside max": 0.1, "roll max": 60.0, "nose_height min": 0.2, "nose_height max": 0.7,
          "eye_mouth min": 0.25, "eye_mouth max": 0.7, "eye_side min": 0.0, "mouth_side min": 0.0,
          "eye_width max": 0.6, "mouth_eye min": 0.3, "mouth_eye max": 1.5}


def test_the_bounds_keep_profiles_and_tilted_heads():
    assert fc.failed_bounds(fc.landmark_features(member(FRONTAL)), BOUNDS) == []
    profile = fc.landmark_features(member(PROFILE_RIGHT))
    assert profile["yaw"] < fc.PROFILE                       # the eyes collapse to one side
    assert fc.failed_bounds(profile, BOUNDS) == []           # no minimum eye distance
    assert fc.failed_bounds(fc.landmark_features(member(rotated(FRONTAL, 45))), BOUNDS) == []


def test_the_bounds_drop_layouts_that_are_not_a_face():
    def broken(points):
        return fc.failed_bounds(fc.landmark_features(member(points)), BOUNDS)
    assert broken(rotated(FRONTAL, 180)) == ["roll max"]                                        # upside down
    mirrored = [FRONTAL[1], FRONTAL[0], FRONTAL[2], FRONTAL[4], FRONTAL[3]]
    assert broken(mirrored) == ["eye_side min", "mouth_side min"]
    assert broken(FRONTAL[:2] + [(150, 145)] + FRONTAL[3:]) == ["nose_height max"]              # nose above the eyes
    assert broken([(150, 160)] * 5) == list(BOUNDS)          # all points on one spot: no axis, every bound fails
    assert broken(FRONTAL[:3] + [(110, 200), (190, 200)]) == ["mouth_eye max"]                   # a mouth twice the eyes


def test_fit_bounds_keeps_the_share_asked_per_bound():
    rng = np.random.default_rng(0)
    feats = []
    for _ in range(2000):
        jitter = [(x + rng.normal(0, 3), y + rng.normal(0, 3)) for x, y in FRONTAL]
        feats.append(fc.landmark_features(member(rotated(jitter, rng.normal(0, 15)))))
    bounds = fc.fit_bounds(feats, 0.995)
    assert set(bounds) == {fc.bound_name(f, s) for f, s in fc.BOUNDS}
    for name in bounds:
        kept = sum(name not in fc.failed_bounds(f, {name: bounds[name]}) for f in feats)
        if name == "outside max":       # every point is inside the box here: the bound keeps them all
            assert bounds[name] == 0.0 and kept == len(feats)
        else:
            assert kept / len(feats) == pytest.approx(0.995, abs=0.002), name
    assert bounds["nose_height min"] < 0.5 < bounds["nose_height max"]


def test_the_learned_rule_keeps_its_share_of_faces():
    rng = np.random.default_rng(1)

    def sample(points, n):
        return [fc.landmark_features(member([(x + rng.normal(0, 3), y + rng.normal(0, 3)) for x, y in points]))
                for _ in range(n)]
    faces, others = sample(FRONTAL, 400), sample(rotated(FRONTAL, 180), 400)
    rule = fc.LearnedRule(faces, others)
    assert (rule.score(faces) >= rule.cut).mean() == pytest.approx(0.995, abs=0.003)
    assert (rule.score(others) >= rule.cut).mean() == 0.0
    face = {"members": [member(FRONTAL, score=0.3), member(rotated(FRONTAL, 180), score=0.8)]}
    assert not rule.keeps(face)                     # the top member's points decide


def test_a_merged_box_takes_its_top_members_landmarks():
    good = member(FRONTAL, score=0.4, pass_="tile")
    bad = member(rotated(FRONTAL, 180), score=0.6)
    face = {"x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2, "score": 0.6, "members": [good, bad]}
    assert fc.top_member(face) is bad
    assert not fc.looks_like_a_face(face, BOUNDS, "top")
    assert fc.looks_like_a_face(face, BOUNDS, "any")                  # the tile's box passes
    tie = {"members": [member(FRONTAL, score=0.5), member(rotated(FRONTAL, 180), score=0.5)]}
    assert fc.top_member(tie) is tie["members"][0]                    # the first on a tie
    assert fc.looks_like_a_face(tie, BOUNDS, "top")


def test_member_spread_is_the_passes_disagreement_over_the_eyes_to_mouth_distance():
    shifted = [(x + 10, y) for x, y in FRONTAL]                       # every point 10 px right: 0.2 L
    face = {"members": [member(FRONTAL, score=0.9), member(shifted, score=0.5)]}
    assert fc.member_spread(face) == pytest.approx(10 / 50)
    assert fc.member_spread({"members": [member(FRONTAL)]}) is None


def test_halves_alternate_by_sorted_photo_id():
    assert fc.halves(["c", "a", "d", "b"]) == {"a": "A", "b": "B", "c": "A", "d": "B"}


def test_landmarks_writes_the_report_from_the_example_output(tmp_path, monkeypatch):
    """Two Open Images photos (one per half), each with a face whose box has a face's points and a
    box elsewhere whose points are upside down, and one dev photo with the same false box."""
    faces_dir, tables = tmp_path / "faces", tmp_path / "tables"
    (faces_dir / "photos").mkdir(parents=True)
    tables.mkdir()
    rows, boxes_csv = [], []
    for k in ("oia", "oib"):
        Image.new("RGB", (4000, 3000), (120, 120, 120)).save(faces_dir / f"photos/{k}.jpg", quality=90)
        rows.append("\t".join([k, "test", f"photos/{k}.jpg", "4000", "3000", "3000000", "CC BY 2.0", "A",
                               "https://x", "1", "160.0", "400.0", "1", "0", "1.0"]))
        boxes_csv.append(f"{k},xclick,/m/0dzct,1,0.40,0.50,0.40,0.53,0,0,0,0,0")   # 160 px stored
    index = tmp_path / "faces_openimages.tsv"
    index.write_text("\t".join(F.COLUMNS) + "\n" + "\n".join(rows) + "\n")
    (tables / F.SPLITS["test"][0]).write_text(
        "ImageID,Source,LabelName,Confidence,XMin,XMax,YMin,YMax,IsOccluded,IsTruncated,IsGroupOf,IsDepiction,IsInside\n"
        + "\n".join(boxes_csv) + "\n")
    photos = tmp_path / "photos"
    (photos / "x").mkdir(parents=True)
    Image.new("RGB", (1000, 800), (40, 160, 40)).save(photos / "x/leaf.jpg", quality=90)
    manifest = tmp_path / "manifest.jsonl"
    manifest.write_text(json.dumps({"case_id": "c", "split": "dev",
                                    "images": [{"file": "x/leaf.jpg", "kind": "leaf_closeup"}]}) + "\n")
    runs = tmp_path / "runs"
    for name, value in [("INDEX", index), ("FACES", faces_dir), ("TABLES", tables)]:
        monkeypatch.setattr(F, name, value)
    for name, value in [("RUNS", runs), ("MANIFEST", manifest), ("EVAL_PHOTOS", photos),
                        ("REPORTS", tmp_path / "reports")]:
        monkeypatch.setattr(fc, name, value)

    def scaled(points, box, size):
        """FRONTAL's layout moved into `box` (normalised) of an image of `size`."""
        bx, by, bw, bh = box[0] * size[0], box[1] * size[1], box[2] * size[0], box[3] * size[1]
        return [((x - 100) / 100 * bw + bx, (y - 100) / 120 * bh + by) for x, y in points], (bx, by, bw, bh)

    def face_with(points, box, size, score):
        kps, px = scaled(points, box, size)
        return {"x": box[0], "y": box[1], "w": box[2], "h": box[3], "score": score,
                "members": [member(kps, px, score)]}

    def line(path, size, faces) -> str:
        return json.dumps({"path": str(path), "width": size[0], "height": size[1], "faces": faces,
                           "landmarks_match": True})

    true_box, false_box = (0.41, 0.41, 0.08, 0.11), (0.8, 0.1, 0.05, 0.07)
    upside_down = rotated(FRONTAL, 180)
    for t in fc.LANDMARK_THRESHOLDS:
        out = fc.raw_path("openimages", "original", t, landmarks=True)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text("".join(line(faces_dir / f"photos/{k}.jpg", (4000, 3000),
                                    [face_with(FRONTAL, true_box, (4000, 3000), 0.9),
                                     face_with(upside_down, false_box, (4000, 3000), 0.5)]) + "\n"
                               for k in ("oia", "oib")))
        fc.raw_path("dev", "original", t, landmarks=True).write_text(
            line(photos / "x/leaf.jpg", (1000, 800), [face_with(upside_down, (0.4, 0.4, 0.1, 0.1), (1000, 800), 0.4)])
            + "\n")

    assert fc.main(["landmarks", "--date", "2026-10-22", "--out", "lm-test", "--crop-threshold", "0.15"]) == 0
    report = json.loads((tmp_path / "reports/lm-test.json").read_text())
    assert report["halves"] == {"A": ["oia"], "B": ["oib"]}
    assert report["checks"]["openimages@0.3"]["same_as_report_run"] is None       # no plain run to compare
    assert report["fits"]["A"]["true_boxes"] == 1 and report["fits"]["A"]["other_boxes"] == 1
    grid = {(x["threshold"], x["grow"], x["filter"]): x for x in report["grid"]}
    off, on = grid[(0.15, 0.25, "off")], grid[(0.15, 0.25, "top")]
    assert off["recall_all"]["caught"] == 2 and off["dev"]["with_a_box"] == 1
    assert off["dev"]["blurred_share_mean"] == pytest.approx(0.0225)               # 0.15 x 0.15
    assert on["recall_held_out_B"] == {"faces": 1, "caught": 1, "recall": 1.0}     # the face's box is kept
    assert on["dev"]["with_a_box"] == 0 and on["dev"]["blurred_share_mean"] == 0.0
    assert report["boxes"]["0.15"]["true"] == [2, 2] and report["boxes"]["0.15"]["other"] == [0, 2]
    assert {x["filter"] for x in report["grid"]} == {"off", "top", "any", "learned"}
    md = (tmp_path / "reports/lm-test.md").read_text()
    assert "| 0.15 | 25% | on (top member) | 100.0% (1/1) | 100.0% (2/2) | 100.0% (1/1) | 0/1 (0.0%) |" in md
    assert (runs / "landmarks/crops/sheet-dropped-true-01.jpg").exists() is False   # no true box dropped
