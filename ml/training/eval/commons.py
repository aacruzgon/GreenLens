"""Licensed photos from Wikimedia Commons for the evaluation set's look-alikes (evaluation.md §2,
"Where the photos come from").

    python -m ml.training.eval.commons fetch
    python -m ml.training.eval.commons add --labeller NAME

iNaturalist has no 2026 photo of an abiotic problem on our crops (searched 2026-10-24), so the
look-alikes come from published datasets (fieldsets.py) and from Commons. `commons.yaml` lists each
case by hand: its files with their shot kinds and its truth. Three rules, chosen by Sam on
2026-10-24:

- A file's licence must allow reuse with credit (LICENCE): CC0, public domain, CC BY or CC BY-SA.
  CC BY-SA is allowed for these evaluation photos only, which stay in the private photo store; it
  is read from the Commons API for each file, never taken from the YAML.
- Photos from before 2026 are used, though BioCLIP 2's training data may hold them (EOL draws on
  Commons *unverified*), so each case records when it was `taken` and the report shows `source`
  Commons as its own slice.
- A teammate reviews each case before `add` writes it (`keep: true`), as for iNaturalist.

`fetch` reads each file's licence, author and date from the Commons API (extmetadata), downloads the
original re-encoded with no metadata (`inat.strip`) to `photos/commons/` (git-ignored), and writes
`photos/commons/candidates.json` and a contact sheet `photos/commons/index.html`. `add` appends every
kept case to `manifest.jsonl`: `label_basis` `source_caption`, the whole-plant file's page as
`source`, each file's licence and credit, and dev (`inat.new_split`).
"""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import yaml

from ml.training.eval import inat

HERE = Path(__file__).resolve().parent
CASES = HERE / "commons.yaml"
MANIFEST = HERE / "manifest.jsonl"
PHOTOS = HERE / "photos"
API = "https://commons.wikimedia.org/w/api.php"
HEADERS = inat.HEADERS
LICENCE = re.compile(r"^(CC0( 1\.0)?|Public domain|CC BY(-SA)? \d\.\d( [a-z]{2})?)$")
CASE_NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


def api(**query) -> dict:
    time.sleep(0.5)  # Commons asks for serial, unhurried requests
    url = API + "?" + urllib.parse.urlencode({"format": "json", **query})
    with urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=60) as r:
        return json.load(r)


def plain(text: str) -> str:
    """Commons' extmetadata HTML as plain text."""
    return " ".join(html.unescape(re.sub(r"<[^>]+>", " ", text or "")).split())


def taken(raw: str) -> str:
    """The photo's date as YYYY-MM-DD when Commons gives one, else its text."""
    text = plain(raw)
    m = re.search(r"\b(\d{4}-\d{2}-\d{2})\b", text)
    return m.group(1) if m else text


def file_info(titles: list[str]) -> dict[str, dict]:
    """Each file's original URL, licence, author and date, keyed by the title as given."""
    out = {}
    for i in range(0, len(titles), 40):
        chunk = titles[i : i + 40]
        res = api(action="query", prop="imageinfo", iiprop="url|extmetadata", titles="|".join(f"File:{t}" for t in chunk))
        norm = {n["to"]: n["from"] for n in res["query"].get("normalized", [])}
        for page in res["query"]["pages"].values():
            given = norm.get(page["title"], page["title"]).removeprefix("File:")
            if "missing" in page or not page.get("imageinfo"):
                out[given] = {"missing": True}
                continue
            ii = page["imageinfo"][0]
            meta = ii.get("extmetadata") or {}
            value = lambda k: (meta.get(k) or {}).get("value", "")  # noqa: E731
            out[given] = {"url": ii["url"], "page": ii["descriptionurl"], "licence": plain(value("LicenseShortName")),
                          "artist": plain(value("Artist")) or plain(value("Credit")), "taken": taken(value("DateTimeOriginal"))}
    return out


