"""`training.eval.species_check`: the species stage on the dev cases."""

from ml.training.eval import species_check as sc


def test_without_an_evaluation_manifest_it_skips(tmp_path, capsys):
    assert sc.main(["--manifest", str(tmp_path / "manifest.jsonl"), "--out", "t"]) == 0
    assert "nothing to score" in capsys.readouterr().out


def test_the_zero_shot_temperature_is_the_manifests():
    assert sc.species_temperature([{"name": "bioclip", "config": {"species_temperature": 1.4}}]) == 1.4
    assert sc.species_temperature([{"name": "bioclip"}]) == 1.0
