"""iNaturalist photos for the species head and the trained plant probe (training.md §6; M13).

    python -m ml.training.data.inat_species [--per-taxon 280] [--only tomato --only corn]

Training photos, never evaluation photos: only observations made before 2026-01-01, while every
iNaturalist case of the evaluation set (`ml.training.eval.inat`) was observed in 2026 or later;
an observation that is in the evaluation review files anyway is skipped. CC0 and CC BY photos
only, as the evaluation fetch takes them.

- Taxa: the five supported crops for the species head, and five plant families for the plant
  probe's plant side (cacti, stonecrops, aroids, mints, gourds: shapes the detector keeps few
  leaves on, and plants of a garden that aren't our crops).
- Observations spread evenly over the quarters of 2019-2025, with at least one identification
  agreeing with the observer's, so a cultivated plant (casual grade on iNaturalist) still has a
  second opinion. The first usable photo of each, at iNaturalist's `large` size (1,024 px on
  the long side), re-encoded without metadata.
- Split 80/20 into train and val by a hash of the observation id.

Writes the photos under data/raw/inat_species/<slug>/ and the index
data/splits/inat_species.tsv: slug, path (under data/raw), taxon, role, observation, photo,
observed_on, licence, credit, split. Downloads already on disk are kept, so a rerun only adds.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
import urllib.error
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from ml.training.eval import inat

ROOT = Path(__file__).resolve().parents[3]
RAW = ROOT / "ml/training/data/raw"
PHOTOS = RAW / "inat_species"
INDEX = ROOT / "ml/training/data/splits/inat_species.tsv"
YEARS = range(2019, 2026)
QUARTERS = (("01-01", "03-31"), ("04-01", "06-30"), ("07-01", "09-30"), ("10-01", "12-31"))
VAL_SHARE = 0.2
COLUMNS = ("slug", "path", "taxon", "role", "observation", "photo", "observed_on", "licence", "credit", "split")

# slug, iNaturalist names to try in order, rank, role. Crop slugs are the species ids of the splits.
TAXA = [
    ("tomato", ["Solanum lycopersicum"], "species", "crop"),
    ("apple", ["Malus domestica", "Malus pumila"], "species", "crop"),
    ("grape", ["Vitis vinifera"], "species", "crop"),
    ("pepper", ["Capsicum annuum"], "species", "crop"),
    ("corn", ["Zea mays"], "species", "crop"),
    ("cacti", ["Cactaceae"], "family", "plant"),
    ("stonecrops", ["Crassulaceae"], "family", "plant"),
    ("aroids", ["Araceae"], "family", "plant"),
    ("mints", ["Lamiaceae"], "family", "plant"),
    ("gourds", ["Cucurbitaceae"], "family", "plant"),
]


def api(path: str, **query) -> dict:
    """`inat.api` with retries: a busy or failing API waits and tries again, up to 4 times."""
    for wait in (10, 30, 90, None):
        try:
            return inat.api(path, **query)
        except (urllib.error.URLError, TimeoutError, ValueError) as e:
            if wait is None:
                raise
            print(f"  API {path}: {e}; retrying in {wait} s", file=sys.stderr, flush=True)
            time.sleep(wait)
    raise AssertionError("unreachable")


def resolve(names: list[str], rank: str) -> tuple[int, str]:
    for name in names:
        try:
            return inat.pick_taxon(api("taxa", q=name, per_page=10)["results"], name, rank), name
        except SystemExit:
            continue
    raise SystemExit(f"iNaturalist has none of {names} at rank {rank}")


def split_of(obs_id: int) -> str:
    h = int(hashlib.sha256(f"inat-species|{obs_id}".encode()).hexdigest()[:8], 16)
    return "val" if h / 0xFFFFFFFF < VAL_SHARE else "train"


def evaluation_observations() -> set[int]:
    """Every observation id in the evaluation set's review files and manifest."""
    ids: set[int] = set()
    for p in [inat.PHOTOS / "inat" / "review.yaml", inat.MANIFEST]:
        if p.exists():
            for tok in p.read_text().replace("/", " ").replace('"', " ").split():
                if tok.isdigit() and len(tok) >= 6:
                    ids.add(int(tok))
    return ids


