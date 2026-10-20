"""Licensed iNaturalist photos for the evaluation set (evaluation.md §2, "Where the photos come from").

    python -m ml.training.eval.inat fetch [--per-target 20] [--only TAXON] [--count]
    python -m ml.training.eval.inat add --labeller NAME

The team's own photos are the core of the evaluation set. Licensed photos fill the groups we can't
find locally, such as diseased crops in late October. Three rules keep the scores meaningful:

- Only CC0 and CC BY photos, with each photo's licence and credit recorded. iNaturalist's default
  licence, CC BY-NC, is left out.
- Only observations from 2026 on (`--since`). BioCLIP 2's training data, TreeOfLife-200M, was
  compiled before 2026 and draws on GBIF, which holds iNaturalist's licensed observations
  *unverified*; PlantDoc dates from 2019. Never use a photo from a web search: PlantDoc was scraped
  from Google Images.
- Each target's taxon is matched exactly, since iNaturalist's filter also returns related taxa, and
  a teammate reviews every observation before it becomes a case.

`fetch` searches each target in `inat_targets.yaml` and downloads the licensed photos at
iNaturalist's largest size (2048 px on the long side) to `photos/inat/` (git-ignored), re-encoded
with no metadata. It writes `photos/inat/candidates.json`, a contact sheet `photos/inat/index.html`
and `photos/inat/review.yaml`, with one entry per observation set to `keep: false`. A rerun adds new
observations and keeps the review's edits.

To review, open the contact sheet. For each observation to use, set `keep: true`, check the host
plant (`species_id`), the problems and the outcome, and give each photo its shot kind or `skip`. A
case is one scan, so it needs exactly one `whole_plant` photo (a wider shot that shows the plant), at
most two `leaf_closeup` photos and at most one of each other kind. `confidence` stays `probable`
unless the identification is research grade and the photos clearly show the problem's signs.

`add` appends every kept observation to `manifest.jsonl` as a case. Each case gets `label_basis`
`source_caption`, the observation's URL as `source`, its photos' licence and credit, and an empty
`reviewed_by` until a second teammate reviews it (evaluation.md §3). The split is by plant, about
60% dev, unless the review sets `split`. Organism and damage boxes are drawn later, in CVAT or Label
Studio. The species head's iNaturalist training photos (training.md §6) must leave out every
observation listed in the manifest.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import io
import json
import sys
import time
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
TARGETS = HERE / "inat_targets.yaml"
MANIFEST = HERE / "manifest.jsonl"
PHOTOS = HERE / "photos"
API = "https://api.inaturalist.org/v1/"
HEADERS = {"User-Agent": "PDM-GreenLens-eval/0.1 (+https://github.com/aacruzgon/GreenLens)"}
EARLIEST = "2026-01-01"
LICENCES = {"cc0": "CC0", "cc-by": "CC BY"}  # iNaturalist's licence codes we may use
KINDS = {"whole_plant": 1, "leaf_closeup": 2, "leaf_underside": 1, "soil_surface": 1, "flower_fruit": 1}
OUTCOMES = ("diagnosed", "healthy", "no_plant_found", "photos_unusable", "rejected_at_upload")
LIFE_STAGES = {"larva": (1, 6)}  # iNaturalist's Life Stage annotation: term 1, value 6
DEV_SHARE = 0.6
PAGE = 200
REVIEW_HEADER = """\
# Review of the iNaturalist candidates (ml/training/eval/inat.py). For each observation to use:
#   keep: true
#   species_id, problems, outcome: the truth; check the host plant on the photos
#   confidence: probable, or sure when research grade and the signs are clear
#   images: photo id -> whole_plant (exactly 1), leaf_closeup (at most 2), leaf_underside,
#           soil_surface, flower_fruit (at most 1 each), or skip
#   split: optional, dev or test (default: by plant, about 60% dev)
# Then: python -m ml.training.eval.inat add --labeller <your GitHub login>
"""

_last_call = 0.0


def api(path: str, **query) -> dict:
    """One iNaturalist API call, at most one a second (their recommended rate)."""
    global _last_call
    wait = 1.0 - (time.monotonic() - _last_call)
    if wait > 0:
        time.sleep(wait)
    _last_call = time.monotonic()
    url = API + path + "?" + urllib.parse.urlencode(query)
    with urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=60) as r:
        return json.load(r)


def pick_taxon(results: list[dict], name: str, rank: str | None = None) -> int:
    """The taxon named `name`, at `rank` when given, else preferring a species: one name can be
    both a species and its complex (Podosphaera xanthii)."""
    hits = [t for t in results if t["name"].lower() == name.lower()]
    if rank:
        hits = [t for t in hits if t["rank"] == rank]
    if not hits:
        raise SystemExit(f"iNaturalist has no taxon named {name!r}" + (f" at rank {rank}" if rank else ""))
    return next((t for t in hits if t["rank"] == "species"), hits[0])["id"]


def taxon_id(name: str, rank: str | None = None) -> int:
    return pick_taxon(api("taxa", q=name, per_page=10)["results"], name, rank)


def matches(obs: dict, tid: int, match: str = "exact") -> bool:
    """Whether the observation's taxon is the target's, or below it when `match` is descendants."""
    taxon = obs.get("taxon") or {}
    if taxon.get("id") == tid:
        return True
    return match == "descendants" and tid in (taxon.get("ancestor_ids") or [])


def usable_photos(obs: dict) -> list[dict]:
    """The observation's photos with a licence we may use: each photo has its own."""
    return [p for p in obs.get("photos") or [] if p.get("license_code") in LICENCES]