def download(url: str, tries: int = 5) -> bytes:
    """A file from upload.wikimedia.org, waiting out a 429 (its rate limit) as Retry-After asks."""
    for attempt in range(tries):
        time.sleep(2)
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=120) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code != 429 or attempt == tries - 1:
                raise
            time.sleep(int(e.headers.get("Retry-After") or 0) or 15 * (attempt + 1))
    raise OSError(f"{url}: no response")


def photo_file(title: str) -> str:
    stem = re.sub(r"[^a-z0-9]+", "-", Path(title).stem.lower()).strip("-")
    return f"commons/{stem}.jpg"


def credit(info: dict) -> str:
    return f"{info['artist'] or 'unknown author'}, via Wikimedia Commons, {info['licence']}; {inat.CHANGES}"


def check_case(c: dict) -> None:
    where = c.get("case", "?")
    if not CASE_NAME.match(str(c.get("case"))):
        raise ValueError(f"{where}: case must be lower-case letters, digits and dashes")
    inat.check_truth(c.get("outcome"), c.get("problems"), c.get("look_alike"), where)
    kinds = [f["kind"] for f in c.get("files") or []]
    for kind, most in inat.KINDS.items():
        if kinds.count(kind) > most:
            raise ValueError(f"{where}: {kinds.count(kind)} {kind} photos, at most {most}")
    if kinds.count("whole_plant") != 1:
        raise ValueError(f"{where}: a scan needs exactly one whole_plant photo")
    if not set(kinds) <= set(inat.KINDS):
        raise ValueError(f"{where}: a kind isn't one of {', '.join(inat.KINDS)}")
    if c.get("confidence", "probable") not in ("sure", "probable"):
        raise ValueError(f"{where}: confidence must be sure or probable")


def contact_sheet(cases: list[dict], cands: dict[str, dict]) -> str:
    parts = ["<!doctype html><meta charset=utf-8><title>Commons look-alikes</title><style>"
             "body{font:14px system-ui;margin:16px}img{height:260px;margin:4px;vertical-align:top}"
             "figure{display:inline-block;margin:0 8px 8px 0}figcaption{font:12px monospace;max-width:420px}"
             ".case{border-top:1px solid #ccc;padding:8px 0}</style>",
             "<h1>Wikimedia Commons look-alikes</h1><p>Set <code>keep: true</code> in "
             "<code>ml/training/eval/commons.yaml</code> for each case to use, then run "
             "<code>python -m ml.training.eval.commons add --labeller &lt;login&gt;</code>.</p>"]
    for c in cases:
        truth = c.get("look_alike") or ", ".join(c.get("problems") or []) or c["outcome"]
        parts.append(f"<div class=case><h2>{html.escape(c['case'])}: {html.escape(c['species_id'] or '?')} · "
                     f"{html.escape(truth)}{' (kept)' if c.get('keep') else ''}</h2>")
        if note := c.get("note"):
            parts.append(f"<p>{html.escape(note)}</p>")
        for f in c["files"]:
            info = cands.get(f["title"], {})
            rel = html.escape(Path(photo_file(f["title"])).name)
            parts.append(f"<figure><a href='{html.escape(info.get('page', ''))}'><img src='{rel}' loading=lazy></a>"
                         f"<figcaption>{html.escape(f['kind'])} · {html.escape(info.get('licence', '?'))} · "
                         f"{html.escape(info.get('taken', ''))}<br>{html.escape(info.get('artist', ''))}</figcaption></figure>")
        parts.append("</div>")
    return "\n".join(parts) + "\n"


def load_cases(path: Path = CASES) -> list[dict]:
    cases = yaml.safe_load(path.read_text())["cases"]
    for c in cases:
        check_case(c)
    if len({c["case"] for c in cases}) != len(cases):
        raise SystemExit(f"{path.name}: two cases share a name")
    return cases


