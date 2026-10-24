"""Our own evaluation captures (evaluation.md §2): the LiDAR cases and repeat scans the app's depth
check saves, and photos taken with the iPhone's Camera app, such as the guided soil checks.

    python -m ml.training.eval.captures scan INBOX [--photos DIR]
    python -m ml.training.eval.captures add --labeller NAME [--photos DIR] [--manifest F] [--soil-manifest F]

INBOX holds one folder per case, named by its case ID (lower-case letters, digits and dashes):

- **A folder the app saved** (a development build with `EXPO_PUBLIC_DEV_TOOLS=1`, Settings › LiDAR ›
  Depth check › Save; frontend roadmap F5 item 2) has a `case.json`
  (`app/src/features/depthcheck/model/evalCase.ts`) naming each shot's photo, its 16-bit depth PNG,
  its DepthMeta and the module's report, with the two-point ruler's reading when one was taken.
  Copy the folders out of the Files app (On My iPhone › PDM › evaluation) as they are.
- **Any other folder** holds JPEG photos. A photo named as the app names them (`01-whole_plant.jpg`)
  takes the kind in its name. In a soil case, `soil-<soil>-<test>[-<state>]-<n>` (SOIL_CASE), every
  photo takes the test's kind, and the review starts from what the name says. Set the Camera app to
  JPEG first (Settings › Camera › Formats › Most Compatible): HEIC is refused.

`scan` checks each case and copies it into `photos/captures/<case_id>/` (git-ignored):
- A photo with depth keeps its pixels in the sensor frame and only its EXIF orientation, since the
  API turns the depth map with the photo (contracts 3); it is re-encoded at JPEG quality 95, which
  keeps its frame and size. Every other photo is turned upright and keeps no metadata, as iNaturalist
  photos are (`inat.strip`). Both lose their location.
- The depth PNG and DepthMeta are copied unchanged, the module's report beside them.

It writes `photos/captures/review.yaml`, one entry per case with `keep: false`, and prints each
ruler's reading against the length typed for it (evaluation 4: a scale card at 30 cm within 3%),
also kept in `photos/captures/rulers.tsv`. A rerun refreshes each case's photos and keeps the review's
edits.

`add` appends every kept case. A scan, with exactly one `whole_plant` photo, goes to `manifest.jsonl`;
a soil case, whose photos are all soil shots or guided checks, goes to `manifest.soil.jsonl`, since
it isn't a scan (evaluation.md §3). New cases go to dev (`inat.new_split`), `source` "team" and
`licence` "team-internal", with an empty `reviewed_by` until a second teammate reviews them.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import re
import shutil
import sys
from pathlib import Path

import yaml

from ml.training.eval import inat

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "manifest.jsonl"
SOIL_MANIFEST = HERE / "manifest.soil.jsonl"
PHOTOS = HERE / "photos"
CASE_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")  # the app's isCaseId
PLANT_KINDS = dict(inat.KINDS)  # the contracts' kind limits for a scan's plant and soil shots
GUIDED_KINDS = ("squeeze_test", "jar_test")
SOIL_KINDS = ("soil_surface", *GUIDED_KINDS)
ALL_KINDS = (*PLANT_KINDS, *GUIDED_KINDS)
NAMED = re.compile(r"^(\d+)-(" + "|".join(ALL_KINDS) + r")\.jpe?g$", re.I)
ROTATIONS = (1, 3, 6, 8)  # the rear camera's EXIF orientations (contracts 3)
MEASURE = ("plant_height_cm", "canopy_width_cm", "leaf_length_cm", "leaf_area_cm2", "lesion_diameter_mm",
           "lesion_count", "affected_area_pct", "hole_diameter_mm", "hole_count")  # contracts 7
SOIL_VALUES = {  # contracts 6: VlmFindingsRaw.soil, and the guided checks' texture
    "moisture": ("dry", "moist", "wet", "waterlogged"),
    "surface": ("normal", "crusted", "cracked"),
    "structure": ("crumbly", "cloddy", "compacted", "unclear"),
    "colour": ("dark", "medium", "pale", "grey_mottled"),
    "cover": ("bare", "mulched", "residue", "ground_cover"),
    "deposits": ("none", "salt_crust", "moss", "algae", "mould"),
    "erosion": ("none", "rills", "exposed_roots"),
    "weeds": ("none", "few", "many"),
    "texture": ("sandy", "loamy", "silty", "clayey"),
}
SQUEEZE = ("crumbles", "holds_ball", "sticky_ribbon", "unclear")  # contracts 6, GuidedTexture
LABEL_BASES = ("extension_office", "expert", "team_consensus", "source_caption")
RULER_TOLERANCE = 0.03  # the app's RULER_TOLERANCE

# A soil case's name: the soil, the test and, for a surface shot or a jar, its state.
SOIL_CASE = re.compile(r"^soil-(?P<soil>[a-z0-9]+)-(?P<test>surface|squeeze|jar)(?:-(?P<state>[a-z]+))?-\d+$")
SOIL_TEST_KINDS = {"surface": "soil_surface", "squeeze": "squeeze_test", "jar": "jar_test"}
TEXTURES = {"sand": "sandy", "loam": "loamy", "silt": "silty", "clay": "clayey"}
# What the squeeze test should do with each texture (the guided prompt, worker vlm/prompt.rs).
EXPECTED_SQUEEZE = {"sandy": "crumbles", "loamy": "holds_ball", "silty": "holds_ball", "clayey": "sticky_ribbon"}
SURFACE_STATES = {"dry": ("moisture", "dry"), "moist": ("moisture", "moist"), "wet": ("moisture", "wet"),
                  "waterlogged": ("moisture", "waterlogged"), "crusted": ("surface", "crusted"),
                  "cracked": ("surface", "cracked"), "mulched": ("cover", "mulched")}
JAR_STATES = {"cloudy": False, "settled": True}

REVIEW_HEADER = """\
# Review of our own captures (ml/training/eval/captures.py). For each case to use:
#   keep: true
#   A scan (one whole_plant photo):
#     outcome, species_id, problems: the truth for that plant on that day
#     measured: what you measured by hand, by contracts 7's names: plant_height_cm (tape),
#               leaf_length_cm (ruler), lesion_diameter_mm and hole_diameter_mm (callipers or the
#               scale card), lesion_count, hole_count; leave out what you didn't measure
#   A soil case (soil shots and guided checks only):
#     soil: the readings you're sure of, by aspect: moisture, surface, structure, colour, cover,
#           deposits, erosion, weeds, and texture from the soil you bought
#     guided: squeeze: result (crumbles, holds_ball, sticky_ribbon); jar: settled, texture
#   confidence: sure, or probable (a bag's texture its label doesn't state is probable)
#   label_basis: team_consensus, or expert / extension_office when one confirmed it
# images, rulers and device_model come from the capture: to change a photo, change the inbox and
# rescan. Leave out any case with a person's face in it: faces aren't blurred on import.
# Then: python -m ml.training.eval.captures add --labeller <your GitHub login>
"""


class CaptureError(ValueError):
    pass


def is_jpeg(data: bytes) -> bool:
    return data[:3] == b"\xff\xd8\xff"


def refuse_heic(path: Path, data: bytes = b"") -> None:
    if path.suffix.lower() in (".heic", ".heif") or data[4:12] in (b"ftypheic", b"ftypheix", b"ftypmif1"):
        raise CaptureError(f"{path.name}: HEIC; set Settings › Camera › Formats to Most Compatible and retake")


def read_photo(path: Path) -> bytes:
    data = path.read_bytes()
    refuse_heic(path, data)
    if not is_jpeg(data):
        raise CaptureError(f"{path.name}: not a JPEG")
    return data


def orientation_of(data: bytes) -> int:
    from PIL import Image

    with Image.open(io.BytesIO(data)) as im:
        return int(im.getexif().get(0x0112, 1))


def keep_orientation(data: bytes) -> bytes:
    """The photo as JPEG in its stored frame, with only its EXIF orientation: no location, no other tag."""
    from PIL import Image

    with Image.open(io.BytesIO(data)) as im:
        orientation = int(im.getexif().get(0x0112, 1))
        out = im.convert("RGB")
    exif = Image.Exif()
    if orientation != 1:
        exif[0x0112] = orientation
    buf = io.BytesIO()
    out.save(buf, "JPEG", quality=95, exif=exif.tobytes())
    return buf.getvalue()


def check_depth(photo: bytes, depth: Path, meta: dict, where: str) -> None:
    """The depth map and DepthMeta as the API accepts them (contracts 3); CaptureError otherwise."""
    from PIL import Image

    with Image.open(depth) as d:
        if d.format != "PNG" or d.mode not in ("I;16", "I;16B", "I"):
            raise CaptureError(f"{where}: {depth.name} isn't a 16-bit greyscale PNG ({d.format} {d.mode})")
        dw, dh = d.size
    with Image.open(io.BytesIO(photo)) as im:
        pw, ph = im.size  # the stored frame, before orientation: the depth's frame
    if abs(dw / dh - pw / ph) > 0.01 * (pw / ph):
        raise CaptureError(f"{where}: depth {dw}×{dh} isn't the photo's {pw}×{ph} aspect ratio within 1%")
    k = meta.get("intrinsics") or {}
    if not all(isinstance(k.get(f), (int, float)) for f in ("fx", "fy", "cx", "cy", "ref_width", "ref_height")):
        raise CaptureError(f"{where}: DepthMeta has no full intrinsics")
    if not meta.get("device_model"):
        raise CaptureError(f"{where}: DepthMeta has no device_model")
    if (o := orientation_of(photo)) not in ROTATIONS:
        raise CaptureError(f"{where}: EXIF orientation {o} can't come from the rear camera")


def soil_prefill(case_id: str) -> dict:
    """The review's starting truth for a soil case, from its name; {} for any other case."""
    m = SOIL_CASE.match(case_id)
    if not m:
        return {}
    texture = TEXTURES.get(m["soil"])
    soil = {"texture": texture} if texture else {}
    guided = {}
    state = m["state"]
    if m["test"] == "surface" and state in SURFACE_STATES:
        aspect, value = SURFACE_STATES[state]
        soil[aspect] = value
    elif m["test"] == "jar":
        settled = JAR_STATES.get(state, True)
        guided = {"settled": settled, "texture": texture or "unclear"} if settled else {"settled": False}
    elif m["test"] == "squeeze":
        guided = {"result": EXPECTED_SQUEEZE.get(texture, "unclear")}
    out = {"soil": soil, "confidence": "sure" if m["soil"] in ("sand", "clay") else "probable"}
    if guided:
        out["guided"] = guided
    return out


