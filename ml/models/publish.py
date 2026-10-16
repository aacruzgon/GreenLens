"""Upload new model files of ours to the private Hugging Face repository pdm-models and pin them
in sources.toml (docs/vision/README.md §6, "Publishing one of our files").

    python -m ml.models.publish                   # every manifest file without a sources.toml row
    python -m ml.models.publish --dry-run

Finds each file named by manifest.toml or manifest.cpu.toml whose SHA-256 has no row in
sources.toml, checks the local copy in ml/models/files against that hash, and uploads them all in
one commit, each at its manifest path. A path already in the repository is never overwritten:
a new file needs a new name. Then it appends a row per file to sources.toml, pinned to that
commit's 40-hex id. Commit the manifests and sources.toml together afterwards, and run
`python -m ml.models.check`.

Needs the ML environment (huggingface_hub) and a login with write access to the repository
(`hf auth login`). Hosts and teammates only ever read, with fine-grained read-only tokens.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = "GreenLens/pdm-models"
MANIFESTS = ("manifest.toml", "manifest.cpu.toml")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def unpublished(files: Path) -> dict[str, tuple[str, str]]:
    """{path in the repository: (sha256, entry names)} for files without a sources.toml row."""
    rows = {r["sha256"] for r in tomllib.loads((HERE / "sources.toml").read_text()).get("file", [])}
    out: dict[str, tuple[str, str]] = {}
    for m in MANIFESTS:
        for e in tomllib.loads((HERE / m).read_text())["model"]:
            if e["sha256"] in rows:
                continue
            prev = out.get(e["file"])
            if prev and prev[0] != e["sha256"]:
                raise SystemExit(f"{e['file']} names two different files across the manifests")
            out[e["file"]] = (e["sha256"], f"{prev[1]}, {e['name']}" if prev and e["name"] not in prev[1] else e["name"])
    for path, (digest, _) in out.items():
        local = files / path
        if not local.is_file():
            raise SystemExit(f"{local} is missing")
        if sha256(local) != digest:
            raise SystemExit(f"{local} doesn't match the manifest's SHA-256 {digest}")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="Upload new model files to pdm-models and pin them.")
    ap.add_argument("--repo", default=REPO)
    ap.add_argument("--files", type=Path, default=HERE / "files")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    todo = unpublished(args.files)
    if not todo:
        print("nothing to publish")
        return 0
    for path, (digest, names) in todo.items():
        print(f"{'would upload' if args.dry_run else 'upload'}  {path}  ({names})")
    if args.dry_run:
        return 0

    from huggingface_hub import CommitOperationAdd, HfApi

    api = HfApi()
    api.create_repo(args.repo, repo_type="model", private=True, exist_ok=True)
    if not api.model_info(args.repo).private:
        raise SystemExit(f"{args.repo} is public; our files go only to a private repository")
    present = set(api.list_repo_files(args.repo))
    clash = sorted(set(todo) & present)
    if clash:
        raise SystemExit(f"already in {args.repo}, never overwritten (give the file a new name): {clash}")
    commit = api.create_commit(
        args.repo,
        operations=[CommitOperationAdd(path_in_repo=p, path_or_fileobj=str(args.files / p)) for p in todo],
        commit_message=f"Add {len(todo)} model files",
    )
    oid = commit.oid
    assert len(oid) == 40, oid
    with (HERE / "sources.toml").open("a") as f:
        for path, (digest, names) in todo.items():
            f.write(f'\n[[file]]                                  # {names}\n'
                    f'sha256 = "{digest}"\n'
                    f'url    = "https://huggingface.co/{args.repo}/resolve/{oid}/{path}"\n'
                    f'auth   = "hf"\n')
    print(f"{len(todo)} files in {args.repo} at commit {oid}; rows appended to sources.toml")
    return 0


if __name__ == "__main__":
    sys.exit(main())
