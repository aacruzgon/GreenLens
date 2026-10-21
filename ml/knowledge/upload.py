"""Puts the species reference photos in RustFS (contracts §10; storage 2). The seed-upload image
runs it once per deploy on the control plane, before api starts with the migration that names
the photos (infra §4).

    python3 -m ml.knowledge.upload build --out DIR   # make the photos, check them against knowledge.surql
    python3 -m ml.knowledge.upload push DIR          # put DIR's photos in RustFS

`build` runs while the image is built (ml/knowledge/Dockerfile). It makes each photo from the seed
as build_migration.py does (photos.py) and fails unless the keys are exactly those that
backend/migrations/knowledge.surql names, so an image whose photos the migration can't find is
never pushed. It needs Pillow and PyYAML, not the embedder or a token.

`push` is what the container runs. It is standard library only and only adds: a key is a content
hash, so a changed photo gets a new key and an older api still finds its own (infra §7). It never
overwrites (`If-None-Match: *`) and never deletes. It first lists knowledge/species/, so wrong keys
or a missing bucket fail even with no photos. Every photo is read back and its SHA-256 compared,
whether it was put now or was already there. Any failure exits non-zero; infra/deploy.sh checks
the exit code. Compose doesn't hold api back on it, since api's dependency is optional (infra §4).

Environment for push: RUSTFS_ACCESS_KEY and RUSTFS_SECRET_KEY (PDMSEED01's, from .env.seed),
RUSTFS_ENDPOINT (default http://rustfs:9000), RUSTFS_BUCKET (default pdm-images), RUSTFS_REGION
(default us-east-1). The keys are never printed.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import hmac
import json
import os
import re
import shutil
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MIGRATION = ROOT / "backend/migrations/knowledge.surql"
PREFIX = "knowledge/species/"
KEY = re.compile(r"^knowledge/species/[a-z][a-z0-9_]{0,63}/([0-9a-f]{16})\.webp$")
ATTEMPTS = 3


class UploadError(Exception):
    pass


# --- build: the photos, checked against the migration -----------------------------------------

def build(seed_root: Path, migration: Path, out: Path) -> int:
    from ml.knowledge.photos import keys_in_migration, reference_images
    from ml.knowledge.seed import load

    photos = reference_images(load(seed_root))
    text = migration.read_text(encoding="utf-8") if migration.is_file() else ""
    want = keys_in_migration(text)
    if set(photos) != want:
        for k in sorted(want - set(photos)):
            print(f"{migration.name} names {k}, which the seed doesn't make")
        for k in sorted(set(photos) - want):
            print(f"the seed makes {k}, which {migration.name} doesn't name")
        import PIL
        print("FAIL: the photos don't match the migration. Run python3 -m "
              "ml.knowledge.build_migration and commit knowledge.surql; if it is current, Pillow "
              f"{PIL.__version__} here encodes differently from the one that wrote it",
              file=sys.stderr)
        return 1
    shutil.rmtree(out / "knowledge", ignore_errors=True)
    entries = []
    for key, data in sorted(photos.items()):
        (out / key).parent.mkdir(parents=True, exist_ok=True)
        (out / key).write_bytes(data)
        entries.append({"key": key, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)})
    out.mkdir(parents=True, exist_ok=True)
    (out / "manifest.json").write_text(json.dumps({"photos": entries}, indent=2) + "\n")
    print(f"ok: {len(entries)} photos, {sum(e['bytes'] for e in entries) / 1024:.0f} KiB, "
          f"matching {migration.name if migration.is_file() else 'no migration'}", file=sys.stderr)
    return 0


# --- push: a minimal S3 client (SigV4, path-style) --------------------------------------------

def _hmac(key: bytes, msg: str) -> bytes:
    return hmac.new(key, msg.encode(), hashlib.sha256).digest()


def sign(method: str, path: str, query: str, headers: dict[str, str], payload_hash: str,
         amz_date: str, access_key: str, secret_key: str, region: str) -> str:
    """The Authorization header of AWS Signature Version 4 for S3. `headers` are lower-case and
    include host, x-amz-content-sha256 and x-amz-date; all of them are signed. `path` is already
    URI-encoded."""
    names = sorted(headers)
    canonical = "\n".join([method, path, query, *(f"{k}:{headers[k].strip()}" for k in names),
                           "", ";".join(names), payload_hash])
    scope = f"{amz_date[:8]}/{region}/s3/aws4_request"
    to_sign = "\n".join(["AWS4-HMAC-SHA256", amz_date, scope,
                         hashlib.sha256(canonical.encode()).hexdigest()])
    key = _hmac(("AWS4" + secret_key).encode(), amz_date[:8])
    for part in (region, "s3", "aws4_request"):
        key = _hmac(key, part)
    signature = hmac.new(key, to_sign.encode(), hashlib.sha256).hexdigest()
    return (f"AWS4-HMAC-SHA256 Credential={access_key}/{scope}, "
            f"SignedHeaders={';'.join(names)}, Signature={signature}")


class S3:
    def __init__(self, endpoint: str, bucket: str, access_key: str, secret_key: str,
                 region: str = "us-east-1", timeout: float = 30.0):
        u = urllib.parse.urlsplit(endpoint)
        if u.scheme not in ("http", "https") or not u.netloc or u.path not in ("", "/"):
            raise UploadError(f"RUSTFS_ENDPOINT {endpoint!r} isn't http(s)://host:port")
        self.base, self.host = f"{u.scheme}://{u.netloc}", u.netloc
        self.bucket, self.region, self.timeout = bucket, region, timeout
        self._ak, self._sk = access_key, secret_key

    def request(self, method: str, key: str | None = None, query: dict | None = None,
                body: bytes = b"", headers: dict | None = None) -> tuple[int, bytes]:
        """(status, body). Retries a connection error or a 5xx; any other status is returned."""
        path = "/" + self.bucket + ("/" + urllib.parse.quote(key, safe="/-_.~") if key else "")
        query = "&".join(f"{urllib.parse.quote(k, safe='-_.~')}={urllib.parse.quote(v, safe='-_.~')}"
                         for k, v in sorted((query or {}).items()))
        for attempt in range(1, ATTEMPTS + 1):
            amz_date = datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ")
            payload = hashlib.sha256(body).hexdigest()
            hdrs = {"host": self.host, "x-amz-content-sha256": payload, "x-amz-date": amz_date,
                    **{k.lower(): v for k, v in (headers or {}).items()}}
            req = urllib.request.Request(self.base + path + (f"?{query}" if query else ""),
                                         data=body if method == "PUT" else None, method=method)
            for k, v in hdrs.items():
                if k != "host":              # urllib sends the URL's host:port itself
                    req.add_header(k, v)
            req.add_header("Authorization", sign(method, path, query, hdrs, payload, amz_date,
                                                 self._ak, self._sk, self.region))
            try:
                with urllib.request.urlopen(req, timeout=self.timeout) as r:
                    return r.status, r.read()
            except urllib.error.HTTPError as e:
                status, data = e.code, e.read()
                if status < 500 or attempt == ATTEMPTS:
                    return status, data
            except (urllib.error.URLError, OSError) as e:
                if attempt == ATTEMPTS:
                    raise UploadError(f"{method} {path}: {getattr(e, 'reason', e)}") from e
            time.sleep(attempt)
        raise AssertionError("unreachable")


def _why(status: int, body: bytes) -> str:
    code = re.search(rb"<Code>([^<]*)</Code>", body or b"")
    return f"HTTP {status}" + (f" {code.group(1).decode(errors='replace')}" if code else "")


def put_photo(s3: S3, key: str, data: bytes) -> str:
    """'uploaded' or 'present'; raises UploadError. Never overwrites."""
    digest = hashlib.sha256(data).hexdigest()

    def verify(state: str) -> str:
        status, body = s3.request("GET", key)
        if status != 200:
            raise UploadError(f"{key}: reading it back gave {_why(status, body)}")
        if hashlib.sha256(body).hexdigest() != digest:
            raise UploadError(f"{key}: RustFS holds {len(body)} bytes that differ from the photo; "
                              "it is never overwritten, so delete it by hand as root and rerun")
        return state

    status, body = s3.request("HEAD", key)
    if status == 200:
        return verify("present")
    if status not in (403, 404):     # without ListBucket on the key, a missing object is a 403
        raise UploadError(f"{key}: HEAD gave {_why(status, body)}")
    status, body = s3.request("PUT", key, body=data,
                              headers={"content-type": "image/webp", "if-none-match": "*"})
    if status == 412:                 # written since the HEAD
        return verify("present")
    if status != 200:
        raise UploadError(f"{key}: PUT gave {_why(status, body)}")
    return verify("uploaded")


def push(src: Path, s3: S3) -> int:
    try:
        photos = json.loads((src / "manifest.json").read_text())["photos"]
    except (OSError, ValueError, KeyError) as e:
        print(f"FAIL: no photo manifest in {src}: {e}", file=sys.stderr)
        return 1
    status, body = s3.request("GET", query={"list-type": "2", "prefix": PREFIX, "max-keys": "1"})
    if status != 200:
        print(f"FAIL: listing {s3.bucket}/{PREFIX} gave {_why(status, body)}", file=sys.stderr)
        return 1
    counts, failed = {"uploaded": 0, "present": 0}, 0
    for p in photos:
        key = p["key"]
        try:
            hit = KEY.match(key)
            data = (src / key).read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            if not hit or digest != p["sha256"] or not digest.startswith(hit.group(1)):
                raise UploadError(f"{key}: the image's copy doesn't match its key")
            state = put_photo(s3, key, data)
        except (UploadError, OSError) as e:
            print(f"failed {e}")
            failed += 1
            continue
        counts[state] += 1
        print(f"{state} {key}")
    if failed:
        print(f"FAIL: {failed} of {len(photos)} photos; api must not start", file=sys.stderr)
        return 1
    print(f"ok: {len(photos)} photos in {s3.bucket}: {counts['uploaded']} uploaded, "
          f"{counts['present']} already there", file=sys.stderr)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build", help="make the photos and check them against the migration")
    b.add_argument("--seed", type=Path, default=ROOT / "ml/knowledge/seed")
    b.add_argument("--migration", type=Path, default=MIGRATION)
    b.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("push", help="put the photos in RustFS")
    p.add_argument("dir", type=Path)
    args = ap.parse_args()
    if args.cmd == "build":
        return build(args.seed, args.migration, args.out)
    ak, sk = os.environ.get("RUSTFS_ACCESS_KEY"), os.environ.get("RUSTFS_SECRET_KEY")
    if not ak or not sk:
        print("FAIL: RUSTFS_ACCESS_KEY and RUSTFS_SECRET_KEY are required (.env.seed)",
              file=sys.stderr)
        return 2
    try:
        s3 = S3(os.environ.get("RUSTFS_ENDPOINT", "http://rustfs:9000"),
                os.environ.get("RUSTFS_BUCKET", "pdm-images"), ak, sk,
                os.environ.get("RUSTFS_REGION", "us-east-1"))
        return push(args.dir, s3)
    except UploadError as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
