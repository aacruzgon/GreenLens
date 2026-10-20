"""`training.eval.calibrate`: the worker's species and disease scoring, and the temperature fits."""

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
    assert cb.leaf_photos(case()) == ["fieldsets/set/0.jpg"]                       # one photo: a leaf on its own
    assert cb.leaf_photos(case(kinds=("whole_plant", "leaf_closeup", "leaf_underside"))) == ["fieldsets/set/1.jpg", "fieldsets/set/2.jpg"]
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
