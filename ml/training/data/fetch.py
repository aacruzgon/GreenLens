"""Fetch every pinned dataset and upstream model in sources.yaml (training.md §4, step 1).

    python -m ml.training.data.fetch                 # everything fetchable
    python -m ml.training.data.fetch plantdoc_det    # one source
    python -m ml.training.data.fetch --pin <name>    # a new source whose sha256 is still null

Archives land in data/raw/<name>.zip and are unpacked to data/raw/<name>/; a single model file
(`kind: file`) lands in models/files/<dest>. A source whose sha256 is null is refused, unless
--pin is given: then it is downloaded and its hash printed, to be pinned in sources.yaml.

Two kinds take part of a big upload:
- `zip_members` reads only the chosen members of remote zips over HTTP Range requests
  (remote_zip.py): those matching an `include` regex, or, with `labels`, the images a CSV of
  per-image label columns gives exactly one label to. The pin is the SHA-256 of the sorted
  "path sha256" lines of the files written, as for `mendeley_folder`.
- `archives` downloads several archives (zip or RAR), each pinned by its own SHA-256, into
  data/raw/<name>.archives/ and unpacks each to data/raw/<name>/<archive name without suffix>/.
"""

from __future__ import annotations

import csv
import hashlib
import io
import json
import re
import shutil
import subprocess
import sys
import urllib.request
import zipfile
import zlib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
RAW = HERE / "raw"
MODELS = HERE.parents[1] / "models" / "files"
UA = "curl/8.9"  # Mendeley's API refuses Python's default User-Agent
TIMEOUT = 60      # seconds without data before a download fails
PIN = "--pin" in sys.argv  # allow sources whose sha256 is null, to print the hash to pin


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def unpack(archive: Path, target: Path) -> None:
    """Unpack into a temporary folder and rename it, so an interrupted unpack never leaves a
    partial target that later runs would take as complete."""
    if target.exists():
        return
    tmp = target.with_name(target.name + ".unpacking")
    if tmp.exists():
        shutil.rmtree(tmp)
    print(f"[{target.name}] unpacking")
    with zipfile.ZipFile(archive) as z:
        z.extractall(tmp)
    tmp.rename(target)


def check_pin(src: dict, digest: str, what: str = "sha256") -> None:
    name = src["name"]
    if src.get("sha256") is None:
        print(f"[{name}] NOT PINNED: {what} {digest}  <- put this in sources.yaml")
        if not PIN:
            raise SystemExit(f"[{name}] refusing an unpinned source; rerun with --pin to fetch it anyway")
    elif digest != src["sha256"]:
        raise SystemExit(f"[{name}] {what} mismatch: expected {src['sha256']}, got {digest}")


def fetch_github_archive(src: dict) -> None:
    name = src["name"]
    archive = RAW / f"{name}.zip"
    target = RAW / name
    if not archive.exists():
        url = f"https://codeload.github.com/{src['repo']}/zip/{src['commit']}"
        print(f"[{name}] downloading {url}")
        tmp = archive.with_suffix(".part")
        with urllib.request.urlopen(url, timeout=TIMEOUT) as r, tmp.open("wb") as f:
            shutil.copyfileobj(r, f, 1 << 20)
        tmp.rename(archive)
    check_pin(src, sha256_file(archive))
    unpack(archive, target)
    print(f"[{name}] ok: {target}")


def download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    tmp = dest.with_suffix(dest.suffix + ".part")
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r, tmp.open("wb") as f:
        shutil.copyfileobj(r, f, 1 << 20)
    tmp.rename(dest)


def fetch_direct(src: dict) -> None:
    """One archive at a fixed URL, checked against its pinned SHA-256."""
    name = src["name"]
    archive = RAW / f"{name}.zip"
    if not archive.exists():
        print(f"[{name}] downloading {src['url']}")
        download(src["url"], archive)
    check_pin(src, sha256_file(archive))
    target = RAW / name
    unpack(archive, target)
    print(f"[{name}] ok: {target}")


def fetch_file(src: dict) -> None:
    """One model file at a fixed URL, checked against its pinned SHA-256."""
    dest = MODELS / src["dest"]
    if not dest.exists():
        print(f"[{src['name']}] downloading {src['url']}")
        download(src["url"], dest)
    check_pin(src, sha256_file(dest))
    print(f"[{src['name']}] ok: {dest}")


def mendeley_listing(dataset: str, version: int) -> list[dict]:
    """Every file of a Mendeley Data version: path, size, SHA-256 and download URL."""
    def get(url: str):
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            return json.load(r)

    api = "https://data.mendeley.com/public-api/datasets"
    folders = get(f"{api}/{dataset}/folders/{version}?version={version}")
    by_id = {f["id"]: f for f in folders}

    def path(f: dict) -> str:
        parts = [f["name"]]
        while f.get("parent_id") in by_id:
            f = by_id[f["parent_id"]]
            parts.append(f["name"])
        return "/".join(reversed(parts))

    rows = []
    for f in [{"id": "root", "name": ""}] + folders:
        for x in get(f"{api}/{dataset}/files?folder_id={f['id']}&version={version}"):
            d = x.get("content_details") or {}
            prefix = path(f) if f["id"] != "root" else ""
            rows.append({
                "path": f"{prefix}/{x['filename']}".lstrip("/"),
                "sha256": d["sha256_hash"],
                "url": d["download_url"],
            })
    return sorted(rows, key=lambda r: r["path"])