def read_case(folder: Path) -> dict:
    """A case folder as a capture: its ID, plant key, device and shots with their source files."""
    case_id = folder.name
    if not CASE_ID.match(case_id):
        raise CaptureError(f"{case_id}: a case ID is lower-case letters, digits and dashes, like tomato-0007")
    case_json = folder / "case.json"
    if case_json.exists():
        app = json.loads(case_json.read_text())
        if app.get("case_id") != case_id:
            raise CaptureError(f"{case_id}: case.json names {app.get('case_id')!r}")
        shots = []
        for img in app.get("images") or []:
            shot = {"kind": img["kind"], "photo": folder / img["file"], "name": img["file"]}
            if img.get("depth"):
                shot["depth"], shot["depth_meta"] = folder / img["depth"], folder / img["depth_meta"]
            if img.get("report"):
                shot["report"] = folder / img["report"]
            if img.get("ruler"):
                shot["ruler"] = img["ruler"]
            shots.append(shot)
        return {"case_id": case_id, "plant_key": app.get("plant_key") or case_id,
                "device_model": app.get("device_model"), "shots": shots, "app": True}
    files = sorted(p for p in folder.iterdir() if p.is_file() and not p.name.startswith("."))
    soil = SOIL_CASE.match(case_id)
    shots = []
    for i, p in enumerate(files, 1):
        refuse_heic(p)
        if named := NAMED.match(p.name):
            kind = named[2].lower()
        elif soil:
            kind = SOIL_TEST_KINDS[soil["test"]]
        else:
            raise CaptureError(f"{case_id}/{p.name}: name it like 01-whole_plant.jpg, or use a soil case's name")
        shots.append({"kind": kind, "photo": p, "name": f"{i:02d}-{kind}.jpg"})
    plant_key = f"soil-{soil['soil']}" if soil else case_id
    return {"case_id": case_id, "plant_key": plant_key, "device_model": None, "shots": shots, "app": False}


