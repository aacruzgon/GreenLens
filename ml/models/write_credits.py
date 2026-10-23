"""Write credits/: the author of every photo in the weights we ship whose dataset has no single
citation (NOTICES.md, "Photo credits").

    python -m ml.models.write_credits

CC BY needs each author's credit, so these photos are listed one per line, and fetch.py copies
the folder beside NOTICES.md and licences/ wherever the model files go:

- `credits/inaturalist.tsv`: the iNaturalist training photos (`inat_species.tsv`, split train),
  which `plant_probe` learns from, with the observation's page, licence and credit.
- `credits/coco.tsv`: the COCO photos in `leaf_detector` (`background_coco_train.tsv`) and
  `plant_probe` (`negatives_probe_train.tsv`), with the photo's licence, that licence's URL as
  COCO's annotations give it (CC BY 2.0, CC BY-SA 2.0 or Flickr Commons' "no known copyright
  restrictions"), and its Flickr page, which names the author: `https://flic.kr/p/<photo id in base 58>`, the id read from `flickr_url` in
  COCO's 2017 annotations (`python -m ml.training.data.fetch coco_annotations_2017`).

A rerun writes the same files, and refuses a photo whose licence in the annotations differs from
the one its list records, or a licence whose URL differs from `LICENCE_URLS`. ml/tests/test_credits.py checks the files cover every listed photo.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
CREDITS = HERE / "credits"
SPLITS = HERE.parent / "training/data/splits"
ANNOTATIONS = HERE.parent / "training/data/raw/coco_annotations_2017/annotations"
INAT = SPLITS / "inat_species.tsv"
# The COCO photo lists of shipped weights: (list, the model trained on it).
COCO_LISTS = ((SPLITS / "background_coco_train.tsv", "leaf_detector"),
              (SPLITS / "negatives_probe_train.tsv", "plant_probe"))
INAT_COLUMNS = "photo\ttaxon\tobservation\tlicence\tcredit"
COCO_COLUMNS = "file\tsplit\tmodels\tlicence\tlicence_url\tflickr_page"
# The licences REUSABLE admits into shipped weights, with their URLs in COCO's 2017 annotations:
# the CC licences are version 2.0 (licences/CC-BY-2.0.txt, licences/CC-BY-SA-2.0.txt).
LICENCE_URLS = {
    "Attribution License": "http://creativecommons.org/licenses/by/2.0/",
    "Attribution-ShareAlike License": "http://creativecommons.org/licenses/by-sa/2.0/",
    "No known copyright restrictions": "http://flickr.com/commons/usage/",
    "United States Government Work": "http://www.usa.gov/copyright.shtml",
}
BASE58 = "123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ"


def read_tsv(path: Path) -> list[dict]:
    lines = path.read_text().splitlines()
    head = lines[0].split("\t")
    return [dict(zip(head, line.split("\t"))) for line in lines[1:]]


def inat_rows() -> list[str]:
    return [f"{r['path']}\t{r['taxon']}\thttps://www.inaturalist.org/observations/{r['observation']}\t"
            f"{r['licence']}\t{r['credit']}" for r in read_tsv(INAT) if r["split"] == "train"]


def flickr_page(flickr_url: str) -> str:
    """Flickr's short link to a photo's page, from its static image URL
    (`http://farm5.staticflickr.com/4125/5083164728_e4746a3f25_z.jpg` -> `https://flic.kr/p/8KbxMb`)."""
    n = int(flickr_url.rsplit("/", 1)[-1].split("_")[0])
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = BASE58[r] + out
    return f"https://flic.kr/p/{out}"


def coco_photos() -> dict[str, dict]:
    """{file name: {"split", "models", "licence"}} over every COCO list of shipped weights."""
    out: dict[str, dict] = {}
    for path, model in COCO_LISTS:
        for r in read_tsv(path):
            name = r["path"].rsplit("/", 1)[-1]
            split = "val2017" if r["dataset"] == "coco_val2017" else "train2017"
            seen = out.setdefault(name, {"split": split, "models": [], "licence": r["licence"]})
            if seen["licence"] != r["licence"] or seen["split"] != split:
                raise SystemExit(f"{name}: {path.name} records {split}, {r['licence']}; another list {seen}")
            seen["models"].append(model)
    return out


def coco_rows(photos: dict[str, dict]) -> list[str]:
    rows = []
    for split in ("train2017", "val2017"):
        doc = json.loads((ANNOTATIONS / f"instances_{split}.json").read_text())
        names = {lic["id"]: lic["name"] for lic in doc["licenses"]}
        for lic in doc["licenses"]:
            if lic["name"] in LICENCE_URLS and lic["url"] != LICENCE_URLS[lic["name"]]:
                raise SystemExit(f"{split}: {lic['name']} is {lic['url']}, LICENCE_URLS has {LICENCE_URLS[lic['name']]}")
        images = {im["file_name"]: im for im in doc["images"]}
        for name, p in sorted(photos.items()):
            if p["split"] != split:
                continue
            im = images[name]
            if names[im["license"]] != p["licence"]:
                raise SystemExit(f"{name}: COCO says {names[im['license']]}, the list says {p['licence']}")
            rows.append(f"{name}\t{split}\t{','.join(p['models'])}\t{p['licence']}\t{LICENCE_URLS[p['licence']]}\t"
                        f"{flickr_page(im['flickr_url'])}")
    return rows


def main(argv=None) -> int:
    argparse.ArgumentParser(description=__doc__.split("\n\n")[0]).parse_args(argv)
    CREDITS.mkdir(exist_ok=True)
    inat = inat_rows()
    (CREDITS / "inaturalist.tsv").write_text("\n".join([INAT_COLUMNS, *inat]) + "\n")
    coco = coco_rows(coco_photos())
    (CREDITS / "coco.tsv").write_text("\n".join([COCO_COLUMNS, *coco]) + "\n")
    print(f"credits/inaturalist.tsv: {len(inat)} photos; credits/coco.tsv: {len(coco)} photos")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
