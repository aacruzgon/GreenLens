# smoke

`smoke.py` drives one scan through the API and checks its event stream. Python 3, standard library only. Run it from `backend/`.

It signs in without a frontend. Clerk's Backend API mints a session token for a fixed test user (`smoke+clerk_test@example.com`), and the script sends it as `Authorization: Bearer`. `api` upserts the user on the first request.

## Prerequisites

- A Clerk **test** instance. `CLERK_SECRET_KEY` must start with `sk_test_`; the script refuses anything else. It reads the key from the environment, or from `--env-file` (default `../infra/dev/.env`). It never prints the key or a token.
- `api` configured with that instance's issuer (its JWKS).
- A plant photo (jpeg, png, webp or heic) that passes the quality gate. For `--face-photo`, a photo with a face.

Clerk endpoints used (confirmed against Clerk's published Backend API OpenAPI spec, `clerk/openapi-specs`, version 2026-08-20; the docs site's reference pages did not load):

| Call | Body or query |
|---|---|
| `GET /v1/users` | `?email_address=<email>` (array query parameter) |
| `POST /v1/users` | `{email_address: [..], password, skip_password_checks: true}` |
| `POST /v1/sessions` | `{user_id}`. Testing only, not available on production instances |
| `POST /v1/sessions/{session_id}/tokens` | `{}` (optional `expires_in_seconds`, minimum 30). Returns `{jwt}` |

A token lives 60 s, so the script mints a new one when the current one is over 45 s old, before each request and each stream reconnect.

## What it does

1. `POST /gardens` (name `smoke <timestamp>`, `--lat`/`--lon`, default Toronto), `POST /scans` with `new_plant`, upload `--photo` as `whole_plant`, `POST /scans/{id}/submit` (expects 202).
2. Follows `GET /scans/{id}/events` and asserts: `seq` runs 1, 2, 3.. with no gaps; `data.seq == id`; the SSE event name equals `data.type`; every started stage is completed; the stream ends at a resting point (`plan_ready` or `failed`). It prints the last status and the event types.
3. Resume, always on: after `--drop-after` events (default 3) it closes the stream and reconnects with `Last-Event-ID`. The first event after must be last + 1, and no seq may repeat.
4. Exits 1 on the first failed assert, and also when the scan ends `failed` unless `--allow-failed`.

If a scan finishes before `--drop-after` events arrive, the resume check is skipped and the script says so.

## Options

| Option | Meaning |
|---|---|
| `--base-url` | default `http://localhost:8080`; use the public control-plane URL for the rig |
| `--photo` | required |
| `--env-file` | default `../infra/dev/.env` |
| `--timeout` | seconds to reach a resting point; default 300, or 600 with `--kill-cmd` |
| `--drop-after N` | resume check after N events; 0 turns it off |
| `--kill-cmd CMD` | shell command run right after the first perception `stage_started` (`plant_check`, `segmentation`, `species`, `disease`, `image_embedding`). It must exit 0, or the script fails (`pkill` exits 1 when nothing matched) |
| `--restart-cmd CMD` | started detached right after the kill, to bring up a replacement worker |
| `--api-restart-cmd CMD` | run while the stream is dropped; then it polls `GET /healthz` until 200 (120 s) and resumes |
| `--face-photo PATH` | uploads to a second scan; asserts 201, prints the quality and any face fields, downloads the stored image |
| `--out-dir DIR` | where the face image goes (default a temp dir) |
| `--allow-failed` | exit 0 when the scan ends `failed` |
| `--verbose` | print every event |

## Locally

```
docker compose -f ../infra/dev.compose.yml up -d        # SurrealDB, RustFS (from the repo root: infra/dev.compose.yml)
cargo run -p api                                        # in backend/, with the dev env
cargo run -p worker -- --caps perception --stub-vision  # the stub worker (flags as the worker's README says)
cargo run -p worker -- --caps fallback                  # the fallback worker
python3 tools/smoke/smoke.py --photo path/to/leaf.jpg --verbose
```

Adjust the worker flags to what `bins/worker` accepts at the time.

## On the rig

Point at the public URL; the key stays a test key:

```
python3 tools/smoke/smoke.py --base-url https://<control-plane-host> --photo leaf.jpg --env-file ../infra/dev/.env
```

## Exit checks (roadmap S1)

A worker killed mid-job. The job's lease lasts 2 minutes (contracts 5); the broker requeues it when the worker's heartbeat is 30 s old or the lease expires, so allow several minutes (the timeout defaults to 600).

```
The killed stub must be slow, or it finishes the stage before the kill lands and nothing is
requeued: start it with `STUB_STAGE_DELAY_MS=10000` (the default is 150 ms a stage). The replacement
runs at the default.

```
# laptop: a slow stub (its own WORKER_ID and token, both in api's WORKER_TOKENS)
STUB_STAGE_DELAY_MS=10000 cargo run -p worker -- --caps perception --stub-vision &
python3 tools/smoke/smoke.py --photo leaf.jpg \
  --kill-cmd "pkill -9 -f 'worker.*stub-vision'" \
  --restart-cmd "WORKER_ID=dev-stub-perception-2 WORKER_TOKEN=... cargo run -p worker -- --caps perception --stub-vision"

# rig
ssh rig 'STUB_STAGE_DELAY_MS=10000 docker compose -f ... up -d perception-stub-worker'
python3 tools/smoke/smoke.py --base-url https://<host> --photo leaf.jpg \
  --kill-cmd "ssh rig 'docker compose -f ... kill perception-stub-worker'" \
  --restart-cmd "ssh rig 'docker compose -f ... up -d perception-stub-worker'"
```

The assert: the stage that was running at the kill starts again at a later seq (the job was
requeued) and completes, and the stream still reaches a resting point.

An api restart mid-scan:

```
python3 tools/smoke/smoke.py --photo leaf.jpg --drop-after 4 \
  --api-restart-cmd "docker compose -f ../infra/... restart api"
```

Every event missed during the restart must arrive once after the resume, none twice.

Face blurring: `--face-photo face.jpg --out-dir /tmp/faces`, then look at the saved image. Faces are blurred and stored, not refused.

Scans and gardens the script creates are left in place (names start `smoke `).
