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

**What it writes.** Without `--scan-id`, it first drives a new scan of `--photo` (and `--leaf-photo` as `leaf_closeup`, without which the disease head never runs) to `plan_ready` in a garden it makes, answering each indicator question with `--answer` (default `yes`). With `--scan-id` it uses that scan: it chats there, and the card check confirms one revision of its plan and rejects a second card. Nothing else touches that scan's garden: the memory, task, belief and map plants go in a fresh `s5 work …` garden, and the frost plants in a fresh `s5 frost …` garden. `--cleanup` deletes the gardens the script made, and only those. `--expect-problem tomato_early_blight` stops the run when the scan's primary problem is another.

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

## S4: the diagnosis loop (`s4.py`)

`s4.py` runs backend roadmap S4's three exit checks ([roadmap 8](../../../docs/backend/roadmap.md#8-s4-the-diagnosis-loop)) against a running `api`, in `s5.py`'s style: it signs in the same way, prints PASS, FAIL or SKIP with the evidence for each check, and exits 1 if any check failed. It makes one `s4 <timestamp>` garden and a tomato per scan, and uploads `--photo` as `whole_plant` and `--leaf-photo` as `leaf_closeup` on every scan. The disease head reads only `leaf_closeup` and `leaf_underside` shots, so leave out `--leaf-photo` and the head never runs.

| Check | What it proves | How long |
|---|---|---|
| `scan` | One scan to `plan_ready`, each indicator question answered from `--answers`. From `GET /scans/{id}`: `planned`; a diagnosis with a `primary` (`--expect-problem` names it); indicator evidence on the candidates (`matched`, `contradicted` or `unsure`); a plan written from that diagnosis, provisional exactly when it is `uncertain`, with at least one step, every step citing a source that is in the plan's `citations`; the fixed safety note on every `chemical` step; a step without a `treatment_id` that names a product of contracts 7's fixed list is `chemical`; no number followed by a measure in the summary, `watch_for` or a step; a `model_version` on every observation, the diagnosis's `model_versions`, and the plan's `llm_model` and `prompt_version`. Prints the diagnosis status, the primary's score and each candidate's score, with `classifier_weight` when the API sends it | the scan's run: 1 to 3 minutes on the rig; 10 minutes more if a photo request is left to the scheduler |
| `kill` | A second scan. Right after `--kill-after`'s event it runs `--api-kill-cmd`, then `--api-start-cmd` detached, waits for `GET /healthz`, and resumes the stream with `Last-Event-ID`. The step holding the claim must start again once its claim is stale (a `stage_started` of its stage at least the limit minus 30 s after the kill's event, by the events' own `ts`), and commit. The scan reaches `planned` with seq 1.. unbroken across the restart, one `diagnosis` and one `plan_ready` in the run, one `candidates_selected` before the VLM stage, no stage left open, and plan version 1. SKIP without both commands | the run plus about 5.5 minutes for `plan` (2.5 for `retrieve` or `score`) plus the restart |
| `late` | A third scan, submitted before the other checks so its wait overlaps them, whose question is never answered: the sweep continues it after 10 minutes (`status_changed` `awaiting_answer → diagnosed`, an `uncertain` diagnosis, a provisional plan holding the open question). The script notes the plan's tasks, then answers `--late-answer` (a photo request gets `--leaf-photo` with `question_id`). Then, in order: `question_resolved` (`by: user`, or `photo`), a `diagnosis` that `supersedes` the old one, `plan_superseded` with `late_answer`; `GET /scans/{id}` shows version 2 superseding version 1, written from the new diagnosis, still `planned`; and every open task of the old plan is dismissed with `plan_superseded`. SKIP when the scan asks no question, or when the old plan had no open task (the dismissal can't be seen) | 10 minutes from the question, up to 30 s for the sweep's tick, then the plan writer; about 1 minute for the re-plan |

A full run takes about 15 minutes. `--late-scan-id` reuses a scan already continued past its question (`planned` with the question still open), so a rerun of `late` skips the 10 minutes; the test user must own it, so use the same `--email`.

