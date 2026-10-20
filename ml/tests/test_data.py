"""Original-photo names (ml/training/data/index.py), on Tomato-Pakistan's file names, and the
committed splits against the mapping files."""

import posixpath
import re

import yaml

from ml.training.data.index import HERE, original_name

STRIP = re.compile(yaml.safe_load((HERE / "mappings/tomato_pakistan_2026.yaml").read_text())["originals"]["strip"], re.I)


def test_augmented_copies_share_their_original():
    copies = [
        "TomatoBlight.JPG_aug_1126_aug_926.jpg",
        "TomatoBlight-JPG_jpg.rf.01961ae562a5040c2063a4d7c865edf4_aug_1079_aug_879.jpg",
    ]
    assert {original_name(n, STRIP) for n in copies} == {"tomatoblight"}
    assert original_name("Gls42_change_270_aug_343.jpg", STRIP) == original_name("Gls42_hight_aug_424.jpg", STRIP) == "gls42"


def test_plain_names_are_kept():
    assert original_name("IMG_20251102_160922_aug_1120.jpg", STRIP) == "img_20251102_160922"
    assert original_name("EB_(973)_aug_434.jpg", STRIP) == "eb_(973)"
    assert original_name("1285217058_1661362739112.jpg", None) == "1285217058_1661362739112"


def test_the_splits_match_the_mapping_files():
    """A class renamed or dropped in a mapping is renamed or removed in the splits too (training.md §4)."""
    prefixes = []
    for f in (HERE / "mappings").glob("*.yaml"):
        m = yaml.safe_load(f.read_text())
        for folder in m["splits"].values():
            prefixes.append((posixpath.normpath(f"{m['source']}/{m.get('root', '.')}/{folder}") + "/", m))
    for split in ("train", "val", "test"):
        for line in (HERE / f"splits/{split}.tsv").read_text().splitlines()[1:]:
            dataset, path, species, problem = line.split("\t")
            prefix, m = next((p, m) for p, m in prefixes if path.startswith(p) and m["dataset"] == dataset)
            c = m["classes"][posixpath.dirname(path[len(prefix):])]
            assert (c["species_id"], c["problem_id"]) == (species, problem), f"{split}: {path}"
