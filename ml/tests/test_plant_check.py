"""The plant check: its guards for a trained probe (review of #85), its case rule and the
whole-plant cap and tie-break (M17)."""

import json

import numpy as np
import pytest

from ml.training.eval import plant_check as pc


def tsv(path, paths):
    path.write_text("dataset\tpath\tgroup\n" + "".join(f"coco\t{p}\toutdoor\n" for p in paths))
    return path


@pytest.fixture
def probe(tmp_path, monkeypatch):
    monkeypatch.setattr(pc, "NEGATIVES", tsv(tmp_path / "negatives_coco.tsv", ["a", "b", "c", "d"]))
    d = tmp_path / "probe"
    d.mkdir()
    tsv(d / "negatives_tune.tsv", ["b", "d"])
    (d / "p.json").write_text("{}")
    return d / "p.json"


def test_a_probe_is_scored_on_its_tune_half_by_default(probe):
    assert pc.negatives_for([probe], None) == probe.parent / "negatives_tune.tsv"
    assert pc.negatives_for([], None) == pc.NEGATIVES


def test_negatives_a_probe_trained_on_are_refused(probe, tmp_path):
    with pytest.raises(ValueError, match="2 photos are ones p.json trained on"):
        pc.negatives_for([probe], pc.NEGATIVES)
    assert pc.negatives_for([probe], tsv(tmp_path / "holdout.tsv", ["x", "d"])).name == "holdout.tsv"


def test_a_probe_without_its_tune_half_is_refused(probe):
    (probe.parent / "negatives_tune.tsv").unlink()
    with pytest.raises(ValueError, match="no negatives_tune.tsv"):
        pc.negatives_for([probe], None)


def test_an_unknown_pair_name_is_a_clean_error_on_a_rescore_too(tmp_path, capsys):
    prev = tmp_path / "prev.json"
    prev.write_text('{"sets": {"A": {}}}')
    with pytest.raises(SystemExit):
        pc.main(["--rescore", str(prev), "--pair", "B:0.4:0.7"])
    assert "unknown prompt set or probe 'B' (known: A)" in capsys.readouterr().err


def test_a_case_passes_when_any_of_its_shots_does():
    # Case 0: a close-up only the probe passes and a whole plant nothing passes; case 1: neither.
    leaf = np.array([0.1, 0.2, 0.1], np.float32)
    p = np.array([0.9, 0.3, 0.6])
    of = np.array([0, 0, 1])
    assert pc.case_pass(leaf, p, of, 2, 0.4, 0.7).tolist() == [True, False]
    assert pc.case_pass(leaf, p, of, 2, 0.4, 0.5).tolist() == [True, True]
    # The highest τ_plant keeping every case is case 1's best shot.
    assert pc.highest_tau_plant(leaf, p, of, 2, 0.4, recall=1.0) == 0.6


def test_the_joint_search_skips_pairs_failing_more_whole_plants_than_the_cap():
    # Leaf photos pass at any τ_plant up to 0.9; one negative reaches 0.8; one whole plant only 0.7.
    pos_leaf, pos_p = np.zeros(4, np.float32), np.full(4, 0.9)
    neg_leaf, neg_p = np.zeros(3, np.float32), np.array([0.8, 0.1, 0.1])
    whole = (np.zeros(4, np.float32), np.array([0.7, 0.95, 0.95, 0.95]))
    assert pc.joint(pos_leaf, pos_p, neg_leaf, neg_p)["tau_plant"] == 0.81
    capped = pc.joint(pos_leaf, pos_p, neg_leaf, neg_p, whole=whole, whole_max=0.0)
    assert capped["tau_plant"] <= 0.7 and capped["whole_plants_false_no_plant"] == 0.0
    assert pc.whole_false(whole, 0.4, 0.81) == 0.25
    # A whole plant the leaf score passes needs no probe.
    assert pc.whole_false((np.array([0.5, 0, 0, 0], np.float32), whole[1]), 0.4, 0.81) == 0.0


def test_ties_go_to_the_lower_tau_leaf_so_the_detector_stays_on():
    # No negative scores a leaf above 0.6, so every τ_leaf from 0.61 up lets the same negatives
    # through; the lowest keeps the two plant photos the probe misses passing on their leaf score.
    pos_leaf, pos_p = np.array([0.7, 0.8, 0.1, 0.1], np.float32), np.array([0.2, 0.2, 0.9, 0.9])
    neg_leaf, neg_p = np.array([0.6, 0.1, 0.1], np.float32), np.array([0.1, 0.1, 0.5])
    j = pc.joint(pos_leaf, pos_p, neg_leaf, neg_p)
    assert (j["tau_leaf"], j["tau_plant"], j["negatives_pass_check"]) == (0.61, 0.51, 0.0)


def test_a_rescore_keeps_the_whole_plants(tmp_path):
    rng = np.random.default_rng(0)
    pos = [{"path": f"p{i}", "dataset": "d1" if i % 2 else "d2", "problem_id": "x"} for i in range(40)]
    neg = [{"path": f"n{i}", "group": "indoor"} for i in range(30)]
    r4 = lambda a: np.round(a, 4)
    pos_leaf, neg_leaf = r4(rng.uniform(0, 1, 40)).astype(np.float32), r4(rng.uniform(0, 0.5, 30)).astype(np.float32)
    probs = {"A": (r4(rng.uniform(0.5, 1, 40)), r4(rng.uniform(0, 0.8, 30)))}
    whole = {"rows": [{"path": f"w{i}", "slug": "cacti"} for i in range(20)],
             "leaf": r4(rng.uniform(0, 0.5, 20)).astype(np.float32), "p": {"A": r4(rng.uniform(0.4, 1, 20))}}
    first = pc.analyse("val", "first", pos, neg, pos_leaf, neg_leaf, probs, 0.4, 100.0, "e", (), whole)
    assert first["whole_plants"]["cap"] == pc.MAX_FALSE_NO_PLANT
    assert first["fixed_pairs"][0]["whole_plants_false_no_plant"] == pc.whole_false((whole["leaf"], whole["p"]["A"]), 0.4, 0.7)
    (tmp_path / "first.json").write_text(json.dumps(first))
    assert pc.main(["--rescore", str(tmp_path / "first.json"), "--out", str(tmp_path), "--label", "again"]) == 0
    again = json.loads((tmp_path / "again.json").read_text())
    assert again["whole_plants"] == first["whole_plants"]
    assert again["sets"]["A"]["joint_every_dataset"] == first["sets"]["A"]["joint_every_dataset"]
    assert "whole plants |" in (tmp_path / "again.md").read_text()
