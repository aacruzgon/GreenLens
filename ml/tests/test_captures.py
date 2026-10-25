"""`training.eval.captures`: the app's LiDAR cases and our own photos become reviewed cases."""

import io
import json

import pytest
import yaml
from PIL import Image

from ml.training.eval import captures

META = {"source": "lidar", "intrinsics": {"fx": 1500.0, "fy": 1500.0, "cx": 960.0, "cy": 720.0,
                                          "ref_width": 1920, "ref_height": 1440},
        "gravity": [0.0, 0.8, 0.6], "device_model": "iPhone18,2", "filtered": False}


def jpeg(size=(40, 30), orientation=None, gps=False) -> bytes:
    exif = Image.Exif()
    if orientation:
        exif[0x0112] = orientation
    if gps:
        exif[0x8825] = {1: "N", 2: (43.0, 39.0, 0.0)}  # a GPS IFD, as a phone writes one
    buf = io.BytesIO()
    Image.new("RGB", size, "green").save(buf, "JPEG", exif=exif.tobytes())
    return buf.getvalue()


def depth_png(size=(32, 24)) -> bytes:
    buf = io.BytesIO()
    Image.new("I;16", size, 300).save(buf, "PNG")
    return buf.getvalue()


def app_case(inbox, case_id="tomato-lidar-01", plant="tomato-a", ruler=True):
    d = inbox / case_id
    d.mkdir(parents=True)
    (d / "01-whole_plant.jpg").write_bytes(jpeg(orientation=6, gps=True))
    (d / "01-whole_plant-depth.png").write_bytes(depth_png())
    (d / "01-whole_plant-depth_meta.json").write_text(json.dumps(META))
    (d / "01-whole_plant-report.json").write_text("{}")
    (d / "02-leaf_closeup.jpg").write_bytes(jpeg(orientation=6))
    (d / "02-leaf_closeup-depth.png").write_bytes(depth_png())
    (d / "02-leaf_closeup-depth_meta.json").write_text(json.dumps(META))
    (d / "02-leaf_closeup-report.json").write_text("{}")
    images = [
        {"file": "01-whole_plant.jpg", "kind": "whole_plant", "depth": "01-whole_plant-depth.png",
         "depth_meta": "01-whole_plant-depth_meta.json", "report": "01-whole_plant-report.json"},
        {"file": "02-leaf_closeup.jpg", "kind": "leaf_closeup", "depth": "02-leaf_closeup-depth.png",
         "depth_meta": "02-leaf_closeup-depth_meta.json", "report": "02-leaf_closeup-report.json",
         **({"ruler": {"points": [{"x": 0.4, "y": 0.5}, {"x": 0.6, "y": 0.5}], "measured_mm": 51.0,
                       "known_mm": 50.0}} if ruler else {})},
    ]
    (d / "case.json").write_text(json.dumps({"case_id": case_id, "plant_key": plant, "device_model": "iPhone18,2",
                                             "images": images, "saved_at": "2026-10-25T15:00:00Z"}))
    return d


@pytest.fixture
def inbox(tmp_path):
    return tmp_path / "inbox"


@pytest.fixture
def photos(tmp_path):
    return tmp_path / "photos"


def run_scan(inbox, photos):
    rc = captures.main(["scan", str(inbox), "--photos", str(photos)])
    review = yaml.safe_load((photos / "captures" / "review.yaml").read_text())
    return rc, review


