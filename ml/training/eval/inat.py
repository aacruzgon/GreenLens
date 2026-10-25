"""Licensed iNaturalist photos for the evaluation set (evaluation.md §2, "Where the photos come from").

    python -m ml.training.eval.inat fetch [--per-target 20] [--only TAXON] [--count]
    python -m ml.training.eval.inat refetch
    python -m ml.training.eval.inat add --labeller NAME

The team's own photos are the core of the evaluation set. Licensed photos fill the groups we can't
find locally, such as diseased crops in late October. Three rules keep the scores meaningful:

- Only CC0 and CC BY photos, with each photo's licence and credit recorded, the credit saying the
  photo was re-encoded without metadata. iNaturalist's default licence, CC BY-NC, is left out.
- Only observations from 2026 on (`--since`). BioCLIP 2's training data, TreeOfLife-200M, was
  compiled before 2026 and draws on GBIF, which holds iNaturalist's licensed observations
  *unverified*; PlantDoc dates from 2019. Never use a photo from a web search: PlantDoc was scraped
  from Google Images.
- Each target's taxon is matched exactly, since iNaturalist's filter also returns related taxa, and
  every observation is reviewed before it becomes a case: by a teammate, or by Claude's screen for
  M10's cases of 2026-10-24, since no one on the team can judge plant photos (those are `probable`,
  `labelled_by` claude-opus-5-5; evaluation.md §3).

A target with a `host` finds organisms whose observer recorded the plant they were on, in the
observation fields `Host`, `Host Plant ID` or `Eating` (HOST_FIELDS), so a pest case's plant is
known rather than guessed from the photo; the reviewer still checks that the plant shows. A case
from a `pests` target is marked `organisms_unboxed` until its animals' boxes are drawn, so a
healthy plant with a pest on it isn't scored as a plain healthy one (vlm_bench).

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
`reviewed_by` until a second teammate reviews it (evaluation.md §3). New cases go to dev: M18 scored
the test split once on 2026-10-23, and it is never added to or scored again (`new_split`). A case
of a plant the manifest already has in test is refused, so a plant never spans the splits.
Observations that may show one plant share a `plant` key, set in the review (evaluation.md §2,
Splits); an entry with `plant` can't also set `split`. Organism and damage boxes are drawn later,
in CVAT or Label Studio. The species head's iNaturalist training photos (training.md §6) must leave
out every observation listed in the manifest.
"""

from __future__ import annotations

import argparse
import datetime
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
EARLIEST = datetime.date(2026, 1, 1)
# iNaturalist's licence codes we may use. Its licences are version 4.0: its help page links
# CC BY to creativecommons.org/licenses/by/4.0 (read 2026-10-19).
LICENCES = {"cc0": "CC0 1.0", "cc-by": "CC BY 4.0"}
CHANGES = "re-encoded, metadata removed"  # CC BY asks that changes be indicated
KINDS = {"whole_plant": 1, "leaf_closeup": 2, "leaf_underside": 1, "soil_surface": 1, "flower_fruit": 1}
OUTCOMES = ("diagnosed", "healthy", "no_plant_found", "photos_unusable", "rejected_at_upload")
# Abiotic problems that mimic disease (evaluation.md §2, look-alikes). The knowledge base names none
# of them, so a look-alike case is `diagnosed` with no problems and its `look_alike`: the scan
# should end without a `confirmed` disease.
LOOK_ALIKES = ("nitrogen_deficiency", "phosphorus_deficiency", "potassium_deficiency", "magnesium_deficiency",
               "nutrient_deficiency", "hail_damage", "sunscald", "frost_damage", "herbicide_injury",
               "overwatering", "salt_burn")
