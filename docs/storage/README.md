# PDM Object Storage (RustFS)

> **Status:** v1, 2026-10-15. Companion to the [architecture document](../architecture/README.md) (section 5, RustFS layout) and [contracts.md](../architecture/contracts.md) (signed image URLs, deletion cascades). The database side is in [docs/database](../database/README.md).
> **Verified:** every point marked ✓ was run against `rustfs/rustfs:1.0.1` with the AWS CLI on 2026-10-15. Points marked *unverified* come from the RustFS docs, source code or issues only.

## Contents

1. [Version and setup](#1-version-and-setup)
2. [Bucket, keys and access](#2-bucket-keys-and-access)
3. [How images are served](#3-how-images-are-served)
4. [Writes and deletes](#4-writes-and-deletes)
5. [Rust client](#5-rust-client)
6. [Operations](#6-operations)
7. [Caveats](#7-caveats)
8. [Fallback](#8-fallback)
9. [Sources](#9-sources)

---

## 1. Version and setup

| Item | Pin |
|---|---|
| Server | `rustfs/rustfs:1.0.1@sha256:1803faef57627e2d9c2e7d89d655d712ddded5389040054987163043fecb6a3c` (2026-10-03, latest stable; Apache-2.0) |
| Admin CLI | `rustfs/rc` 0.1.36 (`sha256:ab024bfebee49a750ce886b4c70963ccd9ddaa03f491704a90710641d7a26699`) |
| Rust client | `aws-sdk-s3`, the client RustFS documents and uses in its own end-to-end tests |
| Mode | Single node, single disk, one volume at `/data` |

**Why pin 1.0.1, and how long.**
- 1.0.0 shipped on 2026-09-16, after about 100 alpha, beta and release-candidate builds this year.
- `1.0.2-preview.1` (2026-10-14) fixes the interrupted-delete leak, a memory leak and a metrics panic. It is a preview, though, and an earlier preview-to-preview upgrade broke buckets and couldn't be rolled back ([#8003](https://github.com/rustfs/rustfs/issues/8003)).
- Stay on 1.0.1 for build week. Move to 1.0.2 only once it's a final release, and only after a backup.

**Where it runs.** On the same host as the API (wherever open decision 2 puts the control plane), on the API's Docker network. It is never exposed to the internet: the app and the workers only ever reach images through the API (section 3).

**Docker Compose service.**

```yaml
services:
  rustfs:
    image: rustfs/rustfs:1.0.1@sha256:1803faef57627e2d9c2e7d89d655d712ddded5389040054987163043fecb6a3c
    restart: unless-stopped                    # memory creeps up on single-disk nodes (caveat 3)
    security_opt: ["no-new-privileges:true"]
    environment:
      RUSTFS_VOLUMES: /data
      RUSTFS_ADDRESS: 0.0.0.0:9000
      RUSTFS_ACCESS_KEY: ${RUSTFS_ROOT_ACCESS_KEY:?set in .env}   # A–Z and 0–9 only, no "/"
      RUSTFS_SECRET_KEY: ${RUSTFS_ROOT_SECRET_KEY:?set in .env}   # never the default "rustfsadmin"
      RUSTFS_REGION: us-east-1
      RUSTFS_CONSOLE_ENABLE: "false"           # the admin console stays off
      RUSTFS_CHECK_UPDATE: "false"             # no outbound version check
      RUSTFS_OBS_LOGGER_LEVEL: info
      RUSTFS_OBS_LOG_STDOUT_ENABLED: "true"    # otherwise docker logs shows nothing in production mode
    ports:
      - "127.0.0.1:9000:9000"                  # host-only, for the admin CLI and backups
    volumes:
      - rustfs-data:/data                      # the image runs as UID 10001; a bind mount needs chown 10001:10001
      - rustfs-logs:/logs
    mem_limit: 2g                              # the documented minimum
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://127.0.0.1:9000/health/ready"]   # ✓ 200 once storage and IAM are ready
      interval: 10s
      timeout: 5s
      retries: 6
      start_period: 30s
volumes:
  rustfs-data:
  rustfs-logs:
```

- ✓ These environment variables start 1.0.1 with the console off.
- The API service sets `depends_on: { rustfs: { condition: service_healthy } }`.
- Don't copy RustFS's 4-volume example compose file. Several data directories on one physical disk fail its disk-independence check at start-up (*unverified*, from the source).

---

## 2. Bucket, keys and access

**One private bucket: `pdm-images`.**
- No versioning: it would turn prefix deletes into delete markers.
- No object lock, no lifecycle rules (caveat 7), and no server-side encryption: the host's disk encryption covers data at rest.
- ✓ Buckets are private by default: an unsigned `GET` returns `403`.

**Key layout** (architecture section 5):

```
gardens/{garden_id}/scans/{scan_id}/{image_id}.webp
gardens/{garden_id}/scans/{scan_id}/masks/{mask_id}.png
gardens/{garden_id}/scans/{scan_id}/depth/{image_id}.png
gardens/{garden_id}/scans/{scan_id}/depth_confidence/{image_id}.png
gardens/{garden_id}/chat/{conversation_id}/{image_id}.webp
```

- Every ID is a ULID that the API generates, and the key is stored on the record (`scan_image.storage_key`, `depth_key` and `depth_confidence_key`, `mask.storage_key`, `chat_image.storage_key`), never rebuilt from user input. Depth maps from LiDAR phones (architecture 6.7) share their photo's `image_id`.
- ✓ An empty segment (`gardens//scans/…`) is rejected with `400 InvalidArgument`, and so are `.` and `..` segments. The key builder asserts every segment is a 26-character ULID.
- Prefixing by garden makes deleting a garden one prefix delete. A plant's objects are deleted scan by scan, through each scan's prefix.

**Keys and policies.**

| Identity | Can do | Used by |
|---|---|---|
| root | Everything | The one-time setup script and backups. Never the API |
| `PDMAPI01` with policy `pdm-api` | ✓ `PutObject`, `GetObject` and `DeleteObject` under `pdm-images/gardens/*`, plus `ListBucket` on `pdm-images`. ✓ Writing outside `gardens/`, creating a bucket and deleting a bucket are all denied | The API |

Setup script, run once per environment (✓ these commands were run against 1.0.1):

```bash
docker run --rm --network container:<rustfs container> -v "$PWD":/w --entrypoint sh rustfs/rc -c '
  rc alias set pdm http://127.0.0.1:9000 "$ROOT_AK" "$ROOT_SK" &&
  rc mb pdm/pdm-images &&
  rc admin policy create pdm/ pdm-api /w/pdm-api.json &&
  rc admin user add pdm/ PDMAPI01 "$API_SK" &&
  rc admin policy attach pdm/ pdm-api --user PDMAPI01'
```

```json
{"Version":"2012-10-17","Statement":[
 {"Effect":"Allow","Action":["s3:ListBucket"],"Resource":["arn:aws:s3:::pdm-images"]},
 {"Effect":"Allow","Action":["s3:GetObject","s3:PutObject","s3:DeleteObject"],"Resource":["arn:aws:s3:::pdm-images/gardens/*"]}]}
```

✓ `DeleteObjects` (the batch delete) is allowed by `s3:DeleteObject`.

---

## 3. How images are served

**The API serves the bytes; RustFS never signs a URL.** This is the pattern contracts section 1 already describes:

1. Every image `url` the API returns is the API's own short-lived signed URL:
   - app images: `GET /scans/{id}/images/{image_id}?exp=&sig=` and `/masks/{mask_id}`, and `GET /conversations/{id}/images/{image_id}`;
   - worker images: `GET /internal/jobs/{job_id}/images/{image_id}`, authorised by the lease token.
   - Depth maps are never served: no route reads `depth/` or `depth_confidence/` except the API's own measurement stage.
2. `sig` is an HMAC-SHA256, under a server secret, of the method, the path and `exp`. The URL expires after 15 minutes. A bad or expired signature gets `403 invalid_signature`.
3. The API checks the signature, calls `GetObject` with the record's `storage_key`, and streams the body. It sets `Content-Type`, `ETag` and `Cache-Control: private, max-age=900`.

**Why not RustFS's own presigned URLs.**
- ✓ A presigned URL stops working when the `Host` header changes: the same URL that returns `200` directly returns `403` with `Host: media.example.com`. The host is part of the SigV4 signature, and RustFS deliberately ignores `X-Forwarded-Host`.
- Behind a Cloudflare Tunnel or any proxy that rewrites the host, buffers bodies or turns `HEAD` into `GET`, presigned URLs fail in ways that are hard to debug.
- Streaming through the API needs no CORS setup for the web build, keeps RustFS off the internet, and works unchanged with any S3 backend.
- The cost is API bandwidth: images are 100–500 KB, a scan has at most about 8, and the API serves them only to members.

---

## 4. Writes and deletes

**Uploads.**
1. The API processes the image first: EXIF orientation applied, metadata stripped, faces blurred, resized, encoded as WebP (architecture 8.1).
2. It then calls `PutObject` with `Content-Type` and `If-None-Match: *`, so a key is never overwritten. A shot from a LiDAR phone also puts its depth map, and its confidence map when sent, as 16-bit and 8-bit PNGs, after they are rotated with the photo and their face regions zeroed. If one of those puts fails, the depth is dropped and the photo kept (contracts 3). ✓ A second put to an existing key fails with `412 PreconditionFailed`.
3. Then it creates the database record.

A crash between steps 2 and 3 leaves an object without a record, never a record without its object. A daily sweep lists each garden's prefix and deletes objects older than an hour that no record points to. Every key field counts as a pointer: `scan_image.storage_key`, `depth_key` and `depth_confidence_key`, `mask.storage_key` and `chat_image.storage_key`. A sweep that checked only `storage_key` would delete every depth map an hour after its upload.

**Deletes** (garden, plant or scan deletion, account deletion, chat images). A shot removed before submit is deleted by key, with its depth and confidence maps:
1. List the prefix with `ListObjectsV2` (paginated).
2. Delete the keys in `DeleteObjects` batches of up to 1000, and check the `Errors` list in each response.
3. ✓ This removes exactly the prefix's objects and leaves every other prefix alone.

**Never cancel a delete.** In 1.0.1, a `DeleteObject` or `DeleteObjects` call interrupted by a client disconnect or timeout can leave objects that can't be listed or deleted, and their disk space is lost ([#8374](https://github.com/rustfs/rustfs/issues/8374), fixed only in 1.0.2-preview.1). Rules:
- run deletes in a detached background task, not inside a request;
- give them a 120-second timeout;
- retry on failure, because deleting a missing key is a no-op.

Don't use RustFS's `x-rustfs-force-delete` recursive delete. It is non-portable, needs its own permission, and holds a bucket-wide write lock while it runs.

The database records are deleted after the objects (database, section 4). A failure partway is retried from the start, which is safe because both steps are idempotent.

---

## 5. Rust client

```rust
use aws_sdk_s3::{config::{BehaviorVersion, Builder, Credentials, Region}, Client};

fn storage_client(endpoint: &str, access_key: &str, secret_key: &str) -> Client {
    Client::from_conf(
        Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))          // must match RUSTFS_REGION
            .endpoint_url(endpoint)                     // http://rustfs:9000 on the Docker network
            .credentials_provider(Credentials::new(access_key, secret_key, None, None, "rustfs"))
            .force_path_style(true)                     // RustFS needs path-style unless RUSTFS_SERVER_DOMAINS is set
            .build(),
    )
}
```

- Use one `Client` per process. It is cheap to clone and pools its connections.
- Wrap it in a small `ObjectStore` trait (`put`, `get_stream`, `delete_prefix`) in the `blob` crate (architecture section 3), so tests use an in-memory store and the backend stays swappable.
- `get_object` returns a `ByteStream`: stream it into the axum response body, don't buffer it.
- The AWS SDK sends CRC32 checksums by default, and RustFS accepts them. ✓ The AWS CLI v2.36, which sends them too, worked for every operation above.

---

## 6. Operations

| Task | How |
|---|---|
| Health | `GET /health/ready` (readiness) and `/health/live`, both unauthenticated on port 9000 |
| Backups | A nightly `rclone sync rustfs:pdm-images <off-host remote>`. The rclone remote is `type = s3`, `provider = Other`, `endpoint = http://127.0.0.1:9000`, `region = us-east-1`, `force_path_style = true`. Use a read-only key for it. Single-disk mode has no redundancy, so this backup is the only protection against disk loss or bitrot |
| Capacity | A stored photo is 100–500 KB. A scan from a LiDAR phone takes roughly two to three times the space of an ordinary scan, by estimate: its photos are stored larger (a long side of at least 2048 pixels, architecture 6.7), and each comes with a 16-bit depth map of a few hundred pixels per side, about 0.3–0.9 MB as a PNG, plus an 8-bit confidence map on the ARKit path. The day-1 LiDAR check records the real sizes; size the disk and the backup remote from them |
| Logs | `docker logs`, with `RUSTFS_OBS_LOG_STDOUT_ENABLED=true` |
| Metrics | None for the hackathon. RustFS only pushes metrics over OTLP, and the 1.0.1 OTLP exporter panics ([#8364](https://github.com/rustfs/rustfs/issues/8364)). The health probe and the status page are enough |
| Upgrades | Back up, change the pinned tag, then `docker compose pull rustfs && docker compose up -d --no-deps rustfs`. Never `down -v`, which deletes the volume. Rollback isn't guaranteed (#8003) |

---

## 7. Caveats

1. **Interrupted deletes leak objects in 1.0.1** ([#8374](https://github.com/rustfs/rustfs/issues/8374), [#8464](https://github.com/rustfs/rustfs/issues/8464)). Never cancel a delete (section 4).
2. ✓ **Presigned URLs break when the host changes.** We don't use them (section 3).
3. **Memory creeps up on single-disk nodes** ([#8121](https://github.com/rustfs/rustfs/issues/8121), open): one report saw a 1 GiB container killed after a week. Use a 2 GiB limit with `restart: unless-stopped`.
4. **No redundancy in single-disk mode.** Bitrot is detected but can't be repaired. Backups are mandatory.
5. ✓ **Keys with `//`, `.` or `..` segments are rejected.** Validate IDs before building keys.
6. **Default credentials are well known** (`rustfsadmin`), and port 9000 also serves the admin API. Set both keys, and keep 9000 bound to `127.0.0.1` and the Docker network.
7. **Any lifecycle rule slows every PUT** ([#8560](https://github.com/rustfs/rustfs/issues/8560), open): each upload then lists its parent directory. We use no lifecycle rules; orphans are cleaned by the API's sweep.
8. **Background disk reads on single-disk nodes** ([#8377](https://github.com/rustfs/rustfs/issues/8377)), mostly on HDDs. If foreground latency suffers, `RUSTFS_SCANNER_ENABLED=false` turns the scanner off.
9. **Pre-1.0 versions are unsupported**, and 35 security advisories were fixed before 1.0.0. Never run an older image.

---

## 8. Fallback

If RustFS proves unstable during build week, **SeaweedFS** (Apache-2.0, 4.48) is the closest drop-in: its S3 gateway on port 8333 supports everything we use. Garage also works but lacks bucket policies, so it would need its own key setup.

MinIO is not an option: its community repository is archived and no longer maintained.

Because the API only uses standard operations through one trait, switching changes the endpoint, the keys and the setup script, nothing else. The standard operations are `PutObject`, `GetObject`, `ListObjectsV2` and `DeleteObjects`, with path-style addressing and no presigning. `If-None-Match` on PUT would be re-checked on the new backend.

---

## 9. Sources

- Releases and issues: https://github.com/rustfs/rustfs/releases, https://github.com/rustfs/rustfs/issues
- Environment variables: https://docs.rustfs.com/en/reference/environment-variables
- Single-node install and requirements: https://docs.rustfs.com/en/installation/linux/single-node-single-disk
- S3 compatibility matrix: https://github.com/rustfs/rustfs/blob/main/docs/architecture/s3-compatibility-matrix.md
- IAM and policies: https://docs.rustfs.com/en/security-compliance/iam
- Rust SDK: https://docs.rustfs.com/en/developer/sdk/rust
- Reverse proxies and signed requests: https://github.com/rustfs/rustfs/blob/main/docs/operations/reverse-proxy.md
- Upgrades: https://docs.rustfs.com/en/operations/upgrade/container
- `rc` CLI: https://github.com/rustfs/cli
- SeaweedFS: https://github.com/seaweedfs/seaweedfs
