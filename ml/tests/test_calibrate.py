"""`training.eval.calibrate`: the worker's species and disease scoring, and the temperature fits."""

from pathlib import Path

import numpy as np

from ml.training.eval import calibrate as cb


def case(kinds=("whole_plant",), outcome="diagnosed", problems=("tomato_early_blight",), species="tomato"):
    return {"case_id": "c", "split": "dev",
            "images": [{"file": f"fieldsets/set/{i}.jpg", "kind": k} for i, k in enumerate(kinds)],
            "truth": {"outcome": outcome, "species_id": species, "problems": list(problems)}}


def test_species_probabilities_weigh_shots_as_the_worker_does():
    # vision::perception::heads tests: a whole-plant shot weighs twice a close-up.
    whole, closeup = np.array([0.0, 0.3]), np.array([0.3, 0.0])
    p = cb.species_probs([whole, closeup], [cb.SHOT_WEIGHT["whole_plant"], cb.SHOT_WEIGHT["leaf_closeup"]], 1.0)
    assert p.argmax() == 1 and abs(p.sum() - 1) < 1e-9
    # A higher temperature flattens each shot before the average.
    assert cb.species_probs([whole], [2.0], 2.0).max() < cb.species_probs([whole], [2.0], 1.0).max()


def test_a_fitted_temperature_recovers_the_one_that_made_the_labels():
    rng = np.random.default_rng(0)
    logits = rng.normal(0, 3, (4000, 5))
    t_true = 2.5
    p = cb.softmax(logits / t_true)
    y = np.array([rng.choice(5, p=row) for row in p])
    items = [(z, int(t), None, "tomato", "x") for z, t in zip(logits, y)]
    assert abs(cb.fit_items(items) - t_true) < 0.2


def test_ece_is_zero_when_confidence_matches_accuracy_and_large_when_not():
    conf = np.full(100, 0.75)
    assert cb.ece(conf, np.arange(100) % 4 != 0) < 1e-9
    assert abs(cb.ece(np.full(10, 0.95), np.zeros(10, bool)) - 0.95) < 1e-9


def test_case_labels_and_leaf_photos():
    index = {("tomato", "healthy"): 0, ("tomato", "tomato_early_blight"): 1, ("tomato", "tomato_septoria_leaf_spot"): 2}
    assert cb.case_label(case(), index) == 1
    assert cb.case_label(case(outcome="healthy", problems=()), index) == 0
    assert cb.case_label(case(problems=("tomato_early_blight", "tomato_septoria_leaf_spot")), index) is None
    assert cb.case_label(case(problems=("tomato_hornworms",)), index) is None
    assert cb.leaf_photos(case()) == [("fieldsets/set/0.jpg", "leaf_closeup")]    # one photo: sent as a close-up
    assert cb.leaf_photos(case(kinds=("whole_plant", "leaf_closeup", "leaf_underside"))) == [
        ("fieldsets/set/1.jpg", "leaf_closeup"), ("fieldsets/set/2.jpg", "leaf_underside")]
    assert cb.leaf_photos(case(kinds=("whole_plant", "flower_fruit"))) == []
    assert cb.source("fieldsets/tomato-bd-2025/x/1.jpg") == "fieldsets/tomato-bd-2025" and cb.source("inat/1-2.jpg") == "inat"


def test_the_disease_logits_are_restricted_to_the_case_species():
    head = {"weight": np.eye(3, 4, dtype=np.float32), "bias": np.zeros(3, np.float32),
            "labels": [{"index": 0, "species_id": "tomato", "problem_id": "healthy"},
                       {"index": 1, "species_id": "tomato", "problem_id": "tomato_early_blight"},
                       {"index": 2, "species_id": "grape", "problem_id": "healthy"}]}
    _, species_of_class = cb.class_index(head)
    z = cb.restricted(cb.raw_logits(head, np.array([0.0, 0.0, 5.0, 0.0])), species_of_class, "tomato")
    assert np.isneginf(z[2]) and cb.softmax(z)[2] == 0