def import_case(cap: dict, photos: Path) -> list[dict]:
    """Copies a capture into `photos/captures/<case_id>/`; the images as the review lists them."""
    case_id = cap["case_id"]
    if not cap["shots"]:
        raise CaptureError(f"{case_id}: no photos")
    for s in cap["shots"]:
        if s["kind"] not in ALL_KINDS:
            raise CaptureError(f"{case_id}: kind {s['kind']!r} isn't one of {', '.join(ALL_KINDS)}")
    dest = photos / "captures" / case_id
    if dest.exists():
        shutil.rmtree(dest)  # a rescan replaces the case's copies
    dest.mkdir(parents=True)
    images = []
    for s in cap["shots"]:
        where = f"{case_id}/{s['photo'].name}"
        data = read_photo(s["photo"])
        entry = {"file": f"captures/{case_id}/{s['name']}", "kind": s["kind"]}
        if "depth" in s:
            meta = json.loads(s["depth_meta"].read_text())
            check_depth(data, s["depth"], meta, where)
            (dest / s["name"]).write_bytes(keep_orientation(data))
            for key in ("depth", "depth_meta"):
                shutil.copyfile(s[key], dest / s[key].name)
                entry[key] = f"captures/{case_id}/{s[key].name}"
        else:
            (dest / s["name"]).write_bytes(inat.strip(data))
        if "report" in s and s["report"].exists():
            shutil.copyfile(s["report"], dest / s["report"].name)
        if "ruler" in s:
            entry["ruler"] = s["ruler"]
        images.append(entry)
    return images


