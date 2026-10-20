"""Evaluation cases from published field datasets that no model trained on (evaluation.md §2,
"Where the photos come from").

    python -m ml.training.eval.fieldsets fetch [--per-class 8] [--only SET]
    python -m ml.training.eval.fieldsets add --labeller NAME

The team can't photograph tomato or corn diseases in a Toronto October, and iNaturalist has almost
none from 2026 (inat.py). Researchers publish field photos of them under CC BY 4.0, and the sets in
`fieldsets.yaml` are none of our training sources. Four rules keep the scores meaningful:

- Never a training photo, nor a copy of one. A published set can hold another's photos
  (Tomato-Pakistan held 302 of PlantDoc's), so every pick is embedded with BioCLIP 2 and dropped
  when it is within cosine COPY of any image in our training sources, whatever its split.
- Only originals, one per leaf. The YAML lists only raw folders. These sets photograph each leaf
  several times, turned or flipped, and those repeats sit at cosine 0.93-0.97 (checked by eye on
  2026-10-19), below the training splits' 0.97. So picks within cosine SAME_PLANT (0.93) of each
  other count as one leaf, of which only the first is kept.
- One photo per case. A dataset photo shows a leaf or a few, so the case sends it as the
  `whole_plant` shot, as a user who only photographs the sick leaf would (the API needs one). The
  harness reports these cases as their own slice, by `source`.
- Reviewed before use, as in inat.py: a teammate marks each photo `sure` (the signs are clear),
  `probable` or `skip`. Only `sure` cases count toward diagnosis accuracy (evaluation.md §3).

`fetch` lists each class folder through Mendeley Data's API and takes photos in a stable hash
order: three times --per-class of them are downloaded, checked against the SHA-256 Mendeley
publishes and re-encoded with no metadata. Copies and repeats are dropped, and --per-class photos
per class are kept. It writes photos/fieldsets/ (git-ignored): the photos, candidates.json, a
contact sheet index.html, review.yaml, and copies.md, which lists every dropped copy beside the
training image it matches. A rerun keeps the review's edits.

`add` appends the reviewed photos to manifest.jsonl. Each case gets `label_basis`
`source_caption`, the dataset's URL with the file's path as `source`, the set's licence and its
citation with the changes made, and an empty `reviewed_by` until a second teammate checks it.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import re
import sys
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import yaml

from ml.training.eval import inat

HERE = Path(__file__).resolve().parent
SETS = HERE / "fieldsets.yaml"
SOURCES = HERE.parent / "data" / "sources.yaml"
RAW = HERE.parent / "data" / "raw"
IMAGE_EXT = {".jpg", ".jpeg", ".png"}
COPY = 0.95        # cosine to a training image at or above which a pick is a copy
SAME_PLANT = 0.93  # repeats of one leaf measured 0.93-0.97 in these sets; the training splits use 0.97
POOL = 5           # photos downloaded per photo kept, to make up for the repeats and copies dropped
VERDICTS = ("sure", "probable", "skip")
REVIEW_HEADER = """\
# Review of the field-dataset photos (ml/training/eval/fieldsets.py). Open index.html beside this
# file, and for each photo write:
#   sure       the photo clearly shows the class's signs (counts toward diagnosis accuracy)
#   probable   it probably does
#   skip       wrong class, unclear, another problem as well, or a person's face in it
# A photo of something else entirely is a skip, not a relabel. Then:
#   python -m ml.training.eval.fieldsets add --labeller <your GitHub login>
"""


def slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def photo_id(set_name: str, folder: str, path: str) -> str:
    """`<set>/<class>/<file stem>`, the key in review.yaml."""
    return f"{set_name}/{slug(folder.rsplit('/', 1)[-1])}/{slug(Path(path).stem)}"


def pick_order(set_name: str, rows: list[dict]) -> list[dict]:
    """The folder's photos in a stable order that doesn't follow the dataset's own numbering."""
    images = [r for r in rows if Path(r["path"]).suffix.lower() in IMAGE_EXT]
    return sorted(images, key=lambda r: hashlib.sha256(f"eval_pick|{set_name}|{r['path']}".encode()).hexdigest())


def normalise(emb: np.ndarray) -> np.ndarray:
    return emb / np.maximum(np.linalg.norm(emb, axis=1, keepdims=True), 1e-12)