def test_disease_scores_count_the_top_3_the_api_keeps_and_both_sides_of_medium():
    inf = -np.inf
    items = [  # 4 classes of one species, index 0 healthy
        (np.array([0.0, 4.0, 3.0, inf]), 2, "a", "tomato", "x"),   # right problem 2nd
        (np.array([3.0, 2.0, 1.0, 0.0]), 3, "b", "tomato", "x"),   # right problem 4th, behind healthy
        (np.array([0.0, 5.0, 1.0, inf]), 0, "c", "tomato", "x"),   # a healthy leaf, confidently called sick
        (np.array([6.0, 1.0, 0.0, inf]), 0, "d", "tomato", "x"),   # a healthy leaf called healthy
    ]
    cold, hot = cb.disease_scores(items, 1.0, {0}), cb.disease_scores(items, 20.0, {0})
    # Only the 2 photos with a problem count; b's problem is 4th, so the worker never sends it.
    assert cold["with_problem"] == 2 and cold["top3_sent"] == hot["top3_sent"] == 0.5
    assert abs(cold["chance_top3_sent"] - (1 + 3 / 4) / 2) < 1e-9
    # Once healthy is dropped, b's problem is in the top 3 problems; 2 and 3 problems are all guessed.
    assert cold["top3_problems"] == 1.0 and cold["chance_top3_problems"] == 1.0
    assert cold["top3_sent_can_miss"] == {"n": 1, "top3": 0.0, "chance": 0.75}
    assert cold["top3_problems_can_miss"]["n"] == 0
    assert cold["top1"] == 0.25
    assert cold["medium"] == {"n": 2, "right": 0}                 # a and c
    assert cold["healthy"] == {"n": 2, "evidence": 1, "blocked": 1}
    assert cold["called_healthy"] == {"n": 2, "called": 1}         # b: healthy at 0.64
    # A high temperature claims nothing either way.
    assert hot["medium"]["n"] == 0 and hot["healthy"]["evidence"] == 0 and hot["called_healthy"]["called"] == 0


def test_temperatures_per_item_and_per_species():
    items = [(None, 0, "a", "tomato", "x"), (None, 0, "b", "corn", "x"), (None, 0, "c", "pepper", "x")]
    assert list(cb.temperatures(items, 2.0)) == [2.0, 2.0, 2.0]
    assert list(cb.temperatures(items, [1.0, 2.0, 3.0])) == [1.0, 2.0, 3.0]
    assert list(cb.temperatures(items, {"*": 9.0, "tomato": 4.0, "corn": 1.5})) == [4.0, 1.5, 9.0]
    # Three classes, healthy last: the top problem is at 0.79 at T = 1 and at 0.38 at T = 10.
    one = [(np.array([2.0, 0.0, 0.0]), 0, "a", "tomato", "x")]
    assert cb.disease_scores(one, {"*": 10.0, "tomato": 1.0}, {2})["medium"]["n"] == 1
    assert cb.disease_scores(one, {"*": 1.0, "tomato": 10.0}, {2})["medium"]["n"] == 0


def test_folds_deal_each_species_across_the_folds():
    cases = [(f"t{i}", "tomato") for i in range(12)] + [(f"g{i}", "grape") for i in range(5)] + [("t0", "tomato")]
    fold = cb.folds(cases)
    assert set(fold) == {c for c, _ in cases} and set(fold.values()) == set(range(cb.FOLDS))
    assert sorted(fold[f"g{i}"] for i in range(5)) == list(range(cb.FOLDS))   # 5 grape cases, one per fold
    assert cb.folds(cases) == fold                                              # seeded


def test_out_of_fold_fits_never_see_the_held_out_case():
    items = [(None, 0, f"c{i // 2}", "tomato" if i % 3 else "corn", "x") for i in range(30)]
    fold = cb.folds([(i[2], i[3]) for i in items])
    # A "fit" that returns how many items it was given: each item gets the size of the other folds.
    tt, fits = cb.out_of_fold(items, lambda train: float(len(train)))
    sizes = {f: sum(fold[i[2]] == f for i in items) for f in range(cb.FOLDS)}
    assert [float(t) for t in tt] == [float(len(items) - sizes[fold[i[2]]]) for i in items]
    assert len(fits) == cb.FOLDS
    # A per-species fit covers a species missing from a fold through "*".
    tt, _ = cb.out_of_fold(items, lambda train: {"*": 7.0})
    assert set(tt) == {7.0}


def test_an_underside_with_no_leaf_box_is_dropped_and_a_closeup_is_used_whole():
    head = {"weight": np.eye(2, 4, dtype=np.float32), "bias": np.zeros(2, np.float32), "temperature": 1.0,
            "labels": [{"index": 0, "species_id": "tomato", "problem_id": "healthy"},
                       {"index": 1, "species_id": "tomato", "problem_id": "tomato_early_blight"}]}
    cases = [case(kinds=("whole_plant", "leaf_underside")), case(kinds=("whole_plant", "leaf_closeup"))]
    whole = np.array([0.0, 1.0, 0.0, 0.0], np.float32)
    items, left_out = cb.disease_items(cases, Path("/p"), head, lambda f: np.zeros((0, 4), np.float32), lambda f: whole)
    assert len(items) == 1 and left_out == {"undersides with no leaf box": 1}
    assert cb.confusion(items, head) == {"tomato": {"tomato_early_blight": {"tomato_early_blight": 1}}}