LIFE_STAGES = {"larva": (1, 6)}  # iNaturalist's Life Stage annotation: term 1, value 6
# A target's `quality`: research grade only; also needs_id; or also casual, which a garden plant
# marked as cultivated always is (iNaturalist's quality grades, read 2026-10-24).
QUALITY = {"research": "research", "any": "research,needs_id", "all": "research,needs_id,casual"}
# Observation fields that name the plant an organism was on, each a taxon (✓ ran 2026-10-24: 42
# observations from 2026 with CC0 or CC BY photos name tomato in one of them, 120 grape, 200 rose).
HOST_FIELDS = ("Host", "Host Plant ID", "Eating")
DEV_SHARE = 0.6
PAGE = 200
REVIEW_HEADER = """\
# Review of the iNaturalist candidates (ml/training/eval/inat.py). For each observation to use:
#   keep: true
#   species_id, problems, outcome: the truth; check the host plant on the photos
#   confidence: probable, or sure when research grade and the signs are clear
#   images: photo id -> whole_plant (exactly 1), leaf_closeup (at most 2), leaf_underside,
#           soil_surface, flower_fruit (at most 1 each), or skip
#   plant: optional, the key of observations that may show the same plant (one observer, one crop,
#          within 100 m), so they share a split; the lowest observation's, e.g. inat-401728510
#   split: optional, and only dev: the test split is frozen since M18 (2026-10-23); not with plant
#   organisms_unboxed: set on a pests target's entries: an animal shows, its box isn't drawn yet
# Skip any photo that shows a person's face: faces aren't blurred on import yet (evaluation.md §2).
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


def target_key(target: dict) -> str:
    """A target's name in the review and on the contact sheet: its taxon, and its host if any."""
    return f"{target['taxon']} on {target['host']}" if target.get("host") else target["taxon"]


def query(target: dict, tid: int, since: str, host: tuple[str, int] | None = None) -> dict:
    """The search for one target; `host`: an observation field and the host plant's taxon id."""
    quality = target.get("quality", "research")
    if quality not in QUALITY:
        raise SystemExit(f"{target_key(target)}: quality must be one of {', '.join(QUALITY)}")
    q = {"taxon_id": tid, "d1": since, "photo_license": ",".join(LICENCES), "photos": "true",
         "quality_grade": QUALITY[quality], "per_page": PAGE, "order_by": "created_at", "order": "desc"}
    if stage := target.get("life_stage"):
        q["term_id"], q["term_value_id"] = LIFE_STAGES[stage]
    if host:
        q[f"field:{host[0]}"] = host[1]
    return q


def search(target: dict, tid: int, since: str, limit: int, max_pages: int = 5,
           host_tid: int | None = None) -> tuple[list[dict], int]:
    """Up to `limit` matching observations with a usable photo, and the API's total before matching.
    With `host_tid`, each of HOST_FIELDS is searched in turn, and an observation counts once."""
    found, seen, total = [], set(), 0
    for host in [(f, host_tid) for f in HOST_FIELDS] if host_tid else [None]:
        for page in range(1, max_pages + 1):
            res = api("observations", page=page, **query(target, tid, since, host))
            for obs in res["results"]:
                if obs["id"] not in seen and matches(obs, tid, target.get("match", "exact")) and usable_photos(obs):
                    seen.add(obs["id"])
                    found.append(obs)
                    if len(found) == limit:
                        return found, total + res["total_results"]
            if page * PAGE >= res["total_results"]:
                break
        total += res["total_results"]
    return found, total


def original_url(url: str) -> str:
    """The full-size photo's URL from the API's thumbnail URL; ValueError when it has no size part,
    so a changed API can't quietly give 75 px thumbnails."""
    if "/square." not in url:
        raise ValueError(f"no /square. in photo URL {url}")
    return url.replace("/square.", "/original.")


def candidate(obs: dict, target: dict) -> dict:
    photos = []
    for p in usable_photos(obs):
        try:
            url = original_url(p["url"])
        except ValueError as e:
            print(f"observation {obs['id']}: skipped a photo: {e}", file=sys.stderr)
            continue
        photos.append({"id": p["id"], "licence": LICENCES[p["license_code"]],
                       "credit": f"{p['attribution']}; {CHANGES}", "url": url,
                       "file": f"inat/{obs['id']}-{p['id']}.jpg"})
    return {
        "obs": obs["id"], "url": obs["uri"], "taxon": obs["taxon"]["name"], "quality": obs["quality_grade"],
        "observed_on": obs.get("observed_on"), "target": target_key(target), "photos": photos,
    }


