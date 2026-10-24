"""`training.data.negatives_probe` and the probe trainer's `--clean` guard: only photos whose licence
allows commercial use and adaptation train a probe that ships (training.md §3)."""

import pytest

from ml.training.data import negatives_probe as npb
from ml.training.heads import train_plant_probe as tpp

BY, NC = "Attribution License", "Attribution-NonCommercial License"


def test_the_pick_takes_each_groups_count_of_reusable_photos_not_held_out():
    rows = [(f"{i:03d}.jpg", g) for i, g in enumerate(["indoor"] * 6 + ["outdoor"] * 4)]
    licence = {f: (NC if i % 3 == 0 else BY) for i, (f, _) in enumerate(rows)}
    picked = npb.pick(rows, {"indoor": 2, "outdoor": 1}, licence, exclude={"001.jpg"})
    assert sorted(g for _, g in picked) == ["indoor", "indoor", "outdoor"]
    assert all(licence[f] == BY for f, _ in picked) and "001.jpg" not in {f for f, _ in picked}
    assert picked == npb.pick(list(reversed(rows)), {"indoor": 2, "outdoor": 1}, licence, exclude={"001.jpg"})
    with pytest.raises(SystemExit):
        npb.pick(rows, {"outdoor": 4}, licence, exclude=set())          # only 2 reusable outdoor photos


def test_the_train_half_keeps_reusable_photos_and_counts_the_rest_per_group():
    rows = [{"path": f"coco_val2017/val2017/{i:03d}.jpg", "group": "indoor" if i % 2 else "outdoor"} for i in range(40)]
    licence = {r["path"].rsplit("/", 1)[-1]: (BY if i % 4 else NC) for i, r in enumerate(rows)}
    keep, dropped = npb.split_train_half(rows, licence)
    train = [r for r in rows if tpp.half(r["path"]) == "train"]
    assert {r["path"] for r in keep} <= {r["path"] for r in train}
    assert all(licence[r["path"].rsplit("/", 1)[-1]] == BY for r in keep)
    assert len(keep) + sum(dropped.values()) == len(train)


def test_the_trainer_refuses_a_clean_list_with_another_licence_a_tune_photo_or_a_held_out_one(tmp_path, monkeypatch):
    from ml.training.data import negatives_holdout

    tsv = tmp_path / "negatives_probe_train.tsv"
    monkeypatch.setattr(npb, "TSV", tsv)
    monkeypatch.setattr(negatives_holdout, "pinned_names", lambda path: {"c.jpg"})
    head = npb.COLUMNS + "\n"
    tsv.write_text(head + "coco_train2017\tcoco_train2017_probe/a.jpg\tindoor\t00\t" + BY + "\n")
    assert [r["path"] for r in tpp.clean_negatives(set())] == ["coco_train2017_probe/a.jpg"]
    for row, tune, counts in (
            ("coco_train2017\tcoco_train2017_probe/a.jpg\tindoor\t00\t" + NC, set(), (1, 0, 0)),
            ("coco_val2017\tcoco_val2017/val2017/b.jpg\tindoor\t00\t" + BY, {"coco_val2017/val2017/b.jpg"}, (0, 1, 0)),
            ("coco_train2017\tcoco_train2017_probe/c.jpg\tindoor\t00\t" + BY, set(), (0, 0, 1))):
        tsv.write_text(head + row + "\n")
        bad, seen, held = counts
        with pytest.raises(SystemExit, match=f"{bad} photos with a licence outside REUSABLE, {seen} in the tune half, "
                                             f"{held} in the hold-out"):
            tpp.clean_negatives(tune)


def test_the_committed_tune_list_is_the_half_no_probe_trains_on():
    from ml.training.eval import plant_check

    tune = npb.read_tsv(plant_check.DATA / "splits/negatives_coco_tune.tsv")
    assert tune == [r for r in npb.read_tsv(plant_check.NEGATIVES) if tpp.half(r["path"]) == "tune"]
    # The committed clean list shares no photo with it or with the hold-out.
    assert tpp.clean_negatives({r["path"] for r in tune})


def test_the_committed_clean_list_holds_only_reusable_licences():
    from ml.training.detector.background import REUSABLE

    rows = npb.read_tsv(npb.TSV)
    assert rows and all(r["licence"] in REUSABLE for r in rows)
    assert len({r["path"] for r in rows}) == len(rows)
