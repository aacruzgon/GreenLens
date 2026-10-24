"""`training.eval.score_test_split` (M18): the counts, intervals and guards of the test split's report, on
made-up inputs. The one test that opens the committed report only recomputes it from its own JSON."""

import json

import numpy as np
import pytest

from ml.training.eval import calibrate as cal
from ml.training.eval import score_test_split as ts


def case(cid, species="tomato", sure=True, images=1, problems=("tomato_early_blight",), outcome="diagnosed"):
    return {"case_id": cid, "split": "test", "label_confidence": "sure" if sure else "probable",
            "truth": {"outcome": outcome, "species_id": species, "problems": list(problems)},
            "images": [{"file": f"{cid}/{n}.jpg", "kind": "whole_plant"} for n in range(images)]}


def test_wilson_gives_the_intervals_devpost_quotes():
    assert [round(v, 2) for v in ts.wilson(3, 3)] == [0.44, 1.0]       # "3 confirmed of 3 is 44–100%"
    assert [round(v, 2) for v in ts.wilson(0, 3)] == [0.0, 0.56]       # "even 0 of 3 is 0–56%"
    assert [round(v, 2) for v in ts.wilson(0, 6)] == [0.0, 0.39]       # "even 0 of 6 is 0–39%"
    assert ts.wilson(0, 0) is None


def test_a_target_is_met_or_missed_only_when_the_whole_interval_is_on_one_side():
    assert ts.rate(58, 58, 0.99)["verdict"] == "undecided"             # 94–100% straddles 99%
    assert ts.rate(980, 980, 0.99)["verdict"] == "met"
    assert ts.rate(10, 20, 0.85)["verdict"] == "missed"
    assert ts.rate(0, 50, 0.03, at_least=False)["verdict"] == "undecided"   # 0–7%
    assert ts.rate(0, 200, 0.03, at_least=False)["verdict"] == "met"
    assert ts.rate(10, 20, 0.03, at_least=False)["verdict"] == "missed"


def test_the_former_dev_cases_are_left_out_of_the_second_set():
    ids = ["x", ts.FORMER_DEV[0], ts.FORMER_DEV[3]]
    sets = ts.subsets(ids)
    assert sets["all"] == set(ids) and sets["without_former_dev"] == {"x"}


def plant_doc(cases, sha, pair, failing=()):
    return {"cases_file": {"file": "m.jsonl", "sha256": sha[:16]}, "cases": len(cases),
            "pairs": [{"set": "A", "tau_leaf": 0.4, "tau_plant": 0.7, "failing_cases": []},
                      {**pair, "failing_cases": [{"case": c} for c in failing]}]}


def test_the_plant_check_is_read_at_the_shipped_pair_on_this_manifest():
    cases = [case("a"), case("b"), case(ts.FORMER_DEV[0])]
    pair, sha = {"set": "probe-v2", "tau_leaf": 0.94, "tau_plant": 0.31}, "ab" * 32
    passes = ts.plant_passes(plant_doc(cases, sha, pair, ["b"]), cases, pair, sha)
    assert passes == {"a": True, "b": False, ts.FORMER_DEV[0]: True}
    r = ts.t1(passes)
    assert (r["all"]["k"], r["all"]["n"]) == (2, 3) and (r["without_former_dev"]["k"], r["without_former_dev"]["n"]) == (1, 2)
    with pytest.raises(SystemExit, match="not this manifest"):
        ts.plant_passes(plant_doc(cases, "cd" * 32, pair), cases, pair, sha)
    with pytest.raises(SystemExit, match="split has 2"):
        ts.plant_passes(plant_doc(cases, sha, pair), cases[:2], pair, sha)
    with pytest.raises(SystemExit, match="shipped one"):
        ts.plant_passes(plant_doc(cases, sha, pair), cases, {**pair, "tau_plant": 0.74}, sha)
    with pytest.raises(SystemExit, match="outside the split"):
        ts.plant_passes(plant_doc(cases, sha, pair, ["zzz"]), cases, pair, sha)