def strip(data: bytes, keep_icc: bool = True) -> bytes:
    """The photo re-encoded as JPEG with its EXIF orientation applied and no metadata (no location),
    keeping only its colour profile. M10's 107 cases were encoded before the profile was kept
    (b047a4a6), so their manifest SHA-256s are of the `keep_icc=False` encoding; only `refetch` uses it."""
    from PIL import Image, ImageOps

    with Image.open(io.BytesIO(data)) as im:
        icc = im.info.get("icc_profile") if keep_icc else None   # kept: a Display P3 photo's colours depend on it
        out = ImageOps.exif_transpose(im).convert("RGB")
    buf = io.BytesIO()
    out.save(buf, "JPEG", quality=95, **({"icc_profile": icc} if icc else {}))
    return buf.getvalue()


def download(photo: dict, photos: Path, keep_icc: bool = True) -> bool:
    """Whether the photo is on disk; a failed download is reported and skipped, so one photo
    that's gone from iNaturalist doesn't block every rerun."""
    dest = photos / photo["file"]
    if dest.exists():
        return True
    try:
        with urllib.request.urlopen(urllib.request.Request(photo["url"], headers=HEADERS), timeout=120) as r:
            data = r.read()
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(".partial")
        tmp.write_bytes(strip(data, keep_icc))
        tmp.rename(dest)
        return True
    except Exception as e:  # network errors, a 404, a file PIL can't read
        print(f"photo {photo['id']}: download failed, skipped: {e}", file=sys.stderr)
        return False


def review_entry(cand: dict, target: dict) -> dict:
    entry = {"obs": cand["obs"], "keep": False, "taxon": f"{cand['taxon']} ({cand['quality']})",
             "species_id": target["species_id"], "problems": list(target["problems"]),
             "outcome": target["outcome"], "confidence": "probable",
             "images": {p["id"]: "skip" for p in cand["photos"]}}
    if target.get("look_alike"):
        entry["look_alike"] = target["look_alike"]
    if target.get("group") == "pests":
        entry["organisms_unboxed"] = True
    return entry


def merge_review(existing: list[dict], new: list[dict]) -> list[dict]:
    """The existing entries, edits kept, then the new observations."""
    seen = {e["obs"] for e in existing}
    return existing + [e for e in new if e["obs"] not in seen]


def split_of(plant_key: str) -> str:
    """The split the first cases took by plant, about 60% dev. New cases take `new_split`'s."""
    h = int(hashlib.sha256(f"eval_split|{plant_key}".encode()).hexdigest()[:16], 16) / 16**16
    return "dev" if h < DEV_SHARE else "test"


def check_truth(outcome, problems, look_alike, where: str) -> None:
    """A case's outcome, problems and look-alike agree (evaluation.md §3); ValueError otherwise."""
    if outcome not in OUTCOMES:
        raise ValueError(f"{where}: outcome must be one of {', '.join(OUTCOMES)}")
    if look_alike is not None:
        if look_alike not in LOOK_ALIKES:
            raise ValueError(f"{where}: look_alike must be one of {', '.join(LOOK_ALIKES)}")
        if outcome != "diagnosed" or problems:
            raise ValueError(f"{where}: a look-alike is diagnosed with no problems: the knowledge base names none")
    elif outcome == "diagnosed" and not problems:
        raise ValueError(f"{where}: a diagnosed case needs its problems, or a look_alike")