def fetch(args) -> int:
    cases = load_cases(args.cases)
    titles = [f["title"] for c in cases for f in c["files"]]
    info = file_info(titles)
    out = args.photos / "commons"
    out.mkdir(parents=True, exist_ok=True)
    problems = []
    for t in titles:
        i = info.get(t) or {"missing": True}
        if i.get("missing"):
            problems.append(f"{t}: not on Commons")
        elif not LICENCE.match(i["licence"]):
            problems.append(f"{t}: licence {i['licence']!r} isn't CC0, public domain, CC BY or CC BY-SA")
        else:
            dest = args.photos / photo_file(t)
            try:
                if not dest.exists():
                    dest.write_bytes(inat.strip(download(i["url"])))
                i["file"] = photo_file(t)
            except (OSError, ValueError) as e:  # network errors, a file PIL can't read
                problems.append(f"{t}: download failed: {e}")
    usable = {t: i for t, i in info.items() if i.get("file")}
    (out / "candidates.json").write_text(json.dumps(usable, indent=1, ensure_ascii=False) + "\n")
    (out / "index.html").write_text(contact_sheet(cases, usable))
    print(f"{len(usable)} of {len(titles)} files in {out}; contact sheet {out / 'index.html'}")
    for p in problems:
        print(f"not fetched: {p}", file=sys.stderr)
    return 1 if problems else 0


def case(c: dict, cands: dict[str, dict], photos: Path, labeller: str, splits: dict[str, str]) -> dict:
    """A manifest case (evaluation.md §3) from a kept entry of commons.yaml; ValueError when it isn't one."""
    check_case(c)
    where = c["case"]
    files = sorted(c["files"], key=lambda f: list(inat.KINDS).index(f["kind"]))
    for f in files:
        if f["title"] not in cands:
            raise ValueError(f"{where}: {f['title']} wasn't fetched; run fetch")
    plant_key = f"commons-{where}"
    try:
        split = inat.new_split(plant_key, splits)
    except ValueError as e:
        raise ValueError(f"{where}: {e}") from None
    whole = cands[next(f["title"] for f in files if f["kind"] == "whole_plant")]
    return {
        "case_id": f"{c.get('species_id') or 'unknown'}-commons-{where}",
        "split": split,
        "plant_key": plant_key,
        "images": [{"file": cands[f["title"]]["file"], "sha256": inat.sha256(photos / cands[f["title"]]["file"]),
                    "kind": f["kind"]} for f in files],
        "truth": inat.truth_of(c),
        "label_basis": "source_caption",
        "label_confidence": c.get("confidence", "probable"),
        "source": whole["page"],
        "licence": " and ".join(sorted({cands[f["title"]]["licence"] for f in files})),
        "credit": "; ".join(dict.fromkeys(credit(cands[f["title"]]) for f in files)),
        "taken": whole["taken"],
        "labelled_by": labeller,
        "reviewed_by": "",
    }


def add(args) -> int:
    cases = load_cases(args.cases)
    cands = json.loads((args.photos / "commons" / "candidates.json").read_text())
    lines = args.manifest.read_text().splitlines() if args.manifest.exists() else []
    have = {json.loads(line)["case_id"] for line in lines if line.strip()}
    splits = inat.plant_splits(args.manifest)
    added, problems = [], []
    for c in cases:
        if not c.get("keep"):
            continue
        try:
            new = case(c, cands, args.photos, args.labeller, splits)
        except (ValueError, KeyError) as e:
            problems.append(str(e))
            continue
        if new["case_id"] not in have:
            added.append(new)
            have.add(new["case_id"])
    if added:
        with args.manifest.open("a") as f:
            for c in added:
                f.write(json.dumps(c, ensure_ascii=False) + "\n")
    print(f"{len(added)} cases added to {args.manifest}, all dev")
    for p in problems:
        print(f"not added: {p}", file=sys.stderr)
    return 1 if problems else 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    for name, help_ in (("fetch", "read the licences, download the photos, write the contact sheet"),
                        ("add", "append the kept cases to manifest.jsonl")):
        p = sub.add_parser(name, help=help_)
        p.add_argument("--cases", type=Path, default=CASES)
        p.add_argument("--photos", type=Path, default=PHOTOS)
        if name == "add":
            p.add_argument("--labeller", required=True, help="your GitHub login")
            p.add_argument("--manifest", type=Path, default=MANIFEST)
    args = ap.parse_args(argv)
    return fetch(args) if args.command == "fetch" else add(args)


if __name__ == "__main__":
    sys.exit(main())