def test_t4_counts_only_a_confident_shared_wrong_species():
    bio = {"a": {"truth": "tomato", "top": "pepper", "confidence": 0.9},     # both wrong, both sure: counts
           "b": {"truth": "tomato", "top": "pepper", "confidence": 0.79},    # BioCLIP under 0.8
           "c": {"truth": "corn", "top": "corn", "confidence": 0.95},        # right
           "d": {"truth": "tomato", "top": "pepper", "confidence": 0.9}}     # the VLM unsure
    vlm = {"a": {"species_id": "pepper", "confidence": 0.65}, "b": {"species_id": "pepper", "confidence": 0.85},
           "c": {"species_id": "corn", "confidence": 0.85}, "d": {"species_id": "pepper", "confidence": 0.4},
           "e": {"species_id": "tomato", "confidence": None}}                # no BioCLIP score: no agreement
    r = ts.t4(bio, vlm)["all"]
    assert (r["k"], r["n"], r["cases"]) == (1, 5, ["a"])


def test_macro_f1_averages_over_the_classes_seen():
    assert ts.macro_f1([(0, 0), (1, 1)]) == 1.0
    assert ts.macro_f1([(0, 1), (1, 0)]) == 0.0
    # class 0: tp 1, fn 1 -> 2/3; class 1: tp 1, fp 1 -> 2/3
    assert ts.macro_f1([(0, 0), (0, 1), (1, 1)]) == pytest.approx(2 / 3)
    assert ts.macro_f1([]) is None


def head():
    labels = [{"index": 0, "species_id": "tomato", "problem_id": "healthy"},
              {"index": 1, "species_id": "tomato", "problem_id": "tomato_early_blight"},
              {"index": 2, "species_id": "tomato", "problem_id": "tomato_late_blight"},
              {"index": 3, "species_id": "tomato", "problem_id": "tomato_leaf_mold"},
              {"index": 4, "species_id": "tomato", "problem_id": "tomato_septoria"}]
    return {"labels": labels, "temperature": 1.82}


def item(cid, label, top):
    z = np.full(5, -1.0)
    z[top] = 3.0
    return (z, label, cid, "tomato", "fieldsets/x")


def test_t5_counts_the_top_3_on_each_set_and_t6_t7_come_with_it():
    cases = [case("a"), case("b", sure=False), case(ts.FORMER_DEV[4]), case(ts.REPEAT_LEAVES[0])]
    items = [item("a", 1, 1), item("b", 2, 1), item(ts.FORMER_DEV[4], 3, 3), item(ts.REPEAT_LEAVES[0], 4, 1),
             item("a", 0, 0)]                                    # a healthy leaf: not in T5
    d = ts.disease(items, head(), cases, draws=200)
    assert (d["t5"]["all"]["k"], d["t5"]["all"]["n"]) == (3, 4)    # the repeat leaf's label is outside its top 3
    assert d["t5"]["sure"]["n"] == 3
    assert d["t5"]["without_former_dev_and_repeats"]["n"] == 2
    assert d["t5"]["all"]["guessing"] == pytest.approx(3 / 5)
    assert d["t6"]["all"]["tomato"]["photos"] == 5 and 0 < d["t6"]["all"]["tomato"]["macro_f1"] < 1
    assert d["t6"]["without_former_dev"]["tomato"]["photos"] == 4
    assert (d["t7"]["all"]["photos"], d["t7"]["without_former_dev"]["photos"]) == (5, 4)
    assert d["t7"]["all"]["ece"] is not None and d["temperature"] == 1.82


def test_the_calibrated_head_is_binned_as_calibrate_bins_the_ece():
    conf = np.random.default_rng(1).uniform(0.2, 1.0, 51)
    right = np.random.default_rng(0).random((5, 51)) < conf          # calibrated_eces' draws, seed 0
    assert ts.calibrated_eces(conf, draws=5) == pytest.approx([cal.ece(conf, r) for r in right])


def test_t7_is_judged_against_a_calibrated_head_with_the_same_confidences():
    # Sure and wrong half the time on 200 photos: far above anything a calibrated head scores.
    conf = np.full(200, 0.95)
    assert ts.ece_row(conf, np.arange(200) < 100, draws=2000)["verdict"] == "missed"
    # 0.9 and right 9 times in 10 on 1,000 photos: under the target, and so is a calibrated head.
    conf = np.full(1000, 0.9)
    assert ts.ece_row(conf, np.arange(1000) % 10 != 0, draws=2000)["verdict"] == "met"
    # 51 spread confidences, right as often as they say: well over 0.05, but no worse than a calibrated head.
    conf = np.linspace(0.2, 0.99, 51)
    r = ts.ece_row(conf, np.random.default_rng(3).random(51) < conf, draws=2000)
    assert r["ece"] > 0.05 and r["calibrated_mean"] > 0.05 and r["verdict"] == "undecided"
    assert ts.ece_row(np.zeros(0), np.zeros(0, bool))["verdict"] == "no photos"