def review_entry(cap: dict, images: list[dict]) -> dict:
    soil = all(i["kind"] in SOIL_KINDS for i in images) and not any(i["kind"] == "whole_plant" for i in images)
    entry = {"case_id": cap["case_id"], "keep": False, "plant_key": cap["plant_key"]}
    if cap.get("device_model"):
        entry["device_model"] = cap["device_model"]
    if soil:
        entry.update({"soil": {}, "confidence": "probable", **soil_prefill(cap["case_id"])})
    else:
        entry.update({"outcome": "healthy", "species_id": None, "problems": [], "measured": {},
                      "confidence": "probable"})
    entry["label_basis"] = "team_consensus"
    entry["images"] = images
    return entry


def merge_review(existing: list[dict], new: list[dict]) -> list[dict]:
    """The existing entries with their edits and refreshed images, then the new cases."""
    fresh = {e["case_id"]: e for e in new}
    out = []
    for e in existing:
        if e["case_id"] in fresh:
            e = {**e, "images": fresh[e["case_id"]]["images"]}
            if dm := fresh[e["case_id"]].get("device_model"):
                e["device_model"] = dm
        out.append(e)
    seen = {e["case_id"] for e in existing}
    return out + [e for e in new if e["case_id"] not in seen]


def ruler_rows(entries: list[dict]) -> list[dict]:
    rows = []
    for e in entries:
        for img in e["images"]:
            r = img.get("ruler")
            if not r:
                continue
            row = {"case_id": e["case_id"], "file": img["file"], "measured_mm": r["measured_mm"],
                   "known_mm": r.get("known_mm"), "error_pct": None, "within_3pct": None}
            if r.get("known_mm"):
                err = (r["measured_mm"] - r["known_mm"]) / r["known_mm"]
                row["error_pct"] = round(100 * err, 2)
                row["within_3pct"] = abs(err) <= RULER_TOLERANCE
            rows.append(row)
    return rows


def check_soil(soil: dict, where: str) -> None:
    for aspect, value in (soil or {}).items():
        if aspect not in SOIL_VALUES:
            raise ValueError(f"{where}: soil aspect {aspect!r} isn't one of {', '.join(SOIL_VALUES)}")
        if value not in SOIL_VALUES[aspect]:
            raise ValueError(f"{where}: soil {aspect} {value!r} isn't one of {', '.join(SOIL_VALUES[aspect])}")


