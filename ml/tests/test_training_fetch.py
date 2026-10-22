"""The training data's partial fetches and groupings (ml/training/data): zip members read over HTTP
Range, a labels CSV choosing them, the pin, augmented copies found by perceptual hash, the
per-class cap and sessions by time gap."""

import hashlib
import io
import re
import threading
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np
import pytest
import yaml
from PIL import Image

from ml.training.data import copies, fetch, index, remote_zip


class RangeServer(ThreadingHTTPServer):
    files: dict[str, bytes]


class RangeHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        data = self.server.files[self.path]
        hit = re.fullmatch(r"bytes=(\d+)-(\d+)", self.headers.get("Range", ""))
        if hit is None:
            self.send_response(200)
            body = data
        else:
            a, b = int(hit[1]), min(int(hit[2]), len(data) - 1)
            self.send_response(206)
            self.send_header("Content-Range", f"bytes {a}-{b}/{len(data)}")
            body = data[a : b + 1]
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


@pytest.fixture
def server():
    srv = RangeServer(("127.0.0.1", 0), RangeHandler)
    srv.files = {}
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    yield srv, f"http://127.0.0.1:{srv.server_port}"
    srv.shutdown()


def a_zip() -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("train/", b"")
        z.writestr(zipfile.ZipInfo("train/a/1.jpg"), b"stored one")                       # stored
        z.writestr("train/a/2.JPG", b"deflated " * 50, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("train/b/3.jpg", b"other class", compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("flat/4.jpg", b"labelled", compress_type=zipfile.ZIP_DEFLATED)
    return buf.getvalue()


def test_members_are_read_by_range_and_checked(server):
    srv, base = server
    srv.files["/d.zip"] = a_zip()
    ms = remote_zip.members(f"{base}/d.zip")
    assert [m.name for m in ms] == ["train/a/1.jpg", "train/a/2.JPG", "train/b/3.jpg", "flat/4.jpg"]
    got = {m.name: data for run in remote_zip.chunks(ms) for m, data in remote_zip.read_run(f"{base}/d.zip", run)}
    assert got["train/a/2.JPG"] == b"deflated " * 50 and got["train/a/1.jpg"] == b"stored one"
    assert len(remote_zip.chunks(ms, max_bytes=1)) == len(ms)
    bad = srv.files["/d.zip"].replace(b"stored one", b"stored 0ne")
    srv.files["/bad.zip"] = bad
    m = remote_zip.members(f"{base}/bad.zip")[0]
    with pytest.raises(ValueError, match="CRC"):
        remote_zip.read_run(f"{base}/bad.zip", [m])


def listing(files: dict[str, bytes]) -> str:
    lines = "".join(f"{p} {hashlib.sha256(b).hexdigest()}\n" for p, b in sorted(files.items()))
    return hashlib.sha256(lines.encode()).hexdigest()


def test_zip_members_fetches_only_the_included_and_checks_the_pin(server, tmp_path, monkeypatch):
    srv, base = server
    srv.files["/d.zip"] = a_zip()
    monkeypatch.setattr(fetch, "RAW", tmp_path)
    src = {"name": "pc", "zips": [{"url": f"{base}/d.zip"}], "include": [r"^train/a/[^/]+\.(?i:jpe?g)$"],
           "sha256": "0" * 64}
    with pytest.raises(SystemExit, match="mismatch"):
        fetch.fetch_zip_members(src)
    assert not (tmp_path / "pc").exists(), "nothing is renamed into place before the pin matches"
    src["sha256"] = listing({"train/a/1.jpg": b"stored one", "train/a/2.JPG": b"deflated " * 50})
    fetch.fetch_zip_members(src)
    assert sorted(p.relative_to(tmp_path / "pc").as_posix() for p in (tmp_path / "pc").rglob("*.*")) == ["train/a/1.jpg", "train/a/2.JPG"]
    fetch.fetch_zip_members(src)    # a rerun checks the files on disk against the pin
    (tmp_path / "pc" / "train" / "a" / "stray.jpg").write_bytes(b"not fetched")
    with pytest.raises(SystemExit, match="aren't in the pin"):
        fetch.fetch_zip_members(src)    # index.py would read it


def test_a_member_name_cannot_leave_the_folder(server, tmp_path, monkeypatch):
    srv, base = server
    for i, bad in enumerate(("train/../../evil.jpg", "/abs/evil.jpg", "C:/evil.jpg", "train\\evil.jpg")):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            z.writestr(zipfile.ZipInfo("train/a/1.jpg"), b"fine")
            info = zipfile.ZipInfo("x.jpg")
            info.filename = bad    # zipfile would clean the name given to ZipInfo()
            z.writestr(info, b"evil")
        srv.files[f"/e{i}.zip"] = buf.getvalue()
        src = {"name": "pc", "zips": [{"url": f"{base}/e{i}.zip"}], "include": [r"\.jpg$"], "sha256": "0" * 64}
        with pytest.raises(SystemExit, match="outside"):
            fetch.zip_member_targets(src)


def test_archives_are_pinned_one_by_one_and_unpacked_under_their_stem(tmp_path, monkeypatch):
    monkeypatch.setattr(fetch, "RAW", tmp_path)
    blob = a_zip()
    monkeypatch.setattr(fetch, "download", lambda url, dest: (dest.parent.mkdir(parents=True, exist_ok=True),
                                                              dest.write_bytes(blob)))
    src = {"name": "mk", "files": [{"name": "MLB_1.zip", "url": "http://x/MLB_1.zip", "sha256": "0" * 64}]}
    with pytest.raises(SystemExit, match="mismatch"):
        fetch.fetch_archives(src)
    assert not (tmp_path / "mk" / "MLB_1").exists()
    src["files"][0]["sha256"] = hashlib.sha256(blob).hexdigest()
    fetch.fetch_archives(src)
    assert (tmp_path / "mk" / "MLB_1" / "train" / "a" / "2.JPG").read_bytes() == b"deflated " * 50


def test_a_labels_csv_chooses_single_label_images(server, tmp_path, monkeypatch):
    srv, base = server
    srv.files["/d.zip"] = a_zip()
    csv = (b"id,filePath,GLS,NCLB,Other,Unknown\n1,4.jpg,1,0,1,0\n2,3.jpg,1,1,0,0\n"
           b"3,1.jpg,0,1,0,1\n4,2.JPG,0,0,1,0\n")
    srv.files["/labels.csv"] = csv
    monkeypatch.setattr(fetch, "RAW", tmp_path)
    src = {"name": "pr", "zips": [{"url": f"{base}/d.zip"}],
           "labels": {"url": f"{base}/labels.csv", "sha256": hashlib.sha256(csv).hexdigest(), "file_column": "filePath",
                      "exclusive": ["GLS", "NCLB", "Unknown"], "keep": ["GLS", "NCLB"]},
           "sha256": listing({"GLS/4.jpg": b"labelled"})}
    fetch.fetch_zip_members(src)    # 4.jpg: GLS and "Other" (not exclusive); 3.jpg two labels; 1.jpg unknown; 2.JPG none
    assert [p.relative_to(tmp_path / "pr").as_posix() for p in (tmp_path / "pr").rglob("*.jpg")] == ["GLS/4.jpg"]


def leafy(seed: int, size=(240, 160)) -> np.ndarray:
    rng = np.random.default_rng(seed)
    small = rng.integers(0, 255, (6, 9, 3)).astype(np.uint8)
    return np.asarray(Image.fromarray(small).resize(size, Image.Resampling.BICUBIC))


def test_copies_are_grouped_flipped_quartered_or_turned(tmp_path, monkeypatch):
    import cv2

    monkeypatch.setattr(copies, "CACHE", tmp_path / "cache")

    photo, other = leafy(1), leafy(2)
    h, w = photo.shape[:2]
    turned = cv2.warpAffine(photo, cv2.getRotationMatrix2D((w / 2, h / 2), 17, 1.0), (w, h))
    files = {"a/photo.png": photo, "a/flip.png": photo[:, ::-1], "a/quarter.png": np.rot90(photo).copy(),
             "a/turned.png": turned, "b/other.png": other}
    for name, arr in files.items():
        (tmp_path / name).parent.mkdir(exist_ok=True)
        Image.fromarray(arr).save(tmp_path / name)
    paths = sorted(tmp_path / n for n in files)
    groups = copies.groups(paths, 4, workers=1, embed=same_embedding)
    same = {p.relative_to(tmp_path).as_posix() for p in paths if groups[p] == groups[tmp_path / "a/photo.png"]}
    assert same == {"a/photo.png", "a/flip.png", "a/quarter.png", "a/turned.png"}
    assert groups[tmp_path / "b/other.png"] == tmp_path / "b/other.png"


def same_embedding(paths):
    """Every file equally close: the search tries every group (there are at most NEAREST)."""
    return np.full((len(paths), 4), 0.5, np.float32)


def test_copies_turned_two_ways_are_found_by_the_search(tmp_path, monkeypatch):
    # No unturned copy: the corner angle turns each back, but only an unturned centre matches it.
    import cv2

    monkeypatch.setattr(copies, "CACHE", tmp_path / "cache")
    photo, other = leafy(1), leafy(2)
    h, w = photo.shape[:2]
    turn = lambda a, t: cv2.warpAffine(a, cv2.getRotationMatrix2D((w / 2, h / 2), t, 1.0), (w, h))
    files = {"a/t17.png": turn(photo, 17), "a/t40.png": turn(photo[:, ::-1].copy(), -40), "b/other.png": turn(other, 25)}
    for name, arr in files.items():
        (tmp_path / name).parent.mkdir(exist_ok=True)
        Image.fromarray(arr).save(tmp_path / name)
    paths = sorted(tmp_path / n for n in files)
    groups = copies.groups(paths, 4, workers=1, embed=same_embedding)
    assert groups[tmp_path / "a/t40.png"] == groups[tmp_path / "a/t17.png"]
    assert groups[tmp_path / "b/other.png"] == tmp_path / "b/other.png"


def a_timed_set(tmp_path, monkeypatch, extra: dict) -> list:
    """Five photos per class, named by their Unix time in ms (as Makerere's), loaded through a mapping."""
    raw = tmp_path / "raw"
    for i, t in enumerate((1621321486000, 1621321500000, 1621321900000, 1621321910000, 1621322500000)):
        for cls in ("A", "B"):
            (raw / "src" / cls).mkdir(parents=True, exist_ok=True)
            Image.new("RGB", (8, 8), (i, 0, 0)).save(raw / "src" / cls / f"{t + (cls == 'B')}.jpg")
    (tmp_path / "m.yaml").write_text(yaml.safe_dump({
        "dataset": "src", "source": "src", "splits": {"train": "."}, **extra,
        "classes": {"A": {"species_id": "corn", "problem_id": "healthy"}, "B": {"species_id": "corn", "problem_id": "x"}}}))
    monkeypatch.setattr(index, "RAW", raw)
    return index.load(tmp_path / "m.yaml")


def test_sessions_chain_photos_at_most_gap_s_apart(tmp_path, monkeypatch):
    recs, _ = a_timed_set(tmp_path, monkeypatch, {"sessions": {"pattern": r"^(\d{13})", "gap_s": 60}})
    by_second = {int(r.path.stem) // 1000: r.session for r in recs}
    assert len(set(by_second.values())) == 3
    assert by_second[1621321486] == by_second[1621321500], "14 s apart: one session"
    assert by_second[1621321900] == by_second[1621321910] != by_second[1621321500], "400 s apart: two sessions"
    day, _ = a_timed_set(tmp_path, monkeypatch, {"sessions": {"pattern": r"^(\d{13})"}})
    assert {r.session for r in day} == {"src:2021-05-18"}


def test_cap_keeps_a_fixed_sample_per_class(tmp_path, monkeypatch):
    recs, dropped = a_timed_set(tmp_path, monkeypatch, {"cap": {"per_class": 3}})
    assert len(recs) == 6 and dropped == {"A (over the cap of 3 per class)": 2, "B (over the cap of 3 per class)": 2}
    again, _ = a_timed_set(tmp_path, monkeypatch, {"cap": {"per_class": 3}})
    assert [r.path for r in again] == [r.path for r in recs]