def fetch_mendeley_folder(src: dict) -> None:
    """Files under `prefix` of a Mendeley Data version, each checked against the hash the
    API publishes. The pin is the SHA-256 of the sorted "path sha256" lines of those files."""
    name = src["name"]
    rows = [r for r in mendeley_listing(src["dataset"], src["version"]) if r["path"].startswith(src["prefix"])]
    listing = "".join(f"{r['path']} {r['sha256']}\n" for r in rows).encode()
    print(f"[{name}] {len(rows)} files listed")
    check_pin(src, hashlib.sha256(listing).hexdigest(), "listing sha256")
    target = RAW / name
    with ThreadPoolExecutor(8) as pool:
        def one(r: dict) -> None:
            dest = target / r["path"][len(src["prefix"]):].lstrip("/")
            if dest.exists() and sha256_file(dest) == r["sha256"]:
                return
            download(r["url"], dest)
            if sha256_file(dest) != r["sha256"]:
                raise SystemExit(f"[{name}] {r['path']}: sha256 mismatch")
        list(pool.map(one, rows))
    print(f"[{name}] ok: {target} ({len(rows)} files)")


def zip_member_targets(src: dict) -> list[tuple[str, object, str]]:
    """(zip URL, member, path under data/raw/<name>/) for every member the source takes.

    Without `labels`, a member is taken when an `include` regex matches its path, and keeps
    that path. With `labels` (a pinned CSV, one row per image), a member is taken when its row
    has exactly one of the `exclusive` columns set to 1 and that column is in `keep`; it is
    written to <column>/<file name>. Columns outside `exclusive` (a "tearing or insects" flag)
    don't count."""
    from ml.training.data import remote_zip

    lab = src.get("labels")
    chosen_label: dict[str, str] = {}
    if lab:
        body = remote_zip_get(lab["url"])
        digest = hashlib.sha256(body).hexdigest()
        if digest != lab["sha256"]:
            raise SystemExit(f"[{src['name']}] labels sha256 mismatch: expected {lab['sha256']}, got {digest}")
        for row in csv.DictReader(io.StringIO(body.decode("utf-8-sig"))):
            on = [c for c in lab["exclusive"] if row[c].strip() == "1"]
            if len(on) == 1 and on[0] in lab["keep"]:
                chosen_label[row[lab["file_column"]].strip().rsplit("/", 1)[-1]] = on[0]
    include = [re.compile(p) for p in src.get("include", [])]
    out = []
    for z in src["zips"]:
        for m in remote_zip.members(z["url"]):
            base = m.name.rsplit("/", 1)[-1]
            if lab:
                if base in chosen_label:
                    out.append((z["url"], m, f"{chosen_label[base]}/{base}"))
            elif any(p.search(m.name) for p in include):
                out.append((z["url"], m, m.name))
    paths = [p for *_, p in out]
    # A member's name comes from the archive, so one like ../x or /x must not leave the folder.
    unsafe = [p for p in paths if p.startswith("/") or "\\" in p or ".." in p.split("/") or re.match(r"[A-Za-z]:", p)]
    if unsafe:
        raise SystemExit(f"[{src['name']}] member {unsafe[0]!r} would be written outside data/raw/{src['name']}/")
    if len(set(paths)) != len(paths):
        raise SystemExit(f"[{src['name']}] two members would land on the same path")
    if lab and len(out) != len(chosen_label):
        raise SystemExit(f"[{src['name']}] {len(chosen_label) - len(out)} labelled images are missing from the zips")
    return out


def remote_zip_get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        return r.read()


def listing_sha256(target: Path, paths: list[str]) -> str:
    lines = "".join(f"{p} {sha256_file(target / p)}\n" for p in sorted(paths))
    return hashlib.sha256(lines.encode()).hexdigest()