def guided_truth(entry: dict, images: list[dict], where: str) -> list[dict]:
    """truth.guided: the expected result of each guided-check photo (contracts 6, GuidedTexture)."""
    g = entry.get("guided") or {}
    out = []
    for i, img in enumerate(images):
        if img["kind"] == "squeeze_test":
            if g.get("result") not in SQUEEZE:
                raise ValueError(f"{where}: a squeeze test needs guided.result, one of {', '.join(SQUEEZE)}")
            out.append({"image": i, "kind": "squeeze_test", "result": g["result"]})
        elif img["kind"] == "jar_test":
            if not isinstance(g.get("settled"), bool):
                raise ValueError(f"{where}: a jar test needs guided.settled, true or false")
            item = {"image": i, "kind": "jar_test", "settled": g["settled"]}
            if g["settled"]:
                if g.get("texture") not in (*SOIL_VALUES["texture"], "unclear"):
                    raise ValueError(f"{where}: a settled jar needs guided.texture, one of {', '.join(SOIL_VALUES['texture'])}")
                item["texture"] = g["texture"]
            out.append(item)
    return out


def case(entry: dict, photos: Path, labeller: str, splits: dict[str, str]) -> tuple[str, dict]:
    """("scan" or "soil", the manifest case) from a kept review entry; ValueError when it isn't one."""
    where = entry["case_id"]
    if entry.get("confidence") not in ("sure", "probable"):
        raise ValueError(f"{where}: confidence must be sure or probable")
    if entry.get("label_basis") not in LABEL_BASES:
        raise ValueError(f"{where}: label_basis must be one of {', '.join(LABEL_BASES)}")
    images = entry.get("images") or []
    for img in images:
        for key in ("file", "depth", "depth_meta"):
            if key in img and not (photos / img[key]).exists():
                raise ValueError(f"{where}: {img[key]} is missing; rescan the inbox")
    kinds = [img["kind"] for img in images]
    soil_case = not any(k in PLANT_KINDS and k != "soil_surface" for k in kinds)
    try:
        split = inat.new_split(entry["plant_key"], splits, entry.get("split"))
    except ValueError as e:
        raise ValueError(f"{where}: {e}") from None
    stored = []
    for img in images:
        s = {"file": img["file"], "sha256": inat.sha256(photos / img["file"]), "kind": img["kind"]}
        for key in ("depth", "depth_meta"):
            if key in img:
                s[key] = img[key]
        if "ruler" in img:
            s["ruler"] = img["ruler"]
        stored.append(s)
    if soil_case:
        check_soil(entry.get("soil") or {}, where)
        truth = {"soil": dict(entry.get("soil") or {}), "guided": guided_truth(entry, images, where)}
    else:
        if kinds.count("whole_plant") != 1:
            raise ValueError(f"{where}: a scan needs exactly one whole_plant photo")
        if any(k in GUIDED_KINDS for k in kinds):
            raise ValueError(f"{where}: put guided checks in a soil case of their own")
        for kind, most in PLANT_KINDS.items():
            if kinds.count(kind) > most:
                raise ValueError(f"{where}: {kinds.count(kind)} {kind} photos, at most {most}")
        inat.check_truth(entry.get("outcome"), entry.get("problems"), entry.get("look_alike"), where)
        measured = entry.get("measured") or {}
        for q, v in measured.items():
            if q not in MEASURE:
                raise ValueError(f"{where}: measured {q!r} isn't one of {', '.join(MEASURE)}")
            if not isinstance(v, (int, float)) or v < 0:
                raise ValueError(f"{where}: measured {q} must be a number of at least 0")
        truth = inat.truth_of(entry)
        if measured:
            truth["measured"] = dict(measured)
        if entry.get("soil"):
            check_soil(entry["soil"], where)
            truth["soil"] = dict(entry["soil"])
    c = {"case_id": entry["case_id"], "split": split, "plant_key": entry["plant_key"], "images": stored,
         "truth": truth, "label_basis": entry["label_basis"], "label_confidence": entry["confidence"],
         "source": "team", "licence": "team-internal", "labelled_by": labeller, "reviewed_by": ""}
    if entry.get("device_model"):
        c["device_model"] = entry["device_model"]
    return ("soil" if soil_case else "scan"), c