def truth_of(entry: dict) -> dict:
    """A scan case's truth from a review entry or a class (evaluation.md §3)."""
    truth = {"outcome": entry["outcome"], "species_id": entry.get("species_id"),
             "problems": list(entry.get("problems") or []), "organisms": [], "damage": []}
    if entry.get("look_alike") is not None:
        truth["look_alike"] = entry["look_alike"]
    if entry.get("organisms_unboxed"):
        truth["organisms_unboxed"] = True   # an animal shows; its box isn't drawn yet
    return truth


def plant_splits(manifest: Path) -> dict[str, str]:
    """Each plant key in the manifest and its split."""
    if not manifest.exists():
        return {}
    return {(c := json.loads(line))["plant_key"]: c["split"]
            for line in manifest.read_text().splitlines() if line.strip()}


def new_split(plant_key: str, splits: dict[str, str], asked: str | None = None) -> str:
    """A new case's split: dev. M18 scored the test split once on 2026-10-23 (evaluation.md §4),
    so nothing joins it; a case of a plant already in test is refused, as a plant never spans the
    splits. ValueError otherwise."""
    if asked not in (None, "dev"):
        raise ValueError(f"split must be dev: the test split is frozen since M18 (2026-10-23), not {asked!r}")
    if splits.get(plant_key) == "test":
        raise ValueError(f"plant {plant_key} is in the test split, which M18 scored once; leave this case out")
    return "dev"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def case(entry: dict, cand: dict, photos: Path, labeller: str, splits: dict[str, str] | None = None) -> dict:
    """A manifest case (evaluation.md §3) from a kept review entry; ValueError when it isn't one.
    `splits`: the manifest's plant keys and their splits (`plant_splits`)."""
    where = f"observation {entry['obs']}"
    check_truth(entry.get("outcome"), entry.get("problems"), entry.get("look_alike"), where)
    if entry.get("confidence") not in ("sure", "probable"):
        raise ValueError(f"{where}: confidence must be sure or probable")
    plant = entry.get("plant")
    if plant is not None and not (isinstance(plant, str) and plant.startswith("inat-")):
        raise ValueError(f"{where}: plant must be a key like inat-401728510, quoted if need be")
    if plant is not None and entry.get("split") is not None:
        raise ValueError(f"{where}: set plant or split, not both: a plant's split follows its key")
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
    plant_key = plant or f"inat-{entry['obs']}"
    try:
        split = new_split(plant_key, splits or {}, entry.get("split"))
    except ValueError as e:
        raise ValueError(f"{where}: {e}") from None
    return {
        "case_id": f"{entry.get('species_id') or 'unknown'}-inat-{entry['obs']}",
        "split": split,
        "plant_key": plant_key,
        "images": [{"file": p["file"], "sha256": sha256(photos / p["file"]), "kind": k} for p, k in used],
        "truth": truth_of(entry),
        "label_basis": "source_caption",
        "label_confidence": entry["confidence"],
        "source": cand["url"],
        "licence": " and ".join(sorted({p["licence"] for p, _ in used})),
        "credit": "; ".join(sorted({p["credit"] for p, _ in used})),
        "labelled_by": labeller,
        "reviewed_by": "",
    }


def contact_sheet(cands: list[dict], targets: list[dict]) -> str:
    notes = {target_key(t): t for t in targets}
    parts = ["<!doctype html><meta charset=utf-8><title>iNaturalist candidates</title><style>"
             "body{font:14px system-ui;margin:16px}img{height:220px;margin:4px;vertical-align:top}"
             "figure{display:inline-block;margin:0 8px 8px 0}figcaption{font:12px monospace}"
             ".obs{border-top:1px solid #ccc;padding:8px 0}</style>",
             "<h1>iNaturalist candidates</h1><p>Fill in <code>review.yaml</code> beside this page, "
             "then run <code>python -m ml.training.eval.inat add --labeller &lt;login&gt;</code>.</p>"]
    for target in targets:
        key = target_key(target)
        group = [c for c in cands if c["target"] == key]
        parts.append(f"<h2>{html.escape(key)} ({len(group)})</h2>")
        if note := notes[key].get("note"):
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
        targets = [t for t in targets if t["taxon"] in only or target_key(t) in only]
        if missing := set(only) - {n for t in targets for n in (t["taxon"], target_key(t))}:
            raise SystemExit(f"not in {TARGETS.name}: {', '.join(sorted(missing))}")
    return targets


