"""`detector.background.pick`: indoor and people/pets only, deterministic, at most `count`."""

from ml.training.detector.background import pick


def test_pick_skips_outdoor_and_is_deterministic():
    rows = [(f"{i:012d}.jpg", g) for i, g in enumerate(["indoor", "outdoor", "people_pets", "unlabelled"] * 25)]
    first = pick(rows, 30)
    assert len(first) == 30
    assert {g for _, g in first} <= {"indoor", "people_pets"}
    assert first == pick(list(reversed(rows)), 30) == sorted(first)
    assert len(pick(rows, 1000)) == 50