def select(groups: list[list[int]], emb: np.ndarray, train: np.ndarray, per_class: int,
           copy: float = COPY, same: float = SAME_PLANT) -> tuple[list[int], dict[int, tuple[int, float]]]:
    """Which candidates to keep, and the copies dropped (candidate -> nearest training image, cosine).

    `groups` holds each class's candidate indices in pick order. A candidate is dropped when it is
    a copy of a training image, or the same plant as a candidate already kept (in any class)."""
    emb, train = normalise(emb), normalise(train)
    nearest = np.zeros(len(emb), int)
    best = np.full(len(emb), -1.0)
    for s in range(0, len(train), 8192):
        sims = emb @ train[s : s + 8192].T
        j = sims.argmax(axis=1)
        better = sims[np.arange(len(emb)), j] > best
        best[better] = sims[np.arange(len(emb)), j][better]
        nearest[better] = j[better] + s
    copies = {i: (int(nearest[i]), float(best[i])) for i in range(len(emb)) if best[i] >= copy}
    kept: list[int] = []
    for group in groups:
        n = 0
        for i in group:
            if n == per_class:
                break
            if i in copies or any(float(emb[i] @ emb[k]) >= same for k in kept):
                continue
            kept.append(i)
            n += 1
    return kept, copies


def training_images() -> list[Path]:
    """Every image in our training sources (sources.yaml), whatever its split or mapping."""
    names = [d["name"] for d in yaml.safe_load(SOURCES.read_text())["datasets"] if not d["name"].startswith("coco")]
    files = []
    for name in names:
        root = RAW / name
        if not root.is_dir():
            raise SystemExit(f"{root} is missing: fetch the training sources first (ml.training.data.fetch)")
        files += sorted(p for p in root.rglob("*") if p.suffix.lower() in IMAGE_EXT | {".webp", ".bmp"})
    return files


def download(row: dict, dest: Path) -> bool:
    """The file at Mendeley, checked against its published SHA-256, re-encoded with no metadata."""
    from ml.training.data.fetch import UA

    if dest.exists():
        return True
    try:
        req = urllib.request.Request(row["url"], headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=120) as r:
            data = r.read()
        if hashlib.sha256(data).hexdigest() != row["sha256"]:
            raise ValueError("SHA-256 doesn't match Mendeley's")
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(".partial")
        tmp.write_bytes(inat.strip(data))
        tmp.rename(dest)
        return True
    except Exception as e:  # network errors, a file PIL can't read, a hash mismatch
        print(f"{row['path']}: download failed, skipped: {e}", file=sys.stderr)
        return False


def source_url(s: dict, path: str) -> str:
    return f"https://data.mendeley.com/datasets/{s['dataset']}/{s['version']}#{urllib.parse.quote(path)}"


def review_block(s: dict, folder: str, ids: list[str]) -> dict:
    c = s["classes"][folder]
    return {"set": s["name"], "class": folder, "species_id": c["species_id"], "problems": list(c["problems"]),
            "outcome": c["outcome"], "photos": {i: "skip" for i in ids}}


def merge_review(existing: list[dict], new: list[dict]) -> tuple[list[dict], int]:
    """The new blocks with the verdicts already given to their photos, then the existing blocks of
    classes not fetched this time; and how many given verdicts were dropped with their photo."""
    verdicts = {pid: v for b in existing for pid, v in (b.get("photos") or {}).items()}
    fetched = {(b["set"], b["class"]) for b in new}
    out = [{**b, "photos": {pid: verdicts.get(pid, "skip") for pid in b["photos"]}} for b in new]
    out += [b for b in existing if (b["set"], b["class"]) not in fetched]
    kept = {pid for b in out for pid in b["photos"]}
    dropped = sum(1 for pid, v in verdicts.items() if v != "skip" and pid not in kept)
    return out, dropped