def scan(args) -> int:
    folders = sorted(p for p in args.inbox.iterdir() if p.is_dir() and not p.name.startswith("."))
    out = args.photos / "captures"
    review_path = out / "review.yaml"
    new, problems, empty = [], [], []
    for folder in folders:
        if not any(p.is_file() and not p.name.startswith(".") for p in folder.iterdir()):
            empty.append(folder.name)  # a case not taken yet
            continue
        try:
            cap = read_case(folder)
            new.append(review_entry(cap, import_case(cap, args.photos)))
        except (CaptureError, OSError, json.JSONDecodeError, KeyError) as e:
            problems.append(f"{folder.name}: {e}" if not str(e).startswith(folder.name) else str(e))
    out.mkdir(parents=True, exist_ok=True)
    existing = yaml.safe_load(review_path.read_text()) if review_path.exists() else []
    review = merge_review(existing or [], new)
    review_path.write_text(REVIEW_HEADER + yaml.safe_dump(review, sort_keys=False, allow_unicode=True))
    rows = ruler_rows(review)
    with (out / "rulers.tsv").open("w", newline="") as f:
        w = csv.DictWriter(f, ["case_id", "file", "measured_mm", "known_mm", "error_pct", "within_3pct"], delimiter="\t")
        w.writeheader()
        w.writerows(rows)
    print(f"{len(new)} cases from {args.inbox}" + (f", {len(empty)} empty folders skipped" if empty else "")
          + f"; review {review_path}")
    for r in rows:
        verdict = "" if r["known_mm"] is None else f" against {r['known_mm']} mm: {r['error_pct']:+.2f}%" + (
            " (within 3%)" if r["within_3pct"] else " (outside 3%)")
        print(f"  ruler {r['file']}: {r['measured_mm']} mm{verdict}")
    for p in problems:
        print(f"not imported: {p}", file=sys.stderr)
    return 1 if problems else 0


def add(args) -> int:
    review = yaml.safe_load((args.photos / "captures" / "review.yaml").read_text()) or []
    have = set()
    for path in (args.manifest, args.soil_manifest):
        if path.exists():
            have |= {json.loads(line)["case_id"] for line in path.read_text().splitlines() if line.strip()}
    splits = {**inat.plant_splits(args.soil_manifest), **inat.plant_splits(args.manifest)}
    added = {"scan": [], "soil": []}
    problems = []
    for entry in review:
        if not entry.get("keep") or entry["case_id"] in have:
            continue
        try:
            kind, c = case(entry, args.photos, args.labeller, splits)
        except (ValueError, KeyError, OSError) as e:
            problems.append(str(e))
            continue
        added[kind].append(c)
        have.add(c["case_id"])
    for kind, path in (("scan", args.manifest), ("soil", args.soil_manifest)):
        if added[kind]:
            with path.open("a") as f:
                for c in added[kind]:
                    f.write(json.dumps(c, ensure_ascii=False) + "\n")
    print(f"{len(added['scan'])} scans added to {args.manifest}, {len(added['soil'])} soil cases to {args.soil_manifest}, all dev")
    for p in problems:
        print(f"not added: {p}", file=sys.stderr)
    return 1 if problems else 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    s = sub.add_parser("scan", help="check the inbox's cases, copy them and write the review")
    s.add_argument("inbox", type=Path, help="a folder with one folder per case")
    s.add_argument("--photos", type=Path, default=PHOTOS)
    a = sub.add_parser("add", help="append the kept cases to the manifests")
    a.add_argument("--labeller", required=True, help="your GitHub login")
    a.add_argument("--photos", type=Path, default=PHOTOS)
    a.add_argument("--manifest", type=Path, default=MANIFEST)
    a.add_argument("--soil-manifest", type=Path, default=SOIL_MANIFEST)
    args = ap.parse_args(argv)
    return scan(args) if args.command == "scan" else add(args)


if __name__ == "__main__":
    sys.exit(main())