def bench_case(cid, species="tomato"):
    return {"case_id": cid, "truth": {"outcome": "diagnosed", "species_id": species, "problems": ["p"]},
            "label_confidence": "sure", "scored_for_diagnosis": True,
            "payload": {"candidates": [{"problem_id": "p", "indicators": [{"indicator_id": "i1"}]}]}}


def bench_row(cid, ok=True, retried=0, species="tomato", ms=60_000, error_on=None):
    calls = [{"schema": "findings@5", "ms": 1}, {"schema": "indicators@3", "ms": 1}] + [{"schema": "findings@5", "ms": 1}] * retried
    calls = [{**c, "error": "timed out" if c["schema"] == error_on else None} for c in calls]
    result = {"findings": {"species_guess": {"species_id": species, "confidence": 0.85}, "damage": [], "organisms": []},
              "indicators": {"answers": [{"indicator_id": "i1", "answer": "yes"}]}}
    return {"case_id": cid, "ok": ok, "calls": calls, "job_ms": ms, "result": result if ok else None}


def test_the_vlm_rows_count_first_try_calls_finished_jobs_species_and_p95():
    bench = {c: bench_case(c) for c in ("a", "b", "c", ts.FORMER_DEV[5])}
    rows = [bench_row("a"), bench_row("b", retried=1, species="pepper"),
            bench_row("c", ok=False, ms=200_000, error_on="indicators@3"), bench_row(ts.FORMER_DEV[5], ms=30_000)]
    run = ts.vlm_jobs(rows, bench)
    r = ts.vlm(run)
    a = r["all"]
    assert (a["jobs"], a["finished"], a["retried_calls"]) == (4, 3, 1)
    # Every job's calls count, the failed job's too: b's retried call and c's failed call missed their first try.
    assert (a["t8_first_try"]["k"], a["t8_first_try"]["n"]) == (6, 8)
    assert (a["t8_after_one_retry"]["k"], a["t8_after_one_retry"]["n"], a["t8_after_one_retry"]["check"]) == (3, 4, "failures")
    assert (a["t9_species"]["k"], a["t9_species"]["n"]) == (2, 3)
    assert a["t10_job_p95_s"] == 200.0 and a["t10_verdict"] == "missed"     # a failed job's time counts
    assert r["without_former_dev"]["jobs"] == 3
    assert ts.species_guesses(run["jobs"]) == {c: {"species_id": s, "confidence": 0.85}
                                        for c, s in (("a", "tomato"), ("b", "pepper"), (ts.FORMER_DEV[5], "tomato"))}


def test_a_call_misses_its_first_try_once_however_it_failed():
    call = lambda schema, error=False: {"schema": schema, "error": error}
    assert ts.first_tries({"calls": [call("f"), call("i")]}) == (2, 0)
    assert ts.first_tries({"calls": [call("f"), call("f"), call("i")]}) == (2, 1)            # retried after bad JSON
    assert ts.first_tries({"calls": [call("f", True), call("f"), call("i")]}) == (2, 1)      # an error, then retried
    assert ts.first_tries({"calls": [call("f"), call("i", True)]}) == (2, 1)                 # an error, no retry


def test_a_vlm_run_must_be_at_the_shipped_pixels_and_slots(tmp_path):
    d = tmp_path / f"px{ts.PIXELS}-c2"
    d.mkdir()
    (d / "cases.jsonl").write_text(json.dumps(bench_case("a")) + "\n")
    (d / "out.jsonl").write_text(json.dumps(bench_row("a")) + "\n")
    (d / "meta.json").write_text(json.dumps({"max_pixels": ts.PIXELS, "concurrency": 2, "cases": "x"}))
    meta, rows, bench = ts.vlm_run(tmp_path, 2, {"a"})
    assert [r["case_id"] for r in rows] == ["a"] and list(bench) == ["a"] and meta["out"]["sha256"]
    with pytest.raises(SystemExit, match="outside the split"):
        ts.vlm_run(tmp_path, 2, {"b"})
    (d / "meta.json").write_text(json.dumps({"max_pixels": 602112, "concurrency": 2, "cases": "x"}))
    with pytest.raises(SystemExit, match="602112 pixels"):
        ts.vlm_run(tmp_path, 2, {"a"})


