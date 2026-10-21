"""`training.data.dedupe.assign_splits`: committed images keep their split, new ones follow them."""

from pathlib import Path

from ml.training.data.dedupe import assign_splits
from ml.training.data.index import Record


def rec(dataset: str, name: str, split: str = "train", problem: str = "healthy") -> Record:
    return Record(dataset, split, Path(dataset) / name, problem, "tomato", problem)


def where(out: dict) -> dict[str, str]:
    return {r.path.name: s for s, rs in out.items() for r in rs}


def test_committed_images_keep_their_split_and_new_ones_join_the_strictest():
    committed = {"old/a.jpg": "train", "old/b.jpg": "test", "old/c.jpg": "val", "old/d.jpg": "val"}
    groups = [
        [rec("old", "a.jpg"), rec("new", "a2.jpg")],                       # joins train
        [rec("old", "b.jpg"), rec("old", "c.jpg"), rec("new", "b2.jpg")],  # test beats val
        [rec("old", "d.jpg"), rec("new", "d2.jpg")],                       # joins val
        [rec("new", "e.jpg")],                                             # free: no shares, so train
    ]
    out, dropped, kept_where = assign_splits(groups, committed, lambda r: r.path.as_posix(), set(), 0.0, 0.0)
    assert where(out) == {"a.jpg": "train", "a2.jpg": "train", "b.jpg": "test", "c.jpg": "val",
                          "b2.jpg": "test", "d.jpg": "val", "d2.jpg": "val", "e.jpg": "train"}
    assert dropped == [] and kept_where == 4


def test_a_new_near_copy_of_a_published_test_image_is_dropped():
    groups = [[rec("pd", "t.jpg", split="test"), rec("pc", "x.jpg")]]
    out, dropped, _ = assign_splits(groups, {}, lambda r: r.path.as_posix(), {"pd"}, 0.15, 0.15)
    assert where(out) == {"t.jpg": "test"} and [r.path.name for r in dropped] == ["x.jpg"]


def test_free_groups_fill_test_only_for_sets_without_a_published_test_split():
    groups = [[rec(d, f"{i}.jpg")] for d in ("field", "pub") for i in range(20)]
    out, _, _ = assign_splits(groups, {}, lambda r: r.path.as_posix(), {"pub"}, 0.15, 0.15)
    count = lambda d, s: sum(r.dataset == d for r in out[s])
    assert (count("field", "test"), count("field", "val"), count("field", "train")) == (3, 3, 14)
    assert (count("pub", "test"), count("pub", "val"), count("pub", "train")) == (0, 3, 17)