def since(value: str) -> datetime.date:
    """`--since` as a date, refused before EARLIEST or when it isn't YYYY-MM-DD."""
    try:
        day = datetime.date.fromisoformat(value)
    except ValueError:
        raise SystemExit(f"--since {value}: not a date in YYYY-MM-DD form") from None
    if day < EARLIEST:
        raise SystemExit(f"--since {value}: only observations from {EARLIEST} on (see the module docstring)")
    return day


def fetch(args) -> int:
    args.since = since(args.since).isoformat()
    targets = load_targets(args.only)
    inat = args.photos / "inat"
    cands_path, review_path = inat / "candidates.json", inat / "review.yaml"
    cands = json.loads(cands_path.read_text()) if cands_path.exists() else []
    known = {c["obs"]: i for i, c in enumerate(cands)}
    new_entries = []
    for target in targets:
        tid = taxon_id(target["taxon"], target.get("rank"))
        host_tid = taxon_id(target["host"], target.get("host_rank")) if target.get("host") else None
        found, total = search(target, tid, args.since, target.get("per_target", args.per_target), host_tid=host_tid)
        print(f"{target_key(target)}: {len(found)} matching observations with CC0 or CC BY photos "
              f"(of {total} from iNaturalist's filter)", flush=True)
        if args.count:
            continue
        for obs in found:
            c = candidate(obs, target)
            if c["obs"] in known:
                cands[known[c["obs"]]] = c  # found again: refresh its licences and credits
            else:
                known[c["obs"]] = len(cands)
                cands.append(c)
            new_entries.append(review_entry(c, target))
    if args.count:
        return 0
    photos = [p for c in cands for p in c["photos"]]
    with ThreadPoolExecutor(4) as pool:
        ok = dict(zip((p["file"] for p in photos), pool.map(lambda p: download(p, args.photos), photos)))
    for c in cands:
        c["photos"] = [p for p in c["photos"] if ok[p["file"]]]
    cands = [c for c in cands if c["photos"]]
    photos = [p for c in cands for p in c["photos"]]
    inat.mkdir(parents=True, exist_ok=True)
    cands_path.write_text(json.dumps(cands, indent=1) + "\n")
    all_targets = load_targets(None)
    (inat / "index.html").write_text(contact_sheet(cands, [t for t in all_targets
                                                          if any(c["target"] == target_key(t) for c in cands)]))
    existing = yaml.safe_load(review_path.read_text()) if review_path.exists() else []
    review = merge_review(existing or [], new_entries)
    review_path.write_text(REVIEW_HEADER + yaml.safe_dump(review, sort_keys=False, allow_unicode=True))
    print(f"{len(cands)} observations, {len(photos)} photos in {inat}; review {review_path}, "
          f"contact sheet {inat / 'index.html'}")
    return 0


def manifest_files(manifest: Path, prefix: str, photos: Path) -> dict[str, str]:
    """The dev cases' image files under `prefix` (e.g. "inat/") that aren't under `photos` or have
    another SHA-256, with the manifest's, for `refetch`: the harness reads dev only, and the test split
    is not touched."""
    out = {}
    for line in manifest.read_text().splitlines():
        if line.strip() and (c := json.loads(line))["split"] == "dev":
            for im in c["images"]:
                f = photos / im["file"]
                if im["file"].startswith(prefix) and not (f.is_file() and sha256(f) == im["sha256"]):
                    out[im["file"]] = im["sha256"]
    return out