def query(target: dict, tid: int, since: str) -> dict:
    q = {"taxon_id": tid, "d1": since, "photo_license": ",".join(LICENCES), "photos": "true",
         "quality_grade": "research" if target.get("quality", "research") == "research" else "research,needs_id",
         "per_page": PAGE, "order_by": "created_at", "order": "desc"}
    if stage := target.get("life_stage"):
        q["term_id"], q["term_value_id"] = LIFE_STAGES[stage]
    return q


def search(target: dict, tid: int, since: str, limit: int, max_pages: int = 5) -> tuple[list[dict], int]:
    """Up to `limit` matching observations with a usable photo, and the API's total before matching."""
    found, total = [], 0
    for page in range(1, max_pages + 1):
        res = api("observations", page=page, **query(target, tid, since))
        total = res["total_results"]
        for obs in res["results"]:
            if matches(obs, tid, target.get("match", "exact")) and usable_photos(obs):
                found.append(obs)
                if len(found) == limit:
                    return found, total
        if page * PAGE >= total:
            break
    return found, total


def candidate(obs: dict, target: dict) -> dict:
    return {
        "obs": obs["id"], "url": obs["uri"], "taxon": obs["taxon"]["name"], "quality": obs["quality_grade"],
        "observed_on": obs.get("observed_on"), "target": target["taxon"],
        "photos": [{"id": p["id"], "licence": LICENCES[p["license_code"]], "credit": p["attribution"],
                    "url": p["url"].replace("/square.", "/original."), "file": f"inat/{obs['id']}-{p['id']}.jpg"}
                   for p in usable_photos(obs)],
    }


def strip(data: bytes) -> bytes:
    """The photo re-encoded as JPEG with its EXIF orientation applied and no metadata (no location)."""
    from PIL import Image, ImageOps

    with Image.open(io.BytesIO(data)) as im:
        out = ImageOps.exif_transpose(im).convert("RGB")
    buf = io.BytesIO()
    out.save(buf, "JPEG", quality=95)
    return buf.getvalue()


def download(photo: dict, photos: Path) -> None:
    dest = photos / photo["file"]
    if dest.exists():
        return
    with urllib.request.urlopen(urllib.request.Request(photo["url"], headers=HEADERS), timeout=120) as r:
        data = r.read()
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(".partial")
    tmp.write_bytes(strip(data))
    tmp.rename(dest)


def review_entry(cand: dict, target: dict) -> dict:
    return {"obs": cand["obs"], "keep": False, "taxon": f"{cand['taxon']} ({cand['quality']})",
            "species_id": target["species_id"], "problems": list(target["problems"]),
            "outcome": target["outcome"], "confidence": "probable",
            "images": {p["id"]: "skip" for p in cand["photos"]}}


def merge_review(existing: list[dict], new: list[dict]) -> list[dict]:
    """The existing entries, edits kept, then the new observations."""
    seen = {e["obs"] for e in existing}
    return existing + [e for e in new if e["obs"] not in seen]