def test_an_app_case_keeps_its_frame_and_orientation_and_loses_its_location(inbox, photos, capsys):
    app_case(inbox)
    rc, review = run_scan(inbox, photos)
    assert rc == 0
    [entry] = review
    assert (entry["case_id"], entry["plant_key"], entry["device_model"]) == ("tomato-lidar-01", "tomato-a", "iPhone18,2")
    assert entry["outcome"] == "healthy" and entry["measured"] == {} and entry["keep"] is False
    whole = entry["images"][0]
    assert whole == {"file": "captures/tomato-lidar-01/01-whole_plant.jpg", "kind": "whole_plant",
                     "depth": "captures/tomato-lidar-01/01-whole_plant-depth.png",
                     "depth_meta": "captures/tomato-lidar-01/01-whole_plant-depth_meta.json"}
    with Image.open(photos / whole["file"]) as im:
        assert im.size == (40, 30)  # still the sensor frame the depth is in
        exif = im.getexif()
        assert dict(exif) == {0x0112: 6} and not exif.get_ifd(0x8825)
    assert (photos / whole["depth"]).read_bytes() == (inbox / "tomato-lidar-01" / "01-whole_plant-depth.png").read_bytes()
    assert (photos / "captures/tomato-lidar-01/01-whole_plant-report.json").exists()
    assert "+2.00% (within 3%)" in capsys.readouterr().out
    assert "02-leaf_closeup.jpg\t51.0\t50.0\t2.0\tTrue" in (photos / "captures" / "rulers.tsv").read_text()


def test_a_camera_photo_is_turned_upright_with_no_metadata(inbox, photos):
    d = inbox / "basil-0001"
    d.mkdir(parents=True)
    (d / "01-whole_plant.jpg").write_bytes(jpeg(orientation=6, gps=True))
    rc, [entry] = run_scan(inbox, photos)
    assert rc == 0 and entry["images"] == [{"file": "captures/basil-0001/01-whole_plant.jpg", "kind": "whole_plant"}]
    with Image.open(photos / entry["images"][0]["file"]) as im:
        assert im.size == (30, 40) and not im.getexif()


@pytest.mark.parametrize("case_id, kind, prefill", [
    ("soil-sand-surface-dry-1", "soil_surface", {"soil": {"texture": "sandy", "moisture": "dry"}, "confidence": "sure"}),
    ("soil-clay-surface-cracked-1", "soil_surface", {"soil": {"texture": "clayey", "surface": "cracked"}, "confidence": "sure"}),
    ("soil-loam-squeeze-2", "squeeze_test", {"soil": {"texture": "loamy"}, "guided": {"result": "holds_ball"},
                                             "confidence": "probable"}),
    ("soil-clay-jar-settled-1", "jar_test", {"soil": {"texture": "clayey"}, "guided": {"settled": True, "texture": "clayey"},
                                             "confidence": "sure"}),
    ("soil-sand-jar-cloudy-1", "jar_test", {"soil": {"texture": "sandy"}, "guided": {"settled": False}, "confidence": "sure"}),
    # topsoil names no texture: the settled jar shows it, and the reviewer writes it in
    ("soil-topsoil-surface-wet-1", "soil_surface", {"soil": {"moisture": "wet"}, "confidence": "probable"}),
    ("soil-topsoil-squeeze-1", "squeeze_test", {"soil": {}, "guided": {"result": "unclear"}, "confidence": "probable"}),
    ("soil-topsoil-jar-settled-1", "jar_test", {"soil": {}, "guided": {"settled": True, "texture": "unclear"},
                                                "confidence": "probable"}),
])
def test_a_soil_case_starts_from_its_name(inbox, photos, case_id, kind, prefill):
    d = inbox / case_id
    d.mkdir(parents=True)
    (d / "IMG_4410.JPG").write_bytes(jpeg())
    rc, [entry] = run_scan(inbox, photos)
    assert rc == 0
    assert entry["images"] == [{"file": f"captures/{case_id}/01-{kind}.jpg", "kind": kind}]
    assert {k: entry[k] for k in prefill} == prefill
    assert entry["plant_key"] == "soil-" + case_id.split("-")[1] and "outcome" not in entry


@pytest.mark.parametrize("make, needle", [
    (lambda d: (d / "IMG_1.HEIC").write_bytes(b"\0\0\0\x18ftypheic" + b"\0" * 20), "Most Compatible"),
    (lambda d: (d / "IMG_1.jpg").write_bytes(jpeg()), "name it like 01-whole_plant.jpg"),
])
def test_a_photo_that_cant_be_read_is_reported(inbox, photos, capsys, make, needle):
    d = inbox / "pepper-0001"
    d.mkdir(parents=True)
    make(d)
    rc, review = run_scan(inbox, photos)
    assert rc == 1 and review == []
    assert needle in capsys.readouterr().err


