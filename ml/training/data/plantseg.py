"""PlantSeg's photos for fitting the spot segmenter's colour thresholds (evaluation 6).

    python -m ml.training.data.plantseg

Reads data/raw/plantseg/plantseg/Metadata.csv (fetched with `python -m ml.training.data.fetch
plantseg`, only the spot-type leaf diseases of the head's crops) and writes
data/splits/spots_plantseg.tsv: one row per photo with its lesion mask, crop, disease, kind, half
and size. Fitting only: these photos are never training data and never committed. The file keeps
only PlantSeg's file names, sizes and labels; each photo's licence statement and web page stay in
PlantSeg's own Metadata.csv.

- **Halves:** PlantSeg's own training split is the `fit` half; its validation and test splits
  are the `holdout` half, which only scores the defaults and the fitted values.
- **Size:** photos with a long side under 500 px are dropped (a quarter of them are web
  thumbnails, too small for the detector and for colour).
- **Cap:** at most 40 photos per disease and half, chosen by a hash of the file name, so a rerun
  picks the same ones.
- **Kind:** `spots` for the diseases whose masks outline discrete spots, `areas` for those whose
  masks paint whole regions (below).
"""

from __future__ import annotations

import csv
import hashlib
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE / "raw" / "plantseg" / "plantseg"
OUT = HERE / "splits" / "spots_plantseg.tsv"

MIN_LONG_SIDE = 500
CAP = 40
HALF = {"Training": "fit", "Validation": "holdout", "Test": "holdout"}
FOLDER = {"Training": "train", "Validation": "val", "Test": "test"}
# PlantSeg's crop name -> ours (models 3's crops); its disease names are kept as they are.
CROPS = {"Tomato": "tomato", "Corn": "corn", "Apple": "apple", "Grape": "grape",
         "Bell pepper": "pepper", "Squash": "squash", "Zucchini": "squash"}
# How PlantSeg draws each disease's lesions, read from its masks (training/runs/spots/ sheets):
# `spots` outlines discrete spots, the segmenter's job; `areas` paints large or diffuse regions
# (mildew over a whole leaf, a blight patch), where colour can't follow the outline. The fit uses
# `spots`; `areas` is only reported.
SPOTS = {"apple black rot", "apple rust", "apple scab", "bell pepper bacterial spot",
         "bell pepper frogeye leaf spot", "corn gray leaf spot", "corn rust", "grape black rot",
         "grape leaf spot", "tomato bacterial leaf spot", "tomato early blight", "tomato septoria leaf spot"}
COLUMNS = ("image", "mask", "crop", "disease", "kind", "half", "width", "height")


def order(name: str) -> str:
    return hashlib.sha256(name.encode()).hexdigest()


def select(rows: list[dict], root: Path = ROOT) -> list[dict]:
    """The chosen photos: on disk, of a known crop, large enough, at most CAP per disease and half."""
    groups: dict[tuple[str, str], list[dict]] = {}
    for r in rows:
        folder = FOLDER.get(r["Split"])
        if folder is None or r["Plant"] not in CROPS:
            continue
        image = Path("images") / folder / r["Name"]
        mask = Path("annotations") / folder / r["Label file"]
        if not (root / image).exists() or not (root / mask).exists():
            continue
        w, h = (int(v) for v in r["Resolution"].lower().split("x"))
        if max(w, h) < MIN_LONG_SIDE:
            continue
        row = {"image": f"plantseg/plantseg/{image.as_posix()}", "mask": f"plantseg/plantseg/{mask.as_posix()}",
               "crop": CROPS[r["Plant"]], "disease": r["Disease"],
               "kind": "spots" if r["Disease"] in SPOTS else "areas", "half": HALF[r["Split"]],
               "width": w, "height": h}
        groups.setdefault((row["disease"], row["half"]), []).append(row)
    out = []
    for key in sorted(groups):
        out += sorted(groups[key], key=lambda r: order(r["image"]))[:CAP]
    return sorted(out, key=lambda r: r["image"])


def main() -> None:
    with (ROOT / "Metadata.csv").open(encoding="utf-8-sig") as f:
        rows = list(csv.DictReader(f))
    chosen = select(rows)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", newline="") as f:
        w = csv.DictWriter(f, COLUMNS, delimiter="\t", lineterminator="\n")
        w.writeheader()
        w.writerows(chosen)
    print(f"{len(chosen)} photos -> {OUT}")
    for (half, crop), n in sorted(Counter((r["half"], r["crop"]) for r in chosen).items()):
        print(f"  {half} {crop}: {n}")


if __name__ == "__main__":
    main()
