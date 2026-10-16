"""Download the model files a manifest names, from the repository's model release, and check
each one's SHA-256 (docs/vision/dev-roadmap.md, task M5).

    python -m ml.models.fetch               # manifest.toml: the fp16 files the GPU workers load
    python -m ml.models.fetch --cpu         # manifest.cpu.toml: the fp32 files for ORT_EP=cpu
    python -m ml.models.fetch --manifest X --dest DIR

Files land in ml/models/files/<file> (or DIR/<file>), next to a copy of the manifest: the worker
resolves `file` against the manifest's own folder, so MODEL_MANIFEST points at that copy
(ml/models/files/manifest.toml, or manifest.cpu.toml). A file already present with the right
hash is skipped. The repository is private, so downloads go
through the GitHub CLI (`gh auth login` once). Standard library only, so it runs on a fresh
machine or in WSL before the training environment exists.

A new model file is published by uploading it to the release under its file name
(`gh release upload <tag> <path>`); file names are unique across the manifest's folders.

INTERIM: the team's design keeps our files in the private Hugging Face repository `pdm-models`,
fetched through `sources.toml` with `--for` groups (docs/vision/README.md §6). This script and the
release are replaced by that as soon as the repository exists (dev-roadmap task M21); the
release is deleted before GreenLens goes public.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = "aacruzgon/GreenLens"
TAG = "models-2026-10-16"   # the release holding the files of the committed manifests


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cpu", action="store_true", help="use manifest.cpu.toml (fp32 files)")
    ap.add_argument("--manifest", type=Path)
    ap.add_argument("--dest", type=Path, default=HERE / "files")
    ap.add_argument("--tag", default=TAG)
    args = ap.parse_args()
    manifest = args.manifest or HERE / ("manifest.cpu.toml" if args.cpu else "manifest.toml")
    if shutil.which("gh") is None:
        sys.exit("the GitHub CLI (gh) is needed: https://cli.github.com, then `gh auth login`")

    entries = tomllib.loads(manifest.read_text())["model"]
    bad = 0
    with tempfile.TemporaryDirectory() as tmp:
        for e in entries:
            target = args.dest / e["file"]
            if target.exists() and sha256(target) == e["sha256"]:
                print(f"ok      {e['file']}")
                continue
            name = Path(e["file"]).name
            print(f"fetch   {e['file']}", flush=True)
            r = subprocess.run(
                ["gh", "release", "download", args.tag, "--repo", REPO, "--pattern", name,
                 "--dir", tmp, "--clobber"],
                capture_output=True, text=True,
            )
            got = Path(tmp) / name
            if r.returncode != 0 or not got.exists():
                print(f"FAILED  {e['file']}: {r.stderr.strip() or 'not in the release'}")
                bad += 1
                continue
            if sha256(got) != e["sha256"]:
                print(f"FAILED  {e['file']}: SHA-256 differs from the manifest")
                bad += 1
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(got, target)
            print(f"ok      {e['file']}")
    args.dest.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(manifest, args.dest / manifest.name)
    print(f"{len(entries) - bad} of {len(entries)} files ready; MODEL_MANIFEST={args.dest / manifest.name}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
