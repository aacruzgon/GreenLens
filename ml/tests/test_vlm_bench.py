"""`training.eval.vlm_bench`: the cases it builds for the worker and the scores it gives."""

import argparse
import json
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


def test_a_one_photo_case_also_goes_to_the_indicator_call_as_a_leaf_closeup():
    b = vb.bench_case(case(), Path("/p"))
    p = b["payload"]
    assert [i["kind"] for i in p["findings_images"]] == ["whole_plant"]
    assert [i["kind"] for i in p["indicator_images"]] == ["leaf_closeup", "whole_plant"]
    closeup, whole = p["indicator_images"]
    assert closeup["image_id"] != whole["image_id"] and ULID.match(closeup["image_id"])
    assert b["photos"][closeup["image_id"]] == b["photos"][whole["image_id"]] == "/p/f/0.jpg"


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
    assert s["retried_calls"] == 1 and s["damage_findings"] == {"leaf_mines": 1} and s["top_ties"] == 0
    assert abs(s["unsure_share"] - 1 / 18) < 1e-9 and s["species_agree"] == 1.0


def test_a_run_cut_short_is_scored_on_what_it_wrote_and_rerun(tmp_path, monkeypatch):
    b = vb.bench_case(case(), Path("/p"))
    cases = tmp_path / "cases.jsonl"
    cases.write_text(json.dumps(b) + "\n" + json.dumps({**b, "case_id": "tomato-z"}) + "\n")
    done, cut, empty = (tmp_path / "fp8" / n for n in ("px602112-c1", "px1003520-c2", "px2007040-c1"))
    for d in (done, cut, empty):
        d.mkdir(parents=True)
    meta = {"label": "fp8", "concurrency": 1, "peak_gpu_mib": 1, "peak_kv_cache": 0.1,
            "models": [{"id": "vlm", "root": "Qwen/Qwen3-VL-8B-Instruct-FP8"}], "cases": str(cases)}
    full = [row(b, ["yes"] * 9), {**row(b, ["no"] * 9), "case_id": "tomato-z"}]
    (done / "meta.json").write_text(json.dumps({**meta, "max_pixels": 602112, "exit": 0}))
    (done / "out.jsonl").write_text("".join(json.dumps(r) + "\n" for r in full))
    (cut / "meta.json").write_text(json.dumps({**meta, "max_pixels": 1003520, "exit": "timeout"}))
    (cut / "out.jsonl").write_text(json.dumps(full[0]) + "\n" + '{"case_id": "tomato-z", "ok"')
    assert vb.finished(done, 2) and not vb.finished(cut, 2) and not vb.finished(empty, 2)
    monkeypatch.setattr(vb, "REPORTS", tmp_path / "reports")
    vb.score(argparse.Namespace(runs=[tmp_path / "fp8"], out="t"))
    report = json.loads((tmp_path / "reports" / "vlm-bench-t.json").read_text())
    assert [(r["max_pixels"], r["finished"], r["jobs"], r["exit"]) for r in report] == [(602112, 2, 2, 0), (1003520, 1, 2, "timeout")]
    assert report[0]["model"] == "Qwen/Qwen3-VL-8B-Instruct-FP8"


def test_answers_are_counted_per_indicator_on_cases_with_its_problem_and_on_the_rest():
    b = vb.bench_case(case(), Path("/p"))
    s = vb.score_run([row(b, ["yes"] * 9)], {b["case_id"]: b})
    truth = set(b["truth"]["problems"])
    for ind, v in s["indicators"].items():
        side = "truth" if any(ind.startswith(p + ".") for p in truth) and b["label_confidence"] == "sure" else "other"
        assert v[side] == {"yes": 1}, ind
    assert sum(bool(v["truth"]) for v in s["indicators"].values()) == 3   # early blight's three


def test_diagnosis_is_split_by_the_photos_size():
    b = {**vb.bench_case(case(), Path("/p")), "megapixels": 0.4}
    s = vb.score_run([row(b, ["yes", "yes", "yes"] + ["no"] * 6)], {b["case_id"]: b})
    assert s["diagnosis_by_photo_size"] == {"under_1mp": {"hits": 1, "n": 1}, "1mp_and_up": {"hits": 0, "n": 0}}
    assert vb.with_megapixels({"photos": {"x": "/nowhere.jpg"}})["megapixels"] is None


def test_a_tie_for_the_top_is_no_diagnosis():
    b = vb.bench_case(case(), Path("/p"))
    s = vb.score_run([row(b, ["yes"] * 9)], {b["case_id"]: b})   # every candidate at 100%
    assert s["diagnosis_top1_sure"] == 0.0 and s["top_ties"] == 1


def test_a_probable_case_counts_on_neither_side_of_the_per_question_table():
    b = {**vb.bench_case(case(), Path("/p")), "label_confidence": "probable"}
    s = vb.score_run([row(b, ["yes"] * 9)], {b["case_id"]: b})
    assert all(not v["truth"] and not v["other"] for v in s["indicators"].values())
