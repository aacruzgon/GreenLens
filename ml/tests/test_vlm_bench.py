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
    vb.score(argparse.Namespace(runs=[tmp_path / "fp8"], out="t", cases=None))
    report = json.loads((tmp_path / "reports" / "vlm-bench-t.json").read_text())
    assert [(r["max_pixels"], r["finished"], r["jobs"], r["exit"]) for r in report] == [(602112, 2, 2, 0), (1003520, 1, 2, "timeout")]
    assert report[0]["model"] == "Qwen/Qwen3-VL-8B-Instruct-FP8"


def test_runs_are_rescored_on_other_cases_with_their_truth(tmp_path, monkeypatch):
    b = vb.bench_case(case(), Path("/p"))                                       # early blight, the first candidate
    d = tmp_path / "q" / "px1003520-c1"
    d.mkdir(parents=True)
    own = tmp_path / "cases.jsonl"
    own.write_text(json.dumps(b) + "\n" + json.dumps({**b, "case_id": "tomato-z"}) + "\n")
    (d / "meta.json").write_text(json.dumps({"label": "q", "max_pixels": 1003520, "concurrency": 1, "peak_gpu_mib": 1,
                                             "peak_kv_cache": 0.1, "models": [], "cases": str(own), "exit": 0}))
    blight = row(b, ["yes"] * 3 + ["no"] * 6)
    (d / "out.jsonl").write_text(json.dumps(blight) + "\n" + json.dumps({**blight, "case_id": "tomato-z"}) + "\n")
    relabelled = tmp_path / "cases-new.jsonl"                                  # tomato-z dropped, tomato-x relabelled
    relabelled.write_text(json.dumps({**b, "truth": {**b["truth"], "problems": ["tomato_septoria_leaf_spot"]}}) + "\n")
    monkeypatch.setattr(vb, "REPORTS", tmp_path / "reports")
    vb.score(argparse.Namespace(runs=[tmp_path / "q"], out="own", cases=None))
    vb.score(argparse.Namespace(runs=[tmp_path / "q"], out="new", cases=relabelled))
    (before,), (after,) = (json.loads((tmp_path / "reports" / f"vlm-bench-{n}.json").read_text()) for n in ("own", "new"))
    assert (before["jobs"], before["finished"], before["diagnosis_top1_sure"]) == (2, 2, 1.0)
    assert (after["jobs"], after["finished"], after["diagnosis_top1_sure"]) == (1, 1, 0.0)
    assert (before["scored_on"]["cases"], after["scored_on"]["cases"]) == (2, 1)
    assert after["scored_on"]["file"].endswith("cases-new.jsonl")
    md = (tmp_path / "reports" / "vlm-bench-new.md").read_text()
    assert f"cases-new.jsonl` (1 case, SHA-256 `{after['scored_on']['sha256']}`) (`--cases`)" in md

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


def test_findings_are_counted_per_certainty_and_the_species_guess_scored():
    sick = vb.bench_case(case(), Path("/p"))
    other = {**vb.bench_case(case(), Path("/p")), "case_id": "tomato-z"}
    a, b = row(sick, ["no"] * 9, damage=("leaf_mines",)), row(other, ["no"] * 9)
    a["result"]["findings"]["species_guess"]["confidence"] = 0.85
    a["result"]["findings"]["damage"][0]["confidence"] = 0.65
    b["result"]["findings"]["species_guess"] = {"species_id": "pepper", "confidence": 0.85}
    s = vb.score_run([a, b], {x["case_id"]: x for x in (sick, other)})
    assert s["certainty"]["species_guess"] == {"0.85": {"n": 2, "right": 1}}
    assert s["certainty"]["damage"] == {"0.65": 1} and s["certainty"]["organisms"] == {}
    lines = vb.certainty_tables([{"label": "m", "max_pixels": 1003520, "concurrency": 1, **s}])
    assert any("0.85: 1/2" in line and "0.65: 1" in line for line in lines)


def test_a_healthy_case_with_an_animal_on_it_is_left_out_of_the_healthy_counts():
    well = vb.bench_case(case(outcome="healthy", problems=()), Path("/p"))
    pest = vb.bench_case({**case(outcome="healthy", problems=()), "case_id": "tomato-pest",
                          "truth": {"outcome": "healthy", "species_id": "tomato", "problems": [],
                                    "organisms_unboxed": True}}, Path("/p"))
    assert pest["truth"]["organisms_unboxed"] and not pest["scored_for_diagnosis"]
    s = vb.score_run([row(well, ["no"] * 9), row(pest, ["yes"] * 9)], {b["case_id"]: b for b in (well, pest)})
    assert (s["healthy_n"], s["healthy_no_candidate"], s["yes_on_healthy"]) == (1, 1.0, 0.0)
    assert s["healthy_with_animals_left_out"] == 1


def test_a_look_alike_is_kept_clean_when_no_candidate_comes_out_on_top():
    look = {**case(problems=()), "truth": {"outcome": "diagnosed", "species_id": "corn", "problems": [],
                                           "look_alike": "potassium_deficiency"}}
    clean = vb.bench_case({**look, "case_id": "corn-a"}, Path("/p"))
    fooled = vb.bench_case({**look, "case_id": "corn-b"}, Path("/p"))
    assert clean["truth"]["look_alike"] == "potassium_deficiency" and not clean["scored_for_diagnosis"]
    n = sum(len(c["indicators"]) for c in clean["payload"]["candidates"])
    s = vb.score_run([row(clean, ["no"] * n), row(fooled, ["yes"] + ["no"] * (n - 1))],
                     {b["case_id"]: b for b in (clean, fooled)})
    assert (s["look_alike_n"], s["look_alike_no_candidate"], s["diagnosis_n"]) == (2, 0.5, 0)


def test_reasons_are_measured_and_answers_compared_with_another_run():
    b = vb.bench_case(case(), Path("/p"))
    old = row(b, ["yes", "no", "unsure"])
    new = row(b, ["yes", "yes", "unsure"])
    for x, reason in zip(old["result"]["indicators"]["answers"], ["Rings.", "x" * 60, "cut at the"]):
        x["reason"] = reason
    q = vb.reason_lengths([old, row(b, [], ok=False)])
    assert q == {"n": 3, "median": 10, "p90": 60, "max": 60, "no_closing_stop": 2}
    assert vb.answer_changes([new], [old]) == {"shared": 3, "changed": 1}
    assert vb.reason_lengths([])["max"] is None
