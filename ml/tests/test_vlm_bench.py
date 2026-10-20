"""`training.eval.vlm_bench`: the cases it builds for the worker and the scores it gives."""

import re
from pathlib import Path

from ml.training.eval import vlm_bench as vb

ULID = re.compile(r"^[0-7][0-9A-HJKMNP-TV-Z]{25}$")


def case(species="tomato", outcome="diagnosed", problems=("tomato_early_blight",), kinds=("whole_plant",), conf="sure"):
    return {"case_id": f"{species}-x", "split": "dev", "label_confidence": conf,
            "images": [{"file": f"f/{i}.jpg", "kind": k, "sha256": "0" * 64} for i, k in enumerate(kinds)],
            "truth": {"outcome": outcome, "species_id": species, "problems": list(problems)}}


def test_image_ids_are_stable_ulids():
    assert ULID.match(vb.ulid("a|0")) and vb.ulid("a|0") == vb.ulid("a|0") != vb.ulid("a|1")


def test_a_case_becomes_the_worker_payload_of_its_species_fixture():
    b = vb.bench_case(case(kinds=("leaf_closeup", "whole_plant")), Path("/p"))
    p = b["payload"]
    assert [i["kind"] for i in p["findings_images"]] == ["whole_plant", "leaf_closeup"]
    assert [i["kind"] for i in p["indicator_images"]] == ["leaf_closeup", "whole_plant"]
    assert p["working_species"]["id"] == "tomato" and len(p["candidates"]) == 3
    iid = p["findings_images"][0]["image_id"]
    assert p["findings_images"][0]["url"] == f"/internal/jobs/{vb.JOB}/images/{iid}"
    assert b["photos"][iid] == "/p/f/1.jpg" and b["scored_for_diagnosis"]


def test_cases_outside_the_fixtures_are_left_out_or_kept_unscored():
    assert vb.bench_case(case(species="apple", problems=("apple_scab",)), Path("/p")) is None
    assert not vb.bench_case(case(problems=("tomato_bacterial_spot",)), Path("/p"))["scored_for_diagnosis"]
    assert vb.bench_case(case(outcome="healthy", problems=()), Path("/p"))["scored_for_diagnosis"]


def row(b, answers, ok=True, calls=("findings@3", "indicators@2"), damage=()):
    ids = [i["indicator_id"] for c in b["payload"]["candidates"] for i in c["indicators"]]
    return {"case_id": b["case_id"], "ok": ok, "job_ms": 1000,
            "calls": [{"schema": s, "ms": 10} for s in calls],
            "result": {"findings": {"species_guess": {"species_id": "tomato"}, "organisms": [],
                                    "damage": [{"pattern": d} for d in damage]},
                       "indicators": {"answers": [{"indicator_id": i, "answer": a} for i, a in zip(ids, answers)]}}}


def test_the_top_candidate_by_yes_share_is_the_diagnosis():
    sick = vb.bench_case(case(), Path("/p"))                                   # early blight is the first candidate
    well = vb.bench_case({**case(outcome="healthy", problems=()), "case_id": "tomato-y"}, Path("/p"))
    rows = [row(sick, ["yes", "yes", "no"] + ["no"] * 6, calls=("findings@3", "findings@3", "indicators@2"),
                damage=("leaf_mines",)),
            row(well, ["no"] * 8 + ["unsure"])]
    s = vb.score_run(rows, {b["case_id"]: b for b in (sick, well)})
    assert s["diagnosis_top1_sure"] == 1.0 and s["healthy_no_candidate"] == 1.0
    assert s["retried_calls"] == 1 and s["damage_findings"] == {"leaf_mines": 1}
    assert abs(s["unsure_share"] - 1 / 18) < 1e-9 and s["species_agree"] == 1.0