def refetch(args) -> int:
    """Downloads again the photos of cases already in the manifest and not on disk, with no new
    search and no review: the observation's photos by id from the API, at the original size, re-encoded
    as `fetch` does. Each is checked against the manifest's SHA-256, and a photo that differs is encoded
    again as before `strip` kept the colour profile; one that still differs is left in place (the
    harness refuses it) and listed."""
    want = manifest_files(args.manifest, "inat/", args.photos)
    for f in want:
        (args.photos / f).unlink(missing_ok=True)   # one of another SHA-256 is downloaded again
    by_obs: dict[int, dict[int, str]] = {}
    for f in want:
        obs, photo = Path(f).stem.split("-")
        by_obs.setdefault(int(obs), {})[int(photo)] = f
    urls: dict[str, str] = {}
    ids = sorted(by_obs)
    for i in range(0, len(ids), 100):
        chunk = ids[i : i + 100]
        res = api("observations/" + ",".join(map(str, chunk)), per_page=len(chunk))
        for obs in res["results"]:
            for p in obs.get("photos") or []:
                if f := by_obs.get(obs["id"], {}).get(p["id"]):
                    try:
                        urls[f] = original_url(p["url"])
                    except ValueError as e:
                        print(f"{f}: {e}", file=sys.stderr)
    todo = [{"id": f, "file": f, "url": urls[f]} for f in want if f in urls]
    with ThreadPoolExecutor(4) as pool:
        ok = list(pool.map(lambda p: download(p, args.photos), todo))
    got = [p["file"] for p, k in zip(todo, ok) if k]
    for f in [f for f in got if sha256(args.photos / f) != want[f]]:   # encoded before the profile was kept?
        (args.photos / f).unlink()
        download(next(p for p in todo if p["file"] == f), args.photos, keep_icc=False)
    differ = [f for f in got if (args.photos / f).is_file() and sha256(args.photos / f) != want[f]]
    got = [f for f in got if (args.photos / f).is_file()]
    print(f"{len(want)} inat photos of the manifest's missing: {len(got)} downloaded, "
          f"{len(want) - len(got)} not (gone from iNaturalist or failed), {len(differ)} with another SHA-256")
    for f in differ:
        print(f"SHA-256 differs from the manifest's: {f}", file=sys.stderr)
    for f in want:
        if f not in got:
            print(f"not fetched: {f}", file=sys.stderr)
    return 1 if differ or len(got) < len(want) else 0


def add(args) -> int:
    inat = args.photos / "inat"
    cands = {c["obs"]: c for c in json.loads((inat / "candidates.json").read_text())}
    review = yaml.safe_load((inat / "review.yaml").read_text()) or []
    lines = args.manifest.read_text().splitlines() if args.manifest.exists() else []
    # By observation, not case_id: a species corrected after an `add` changes the case_id.
    have = {json.loads(line)["source"] for line in lines if line.strip()}
    splits = plant_splits(args.manifest)
    added, problems = [], []
    for entry in review:
        if not entry.get("keep"):
            continue
        try:
            c = case(entry, cands[entry["obs"]], args.photos, args.labeller, splits)
        except (ValueError, KeyError) as e:
            problems.append(str(e))
            continue
        if c["source"] not in have:
            added.append(c)
            have.add(c["source"])
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
    f.add_argument("--since", default=EARLIEST.isoformat(), help=f"observed on or after (not before {EARLIEST})")
    f.add_argument("--count", action="store_true", help="only print how many observations match")
    f.add_argument("--photos", type=Path, default=PHOTOS)
    r = sub.add_parser("refetch", help="download again the photos of cases already in the manifest")
    r.add_argument("--photos", type=Path, default=PHOTOS)
    r.add_argument("--manifest", type=Path, default=MANIFEST)
    a = sub.add_parser("add", help="append the kept observations to manifest.jsonl")
    a.add_argument("--labeller", required=True, help="your GitHub login")
    a.add_argument("--photos", type=Path, default=PHOTOS)
    a.add_argument("--manifest", type=Path, default=MANIFEST)
    args = ap.parse_args(argv)
    return {"fetch": fetch, "refetch": refetch, "add": add}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
