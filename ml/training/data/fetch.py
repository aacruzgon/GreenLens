"""Fetch every pinned dataset and upstream model in sources.yaml (training.md §4, step 1).

    python ml/training/data/fetch.py                 # everything fetchable
    python ml/training/data/fetch.py plantdoc_det    # one source

Archives land in data/raw/<name>.zip and are unpacked to data/raw/<name>/. A source whose
sha256 is null is downloaded and its hash printed, to be pinned in sources.yaml.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
import urllib.request
import zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
RAW = HERE / "raw"
MODELS = HERE.parents[1] / "models" / "files"
UA = "curl/8.9"  # Mendeley's API refuses Python's default User-Agent


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def fetch_github_archive(src: dict) -> None:
    name = src["name"]
    archive = RAW / f"{name}.zip"
    target = RAW / name
    if not archive.exists():
        url = f"https://codeload.github.com/{src['repo']}/zip/{src['commit']}"
        print(f"[{name}] downloading {url}")
        tmp = archive.with_suffix(".part")
        with urllib.request.urlopen(url) as r, tmp.open("wb") as f:
            shutil.copyfileobj(r, f, 1 << 20)
        tmp.rename(archive)
    digest = sha256_file(archive)
    if src.get("sha256") is None:
        print(f"[{name}] NOT PINNED: sha256 {digest}  <- put this in sources.yaml")
    elif digest != src["sha256"]:
        raise SystemExit(f"[{name}] sha256 mismatch: expected {src['sha256']}, got {digest}")
    if not target.exists():
        print(f"[{name}] unpacking")
        with zipfile.ZipFile(archive) as z:
            z.extractall(target)
    print(f"[{name}] ok: {target}")


def download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    tmp = dest.with_suffix(dest.suffix + ".part")
    with urllib.request.urlopen(req) as r, tmp.open("wb") as f:
        shutil.copyfileobj(r, f, 1 << 20)
    tmp.rename(dest)


def fetch_direct(src: dict) -> None:
    """One archive at a fixed URL, checked against its pinned SHA-256."""
    name = src["name"]
    archive = RAW / f"{name}.zip"
    if not archive.exists():
        print(f"[{name}] downloading {src['url']}")
        download(src["url"], archive)
    digest = sha256_file(archive)
    if src.get("sha256") is None:
        print(f"[{name}] NOT PINNED: sha256 {digest}  <- put this in sources.yaml")
    elif digest != src["sha256"]:
        raise SystemExit(f"[{name}] sha256 mismatch: expected {src['sha256']}, got {digest}")
    target = RAW / name
    if not target.exists():
        with zipfile.ZipFile(archive) as z:
            z.extractall(target)
    print(f"[{name}] ok: {target}")


def mendeley_listing(dataset: str, version: int) -> list[dict]:
    """Every file of a Mendeley Data version: path, size, SHA-256 and download URL."""
    def get(url: str):
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req) as r:
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
    digest = hashlib.sha256(listing).hexdigest()
    if src.get("sha256") is None:
        print(f"[{name}] NOT PINNED: listing sha256 {digest} ({len(rows)} files)  <- put this in sources.yaml")
    elif digest != src["sha256"]:
        raise SystemExit(f"[{name}] listing changed: expected {src['sha256']}, got {digest}")
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
        elif kind == "huggingface":
            fetch_huggingface(src)
        else:
            print(f"[{src['name']}] skipped: kind {kind!r} needs a URL in sources.yaml")


if __name__ == "__main__":
    main(sys.argv[1:])
