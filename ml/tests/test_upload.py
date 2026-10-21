"""The seed-upload step (ml/knowledge/upload.py): the photos checked against the migration, the
SigV4 signature, and push's add-only, read-back behaviour against an in-memory S3."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from ml.knowledge import upload
from ml.knowledge.seed import ROOT

FIXTURE = Path(__file__).parent / "fixtures/knowledge_seed"
# Written by build_migration.py for the fixture seed on another machine, so a match here also
# shows that the pinned Pillow gives the same WebP bytes on this one.
STORE_FIXTURE = ROOT / "backend/crates/store/tests/embedded/knowledge_fixture.surql"


def test_build_matches_the_committed_fixture_migration(tmp_path):
    assert upload.build(FIXTURE, STORE_FIXTURE, tmp_path) == 0
    (entry,) = json.loads((tmp_path / "manifest.json").read_text())["photos"]
    data = (tmp_path / entry["key"]).read_bytes()
    assert f"storage_key: '{entry['key']}'" in STORE_FIXTURE.read_text()
    assert hashlib.sha256(data).hexdigest() == entry["sha256"]
    assert entry["key"].endswith(f"/{entry['sha256'][:16]}.webp")


def test_build_fails_when_the_migration_names_other_photos(tmp_path, capsys):
    stale = tmp_path / "knowledge.surql"
    stale.write_text("UPSERT species:`tomato` CONTENT { reference_images: [{ storage_key: "
                     "'knowledge/species/tomato/0000000000000000.webp' }] } RETURN NONE;\n")
    assert upload.build(FIXTURE, stale, tmp_path / "out") == 1
    out = capsys.readouterr().out
    assert "names knowledge/species/tomato/0000000000000000.webp" in out
    assert "the seed makes knowledge/species/tomato/" in out
    assert not (tmp_path / "out/manifest.json").exists()


def test_no_seed_and_no_migration_is_zero_photos(tmp_path):
    assert upload.build(tmp_path / "no-seed", tmp_path / "none.surql", tmp_path / "out") == 0
    assert json.loads((tmp_path / "out/manifest.json").read_text()) == {"photos": []}


def test_sign_matches_aws_s3_example():
    # "GET Object" in AWS's Signature Version 4 examples for S3 (sig-v4-header-based-auth).
    empty = hashlib.sha256(b"").hexdigest()
    auth = upload.sign("GET", "/test.txt", "", {
        "host": "examplebucket.s3.amazonaws.com", "range": "bytes=0-9",
        "x-amz-content-sha256": empty, "x-amz-date": "20130524T000000Z"}, empty,
        "20130524T000000Z", "AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        "us-east-1")
    assert auth == ("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/"
                    "aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, "
                    "Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41")


class FakeS3:
    """pdm-seed's view of a bucket: list, head, get and a put that honours If-None-Match."""
    bucket = "pdm-images"

    def __init__(self, list_status=200):
        self.objects: dict[str, bytes] = {}
        self.list_status, self.puts = list_status, 0

    def request(self, method, key=None, query=None, body=b"", headers=None):
        if key is None:
            return self.list_status, b"<Code>AccessDenied</Code>" if self.list_status != 200 else b""
        if method in ("HEAD", "GET"):
            return (200, self.objects[key] if method == "GET" else b"") if key in self.objects else (404, b"")
        assert method == "PUT" and headers["if-none-match"] == "*"
        if key in self.objects:
            return 412, b"<Code>PreconditionFailed</Code>"
        self.objects[key] = body
        self.puts += 1
        return 200, b""


@pytest.fixture
def built(tmp_path):
    assert upload.build(FIXTURE, STORE_FIXTURE, tmp_path) == 0
    return tmp_path


def test_push_adds_once_and_then_finds_the_photo(built, capsys):
    s3 = FakeS3()
    assert upload.push(built, s3) == 0
    assert s3.puts == 1 and "1 uploaded, 0 already there" in capsys.readouterr().err
    assert upload.push(built, s3) == 0
    assert s3.puts == 1 and "0 uploaded, 1 already there" in capsys.readouterr().err


def test_push_fails_on_a_different_object_and_never_overwrites_it(built, capsys):
    s3 = FakeS3()
    (entry,) = json.loads((built / "manifest.json").read_text())["photos"]
    s3.objects[entry["key"]] = b"not the photo"
    assert upload.push(built, s3) == 1
    assert s3.objects[entry["key"]] == b"not the photo"
    assert "differ from the photo" in capsys.readouterr().out


def test_push_fails_when_it_cannot_list(built, capsys):
    assert upload.push(built, FakeS3(list_status=403)) == 1
    assert "HTTP 403 AccessDenied" in capsys.readouterr().err


def test_push_fails_on_a_damaged_copy(built, capsys):
    (entry,) = json.loads((built / "manifest.json").read_text())["photos"]
    (built / entry["key"]).write_bytes(b"damaged")
    s3 = FakeS3()
    assert upload.push(built, s3) == 1
    assert s3.puts == 0 and "doesn't match its key" in capsys.readouterr().out