def test_the_upload_rows_count_rejected_photos_and_photos_with_a_box(tmp_path):
    cases = [case("a", images=2), case(ts.FORMER_DEV[6])]
    lines = [{"path": str(tmp_path / "a/0.jpg"), "verdict": None, "faces": [{"x": 1}]},
             {"path": str(tmp_path / "a/1.jpg"), "verdict": "TooBlurry", "faces": []},
             {"path": str(tmp_path / f"{ts.FORMER_DEV[6]}/0.jpg"), "verdict": None, "faces": []}]
    raw = tmp_path / "test-original-0.3.jsonl"
    raw.write_text("".join(json.dumps(r) + "\n" for r in lines))
    per = ts.upload_photos(raw, cases, tmp_path)
    r = ts.upload(per)
    assert (r["all"]["t17_rejected"]["k"], r["all"]["t17_rejected"]["n"]) == (1, 3)
    assert (r["all"]["t18_face_box"]["k"], r["without_former_dev"]["t18_face_box"]["n"]) == (1, 2)
    assert r["all"]["reasons"] == {"TooBlurry": 1}
    raw.write_text("".join(json.dumps(r) + "\n" for r in lines[:2]))
    with pytest.raises(SystemExit, match="no result"):
        ts.upload_photos(raw, cases, tmp_path)
    resized = tmp_path / "test-640-0.3.jsonl"
    resized.write_text(raw.read_text())
    with pytest.raises(SystemExit, match="must be face_check's test-original-0.3.jsonl"):
        ts.upload_photos(resized, cases, tmp_path)


def test_the_shipped_values_come_from_the_final_manifest():
    import tomllib

    s = ts.shipped(tomllib.loads((ts.MODELS / "manifest.toml").read_text())["model"])
    assert s["plant_pair"] == {"set": "plant-probe-v2-2026-10-23", "tau_leaf": 0.94, "tau_plant": 0.31}
    assert s["species_temperature"] == 1.3747
    assert s["disease_head"]["version"] == "disease-head-v3c-2026-10-20"
    assert s["matrix"]["version"] == "edb29fe878952e7b"


def empty_report(**kw):
    s = {"matrix": {"version": "m"}, "species_temperature": 1.0, "disease_head": {"version": "h"},
         "plant_pair": {"set": "p", "tau_leaf": 0.9, "tau_plant": 0.3}}
    return {"command": "c", "date": "d", "split": "test", "cases": 0, "shipped": s, "missing": {},
            "inputs": {"manifest": {"file": "m", "sha256": "1"}, "models_manifest": {"file": "t", "sha256": "2"}},
            "t1": None, "t2": None, "t3": None, "t4": None, "disease": None, "vlm": None, "vlm_meta": None, "upload": None,
            "measured": {}, **kw}


def test_the_report_says_what_wasnt_measured():
    rep = empty_report(missing={"T1": "no plant_check report given"}, notes="## How it ran\n\nOnce.")
    md = ts.markdown(rep, "t")
    assert md.count(ts.NOT_BUILT) == 6 and "no plant_check report given" in md and "## How it ran" in md


def test_a_split_with_no_leaf_photos_still_gets_a_report():
    head = {"labels": [{"index": 0, "species_id": "tomato", "problem_id": "healthy"},
                       {"index": 1, "species_id": "tomato", "problem_id": "tomato_early_blight"}], "temperature": 1.0}
    rep = empty_report(disease=ts.disease([], head, []), measured={"disease": {"left_out": {}}})
    md = ts.markdown(rep, "t")
    assert "- **T5–T7.** No leaf photos." in md and "| T7 | Disease head: ECE (15 bins) at the file's temperature | 0.05 or less | no photos |" in md


def test_notes_are_kept_beside_what_was_measured_not_in_it():
    path = ts.REPORTS / "test-split-2026-10-23"
    doc = json.loads(path.with_suffix(".json").read_text())
    assert "notes" not in doc["measured"] and doc["notes"].startswith("## How it ran")
    old = {k: v for k, v in doc.items() if k != "notes"} | {"measured": doc["measured"] | {"notes": doc["notes"]}}
    again = ts.rebuilt(old, old["measured"], None)
    assert "notes" not in again["measured"] and again["notes"] == doc["notes"]
    assert ts.rebuilt(doc, doc["measured"], None, notes="Other.")["notes"] == "Other."


def test_the_committed_test_split_report_rebuilds_from_its_json_exactly():
    path = ts.REPORTS / "test-split-2026-10-23"
    doc = json.loads(path.with_suffix(".json").read_text())
    again = json.loads(json.dumps(ts.rebuilt(doc, doc["measured"], None)))
    assert again == doc
    assert ts.markdown(again, path.name) == path.with_suffix(".md").read_text()