def case(block: dict, pid: str, verdict: str, cand: dict, s: dict, photos: Path, labeller: str) -> dict:
    """A manifest case (evaluation.md §3) for one reviewed photo; ValueError when it isn't one."""
    if verdict not in ("sure", "probable"):
        raise ValueError(f"{pid}: verdict must be one of {', '.join(VERDICTS)}")
    if block.get("outcome") not in inat.OUTCOMES:
        raise ValueError(f"{pid}: outcome must be one of {', '.join(inat.OUTCOMES)}")
    if block["outcome"] == "diagnosed" and not block.get("problems"):
        raise ValueError(f"{pid}: a diagnosed case needs its problems")
    plant_key = pid.replace("/", "-")
    return {
        "case_id": f"{block.get('species_id') or 'unknown'}-{plant_key}",
        "split": inat.split_of(plant_key),
        "plant_key": plant_key,
        "images": [{"file": cand["file"], "sha256": inat.sha256(photos / cand["file"]), "kind": "whole_plant"}],
        "truth": {"outcome": block["outcome"], "species_id": block.get("species_id"),
                  "problems": list(block.get("problems") or []), "organisms": [], "damage": []},
        "label_basis": "source_caption",
        "label_confidence": verdict,
        "source": cand["source"],
        "licence": s["licence"],
        "credit": f"{s['citation']}; {inat.CHANGES}",
        "labelled_by": labeller,
        "reviewed_by": "",
    }


def contact_sheet(sets: list[dict], cands: dict[str, dict]) -> str:
    parts = ["<!doctype html><meta charset=utf-8><title>Field-dataset photos</title><style>"
             "body{font:14px system-ui;margin:16px}img{height:240px;display:block}"
             "figure{display:inline-block;margin:0 8px 8px 0}figcaption{font:12px monospace}</style>",
             "<h1>Field-dataset photos</h1><p>Write <code>sure</code>, <code>probable</code> or "
             "<code>skip</code> for each photo in <code>review.yaml</code> beside this page, then run "
             "<code>python -m ml.training.eval.fieldsets add --labeller &lt;login&gt;</code>.</p>"]
    for s in sets:
        for folder, c in s["classes"].items():
            group = [(pid, x) for pid, x in cands.items() if x["set"] == s["name"] and x["class"] == folder]
            if not group:
                continue
            truth = ", ".join(c["problems"]) or c["outcome"]
            parts.append(f"<h2>{html.escape(s['name'])} · {html.escape(folder.rsplit('/', 1)[-1])} → "
                         f"{html.escape(c['species_id'] or '?')}: {html.escape(truth)} ({len(group)})</h2>")
            if note := c.get("note"):
                parts.append(f"<p><b>Note:</b> {html.escape(note)}</p>")
            for pid, x in group:
                rel = html.escape(x["file"].split("/", 1)[1])
                parts.append(f"<figure><a href='{rel}'><img src='{rel}' loading=lazy></a>"
                             f"<figcaption>{html.escape(pid)}</figcaption></figure>")
    return "\n".join(parts) + "\n"


def load_sets(only: list[str] | None) -> list[dict]:
    sets = yaml.safe_load(SETS.read_text())["sets"]
    if only:
        sets = [s for s in sets if s["name"] in only]
        if missing := set(only) - {s["name"] for s in sets}:
            raise SystemExit(f"not in {SETS.name}: {', '.join(sorted(missing))}")
    return sets