def split_of(plant_key: str) -> str:
    h = int(hashlib.sha256(f"eval_split|{plant_key}".encode()).hexdigest()[:16], 16) / 16**16
    return "dev" if h < DEV_SHARE else "test"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def case(entry: dict, cand: dict, photos: Path, labeller: str) -> dict:
    """A manifest case (evaluation.md §3) from a kept review entry; ValueError when it isn't one."""
    where = f"observation {entry['obs']}"
    if entry.get("outcome") not in OUTCOMES:
        raise ValueError(f"{where}: outcome must be one of {', '.join(OUTCOMES)}")
    if entry.get("confidence") not in ("sure", "probable"):
        raise ValueError(f"{where}: confidence must be sure or probable")
    if entry.get("split") not in (None, "dev", "test"):
        raise ValueError(f"{where}: split must be dev or test")
    if entry["outcome"] == "diagnosed" and not entry.get("problems"):
        raise ValueError(f"{where}: a diagnosed case needs its problems")
    kinds = {int(k): v for k, v in (entry.get("images") or {}).items()}
    used = []
    for p in cand["photos"]:
        kind = kinds.get(p["id"], "skip")
        if kind == "skip":
            continue
        if kind not in KINDS:
            raise ValueError(f"{where}: photo {p['id']} has kind {kind!r}; use {', '.join(KINDS)} or skip")
        used.append((p, kind))
    for kind, most in KINDS.items():
        n = sum(1 for _, k in used if k == kind)
        if n > most:
            raise ValueError(f"{where}: {n} {kind} photos, at most {most}")
    if sum(1 for _, k in used if k == "whole_plant") != 1:
        raise ValueError(f"{where}: a scan needs exactly one whole_plant photo")
    order = list(KINDS)
    used.sort(key=lambda pk: order.index(pk[1]))
    plant_key = f"inat-{entry['obs']}"
    return {
        "case_id": f"{entry.get('species_id') or 'unknown'}-inat-{entry['obs']}",
        "split": entry.get("split") or split_of(plant_key),
        "plant_key": plant_key,
        "images": [{"file": p["file"], "sha256": sha256(photos / p["file"]), "kind": k} for p, k in used],
        "truth": {"outcome": entry["outcome"], "species_id": entry.get("species_id"),
                  "problems": list(entry.get("problems") or []), "organisms": [], "damage": []},
        "label_basis": "source_caption",
        "label_confidence": entry["confidence"],
        "source": cand["url"],
        "licence": " and ".join(sorted({p["licence"] for p, _ in used})),
        "credit": "; ".join(sorted({p["credit"] for p, _ in used})),
        "labelled_by": labeller,
        "reviewed_by": "",
    }


def contact_sheet(cands: list[dict], targets: list[dict]) -> str:
    notes = {t["taxon"]: t for t in targets}
    parts = ["<!doctype html><meta charset=utf-8><title>iNaturalist candidates</title><style>"
             "body{font:14px system-ui;margin:16px}img{height:220px;margin:4px;vertical-align:top}"
             "figure{display:inline-block;margin:0 8px 8px 0}figcaption{font:12px monospace}"
             ".obs{border-top:1px solid #ccc;padding:8px 0}</style>",
             "<h1>iNaturalist candidates</h1><p>Fill in <code>review.yaml</code> beside this page, "
             "then run <code>python -m ml.training.eval.inat add --labeller &lt;login&gt;</code>.</p>"]
    for target in targets:
        group = [c for c in cands if c["target"] == target["taxon"]]
        parts.append(f"<h2>{html.escape(target['taxon'])} ({len(group)})</h2>")
        if note := notes[target["taxon"]].get("note"):
            parts.append(f"<p><b>Note:</b> {html.escape(note)}</p>")
        for c in group:
            figs = "".join(f"<figure><a href='{html.escape(Path(p['file']).name)}'><img src='{html.escape(Path(p['file']).name)}' loading=lazy></a>"
                           f"<figcaption>{p['id']} · {p['licence']}</figcaption></figure>" for p in c["photos"])
            parts.append(f"<div class=obs><a href='{html.escape(c['url'])}'>observation {c['obs']}</a> · "
                         f"{html.escape(c['taxon'])} · {c['quality']} · {c.get('observed_on') or ''} · "
                         f"{html.escape(c['photos'][0]['credit'])}<br>{figs}</div>")
    return "\n".join(parts) + "\n"


