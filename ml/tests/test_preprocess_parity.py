"""J1's pre-processing parity check (eval/preprocess_parity) and its committed report."""

import json
import math
import random

import numpy as np
import pytest
from PIL import Image

from ml.pdm_vision import preprocess as pp
from ml.training.eval import preprocess_parity as par

REPORT = par.REPORTS / "preprocess-parity-2026-10-24.json"


def report() -> dict:
    return json.loads(REPORT.read_text())


@pytest.mark.parametrize("manifest", ["manifest.toml", "manifest.cpu.toml"])
def test_pp_recipes_are_the_manifests(manifest):
    entries = par.manifest_entries(par.ROOT / "ml/models" / manifest)
    for name, recipe in par.RECIPES.items():
        assert par.manifest_recipe(entries[name]) == recipe, (manifest, name)


def test_one_level_is_a_level_at_the_smallest_std():
    assert par.one_level(pp.BIOCLIP) == pytest.approx(1 / 255 / 0.26130258)
    assert par.one_level(pp.RFDETR) == pytest.approx(1 / 255 / 0.224)


def test_diff_and_levels():
    a = np.zeros((2, 4), np.uint8)
    b = a.copy()
    b[0, 0], b[1, 3] = 1, 3
    lv = par.levels(a, b)
    assert lv["max"] == 3 and lv["share_nonzero"] == 0.25 and lv["share_over_1"] == 0.125
    assert par.diff(a, b[:, :3]) == {"shape": [[2, 4], [2, 3]]}
    d = par.diff(np.zeros(4, np.float32), np.array([0, 0.01, 0.02, 0.05], np.float32), unit=0.015)
    assert d["max"] == pytest.approx(0.05) and d["share_over_1_level"] == 0.5


def rows(n: int) -> list[dict]:
    rng = random.Random(1)
    out = []
    for i in range(n):
        source = rng.choice(["inat", "fieldsets", "commons"])
        size = rng.choice([(640, 480), (480, 640), (3000, 2000), (2000, 3000)])
        out.append({"key": f"c{i}#0", "file": f"{source}/x{i}.jpg", "sha256": f"{rng.getrandbits(64):016x}", "size": size})
    return out


def test_select_photos_covers_every_group_and_ignores_file_order():
    rs = rows(80)
    picked = par.select_photos(rs, 10)
    shuffled = rs[:]
    random.Random(2).shuffle(shuffled)
    assert picked == par.select_photos(shuffled, 10)
    groups = {(r["file"].split("/")[0], max(r["size"]) <= par.STORED_LONG_SIDE, r["size"][1] > r["size"][0]) for r in rs}
    got = {(r["file"].split("/")[0], max(r["size"]) <= par.STORED_LONG_SIDE, r["size"][1] > r["size"][0]) for r in picked}
    assert len(groups) > 10 and len(picked) == 10 and len(got) == 10   # one photo from each of 10 groups


def test_crop_box_cuts_the_workers_rectangles():
    """`pp.crop_box` against the rectangles `PixelRect::from_normalised` gave in the committed run,
    read back from an image whose pixel values are their own index."""
    r = report()
    stored = {p["id"]: p["stored"] for p in r["photos"]}
    for c in r["crops"]:
        w, h = stored[c["photo"]]
        index = Image.fromarray(np.arange(w * h, dtype=np.int32).reshape(h, w), "I")
        cut = pp.crop_box(index, tuple(c["bbox"]), r["inputs"]["worker"]["crop_grow"])
        first = int(np.asarray(cut)[0, 0])
        assert [first % w, first // w, cut.size[0], cut.size[1]] == c["rect"], c["id"]
        assert c["same_rect"], c["id"]


def test_crop_box_rounds_outwards_and_clamps():
    """Left and top floored, right and bottom ceiled, then clamped, as `PixelRect::from_normalised`."""
    im = Image.new("RGB", (100, 50))
    x, y, w, h, g = 0.101, 0.2, 0.2, 0.2, 0.001
    gx, gy, gw, gh = x - w * g, y - h * g, w * (1 + 2 * g), h * (1 + 2 * g)
    x0, y0 = math.floor(gx * 100), math.floor(gy * 50)
    x1, y1 = math.ceil((gx + gw) * 100), math.ceil((gy + gh) * 50)
    assert (x0, y0, x1, y1) == (10, 9, 31, 21)
    assert pp.crop_box(im, (x, y, w, h), g).size == (x1 - x0, y1 - y0)
    assert pp.crop_box(im, (0.0, 0.0, 1.0, 1.0), 0.1).size == (100, 50)   # grown past every edge


def test_committed_report():
    r = report()
    s = r["summary"]
    assert all(v["matches_pp"] for v in r["recipes"].values())
    assert s["crops"]["same_rect"]
    # The finding: the worker's resize differs from Pillow's by its pass order, so the target fails
    # as the worker is today, and a horizontal-first resize brings BioCLIP's tensors within a level.
    assert not s["passes"]
    assert s["bioclip"]["hfirst_max"] <= s["bioclip"]["one_level"] * 1.001
    assert s["crops"]["hfirst_max"] <= s["bioclip"]["one_level"] * 1.001
    assert s["resize_hfirst"]["max"] <= 2
    assert REPORT.with_suffix(".md").read_text() == par.report_md(r)
