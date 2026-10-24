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

## S5: chat and the garden map (`s5.py`)

`s5.py` runs backend roadmap S5's live checks against a running `api`, in `smoke.py`'s style, and signs in the same way. It prints PASS, FAIL or SKIP with the evidence for each check, then the chat turns' token usage. It exits 1 if any check failed.

| Check | What it does |
|---|---|
| `chat` | Two turns on a scan conversation. The first must call a tool (`get_forecast`, with `tool_started` and `tool_finished`). The second runs on context rebuilt as text (agent open question 18.2). Prints the route, the tools, the time to first text and the usage |
| `card` | Asks for a plan change. Expects one `revise_plan` card that changes a step, confirms it (new version, `supersedes` the base, `reason: chat_revision` with `--db-check`, a retried confirm gives the same plan), then rejects a second card (confirming it after gives `409`) |
| `tasks` | A `create_task` card carrying the plant, a due date, a 3-day repeat and a detail, confirmed. Then a `complete_task` card, confirmed (`done_at` set) |
| `memory` | A garden fact gives `memory_saved` with a private `chat` memory before `message_completed`; then `POST /memories/{id}/share`. That the plan writer reads only shared memories is covered by unit tests, named in the output |
| `beliefs` | Agent Memory. Two watering notes and a soil note on a new plant: one current watering belief and one superseded, within one sweep (10 s). A private chat memory never appears. Deleting a note removes its belief while the others stay. Deleting the plant leaves no entity in Agent Memory. SKIP when `GET /plants/{id}/beliefs` says `available: false` |
| `map` | `PUT /gardens/{id}/map` with two tomatoes 30 cm apart (under 0.75 × 60 cm), a lettuce and a basil. A second `PUT` from the same `base_version` gives `409`. `GET` back, the neighbours within 1.5 m and the crowding, then a chat turn that must run `get_garden_map` and mention the crowding |
| `dose` | A prompt asking for tablespoons per gallon and ml per litre. No streamed sentence may hold an amount |
| `unset` | Runs `--restart-without-agent-memory`, then expects `available: false` and a working chat turn |

Without `--scan-id` it first drives a new scan of `--photo` to `plan_ready`, answering each indicator question with `--answer` (default `yes`). With `--scan-id` it uses that scan, for example the rig's early-blight scan. The test user must own the scan.

**One test user per run.** Each user has a daily chat budget (`daily_chat_tokens`), and a run's tool turns use a lot of it, so give each run its own `--email` (a `+clerk_test` address) and `--phone-suffix` (Clerk's test numbers +1555555 0100–0199; one already in use is refused). Split a full run into batches if needed.

**Cost.** One full run is about 15 chat turns, about 1.2 million input tokens (most of them cache reads) and 25,000 output tokens on `claude-opus-5-5`. That is about US$2 to US$3. The script prints an upper bound at uncached prices.

### Locally

As for `smoke.py`, plus Agent Memory's three variables for `api` only, and both workers: the stub for perception (with the label versions of `migrations/knowledge.surql`'s first line) and the fallback for the VLM job. Without `HF_TOKEN`, give the fallback a manifest without the `bioclip_labels` and `disease_head` entries. It then answers VLM jobs only.

```
AGENT_MEMORY_URL=… AGENT_MEMORY_CONTEXT_ID=… AGENT_MEMORY_API_KEY=… cargo run -p api
WORKER_ID=… WORKER_TOKEN=… STUB_LABEL_SET_VERSION=… STUB_CLASSIFIER_LABELS_VERSION=… \
  STUB_PROBLEMS=tomato_early_blight cargo run -p worker -- --caps perception --stub-vision
cargo run -p worker -- --caps fallback
python3 tools/smoke/s5.py --photo ../app/assets/mock/tomato-blight.jpg \
  --email s5a+clerk_test@example.com --phone-suffix 157 \
  --agent-memory-env ../.env.agent-memory --db-check \
  --restart-without-agent-memory "<restart api without the AGENT_MEMORY_* variables>"
```

`--agent-memory-env` reads the plant's entity straight from Agent Memory with the same narrowed key, for the private-memory and plant-deletion checks. It accepts the `AGENT_MEMORY_*` names or the vendor's old `SPECTRON_*` ones. `--db-check` reads the revised plan's stored reason with the dev compose's SurrealDB root user, so it is for the laptop only.

### Against the deployed control plane

```
python3 tools/smoke/s5.py --base-url https://<control-plane-host> --photo <an early-blight leaf> \
  --email s5-rig+clerk_test@example.com --phone-suffix 191 \
  --agent-memory-env <file with the deployment's AGENT_MEMORY_*> \
  --restart-without-agent-memory "ssh rig '<restart api without AGENT_MEMORY_*>'"
```

Leave out `--db-check`. With `--photo`, the rig's models diagnose the scan the script makes, which gives S5's early-blight exit check its real diagnosis. A later run can reuse that scan with `--scan-id` and the same `--email`, because the test user must own the scan.