def window(tid: int, year: int, q: tuple[str, str], want: int, skip: set[int]) -> list[dict]:
    res = api("observations", taxon_id=tid, d1=f"{year}-{q[0]}", d2=f"{year}-{q[1]}",
              photo_license=",".join(inat.LICENCES), photos="true", identifications="most_agree",
              per_page=min(200, max(want * 4, 30)), order_by="votes", order="desc")
    out = []
    for obs in res["results"]:
        if obs["id"] in skip or (obs.get("num_identification_agreements") or 0) < 1:
            continue
        if not inat.matches(obs, tid, "descendants"):
            continue
        photos = inat.usable_photos(obs)
        if not photos or "/square." not in photos[0].get("url", ""):
            continue
        out.append(obs)
        if len(out) == want:
            break
    return out


def fetch_taxon(slug: str, names: list[str], rank: str, role: str, per_taxon: int, skip: set[int]) -> list[dict]:
    tid, name = resolve(names, rank)
    windows = [(y, q) for y in YEARS for q in QUARTERS]
    want = -(-per_taxon // len(windows))
    rows = []
    for y, q in windows:
        try:
            found = window(tid, y, q, want, skip)
        except Exception as e:  # one window failing costs its photos, not the night
            print(f"  {slug} {y} {q[0]}: skipped: {e}", file=sys.stderr, flush=True)
            continue
        for obs in found:
            p = inat.usable_photos(obs)[0]
            rows.append({
                "slug": slug, "path": f"inat_species/{slug}/{obs['id']}-{p['id']}.jpg", "taxon": obs["taxon"]["name"],
                "role": role, "observation": obs["id"], "photo": p["id"], "observed_on": obs.get("observed_on") or "",
                "licence": inat.LICENCES[p["license_code"]], "credit": f"{p['attribution']}; {inat.CHANGES}",
                "split": split_of(obs["id"]), "url": p["url"].replace("/square.", "/large."),
            })
        skip.update(o["id"] for o in found)
    print(f"{slug} ({name}, taxon {tid}): {len(rows)} observations", file=sys.stderr, flush=True)
    return rows[:per_taxon]


def download(row: dict) -> bool:
    return inat.download({"id": row["photo"], "url": row["url"], "file": row["path"]}, RAW)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--per-taxon", type=int, default=280)
    ap.add_argument("--only", action="append", help="a slug; repeatable")
    args = ap.parse_args(argv)
    skip = evaluation_observations()
    print(f"{len(skip)} evaluation observation ids excluded", file=sys.stderr)
    old = {}
    if INDEX.exists():
        for line in INDEX.read_text().splitlines()[1:]:
            r = dict(zip(COLUMNS, line.split("\t")))
            old[r["path"]] = r
    rows = []
    for slug, names, rank, role in TAXA:
        if args.only and slug not in args.only:
            rows += [r for r in old.values() if r["slug"] == slug]
            continue
        rows += fetch_taxon(slug, names, rank, role, args.per_taxon, skip)
    with ThreadPoolExecutor(4) as pool:
        ok = list(pool.map(lambda r: "url" not in r or download(r), rows))
    kept = [r for r, o in zip(rows, ok) if o and (RAW / r["path"]).exists()]
    kept.sort(key=lambda r: (r["slug"], int(r["observation"])))
    INDEX.write_text("\t".join(COLUMNS) + "\n" + "".join("\t".join(str(r[c]) for c in COLUMNS) + "\n" for r in kept))
    counts = {}
    for r in kept:
        counts.setdefault(r["slug"], {"train": 0, "val": 0})[r["split"]] += 1
    print(json.dumps(counts))
    print(f"{len(kept)} photos ({len(rows) - len(kept)} failed) in {INDEX}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
