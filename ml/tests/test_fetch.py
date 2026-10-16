"""fetch.py end to end against file:// sources (docs/vision/README.md §6, roadmap item M1)."""

import hashlib
import tomllib
from pathlib import Path

import pytest

from ml.models import fetch


def digest(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


@pytest.fixture
def repo(tmp_path):
    """A models folder with a manifest of an api file and a private perception file, and a store."""
    models, store = tmp_path / "models", tmp_path / "store"
    (models / "licences").mkdir(parents=True)
    store.mkdir()
    (models / "NOTICES.md").write_text("## yunet\n## bioclip\n")
    (models / "licences" / "MIT.txt").write_text("MIT")
    blobs = {"yunet/y.onnx": b"face", "bioclip/b.onnx": b"tower"}
    for path, data in blobs.items():
        (store / Path(path).name).write_bytes(data)
    manifest = models / "manifest.toml"
    manifest.write_text(
        f'[[model]]\nname = "yunet"\nversion = "1"\nfile = "yunet/y.onnx"\nsha256 = "{digest(b"face")}"\n\n'
        f'[[model]]\nname = "bioclip"\nversion = "1"\nfile = "bioclip/b.onnx"\nsha256 = "{digest(b"tower")}"\n')
    sources = models / "sources.toml"
    sources.write_text(
        f'[[file]]\nsha256 = "{digest(b"face")}"\nurl = "{(store / "y.onnx").as_uri()}"\n\n'
        f'[[file]]\nsha256 = "{digest(b"tower")}"\nurl = "{(store / "b.onnx").as_uri()}"\nauth = "hf"\n')
    return models, store, manifest, sources, tmp_path / "dest"


def test_fetches_then_downloads_nothing(repo, capsys, monkeypatch):
    models, _, manifest, sources, dest = repo
    monkeypatch.setenv("HF_TOKEN", "test")
    fetch.fetch(manifest, sources, ["perception", "api"], dest)
    assert (dest / "yunet/y.onnx").read_bytes() == b"face"
    assert (dest / "bioclip/b.onnx").read_bytes() == b"tower"
    assert (dest / "manifest.toml").read_text() == manifest.read_text()
    assert (dest / "NOTICES.md").exists() and (dest / "licences/MIT.txt").exists()
    capsys.readouterr()
    fetch.fetch(manifest, sources, ["perception", "api"], dest)
    assert "fetch " not in capsys.readouterr().out


def test_a_group_fetches_only_its_entries(repo):
    _, _, manifest, sources, dest = repo
    fetch.fetch(manifest, sources, ["api"], dest)          # no HF_TOKEN needed: upstream only
    assert (dest / "yunet/y.onnx").exists() and not (dest / "bioclip").exists()


def test_a_changed_byte_exits_2_and_keeps_the_manifest(repo, monkeypatch):
    _, store, manifest, sources, dest = repo
    fetch.fetch(manifest, sources, ["api"], dest)
    before = (dest / "manifest.toml").read_text()
    (store / "y.onnx").write_bytes(b"fac3")
    (dest / "yunet/y.onnx").write_bytes(b"fac3")
    manifest.write_text(manifest.read_text() + "\n# changed\n")
    with pytest.raises(fetch.Fail) as err:
        fetch.fetch(manifest, sources, ["api"], dest)
    assert err.value.code == 2
    assert (dest / "manifest.toml").read_text() == before
    assert not list((dest / ".partial").glob("*")) if (dest / ".partial").exists() else True


def test_refuses_before_downloading(repo, monkeypatch):
    models, _, manifest, sources, dest = repo
    monkeypatch.delenv("HF_TOKEN", raising=False)
    with pytest.raises(fetch.Fail) as err:
        fetch.fetch(manifest, sources, ["perception"], dest)
    assert err.value.code == 1 and "HF_TOKEN" in str(err.value)
    sources.write_text(sources.read_text().split("\n\n")[0] + "\n")    # the bioclip row removed
    with pytest.raises(fetch.Fail) as err:
        fetch.fetch(manifest, sources, ["perception"], dest)
    assert err.value.code == 1 and "bioclip" in str(err.value)
    assert not dest.exists()


def test_never_cleans_a_directory_it_did_not_make(repo, tmp_path):
    _, _, manifest, sources, _ = repo
    other = tmp_path / "home"
    other.mkdir()
    (other / "keep.txt").write_text("mine")
    with pytest.raises(fetch.Fail) as err:
        fetch.fetch(manifest, sources, ["api"], other)
    assert err.value.code == 1 and (other / "keep.txt").exists()


def test_a_new_manifest_keeps_the_previous_and_removes_what_neither_names(repo, monkeypatch):
    models, store, manifest, sources, dest = repo
    monkeypatch.setenv("HF_TOKEN", "test")
    fetch.fetch(manifest, sources, ["perception", "api"], dest)
    first = manifest.read_text()
    (store / "y2.onnx").write_bytes(b"face2")
    manifest.write_text(first.replace("yunet/y.onnx", "yunet/y2.onnx").replace(digest(b"face"), digest(b"face2")))
    sources.write_text(sources.read_text() + f'\n[[file]]\nsha256 = "{digest(b"face2")}"\nurl = "{(store / "y2.onnx").as_uri()}"\n')
    (dest / "stray.bin").write_bytes(b"x")
    fetch.fetch(manifest, sources, ["perception", "api"], dest)
    assert (dest / "manifest.previous.toml").read_text() == first
    assert (dest / "yunet/y.onnx").exists(), "the previous manifest still names it"
    assert not (dest / "stray.bin").exists()
    fetch.fetch(manifest, sources, ["perception", "api"], dest)   # same manifest: previous unchanged
    assert tomllib.loads((dest / "manifest.previous.toml").read_text())["model"][0]["file"] == "yunet/y.onnx"


def test_vlm_is_not_built_yet(repo):
    _, _, manifest, sources, dest = repo
    with pytest.raises(fetch.Fail) as err:
        fetch.fetch(manifest, sources, ["vlm"], dest)
    assert err.value.code == 1