def fetch_zip_members(src: dict) -> None:
    """The chosen members of remote zips, read over HTTP Range requests. Members go to
    data/raw/<name>.partial/ and the folder is renamed to data/raw/<name>/ once the listing hash
    matches the pin; a rerun skips members already on disk with the right CRC-32."""
    from ml.training.data import remote_zip

    name = src["name"]
    target = RAW / name
    if src.get("sha256") is None and not PIN:
        raise SystemExit(f"[{name}] refusing an unpinned source; rerun with --pin to fetch it anyway")
    todo = zip_member_targets(src)
    print(f"[{name}] {len(todo)} members chosen, {sum(m.csize for _, m, _ in todo) / 1e9:.2f} GB")
    if target.exists():
        # index.py reads every image under the folder, so a file the pin doesn't cover fails too.
        chosen = {p for *_, p in todo}
        extra = sorted(q for q in (f.relative_to(target).as_posix() for f in target.rglob("*") if f.is_file())
                       if q not in chosen)
        if extra:
            raise SystemExit(f"[{name}] {len(extra)} files under {target} aren't in the pin, such as {extra[0]}; "
                             "remove them or the folder and rerun")
        check_pin(src, listing_sha256(target, sorted(chosen)), "listing sha256")
        print(f"[{name}] ok: {target} ({len(todo)} files)")
        return
    part = target.with_name(name + ".partial")
    dest_of = {(u, m.offset): part / p for u, m, p in todo}

    def done(u: str, m) -> bool:
        d = dest_of[(u, m.offset)]
        return d.exists() and d.stat().st_size == m.usize and zlib.crc32(d.read_bytes()) == m.crc

    runs = []
    for url in dict.fromkeys(u for u, _, _ in todo):
        missing = [m for u, m, _ in todo if u == url and not done(u, m)]
        runs += [(url, r) for r in remote_zip.chunks(missing)]
    print(f"[{name}] {len(runs)} range requests")

    def one(job) -> int:
        url, run = job
        for m, data in remote_zip.read_run(url, run):
            d = dest_of[(url, m.offset)]
            d.parent.mkdir(parents=True, exist_ok=True)
            tmp = d.with_name(d.name + ".tmp")
            tmp.write_bytes(data)
            tmp.rename(d)
        return len(run)

    n = 0
    with ThreadPoolExecutor(6) as pool:
        for k, got in enumerate(pool.map(one, runs)):
            n += got
            if k % 20 == 0:
                print(f"[{name}]   {n} members written", flush=True)
    check_pin(src, listing_sha256(part, [p for *_, p in todo]), "listing sha256")
    part.rename(target)
    print(f"[{name}] ok: {target} ({len(todo)} files)")


def unpack_any(archive: Path, target: Path) -> None:
    """`unpack` for zip archives; RAR archives through `unar`, into a temporary folder renamed
    when complete. An archive whose content is one folder named like the target (Makerere's
    MLB_1.rar holds MLB_1/) is unpacked without that extra level."""
    if archive.suffix.lower() == ".zip":
        return unpack(archive, target)
    if target.exists():
        return
    tmp = target.with_name(target.name + ".unpacking")
    if tmp.exists():
        shutil.rmtree(tmp)
    print(f"[{target.name}] unpacking {archive.name}")
    subprocess.run(["unar", "-quiet", "-no-directory", "-output-directory", str(tmp), str(archive)], check=True)
    inner = list(tmp.iterdir())
    if len(inner) == 1 and inner[0].is_dir() and inner[0].name == target.name:
        inner[0].rename(target)
        tmp.rmdir()
    else:
        tmp.rename(target)


def fetch_archives(src: dict) -> None:
    """Several archives, each checked against its own pinned SHA-256 and unpacked to
    data/raw/<name>/<archive name without its suffix>/."""
    name = src["name"]
    store = RAW / f"{name}.archives"
    for f in src["files"]:
        archive = store / f["name"]
        if f.get("sha256") is None and not PIN:
            raise SystemExit(f"[{name}] {f['name']}: refusing an unpinned archive; rerun with --pin")
        if not archive.exists():
            print(f"[{name}] downloading {f['name']} from {f['url']}")
            download(f["url"], archive)
        check_pin({"name": f"{name}/{f['name']}", "sha256": f.get("sha256")}, sha256_file(archive))
        unpack_any(archive, RAW / name / Path(f["name"]).stem)
    print(f"[{name}] ok: {RAW / name}")


def fetch_huggingface(src: dict) -> None:
    from huggingface_hub import snapshot_download

    name = src["name"]
    is_dataset = src.get("repo_type") == "dataset"
    path = snapshot_download(
        repo_id=src["repo"],
        repo_type=src.get("repo_type", "model"),
        revision=src["revision"],
        allow_patterns=src.get("files"),
        local_dir=(RAW if is_dataset else MODELS) / name,
    )
    if is_dataset:
        for z in Path(path).rglob("*.zip"):
            out = z.with_suffix("")
            if not out.exists():
                with zipfile.ZipFile(z) as zf:
                    zf.extractall(out)
    print(f"[{name}] ok: {path}")


def main(only: list[str]) -> None:
    only = [a for a in only if a != "--pin"]
    RAW.mkdir(parents=True, exist_ok=True)
    sources = yaml.safe_load((HERE / "sources.yaml").read_text())
    for src in sources["datasets"] + sources["models"]:
        if only and src["name"] not in only:
            continue
        kind = src["kind"]
        if kind == "github_archive":
            fetch_github_archive(src)
        elif kind == "direct":
            fetch_direct(src)
        elif kind == "mendeley_folder":
            fetch_mendeley_folder(src)
        elif kind == "zip_members":
            fetch_zip_members(src)
        elif kind == "archives":
            fetch_archives(src)
        elif kind == "huggingface":
            fetch_huggingface(src)
        elif kind == "file":
            fetch_file(src)
        elif kind == "inaturalist":
            print(f"[{src['name']}] fetched by {src['fetched_by']}, not here")
        else:
            print(f"[{src['name']}] skipped: kind {kind!r} needs a URL in sources.yaml")


if __name__ == "__main__":
    main(sys.argv[1:])
