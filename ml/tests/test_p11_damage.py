"""`training.eval.p11_damage`: the worker's trace rule, ported, and the counts built on it."""

from ml.training.eval import p11_damage as pd

# The examples of decode.rs's `a_trace_names_an_animal_or_what_it_left`, so the two rules agree.
KEPT = ["aphids", "Caterpillar on the leaf", "fine silk webbing", "dried slime", "dark frass line inside the mine",
        "holes chewed by beetles", "deer footprints", "Tomato hornworm", "whiteflies under the leaf",
        "frass, but no larva seen", "larva tunnels, no adult visible", "unknown insect",
        "clean cut stem at the soil line", "eggs on the underside", "sticky honeydew"]
DROPPED = ["", "none", "N/A", "not visible", "no animal seen", "without any insect", "Brown spots",
           "dark brown lesions with concentric rings", "yellowing leaf margins", "grey powdery mould on the upper leaves",
           "necrotic tissue", "brown spots on the tomato leaf", "long tan lesions on the corn leaf", "early blight lesions",
           "septoria leaf spot", "water-soaked lesions on the eggplant", "spots on the plant's leaves",
           "2 brown spots/lesions", "small round spots with a yellow halo ar", "angular spots limited by veins",
           "mineral deficiency between the veins", "flyspeck on the fruit", "leaf dropping from the lower stem"]


def test_the_trace_rule_matches_the_workers_examples():
    assert all(pd.names_a_trace(t) for t in KEPT)
    assert not any(pd.names_a_trace(t) for t in DROPPED)


def test_untraced_and_repeated_findings_are_dropped_and_disease_signs_flagged():
    d = lambda trace, box=None, pattern="shot_holes": {"trace": trace, "pattern": pattern, "image": "img1",
                                                        "bbox_2d": box, "other_description": None}
    kept, dropped = pd.kept_damage([d("flea beetles"), d("flea beetles"), d("brown spots"), d("bite marks", [0, 0, 0, 0])])
    assert len(kept) == 1 and [x["trace"] for x in dropped] == ["brown spots"]   # a box with no area is no box
    doc = {"recordings": [{"schema_name": "findings", "status": 200, "response": {"choices": [{"message": {
        "content": '{"damage": [{"trace": "brown spots", "pattern": "leaf_mines"}, {"trace": "larva", "pattern": "leaf_mines"}]}'}}]}}]}
    s = pd.summary(doc)
    assert (s["damage_raw"], s["damage_kept"], s["dropped"], s["dropped_disease_sign"]) == (2, 1, 1, 1)


def test_an_animal_case_has_a_pest_problem_or_an_unboxed_organism():
    pests = pd.pest_problems()
    assert {"tomato_hornworms", "tomato_spider_mites"} <= pests and "tomato_early_blight" not in pests
    assert pd.has_animal({"problems": ["tomato_hornworms"]}, pests)
    assert pd.has_animal({"problems": [], "organisms_unboxed": ["aphid"]}, pests)
    assert not pd.has_animal({"problems": ["tomato_septoria_leaf_spot"]}, pests)