def species_item(cos, case_id, y, species="tomato"):
    return ([np.asarray(cos, float)], [2.0], y, {"case_id": case_id, "truth": {"species_id": species}})


def test_the_agreement_threshold_counts_cases_outside_the_matrix_as_wrong():
    # Ten named cases scored high and right at T 1; one outside case at 0.88 (softmax of 100 x 0.02).
    named = [species_item([0.30, 0.0], f"n{i}", 0) for i in range(10)]
    t1 = np.ones(10)
    assert cb.agreement(named, t1, [], np.zeros(0))["accuracy"] == 1.0
    squash = [species_item([0.02, 0.0], "s0", None, "squash")]
    a = cb.agreement(named, t1, squash, np.ones(1))
    assert a["n"] == 11 and a["outside"] == 1 and abs(a["accuracy"] - 10 / 11) < 1e-9
    # With nothing outside, the rule holds at 0.8.
    assert cb.agreement_threshold(named, t1, [], np.zeros(0)) == {"threshold": 0.8, "n": 10, "outside": 0, "accuracy": 1.0}
    # With it, the threshold rises just past its 0.881, and the temperature isn't touched.
    assert cb.agreement_threshold(named, t1, squash, np.ones(1)) == {"threshold": 0.89, "n": 10, "outside": 0, "accuracy": 1.0}


def test_no_threshold_holds_when_a_wrong_case_is_as_sure_as_the_right_ones():
    same = [species_item([0.30, 0.0], f"n{i}", 0) for i in range(3)]
    twin = [species_item([0.30, 0.0], "s0", None, "squash")]
    assert cb.agreement_threshold(same, np.ones(3), twin, np.ones(1)) is None


def test_the_report_inputs_give_back_the_same_fits():
    named = [species_item([0.30, 0.10, 0.0], "a", 0), species_item([0.12, 0.10, 0.0], "b", 1),
             species_item([0.05, 0.08, 0.0], "c", 1)]
    outside = [species_item([0.01, 0.0, 0.02], "d", None, "squash")]
    back, back_out = cb.species_data(cb.species_inputs(named + outside))
    assert [b[3]["case_id"] for b in back] == ["a", "b", "c"] and [b[3]["case_id"] for b in back_out] == ["d"]
    assert abs(cb.fit_species(back) - cb.fit_species(named)) < 1e-4
    z = np.array([2.0, -np.inf, 0.5, -1.0])
    items = [(z, 2, "a", "tomato", "inat"), (z + np.array([0, 0, 1.5, 0]), 0, "b", "tomato", "inat")]
    again = cb.disease_items_from(cb.disease_inputs(items), 4)
    assert np.array_equal(again[0][0], z) and again[1][1:] == (0, "b", "tomato", "inat")
    assert abs(cb.fit_items(again) - cb.fit_items(items)) < 1e-6


def test_the_committed_calibration_follows_from_its_inputs():
    """M16's numbers can be checked without the photos: the newest committed report's inputs give
    back its fitted temperatures, ECE and agreement counts."""
    import json

    reports = sorted(p for p in cb.REPORTS.glob("calibration-*.json")
                     if "inputs" in json.loads(p.read_text())["species"])
    assert reports, "no committed calibration report keeps its inputs"
    r = json.loads(reports[-1].read_text())
    sp = r["species"]
    named, outside = cb.species_data(sp["inputs"])
    t = cb.fit_species(named)
    assert abs(t - sp["fitted_temperature"]) < 2e-3
    s = cb.species_summary(named, sp["species_ids"], np.full(len(named), sp["fitted_temperature"]),
                           outside, np.full(len(outside), sp["fitted_temperature"]))
    assert abs(s["ece"] - sp["fitted"]["ece"]) < 1e-3
    assert s["at_agreement"]["n"] == sp["fitted"]["at_agreement"]["n"]
    assert s["agreement_threshold"] == sp["fitted"]["agreement_threshold"]
    for d in r["disease"]:
        items = cb.disease_items_from(d["inputs"], d["classes"])
        assert len(items) == d["dev_photos"]
        assert abs(cb.fit_items(items) - d["temperatures"]["t_dev"]) < 2e-3
