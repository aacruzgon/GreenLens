"""`ml/models/credits/`: every photo in shipped weights whose author needs a credit is listed,
with the licence its training list records (NOTICES.md, "Photo credits")."""

import tomllib

from ml.models import write_credits as wc


def test_every_inaturalist_training_photo_is_credited():
    rows = wc.read_tsv(wc.CREDITS / "inaturalist.tsv")
    train = [r for r in wc.read_tsv(wc.INAT) if r["split"] == "train"]
    assert [r["photo"] for r in rows] == [r["path"] for r in train]
    assert all(r["credit"] and r["licence"] in ("CC BY 4.0", "CC0 1.0") for r in rows)
    assert (wc.CREDITS / "inaturalist.tsv").read_text() == "\n".join([wc.INAT_COLUMNS, *wc.inat_rows()]) + "\n"


def test_every_coco_photo_in_shipped_weights_is_credited_with_its_licence():
    rows = {r["file"]: r for r in wc.read_tsv(wc.CREDITS / "coco.tsv")}
    photos = wc.coco_photos()
    assert rows.keys() == photos.keys()
    for name, p in photos.items():
        r = rows[name]
        assert (r["split"], r["models"].split(","), r["licence"]) == (p["split"], p["models"], p["licence"]), name
        assert r["licence_url"] == wc.LICENCE_URLS[p["licence"]], name
        assert r["flickr_page"].startswith("https://flic.kr/p/"), name


def test_each_cc_licence_in_the_credits_ships_its_text():
    # COCO's photos are CC BY 2.0 and CC BY-SA 2.0, not 4.0 (Alan's review of #167).
    texts = {"by/2.0": "CC-BY-2.0.txt", "by-sa/2.0": "CC-BY-SA-2.0.txt"}
    used = {r["licence_url"] for r in wc.read_tsv(wc.CREDITS / "coco.tsv")}
    for url in used:
        if "creativecommons.org" in url:
            name = texts[url.split("/licenses/")[1].rstrip("/")]
            assert (wc.HERE / "licences" / name).is_file(), name


def test_the_coco_lists_are_the_shipped_models_training_lists():
    # A new shipped model trained on COCO photos adds its list to COCO_LISTS.
    manifest = tomllib.loads((wc.HERE / "manifest.toml").read_text())
    names = {e["name"] for e in manifest["model"]}
    assert {m for _, m in wc.COCO_LISTS} <= names


def test_flickr_short_links_are_base_58_of_the_photo_id():
    assert wc.flickr_page("http://farm5.staticflickr.com/4125/5083164728_e4746a3f25_z.jpg") == "https://flic.kr/p/8KbxMb"