**Kill points.** `--kill-after` takes `diagnosis` (the default: `score` has committed and set the claim to `plan`), `stage_started:plan`, `stage_started:scoring` or `stage_started:fusion` (`score`), and `stage_started:context` or `stage_started:retrieve` (`retrieve`, which starts after the last perception stage's result is stored). Each step sends its progress events as it works and its outcome events only after its commit ([backend flows 3](../../../docs/backend/flows.md#3-a-scan-from-submit-to-plan)), so these events mean the claim is held and not yet committed. `plan` is the widest window, because the plan writer calls the hosted LLM; `retrieve` lasts a few seconds, so a kill that lands after its commit fails the check with "committed before the kill landed". The sweep runs every 30 s (the first 10 s after start) and re-claims a step after 2 minutes, 5 for `plan` (`Pipeline::sweep`).

**Answers.** `--answers` is a comma list of `<indicator>=yes|no|unsure`, with `*` as the default (`*=unsure` when absent). A question's `indicator_id` is the indicator's own slug, without its problem (contracts 3), and a key matches it exactly, as `<problem>.<slug>`, or as its suffix: `concentric_rings_bullseye=yes,*=unsure` answers early blight's target-spot question `yes` and the rest `unsure`. The `late` scan's question is never answered from it.

**`--real-models`** fails `scan` when an observation's `model_version` is `stub` or `partial`, or the diagnosis's `model_versions` name the stub: J4 needs a real model's version on every result. Leave it off on a laptop's stub.

**`--out FILE`** writes a JSON fixture (vision dev-roadmap J4's event log): for each check, every event of the scan's stream (`type`, `seq`, `data`), the last `GET /scans/{id}`, and the model versions it shows. Signed URLs keep their path, without the signature.

**Photos.** S2 and S5 ran on PlantDoc's test photo `dscn3175.jpg`, a tomato with early blight: `python -m ml.training.data.fetch plantdoc_cls` puts it, git-ignored, at `ml/training/data/raw/plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/test/Tomato Early blight leaf/dscn3175.jpg`. Upload the whole photo as `--photo` and a leaf close-up cropped from it as `--leaf-photo`; the crop isn't in the repository. `../app/assets/mock/tomato-blight.jpg` (early-blight leaves, CC BY 2.0) is another close-up. On S5's run, early blight won only with its target-spot question answered `yes` ([roadmap 9](../../../docs/backend/roadmap.md#9-s5-chat-and-the-garden-map)).

### Locally

`api` runs from cargo on the laptop, not in `infra/dev.compose.yml`. Build it once, start it from `backend/` as the kill command expects, and keep the workers of `s5.py`'s section running:

```
cargo build -p api
(set -a; . ../infra/dev/.env; set +a; exec ./target/debug/api) >> /tmp/s4-api.log 2>&1 &
python3 tools/smoke/s4.py --photo plant.jpg --leaf-photo leaf.jpg \
  --answers concentric_rings_bullseye=yes,*=unsure --out /tmp/s4-fixture.json \
  --api-kill-cmd "pkill -9 -f '[t]arget/debug/api'" \
  --api-start-cmd "(set -a; . ../infra/dev/.env; set +a; exec ./target/debug/api) >> /tmp/s4-api.log 2>&1"
```

The `[t]` keeps the pattern from matching the command lines that contain it, the script's own included; `pkill` exits 1 when nothing matched, and the check fails. Kill with `-9`: on SIGTERM `api` drains, and the step would finish. Add `api`'s other variables (Agent Memory, `POISON_CONTROL_TEXT`) to both commands if you run with them.

### Against the deployed control plane

The control plane runs on the rig's 3070 host, from the checkout at `/srv/pdm/repo` ([infra 1](../../../docs/infra/README.md#1-scope-and-layout)):

```
CP="cd /srv/pdm/repo && docker compose -f infra/control-plane.compose.yml --env-file /srv/pdm/env/.env.compose"
python3 tools/smoke/s4.py --base-url https://<control-plane-host> \
  --photo dscn3175.jpg --leaf-photo dscn3175-leaf.jpg \
  --answers concentric_rings_bullseye=yes,*=unsure --expect-problem tomato_early_blight --real-models \
  --email s4-rig+clerk_test@example.com --phone-suffix 192 --out s4-rig.json \
  --api-kill-cmd "ssh <rig> '$CP kill api'" --api-start-cmd "ssh <rig> '$CP start api'"
```

`docker compose kill` sends SIGKILL. Don't use `stop` or `restart`: they send SIGTERM, and `api`'s 190 s grace period lets the step finish. `start` starts the same container with the same environment; if the `unless-stopped` policy has already brought it back, `start` does nothing and exits 0. `api`'s health check allows 60 s for start-up, and the script waits up to `--health-timeout` (300 s) for `/healthz` through the public URL. While `api` is down, the rig's workers can't claim or complete jobs; they retry.
