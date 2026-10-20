"""The plant check's guards for a trained probe (review of #85)."""

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