def load_targets(only: list[str] | None) -> list[dict]:
    targets = yaml.safe_load(TARGETS.read_text())["targets"]
    if only:
        targets = [t for t in targets if t["taxon"] in only]
        if missing := set(only) - {t["taxon"] for t in targets}:
            raise SystemExit(f"not in {TARGETS.name}: {', '.join(sorted(missing))}")
    return targets


def fetch(args) -> int:
    if args.since < EARLIEST:
        raise SystemExit(f"--since {args.since}: only observations from {EARLIEST} on (see the module docstring)")
    targets = load_targets(args.only)
    inat = args.photos / "inat"
    cands_path, review_path = inat / "candidates.json", inat / "review.yaml"
    cands = json.loads(cands_path.read_text()) if cands_path.exists() else []
    known = {c["obs"] for c in cands}
    new_entries = []
    for target in targets:
        tid = taxon_id(target["taxon"], target.get("rank"))
        found, total = search(target, tid, args.since, args.per_target)
        print(f"{target['taxon']}: {len(found)} matching observations with CC0 or CC BY photos "
              f"(of {total} from iNaturalist's filter)", flush=True)
        if args.count:
            continue
        for obs in found:
            c = candidate(obs, target)
            if c["obs"] not in known:
                cands.append(c)
                known.add(c["obs"])
            new_entries.append(review_entry(c, target))
    if args.count:
        return 0
    photos = [p for c in cands for p in c["photos"]]
    with ThreadPoolExecutor(4) as pool:
        list(pool.map(lambda p: download(p, args.photos), photos))
    inat.mkdir(parents=True, exist_ok=True)
    cands_path.write_text(json.dumps(cands, indent=1) + "\n")
    all_targets = load_targets(None)
    (inat / "index.html").write_text(contact_sheet(cands, [t for t in all_targets
                                                          if any(c["target"] == t["taxon"] for c in cands)]))
    existing = yaml.safe_load(review_path.read_text()) if review_path.exists() else []
    review = merge_review(existing or [], new_entries)
    review_path.write_text(REVIEW_HEADER + yaml.safe_dump(review, sort_keys=False, allow_unicode=True))
    print(f"{len(cands)} observations, {len(photos)} photos in {inat}; review {review_path}, "
          f"contact sheet {inat / 'index.html'}")
    return 0


def add(args) -> int:
    inat = args.photos / "inat"
    cands = {c["obs"]: c for c in json.loads((inat / "candidates.json").read_text())}
    review = yaml.safe_load((inat / "review.yaml").read_text()) or []
    lines = args.manifest.read_text().splitlines() if args.manifest.exists() else []
    have = {json.loads(line)["case_id"] for line in lines if line.strip()}
    added, problems = [], []
    for entry in review:
        if not entry.get("keep"):
            continue
        try:
            c = case(entry, cands[entry["obs"]], args.photos, args.labeller)
        except (ValueError, KeyError) as e:
            problems.append(str(e))
            continue
        if c["case_id"] not in have:
            added.append(c)
            have.add(c["case_id"])
    if added:
        with args.manifest.open("a") as f:
            for c in added:
                f.write(json.dumps(c, ensure_ascii=False) + "\n")
    print(f"{len(added)} cases added to {args.manifest} "
          f"({sum(c['split'] == 'dev' for c in added)} dev, {sum(c['split'] == 'test' for c in added)} test)")
    for p in problems:
        print(f"not added: {p}", file=sys.stderr)
    return 1 if problems else 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch", help="search, download and write the review files")
    f.add_argument("--per-target", type=int, default=20, help="observations per target")
    f.add_argument("--only", action="append", help="a target's taxon; repeatable")
    f.add_argument("--since", default=EARLIEST, help=f"observed on or after (not before {EARLIEST})")
    f.add_argument("--count", action="store_true", help="only print how many observations match")
    f.add_argument("--photos", type=Path, default=PHOTOS)
    a = sub.add_parser("add", help="append the kept observations to manifest.jsonl")
    a.add_argument("--labeller", required=True, help="your GitHub login")
    a.add_argument("--photos", type=Path, default=PHOTOS)
    a.add_argument("--manifest", type=Path, default=MANIFEST)
    args = ap.parse_args(argv)
    return fetch(args) if args.command == "fetch" else add(args)


if __name__ == "__main__":
    sys.exit(main())
