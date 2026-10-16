"""Original-photo names (ml/training/data/index.py), on Tomato-Pakistan's file names."""

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
