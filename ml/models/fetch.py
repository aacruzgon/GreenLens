"""Fetch the model files a host needs into one directory, checking every SHA-256
(docs/vision/README.md §6, "How files reach /models").

    python3 -m ml.models.fetch --for api,fallback --dest /models          # control plane
    python3 -m ml.models.fetch --for perception --dest /models            # rig
    python3 -m ml.models.fetch --for ingest --dest ~/.cache/pdm-models    # ingestion on a dev machine
    python3 -m ml.models.fetch --for perception --cpu --dest ~/.cache/pdm-models   # a laptop, ORT_EP=cpu

The manifest names the files and their hashes; sources.toml says where each is downloaded from,
keyed by hash. Our files are in a private Hugging Face repository, so rows with `auth = "hf"` need
`HF_TOKEN`: a fine-grained read-only token for that repository, from the host's env file, never
committed. Upstream files need no token. Afterwards point MODEL_MANIFEST (or api's MODELS_DIR) at
DEST/manifest.toml; the loaders resolve every `file` against the manifest's own folder.

Steps, as vision §6 lists them:
1. Parse the manifest and the sources. Every chosen entry needs a source row, or nothing is downloaded.
2. Keep a file already present with the right hash. Otherwise download it to DEST/.partial/<sha256>,
   hashing as it streams, and rename it into place. A mismatch deletes the partial file and tries
   once more.
3. Copy NOTICES.md, licences/ and credits/ into DEST.
4. Write the manifest as DEST/manifest.toml.new and rename it over DEST/manifest.toml, keeping the
   old one as DEST/manifest.previous.toml. The manifest moves last, so a consumer never reads one
   whose files aren't in place.
5. Delete files that neither manifest names.

Exit codes: 0 done; 1 a bad manifest, a missing source row, a missing token, or a DEST that
fetch.py didn't create; 2 a hash mismatch twice; 3 a download error. On any failure,
DEST/manifest.toml and its files stay as they were.

Standard library only (Python 3.11+), so it runs in python:3.12-slim or on a fresh machine.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import sys
import tomllib
import urllib.error
import urllib.request
from fnmatch import fnmatch
from pathlib import Path

HERE = Path(__file__).resolve().parent
MARKER = ".pdm-models"            # marks a directory fetch.py owns, so step 5 never deletes elsewhere
KEEP = {MARKER, "manifest.toml", "manifest.previous.toml", "manifest.toml.new", "NOTICES.md"}
NOTICE_DIRS = ("licences", "credits")   # copied beside NOTICES.md, never cleaned

# The entry names each consumer loads (backend vision.md §5). Change with that list.
GROUPS = {
    "api": ["text_embedder", "text_embedder_tokenizer", "yunet"],
    "fallback": ["bioclip_labels*", "disease_head*"],
    "perception": ["leaf_detector", "sam_encoder", "sam_decoder", "bioclip", "bioclip_labels*",
                   "disease_head*", "plant_probe"],
    "ingest": ["text_embedder", "text_embedder_tokenizer"],
}


class Fail(Exception):
    def __init__(self, code: int, message: str):
        super().__init__(message)
        self.code = code


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def chosen(entries: list[dict], groups: list[str]) -> list[dict]:
    patterns = [p for g in groups for p in GROUPS[g]]
    return [e for e in entries if any(fnmatch(e["name"], p) for p in patterns)]


def download(url: str, auth: str | None, partial: Path) -> str:
    """Stream `url` into `partial` and return its SHA-256."""
    req = urllib.request.Request(url, headers={"User-Agent": "pdm-fetch/1"})
    if auth == "hf":
        # Not forwarded on redirect: the Hub redirects to a pre-signed storage URL on another host.
        req.add_unredirected_header("Authorization", f"Bearer {os.environ['HF_TOKEN']}")
    h = hashlib.sha256()
    try:
        with urllib.request.urlopen(req, timeout=60) as r, partial.open("wb") as f:
            for block in iter(lambda: r.read(1 << 20), b""):
                h.update(block)
                f.write(block)
    except (urllib.error.URLError, OSError) as err:
        partial.unlink(missing_ok=True)
        raise Fail(3, f"download failed: {url}: {err}") from err
    return h.hexdigest()


def fetch(manifest: Path, sources: Path, groups: list[str], dest: Path) -> None:
    for g in groups:
        if g == "vlm":
            raise Fail(1, "--for vlm isn't built yet (vision 6, roadmap item M10)")
        if g not in GROUPS:
            raise Fail(1, f"unknown group {g!r}; choose from {', '.join(GROUPS)}")
    try:
        entries = tomllib.loads(manifest.read_text())["model"]
        rows = {r["sha256"]: r for r in tomllib.loads(sources.read_text()).get("file", [])}
    except (OSError, KeyError, tomllib.TOMLDecodeError) as err:
        raise Fail(1, f"can't read the manifest or the sources: {err}") from err
    notices = manifest.parent / "NOTICES.md"
    if not notices.exists():
        raise Fail(1, f"{notices} is missing: model files never travel without their notices")

    # 1. Every chosen entry has a source row; a token is there if a private file must be fetched.
    want = chosen(entries, groups)
    missing = [e["name"] for e in want if e["sha256"] not in rows]
    if missing:
        raise Fail(1, f"no row in {sources.name} for: {', '.join(missing)}")
    if dest.exists() and any(dest.iterdir()) and not (dest / MARKER).exists():
        raise Fail(1, f"{dest} isn't empty and wasn't made by fetch.py; choose another --dest")
    todo = [e for e in want
            if not ((dest / e["file"]).is_file() and sha256(dest / e["file"]) == e["sha256"])]
    if any(rows[e["sha256"]].get("auth") == "hf" for e in todo) and not os.environ.get("HF_TOKEN"):
        raise Fail(1, "HF_TOKEN is needed for files in the private repository pdm-models")
    dest.mkdir(parents=True, exist_ok=True)
    (dest / MARKER).touch()

    # 2. Download what's missing or changed.
    for e in want:
        if e not in todo:
            print(f"ok      {e['file']}")
    partial_dir = dest / ".partial"
    for e in todo:
        row = rows[e["sha256"]]
        partial_dir.mkdir(exist_ok=True)
        partial = partial_dir / e["sha256"]
        print(f"fetch   {e['file']}", flush=True)
        got = download(row["url"], row.get("auth"), partial)
        if got != e["sha256"]:
            partial.unlink(missing_ok=True)
            print(f"retry   {e['file']}: SHA-256 {got} differs", flush=True)
            got = download(row["url"], row.get("auth"), partial)
            if got != e["sha256"]:
                partial.unlink(missing_ok=True)
                raise Fail(2, f"{e['name']}: expected SHA-256 {e['sha256']}, got {got}")
        target = dest / e["file"]
        target.parent.mkdir(parents=True, exist_ok=True)
        os.replace(partial, target)
        print(f"ok      {e['file']}")
    shutil.rmtree(partial_dir, ignore_errors=True)

    # 3. The notices, licence texts and photo credits travel with the files.
    shutil.copyfile(notices, dest / "NOTICES.md")
    for folder in NOTICE_DIRS:
        if (manifest.parent / folder).is_dir():
            shutil.copytree(manifest.parent / folder, dest / folder, dirs_exist_ok=True)

    # 4. The manifest moves last.
    current = dest / "manifest.toml"
    text = manifest.read_text()
    if not (current.exists() and current.read_text() == text):
        (dest / "manifest.toml.new").write_text(text)
        if current.exists():
            shutil.copyfile(current, dest / "manifest.previous.toml")
        os.replace(dest / "manifest.toml.new", current)

    # 5. Delete files that neither manifest names.
    named = {e["file"] for e in entries}
    previous = dest / "manifest.previous.toml"
    if previous.exists():
        named |= {e["file"] for e in tomllib.loads(previous.read_text()).get("model", [])}
    for path in sorted(dest.rglob("*"), reverse=True):
        rel = path.relative_to(dest).as_posix()
        if rel in KEEP or rel.split("/", 1)[0] in NOTICE_DIRS:
            continue
        if path.is_file() and rel not in named:
            path.unlink()
            print(f"removed {rel}")
        elif path.is_dir() and not any(path.iterdir()):
            path.rmdir()
    print(f"{len(want)} files ready; MODEL_MANIFEST={current}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Fetch a host's model files into one directory.")
    ap.add_argument("--for", dest="groups", required=True,
                    help=f"comma-separated groups: {', '.join(GROUPS)}")
    ap.add_argument("--dest", type=Path, required=True)
    ap.add_argument("--cpu", action="store_true", help="manifest.cpu.toml: fp32 files for ORT_EP=cpu")
    ap.add_argument("--manifest", type=Path)
    ap.add_argument("--sources", type=Path, default=HERE / "sources.toml")
    args = ap.parse_args()
    manifest = args.manifest or HERE / ("manifest.cpu.toml" if args.cpu else "manifest.toml")
    try:
        fetch(manifest, args.sources, args.groups.split(","), args.dest.expanduser())
    except Fail as err:
        print(f"error: {err}", file=sys.stderr)
        return err.code
    return 0


if __name__ == "__main__":
    sys.exit(main())
