"""`training.eval.species_check`: the species stage on the dev cases."""

import numpy as np

from ml.training.eval import species_check as sc
from ml.training.heads.train_species import head_in_share


def test_without_an_evaluation_manifest_it_skips(tmp_path, capsys):
    assert sc.main(["--manifest", str(tmp_path / "manifest.jsonl"), "--out", "t"]) == 0
    assert "nothing to score" in capsys.readouterr().out


def test_the_zero_shot_temperature_is_the_manifests():
    assert sc.species_temperature([{"name": "bioclip", "config": {"species_temperature": 1.4}}]) == 1.4
    assert sc.species_temperature([{"name": "bioclip"}]) == 1.0


def test_the_head_in_share_keeps_zero_shots_share_of_the_crops():
    p_zs = np.array([[0.6, 0.3, 0.1]])                         # crops a, b; c is outside the head
    out = head_in_share(p_zs, ["a", "b", "c"], np.array([[0.2, 0.8]]), ["a", "b"])
    assert np.allclose(out, [[0.18, 0.72, 0.1]])


def test_confusions_count_each_species_top_1():
    p = np.array([[0.9, 0.1], [0.2, 0.8], [0.7, 0.3]])
    assert sc.confusions(["x", "x", "y"], p, ["x", "y"]) == {"x": {"x": 1, "y": 1}, "y": {"x": 1}}


def test_commons_cases_are_found_by_their_source(tmp_path):
    import json

    m = tmp_path / "manifest.jsonl"
    m.write_text("\n".join(json.dumps(c) for c in [
        {"case_id": "apple-commons-apple-hail", "source": "https://commons.wikimedia.org/wiki/File:Hail.jpg", "taken": "2019-06-01"},
        {"case_id": "corn-fs-x", "source": "https://data.mendeley.com/datasets/34gb2gr7p2/1/files/x.jpg"},
        {"case_id": "tomato-inat-1", "source": "https://www.inaturalist.org/observations/1"},
    ]) + "\n")
    assert sc.commons_cases(m) == {"apple-commons-apple-hail"}
