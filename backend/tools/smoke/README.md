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

`s5.py` runs backend roadmap S5's live checks against a running `api`, in `smoke.py`'s style, and signs in the same way. It prints PASS, FAIL or SKIP with the evidence for each check, then the chat turns' token usage and a cost range. It exits 1 if any check failed. SKIP means the check couldn't prove its claim on this run (an option missing, or the case didn't come up), never a pass.

| Check | What it proves |
|---|---|
| `openapi` | `Message` and `MessageReplay` are schemas in `/openapi.json` (or the committed file), and `POST /conversations/{id}/messages` lists `application/json` (`MessageReplay`) and `text/event-stream` under `200` |
| `poison` | With `--poison-text` (the `POISON_CONTROL_TEXT` `api` runs with): a poisoning message streams exactly that text, with no tool call. SKIP without it |
| `chat` | Two turns on a scan conversation. The first calls `get_forecast` (`tool_started` and `tool_finished`). The second runs on context rebuilt as text (agent open question 18.2). Prints the route, the tools, the time to first text and the usage |
| `card` | One `revise_plan` card with a step about the lowest leaves, confirmed: a new version that `supersedes` the base and holds that step's title and detail as shown, `reason: chat_revision` with `--db-check`, and a retried confirm gives the same plan. A second card is rejected, and confirming it then gives `409` |
| `tasks` | A `create_task` card with the plant, the asked due date, a 3-day repeat and a detail that says to look under the leaves, confirmed. Then a `complete_task` card for that task, confirmed (`done_at`) |
| `memory` | A garden fact gives `memory_saved` with a private `chat` memory before `message_completed`, then `POST /memories/{id}/share`. That the plan writer reads only shared memories is covered by unit tests, named in the output |
| `beliefs` | Agent Memory, on a new plant. Two watering notes and a soil note leave one current watering belief and the older one with `superseded_at`, within one sweep (10 s). A private chat memory about the plant never appears in the beliefs, nor in the entity read with `--agent-memory-env`, which must first hold the soil note's text. Deleting the newer note removes it, and the older one is current again. SKIP when `available: false` |
| `beliefs-erase` | With `--agent-memory-env`: deleting the plant leaves no entity in Agent Memory within one sweep. SKIP without it |
| `map` | `PUT /gardens/{id}/map` with two tomatoes 30 cm apart (under 0.75 × 60 cm), a lettuce and a basil; a second `PUT` from the same `base_version` gives `409`. Then a chat answer that runs `get_garden_map` and shows a `garden_map` block: the API's own neighbours (tomato A with tomato B and the lettuce, basil alone), and the answer names the tomatoes' crowding with a distance. The crowding rule isn't in `get_garden_map`'s result (fusion applies it during a scan), so the answer's words are its evidence |
| `frost` | In a new garden at `--frost-lat`/`--frost-lon` (default Yellowknife) with a tomato and a lettuce: "Will the frost on Thursday hurt my plants?" streams a `frost_planner` block after its `get_forecast` call's `tool_finished`, each plant's `frost_tender` as `--frost-expect` says (`tender` true, `hardy` false, `none` absent), and the stored message holds the same blocks. SKIP when that forecast has no frost night |
| `dose` | "How many tablespoons … per gallon …?": the route is `safety_sensitive` and no streamed sentence holds an amount (digits or number words with a unit, or a unit per a volume). SKIP (not exercised) unless the dose check replaced a sentence, which shows as the safety note in the stream |
| `unset` | With `--restart-without-agent-memory` and `--restore-cmd`: `available: false` and a working chat turn, then `api` restored and Agent Memory back. SKIP without both |

**What it writes.** Without `--scan-id`, it first drives a new scan of `--photo` to `plan_ready` in a garden it makes, answering each indicator question with `--answer` (default `yes`). With `--scan-id` it uses that scan: it chats there, and the card check confirms one revision of its plan and rejects a second card. Nothing else touches that scan's garden: the memory, task, belief and map plants go in a fresh `s5 work …` garden, and the frost plants in a fresh `s5 frost …` garden. `--cleanup` deletes the gardens the script made, and only those. `--expect-problem tomato_early_blight` stops the run when the scan's primary problem is another.

**One test user per run.** Each user has a daily chat budget (`daily_chat_tokens`). Give each run its own `--email` (a `+clerk_test` address) and `--phone-suffix` (Clerk's test numbers +1555555 0100–0199; one already in use is refused).

**Cost.** One full run is about 15 chat turns on `claude-opus-5-5`, and costs about US$2 to US$3. The script prints a range, because the stream reports only the total input: every input token a cache read (low) or a cache write (high).

**Clerk.** The control plane trusts the same Clerk **development** instance as the laptop (S1), so `--env-file` is any file holding that instance's `CLERK_SECRET_KEY` (`sk_test_…`). The script refuses any other key.

### Locally

As for `smoke.py`, plus Agent Memory's three variables and `POISON_CONTROL_TEXT` for `api`, and both workers: the stub for perception (with the label versions of `migrations/knowledge.surql`'s first line) and the fallback for the VLM job. Without `HF_TOKEN`, give the fallback a manifest without the `bioclip_labels` and `disease_head` entries. It then answers VLM jobs only.

The seed has no species `frost` yet (the knowledge track's job), so `--frost-expect` defaults to `none` for both. To see a `true` on the laptop, set one in the dev database, restart `api` (its knowledge index is read at start), and put it back after:

```
UPDATE species:tomato SET frost = 'tender';     -- then --frost-expect tomato=tender,lettuce=none
UPDATE species:tomato UNSET frost;              -- afterwards, then restart api again
```

```
AGENT_MEMORY_URL=… AGENT_MEMORY_CONTEXT_ID=… AGENT_MEMORY_API_KEY=… POISON_CONTROL_TEXT="…" cargo run -p api
WORKER_ID=… WORKER_TOKEN=… STUB_LABEL_SET_VERSION=… STUB_CLASSIFIER_LABELS_VERSION=… \
  STUB_PROBLEMS=tomato_early_blight cargo run -p worker -- --caps perception --stub-vision
cargo run -p worker -- --caps fallback
python3 tools/smoke/s5.py --photo ../app/assets/mock/tomato-blight.jpg \
  --email s5a+clerk_test@example.com --phone-suffix 157 \
  --agent-memory-env ../.env.agent-memory --db-check --poison-text "…" \
  --restart-without-agent-memory "<restart api without AGENT_MEMORY_*>" \
  --restore-cmd "<restart api as it was>" --cleanup
```

`--agent-memory-env` is read as a file, never from the environment. It accepts the `AGENT_MEMORY_*` names or the vendor's old `SPECTRON_*` ones. `--db-check` reads the revised plan's stored reason with the dev compose's SurrealDB root user, so it is for the laptop only.

### Against the deployed control plane

```
python3 tools/smoke/s5.py --base-url https://<control-plane-host> --photo <an early-blight leaf> \
  --expect-problem tomato_early_blight --email s5-rig+clerk_test@example.com --phone-suffix 191 \
  --agent-memory-env <file with the deployment's AGENT_MEMORY_*> --poison-text "<its POISON_CONTROL_TEXT>" \
  --restart-without-agent-memory "ssh rig '<restart api without AGENT_MEMORY_*>'" \
  --restore-cmd "ssh rig '<restart api as deployed>'" --cleanup
```

Leave out `--db-check`. With `--photo`, the rig's models diagnose the scan the script makes, which gives S5's early-blight exit check its real diagnosis. A later run can reuse that scan with `--scan-id` and the same `--email`, because the test user must own the scan.