def test_an_empty_case_folder_is_a_case_not_taken_yet(inbox, photos, capsys):
    (inbox / "soil-sand-jar-settled-1").mkdir(parents=True)
    rc, review = run_scan(inbox, photos)
    assert rc == 0 and review == [] and "1 empty folders skipped" in capsys.readouterr().out


def test_depth_in_another_aspect_ratio_is_refused(inbox, photos, capsys):
    d = app_case(inbox)
    (d / "01-whole_plant-depth.png").write_bytes(depth_png((32, 32)))
    rc, review = run_scan(inbox, photos)
    assert rc == 1 and review == [] and "aspect ratio" in capsys.readouterr().err


def test_a_rescan_keeps_the_review_and_refreshes_the_photos(inbox, photos):
    app_case(inbox)
    run_scan(inbox, photos)
    path = photos / "captures" / "review.yaml"
    review = yaml.safe_load(path.read_text())
    review[0].update(keep=True, species_id="tomato", measured={"leaf_length_cm": 9.5})
    path.write_text(yaml.safe_dump(review))
    (inbox / "tomato-lidar-01" / "case.json").write_text(
        (inbox / "tomato-lidar-01" / "case.json").read_text().replace('"known_mm": 50.0', '"known_mm": 51.0'))
    _, [entry] = run_scan(inbox, photos)
    assert entry["keep"] and entry["species_id"] == "tomato" and entry["measured"] == {"leaf_length_cm": 9.5}
    assert entry["images"][1]["ruler"]["known_mm"] == 51.0


def add(photos, tmp_path):
    rc = captures.main(["add", "--labeller", "sam-michaels", "--photos", str(photos),
                        "--manifest", str(tmp_path / "manifest.jsonl"),
                        "--soil-manifest", str(tmp_path / "manifest.soil.jsonl")])
    read = lambda name: [json.loads(line) for line in (tmp_path / name).read_text().splitlines()] \
        if (tmp_path / name).exists() else []
    return rc, read("manifest.jsonl"), read("manifest.soil.jsonl")


def keep_all(photos, **over):
    path = photos / "captures" / "review.yaml"
    review = [{**e, "keep": True, **over.get(e["case_id"], {})} for e in yaml.safe_load(path.read_text())]
    path.write_text(yaml.safe_dump(review))


def test_kept_cases_go_to_dev_in_their_own_manifests(inbox, photos, tmp_path):
    app_case(inbox)
    for case_id in ("soil-sand-squeeze-1", "soil-sand-jar-settled-1"):
        (inbox / case_id).mkdir()
        (inbox / case_id / "a.jpg").write_bytes(jpeg())
    run_scan(inbox, photos)
    keep_all(photos, **{"tomato-lidar-01": {"species_id": "tomato", "measured": {"leaf_length_cm": 9.5}}})
    rc, scans, soils = add(photos, tmp_path)
    assert rc == 0
    [scan] = scans
    assert (scan["split"], scan["plant_key"], scan["source"], scan["licence"]) == ("dev", "tomato-a", "team", "team-internal")
    assert scan["truth"] == {"outcome": "healthy", "species_id": "tomato", "problems": [], "organisms": [],
                             "damage": [], "measured": {"leaf_length_cm": 9.5}}
    assert scan["images"][0]["depth"].endswith("-depth.png") and len(scan["images"][0]["sha256"]) == 64
    assert scan["images"][1]["ruler"]["known_mm"] == 50.0 and scan["reviewed_by"] == ""
    by_id = {c["case_id"]: c for c in soils}
    assert by_id["soil-sand-squeeze-1"]["truth"] == {"soil": {"texture": "sandy"},
                                                     "guided": [{"image": 0, "kind": "squeeze_test", "result": "crumbles"}]}
    assert by_id["soil-sand-jar-settled-1"]["truth"]["guided"] == [
        {"image": 0, "kind": "jar_test", "settled": True, "texture": "sandy"}]
    assert {c["split"] for c in soils} == {"dev"}
    assert add(photos, tmp_path)[1:] == ([scan], soils)  # a second add adds nothing