def fetch(args) -> int:
    from ml.pdm_vision.features import Embedder
    from ml.training.data.fetch import mendeley_listing

    sets = load_sets(args.only)
    out = args.photos / "fieldsets"
    cands_path, review_path = out / "candidates.json", out / "review.yaml"
    old = json.loads(cands_path.read_text()) if cands_path.exists() else {}

    pool: list[dict] = []  # every downloaded photo, class by class
    groups: list[list[int]] = []
    for s in sets:
        listing = mendeley_listing(s["dataset"], s["version"])
        for folder in s["classes"]:
            rows = pick_order(s["name"], [r for r in listing if r["path"].rsplit("/", 1)[0] == folder])
            if not rows:
                raise SystemExit(f"{s['name']}: no photos in {folder!r}")
            todo = [{"set": s["name"], "class": folder, "path": r["path"], "row": r,
                     "id": (pid := photo_id(s["name"], folder, r["path"])), "file": f"fieldsets/{pid}.jpg",
                     "source": source_url(s, r["path"])} for r in rows[: args.per_class * POOL]]
            with ThreadPoolExecutor(8) as ex:
                ok = list(ex.map(lambda c: download(c["row"], args.photos / c["file"]), todo))
            got = [c for c, k in zip(todo, ok) if k]
            groups.append(list(range(len(pool), len(pool) + len(got))))
            pool += got
            print(f"{s['name']} · {folder}: {len(rows)} photos listed, {len(got)} downloaded", flush=True)

    embedder = Embedder()
    train_files = training_images()
    print(f"embedding {len(pool)} picks and {len(train_files)} training images (cached after the first run)", flush=True)
    emb = embedder.files([args.photos / c["file"] for c in pool])
    train = embedder.files(train_files)
    kept, copies = select(groups, emb, train, args.per_class)

    run = {s["name"] for s in sets}
    cands = {pid: c for pid, c in old.items() if c["set"] not in run}  # this run rebuilds its sets' picks
    for i in kept:
        c = pool[i]
        cands[c["id"]] = {k: c[k] for k in ("set", "class", "path", "file", "source")} | {"sha256_mendeley": c["row"]["sha256"]}
    out.mkdir(parents=True, exist_ok=True)
    cands_path.write_text(json.dumps(cands, indent=1, sort_keys=True) + "\n")
    lines = ["# Picks dropped as copies of training images", "",
             f"Cosine at or above {COPY} to an image in a training source (BioCLIP 2). Check a few by eye.", "",
             "| Pick | Cosine | Training image |", "|---|---|---|"]
    lines += [f"| {pool[i]['id']} | {cos:.3f} | {train_files[j].relative_to(RAW)} |" for i, (j, cos) in sorted(copies.items())]
    (out / "copies.md").write_text("\n".join(lines) + "\n")
    all_sets = load_sets(None)
    (out / "index.html").write_text(contact_sheet(all_sets, cands))
    new_blocks = [review_block(s, folder, [i for i, c in cands.items() if c["set"] == s["name"] and c["class"] == folder])
                  for s in sets for folder in s["classes"]]
    existing = yaml.safe_load(review_path.read_text()) if review_path.exists() else []
    review, dropped = merge_review(existing or [], [b for b in new_blocks if b["photos"]])
    if dropped:
        print(f"warning: {dropped} reviewed photos are no longer picked; their verdicts are dropped", file=sys.stderr)
    review_path.write_text(REVIEW_HEADER + yaml.safe_dump(review, sort_keys=False, allow_unicode=True, width=120))

    for s in sets:
        for folder in s["classes"]:
            n = sum(1 for c in cands.values() if c["set"] == s["name"] and c["class"] == folder)
            d = sum(1 for i in copies if pool[i]["set"] == s["name"] and pool[i]["class"] == folder)
            print(f"  {s['name']} · {folder.rsplit('/', 1)[-1]}: {n} kept, {d} copies of training images dropped")
    print(f"{len(cands)} photos in {out}; review {review_path}, contact sheet {out / 'index.html'}, copies {out / 'copies.md'}")
    return 0


def add(args) -> int:
    out = args.photos / "fieldsets"
    cands = json.loads((out / "candidates.json").read_text())
    review = yaml.safe_load((out / "review.yaml").read_text()) or []
    sets = {s["name"]: s for s in load_sets(None)}
    lines = args.manifest.read_text().splitlines() if args.manifest.exists() else []
    have = {json.loads(line)["source"] for line in lines if line.strip()}
    added, problems, undecided = [], [], 0
    for block in review:
        for pid, verdict in (block.get("photos") or {}).items():
            if verdict == "skip":
                undecided += 1
                continue
            try:
                c = case(block, pid, verdict, cands[pid], sets[block["set"]], args.photos, args.labeller)
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
    print(f"{len(added)} cases added to {args.manifest} ({sum(c['split'] == 'dev' for c in added)} dev, "
          f"{sum(c['split'] == 'test' for c in added)} test); {undecided} photos marked skip")
    for p in problems:
        print(f"not added: {p}", file=sys.stderr)
    return 1 if problems else 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch", help="pick, download, drop copies and write the review files")
    f.add_argument("--per-class", type=int, default=8, help="photos kept per class")
    f.add_argument("--only", action="append", help="a set's name; repeatable")
    f.add_argument("--photos", type=Path, default=inat.PHOTOS)
    a = sub.add_parser("add", help="append the reviewed photos to manifest.jsonl")
    a.add_argument("--labeller", required=True, help="your GitHub login")
    a.add_argument("--photos", type=Path, default=inat.PHOTOS)
    a.add_argument("--manifest", type=Path, default=inat.MANIFEST)
    args = ap.parse_args(argv)
    return fetch(args) if args.command == "fetch" else add(args)


if __name__ == "__main__":
    sys.exit(main())