def test_a_case_of_a_plant_in_the_test_split_is_refused(inbox, photos, tmp_path, capsys):
    app_case(inbox, plant="maize-bd-2026-common-rust-img-234")
    run_scan(inbox, photos)
    keep_all(photos)
    (tmp_path / "manifest.jsonl").write_text(json.dumps({"case_id": "x", "plant_key": "maize-bd-2026-common-rust-img-234",
                                                         "split": "test"}) + "\n")
    rc, scans, _ = add(photos, tmp_path)
    assert rc == 1 and len(scans) == 1 and "test split" in capsys.readouterr().err


@pytest.mark.parametrize("over, needle", [
    ({"outcome": "diagnosed"}, "needs its problems"),
    ({"measured": {"height_cm": 30}}, "measured 'height_cm'"),
    ({"measured": {"plant_height_cm": "tall"}}, "a number"),
    ({"confidence": "maybe"}, "confidence"),
    ({"label_basis": "guess"}, "label_basis"),
    ({"soil": {"moisture": "soggy"}}, "soil moisture 'soggy'"),
    ({"split": "test"}, "frozen"),
])
def test_a_scan_with_bad_truth_is_refused(inbox, photos, tmp_path, capsys, over, needle):
    app_case(inbox)
    run_scan(inbox, photos)
    keep_all(photos, **{"tomato-lidar-01": over})
    rc, scans, _ = add(photos, tmp_path)
    assert rc == 1 and scans == [] and needle in capsys.readouterr().err


def test_a_soil_case_needs_its_guided_truth(inbox, photos, tmp_path, capsys):
    (inbox / "soil-silt-squeeze-1").mkdir(parents=True)
    (inbox / "soil-silt-squeeze-1" / "a.jpg").write_bytes(jpeg())
    run_scan(inbox, photos)
    keep_all(photos, **{"soil-silt-squeeze-1": {"guided": {}}})
    rc, _, soils = add(photos, tmp_path)
    assert rc == 1 and soils == [] and "guided.result" in capsys.readouterr().err


def test_a_scan_needs_one_whole_plant_photo(inbox, photos, tmp_path, capsys):
    d = inbox / "pepper-0002"
    d.mkdir(parents=True)
    (d / "01-leaf_closeup.jpg").write_bytes(jpeg())
    run_scan(inbox, photos)
    keep_all(photos)
    rc, scans, _ = add(photos, tmp_path)
    assert rc == 1 and scans == [] and "exactly one whole_plant" in capsys.readouterr().err


@pytest.mark.parametrize("field, name", [
    ("file", "../01-whole_plant.jpg"), ("depth", "/etc/passwd"), ("report", "sub/01-report.json"),
    ("depth_meta", ".."), ("file", "..\\01-whole_plant.jpg"),
])
def test_case_json_names_files_in_its_own_folder_only(inbox, photos, capsys, field, name):
    d = app_case(inbox)
    case = json.loads((d / "case.json").read_text())
    case["images"][0][field] = name
    (d / "case.json").write_text(json.dumps(case))
    rc, review = run_scan(inbox, photos)
    assert rc == 1 and review == [] and "isn't a file name in the case folder" in capsys.readouterr().err
    assert not (photos / "captures" / "tomato-lidar-01").exists()


def test_both_kinds_of_photo_keep_their_colour_profile(inbox, photos):
    from PIL import ImageCms

    icc = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()   # stands in for Display P3
    buf = io.BytesIO()
    Image.new("RGB", (40, 30), "green").save(buf, "JPEG", icc_profile=icc)
    for keep in (captures.keep_orientation, captures.inat.strip):
        with Image.open(io.BytesIO(keep(buf.getvalue()))) as im:
            assert im.info.get("icc_profile") == icc
    with Image.open(io.BytesIO(captures.keep_orientation(jpeg()))) as im:
        assert "icc_profile" not in im.info   # none added to a photo that had none
