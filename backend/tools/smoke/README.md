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

The control plane runs on the rig's 3070 host ([infra 1](../../../docs/infra/README.md#1-scope-and-layout)). On that host the compose project runs from the checkout's `infra/` at `~/Github/GreenLens/infra`, with its env files in `/srv/pdm/env`, and the scripts run from `~/Github/GreenLens/backend`. There the commands need no `ssh`; from another machine, wrap each in `ssh <rig> '…'`:

```
CP="docker compose -f $HOME/Github/GreenLens/infra/control-plane.compose.yml --env-file /srv/pdm/env/.env.compose"
python3 tools/smoke/s4.py --base-url https://pdm-cp.tailb3ce07.ts.net --env-file /srv/pdm/env/.env.api \
  --photo dscn3175.jpg --leaf-photo dscn3175-leaf.jpg \
  --answers concentric_rings_bullseye=yes,*=unsure --expect-problem tomato_early_blight --real-models \
  --email s4-rig+clerk_test@example.com --phone-suffix 192 --out s4-rig.json \
  --api-kill-cmd "$CP kill api" --api-start-cmd "$CP start api"
```

`docker compose kill` sends SIGKILL. Don't use `stop` or `restart`: they send SIGTERM, and `api`'s 190 s grace period lets the step finish. `start` starts the same container with the same environment; if the `unless-stopped` policy has already brought it back, `start` does nothing and exits 0. `api`'s health check allows 60 s for start-up, and the script waits up to `--health-timeout` (300 s) for `/healthz` through the public URL. While `api` is down, the rig's workers can't claim or complete jobs; they retry.

## S9: farm connectors and Era (`s9.py`)

`s9.py` runs backend roadmap S9's live exit checks 1 and 4 ([roadmap 12](../../../docs/backend/roadmap.md#12-s9-farm-connectors-and-era)) against a running `api`, in `s4.py`'s and `s5.py`'s style. It signs in the same way, as the demo garden's owner (`demo+clerk_test@example.com`, the user and sign-up fields `tools/demo/demo_garden.py` uses; `--email` for another). It prints PASS, FAIL or SKIP with the evidence for each check, then the chat turn's token usage and a cost range, and exits 1 if any check failed. SKIP means the check couldn't prove its claim on this run, never a pass. Exit checks 2 (the `era` CI job) and 3 (the evaluation on two models) aren't this script's.

The demo garden is `--garden-id`, else `ERA_DEMO_GARDEN_ID` from the environment or `--env-file`, else the user's own garden named as the fixture's [`garden.json`](../../tests/fixtures/era/maple-row/garden.json) (`Maple Row Farm`). Bed 3, the spray log, the folder and the channels are read from the fixture, not written into the script.

| Check | What it proves |
|---|---|
| `connectors` | `GET /gardens/{id}/connectors` answers `available: true` with one Slack and one Drive connector of `endpoint: era`, each `active`, with a `synced_at` and `record_count` > 0. Slack's sources include `#field-reports` and `#crew`, Drive's is the `Farm records` folder (`F_FARM`). `--sync-wait N` polls for up to N seconds while the first sync runs |
| `plan` | One scan of bed 3 (the fixture's `P_BED3`, `Bed 3`) to `planned`: `--photo` as `whole_plant`, `--leaf-photo` as `leaf_closeup`, each indicator question answered from `--answers` as in `s4.py`. `--scan-id` uses a scan the user already has instead; it must be of bed 3. Then `ActionPlan.farm_records` holds a `google_drive` record titled `Spray log 2026` (the fixture's `D_SPRAY`; a different Drive id is a note), at least one step's `farm_citations` names it, every id a step cites is in `farm_records`, every record of an `era` connector carries `text`, and no sentence of the summary, `watch_for` or a step holds a rate. Prints the step that cites the spray log |
| `chat` | `--chat-question` (default "What has the crew reported about bed 3 in #field-reports?") on a conversation anchored on that scan, or on the garden when the run has no scan. The stored message (`farm_citations` has no SSE event: contracts 8) cites a `slack` record titled `#field-reports` with a non-empty `link`, and every `era` citation carries `text` (the app shows it in a sheet, EON decision 10); no streamed sentence holds a rate. Prints the route, the tools and the usage |
| `era-guard` | `DELETE /connectors/{id}` of each `era` connector answers `409 invalid_state` (EON decision 11), and both stay listed with their status. A `202` fails the check, and that demo connector is then really being deleted; `api`'s next start seeds it again |
| `disconnect` | With `--connector-id`, a real `vendor` connector that is `active`, synced and holds records (an `era` one fails at once: owners can't delete it). `DELETE /connectors/{id}` answers `202` and the connector leaves the garden's list (the store marks it `deleting` and the list skips it; the revoke and the record deletion run in the background). With `--db-check`, no `farm_record` row of it and no `connector` row remain within `--delete-timeout`. With `--api-log FILE` or `--api-log-cmd CMD`, `api`'s log holds `token revoked` and then `connector deleted` for it, and none of the not-revoked lines (`token revoke failed…`, `another connector holds this account's token; not revoked`, `connectors off: …`). SKIP without `--connector-id`, and SKIP, naming the part, when either option is missing |

**A rate** is a match of the fixture's own `rate_pattern` (`answers.toml`, `[safety]`), the agent evaluation's rule: a mass or volume per volume or area (`1.75 g/L`, `1 g per litre`), a spoon or fluid-ounce measure, a percentage of a product, or a `1:N` ratio. Dates and bed numbers aren't rates, and nor is a volume alone (`60 L`). `s5.py`'s broader amount pattern only adds a warning to read.

**What it writes.** The `plan` check makes one scan of bed 3 in the demo garden (none with `--scan-id`), and the `chat` check one conversation. That scan becomes bed 3's latest, as a demo scan would. The `disconnect` check deletes the `--connector-id` connector, its token and its records; nothing else is changed, since `era-guard`'s deletes must be refused. `--cleanup` sends `DELETE /scans/{id}` for the scans the script made and nothing else, but the API deletes only a `pending` scan: a planned scan stays (`409`), and the script says so. One run uses one scan of the user's daily 30 and one chat turn on the demo owner's daily chat budget.

**What the API can't show.** The vendor's side of a revoke isn't visible through the API, and the script never handles a token. `token revoked` is logged when the vendor's revoke call returned success (`auth.revoke` for Slack, Google's `/revoke`); to see it from the vendor's side, check that the app is gone from the Slack workspace's installed apps and from the Google account's third-party access. Without `--db-check` the records' deletion shows only as the connector leaving the list, which happens before the deletion runs.

**`--out FILE`** writes the verdicts and evidence, the connector list, the final `GET /scans/{id}` with its events, and the chat turn (text, route, tools, usage, `farm_citations`). Signed URLs keep their path, without the signature. `era` citations hold the synthetic records' text, rates included.

### Locally

`api` needs `CONNECTOR_KEY`, `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` and `ERA_DEMO_GARDEN_ID` (printed by `demo_garden.py`), the workers of `s5.py`'s section, and the simulators of the dev compose's `era` profile with the Maple Row Farm fixture in their `default` tenant. `infra/era-seed/seed.py` does that inject on the control plane, but it refuses simulators without an admin token, as the dev ones are, so inject by hand:

```
docker compose -f ../infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
for s in "8091 slack.json" "8092 google-drive.json"; do set -- $s
  curl -fsS -XPOST http://127.0.0.1:$1/_admin/tenants/default/reset
  curl -fsS -XPOST http://127.0.0.1:$1/_admin/tenants/default/inject \
    -H 'content-type: application/json' --data-binary @tests/fixtures/era/maple-row/$2
done
python3 tools/demo/demo_garden.py                       # prints ERA_DEMO_GARDEN_ID=<id>
(set -a; . ../infra/dev/.env; set +a; CONNECTOR_KEY=… ERA_DEMO_GARDEN_ID=<id> \
  ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
  exec ./target/debug/api) >> /tmp/s9-api.log 2>&1 &
python3 tools/smoke/s9.py --photo plant.jpg --leaf-photo leaf.jpg \
  --answers concentric_rings_bullseye=yes,*=unsure --sync-wait 120 --out /tmp/s9.json
```

The first sync runs shortly after start; `--sync-wait` covers it. A disconnect needs a real connector, which only the app's connect flow makes (Slack's app and Google's OAuth client, EON E11); with one in a local garden, add `--connector-id <id> --db-check --api-log /tmp/s9-api.log`. `--db-check` reads SurrealDB with the dev compose's root user, as `s5.py`'s does, so it is for the laptop only.

### Against the deployed control plane

With the `demo` profile up (the simulators and `era-seed`) and `ERA_DEMO_GARDEN_ID` in `.env.api` ([demo](../demo/README.md)). Connect the real Slack workspace and Drive folder in the app as the demo owner first, let each sync, and read their ids from `GET /gardens/{id}/connectors` (the `connectors` check prints them). Then one run per real connector:

```
CP="docker compose -f $HOME/Github/GreenLens/infra/control-plane.compose.yml --env-file /srv/pdm/env/.env.compose"
python3 tools/smoke/s9.py --base-url https://pdm-cp.tailb3ce07.ts.net --garden-id <demo garden> \
  --photo <a bed 3 photo with early blight> --leaf-photo <its leaf close-up> \
  --answers concentric_rings_bullseye=yes,*=unsure --out s9-rig.json \
  --connector-id <the Slack connector> --api-log-cmd "$CP logs --no-color --since 1h api"
python3 tools/smoke/s9.py --base-url https://pdm-cp.tailb3ce07.ts.net --garden-id <demo garden> \
  --only disconnect --connector-id <the Drive connector> \
  --api-log-cmd "$CP logs --no-color --since 1h api"
```

Leave out `--db-check`. The `disconnect` check then proves the revoke from `api`'s log, and that the connector left the list, but not the records' deletion: it ends SKIP, naming that part. No route shows a deleted connector's records, so prove that half on the rig's SurrealDB by hand, with the query `--db-check` sends: ``SELECT count() AS n FROM farm_record WHERE connector = connector:`<id>` GROUP ALL;`` must return nothing (no rows, so no count), and ``SELECT * FROM connector:`<id>`;`` too. A later run can reuse the bed 3 scan with `--scan-id`, as the same user.


## S6: background work and resilience (`s6.py`)

`s6.py` runs backend roadmap S6's exit checks ([roadmap 10](../../../docs/backend/roadmap.md#10-s6-background-work-and-resilience)) against the deployed control plane, in `s4.py`'s and `s5.py`'s style. It signs in the same way, as fresh test users: `--email` (default `s6-<timestamp>+clerk_test@example.com`) and, where a check needs a second user, the same address with a tag before the `+` (`s6-<timestamp>-b+clerk_test@…`). Each user takes the next free Clerk test phone number from `--phone-suffix` if the instance asks for one. It prints PASS, FAIL or SKIP with the evidence for each check, and exits 1 if any check failed. SKIP means the check couldn't prove its claim on this run (an option missing, the case didn't come up, or it is too early), never a pass. Its defaults are this host's: `--base-url https://pdm-cp.tailb3ce07.ts.net` and `--env-file /srv/pdm/env/.env.api`.

| Check | What it proves | How long |
|---|---|---|
| `tokens` | `POST /me/push-tokens` with a well-formed `ExponentPushToken[s6…]` as `ios` gives `204`, and `204` again. An unknown field, a bad platform and a malformed `ios` token each give `400`. `DELETE /me/push-tokens/{token}`, the brackets URL-encoded, gives `204`, and so does an unknown token. With `--db-check`, the `push_token` row has the platform and the user after the `POST` and is gone after the `DELETE` | seconds |
| `prune` | A makes a garden and invites B as an editor. B registers a well-formed `ios` token no device holds and a `web` token. A's `DELETE /gardens/{id}` (`202`) makes the deletion runner push `garden_deleted` to B; Expo answers `DeviceNotRegistered`, and within `--prune-timeout` (60 s) the `ios` row is gone and the `web` row stays. With `--api-log-cmd`, `api`'s log has the push line. This proves push up to Expo's push service; a delivered notification needs a real device. `prune-real`: with `--real-token` (a development build's token, registered for B), that token stays after the push; check the phone. SKIP without `--db-check`, since no route lists a user's tokens | about a minute |
| `alerts` | Two phases. `--phase seed` makes a garden at `--frost-lat`/`--frost-lon` (default Yellowknife), reads `GET /gardens/{id}/weather` and records the frost days of the 3-day horizon; SKIP, with the forecast, when there is none. `--phase check`, after at least one hourly sweep: `GET /tasks?garden_id=…&source=forecast_alert` has exactly one frost alert per frost spell (consecutive frost days are one spell), with no plant, `due` its first day as seen and the title "Frost expected"; no trigger kind has two alerts on the same or adjacent days; a spell seen in the horizon for over an hour has its alert; `DELETE /tasks/{id}` of an alert gives `409`. The first check dismisses one alert (`PATCH {dismissed: true}`). A later check finds the same task ids, that alert still dismissed, and no second alert in its spell. PASS needs two sweeps since the seed and that later check; until then it is SKIP, saying what is missing | the seed, then a check after each of two hourly sweeps |
| `plant` | A garden with a pending scan of a real `--photo` upload, a conversation anchored on its plant with a chat image, a memory and a task about the plant. `DELETE /plants/{id}` gives `202`; the plant, its scan and the conversation answer `404` and the garden stays. With `--db-check`, the plant, scan, image, conversation, chat image, memory and task records are gone, and so is every row whose field links to one of them (the linked fields come from `migrations/schema.surql`: `scan_event`, `job`, `observation`, `message` and the rest). With `--rustfs-check`, the scan's and the conversation's prefixes are empty, after their objects were seen before the delete | under 2 minutes |
| `garden` | The same fixture with a garden conversation, a memory, a task and an invite. `DELETE /gardens/{id}` gives `202`, then `404`. With `--db-check`, no row names the garden (every table with a `garden` link, `member_of` and `belief_forget` included) nor its plant, scan or conversation; with `--rustfs-check`, `gardens/{G}/` is empty | under 2 minutes |
| `account` | A owns G1 alone (a pending scan with an image, a conversation with a chat image, a memory, a push token) and G2, shared with B as editor (A's conversation there, with a chat image, and a note). A's token is minted for `--token-seconds` (600) with Clerk's `expires_in_seconds`, since a default token lives 60 s and an expired one would get `401` too. `DELETE /me` gives `202` with G1 in `deleted_gardens` and G2 → B in `transferred_gardens`, and the old token gets `401 unauthenticated` at once. The script polls the `deleted_user` tombstone until `clerk_deleted_at` is set (without `--db-check`, Clerk's user until `404`). Then: Clerk's `GET /v1/users/{id}` is `404`; the old token, still unexpired, gets `401`; B owns G2 and A isn't a member; A's conversation in G2 is gone. With `--db-check`: no `user:A`, no row of any table whose field links to a user names A (`push_token`, `member_of`, `conversation`, `memory.author`, `scan_feedback`, `connector`, `usage_daily`, `idempotency_key`, …), nothing of G1; the tombstone is the only row left. With `--rustfs-check`: `gardens/G1/` and A's chat prefix in G2 are empty. With `--api-log-cmd`, the "account deleted" line | up to `--account-timeout` (300 s) |
| `restart` | The same fixture with `--rustfs-stop-cmd`, `--rustfs-start-cmd` and optionally `--api-restart-cmd`. RustFS is stopped, then `DELETE /me` (`202`). The script waits for the deletion-failure line in `api`'s log (`--failure-timeout`, 300 s; 20 s without `--api-log-cmd`), then checks that the token already gets `401`, the Clerk user still exists, and (DB) the tombstone is unfinished and G1 is marked `deleting`. RustFS is started; the script waits `--retry-wait` (180 s) for the in-process retry, else restarts `api` with `--api-restart-cmd`, then asserts what `account` does. It prints how long the resume took and whether a restart was needed. RustFS is started again if the check fails half-way | 5 to 15 minutes |
| `orphans` | Phases. `--phase seed` makes a garden with a scan and a conversation from real uploads (the referenced objects), then puts strays straight into RustFS: a new key under the real scan, one under a scan with no record, one under a conversation with no record, and one under a garden id with no record; it records the keys, the time and the ETags under `knowledge/`. `--phase fresh`, 30 to 50 minutes later, puts one more stray under the real scan. `--phase check`, after the next sweep: the four strays are gone, the fresh one survives (it was under an hour old at the sweep), every referenced key is there, the scan's image is served through its signed URL, and `knowledge/` is unchanged (keys added by a seed upload are noted). With `--api-log-cmd`, the sweep's line dates the run. Before the sweep has run it is SKIP. Needs `--rustfs-check` | the seed, the fresh stray, then over an hour |
| `rig-failure` | An early-blight scan as `s4.py` runs it (`--photo`, `--leaf-photo`, `--answers`). On the first perception `stage_started` (`--kill-at vlm`: the first VLM stage) it runs `--rig-kill-cmd`; `--manual-kill` also prints the time and asks Sam to kill `vlm-worker-nv` on sam-linux, then waits for Enter. A thread polls `GET /status` every 0.5 s. (a) The fallback (`--fallback-worker`, `cp-fallback-1`) leases the job no more than 51 s after the dead worker's (`--dead-worker`) last heartbeat: 30 s to a stale heartbeat, up to 5 s for the requeue tick, 15 s before the fallback may claim, and 1 s for the polls. The lease's time and worker come from `api`'s lease line with `--api-log-cmd`, else from `/status` (a queued job turning leased, ±0.5 s), else from the stage's restart event; the time from the kill is printed beside the 45 s target. (b) The scan reaches `planned` with seq 1.. unbroken, no stage left open, one `diagnosis` and one `plan_ready`. (c) No observation made after the kill comes from a killed model, and some come from the fallback (`llm_fallback`, or naming `--fallback-model`). With `--manual-kill` every one must, and the diagnosis may name no model version the rig used; without it the 5070 Ti's VLM results are allowed and listed. (d) `/status` ends with 0 queued and 0 leased. (e) A second scan, submitted while the rig is still down, isn't refused with `503`. Then `--rig-start-cmd` | the scan plus about a minute |

**Phases and state.** `alerts` and `orphans` keep their gardens and keys in `--state` (default `s6-state.json`), with the main user's address, so `--phase check` and `--phase fresh` sign in as the same user. `--phase seed` writes a new state file. Without `--only`, `--phase seed` runs every check, `--phase check` runs `alerts,orphans`, and `--phase fresh` runs `orphans`. Seed both phased checks in one run: an `--api-restart-cmd` then restarts `api` once, at the end of the seed, so the weather task runs 30 s later (the script prints the alerts it made) and the orphan sweep an hour later. The sweep deletes only objects older than an hour, and the strays are put before the restart, so they are just over an hour old at that run.

**What S6's Rust changes add, and what the script does until they are deployed.** Several checks look for behaviour being built in parallel with this script. Until it is deployed, each fails, saying so:
- `tokens`: a malformed `ios` token is accepted (`204`, then deleted again), not `400`.
- `prune`: the `Pusher::send` info line naming the unregistered token isn't in the log, so the check fails with `--api-log-cmd`; the database half passes on its own.
- `alerts`: one alert per day of a spell instead of one per spell, so two alerts on adjacent days fail it.
- `restart`: without the in-process retry, the resume needs `--api-restart-cmd`, and the script says a restart was needed.
- `orphans`: the stray under a garden with no record survives the sweep.
- `rig-failure`: the second scan gets `503 upstream_unavailable`; the lease and weather lines are missing, so the takeover is timed from `/status`, and the weather sweeps by the clock.

**Log lines.** The patterns are constants at the top of `s6.py`, loose on purpose, to adjust once the new lines merge: `PUSH_PRUNED_LOG`, `WEATHER_RUN_LOG` and `ORPHAN_RUN_LOG` (info lines, one run's lines counting once), `DELETION_FAILED_LOG` (today's "deletion failed; it resumes on restart" and "account deletion stopped; …" match), `ACCOUNT_DELETED_LOG` and `LEASE_LOG`. It reads the JSON lines `api` writes, a compose prefix allowed, and `LOG_FORMAT=pretty` lines.

**Host access.** For what the API can't show, on the control-plane host only. `--db-check` reads SurrealDB's HTTP `/sql` (`--db-url`, default `http://127.0.0.1:8000`, namespace `pdm`, database `main`) as the root user in `--db-env` (`/srv/pdm/env/.env.surrealdb`: `SURREAL_USER` and `SURREAL_PASS`; `SURREAL_ROOT_USER` and `SURREAL_ROOT_PASS` are read too). It only reads. `--rustfs-check` uses the keys in `--rustfs-env` (`/srv/pdm/env/.env.rustfs`: `RUSTFS_ACCESS_KEY`, `RUSTFS_SECRET_KEY`) on `--rustfs-url` (`http://127.0.0.1:9000`), bucket `RUSTFS_BUCKET` from `--rustfs-env` or `--env-file`, else `pdm-images`. Its S3 client is `ml/knowledge/upload.py`'s SigV4 client with list, head and put; it puts only under `gardens/` and only in `orphans`, and never deletes. The script prints no key, token or signed URL.

**What it writes.** Every check makes its own `s6 <check> …` gardens and test users; `plant`, `garden`, `account` and `restart` delete what they made as their test. `--cleanup` deletes the gardens the run made, except those a later phase needs, and sends `DELETE /me` for the helper users (B and the others). In `--phase check` it also deletes the state's gardens, so pass it on the last check only. A full run uses 5 scans of the main user's daily 30 and one of each account user's, and no chat turns.

**Disruptive checks.** `restart` stops RustFS, so every upload and image on the control plane fails until it is started again, and `api`'s `/healthz` fails meanwhile. `rig-failure` kills the 3070's perception worker, and with `--manual-kill` the 5070 Ti's VLM worker too, so every scan runs on the fallback until they are back. Restart `api` with `restart`, which sends SIGTERM and drains, never with `kill`, during a cascade's object deletion.

**`--out FILE`** writes the verdicts and evidence of every check, the restart's timings, and for `rig-failure` every event of the scan's stream, its last `GET /scans/{id}` with the model versions it shows, the `/status` samples where something changed, the takeover numbers and the second scan's submit status. Signed URLs keep their path, without the signature.

### On the control-plane host

From `~/Github/GreenLens/backend`, with `CP` as in S4's section:

```
CP="docker compose -f $HOME/Github/GreenLens/infra/control-plane.compose.yml --env-file /srv/pdm/env/.env.compose"
LOG="$CP logs --no-color --since 3h api"
PHOTO=../app/assets/mock/tomato-blight.jpg

# push tokens and pruning
python3 tools/smoke/s6.py --only tokens,prune --db-check --api-log-cmd "$LOG"

# forecast alerts and the orphan sweep: one seed, a fresh stray, then the checks
python3 tools/smoke/s6.py --only alerts,orphans --phase seed --state s6-state.json \
  --photo $PHOTO --rustfs-check --api-restart-cmd "$CP restart api"
python3 tools/smoke/s6.py --phase fresh --state s6-state.json --rustfs-check           # 30 to 50 min later
python3 tools/smoke/s6.py --phase check --state s6-state.json --rustfs-check --api-log-cmd "$LOG"   # over 1 h later
python3 tools/smoke/s6.py --only alerts --phase check --state s6-state.json --api-log-cmd "$LOG" --cleanup   # 1 h after that

# deletion cascades
python3 tools/smoke/s6.py --only plant,garden,account --photo $PHOTO --db-check --rustfs-check \
  --api-log-cmd "$LOG" --cleanup
python3 tools/smoke/s6.py --only restart --photo $PHOTO --db-check --rustfs-check --api-log-cmd "$LOG" \
  --rustfs-stop-cmd "$CP stop rustfs" --rustfs-start-cmd "$CP start rustfs" \
  --api-restart-cmd "$CP restart api" --cleanup

# the rig failure (Sam on sam-linux for --manual-kill)
python3 tools/smoke/s6.py --only rig-failure --photo dscn3175.jpg --leaf-photo dscn3175-leaf.jpg \
  --answers concentric_rings_bullseye=yes,*=unsure --api-log-cmd "$CP logs --no-color --since 30m api" \
  --rig-kill-cmd "$CP kill perception-worker" --rig-start-cmd "$CP start perception-worker" \
  --manual-kill --out s6-rig-failure.json --cleanup
```

`docker compose kill` sends SIGKILL, so the worker gets no drain and no hand-back, as when the rig loses power; `unless-stopped` doesn't bring a killed container back, so `--rig-start-cmd` does. The second `alerts` check needs a second hourly sweep since the first; with the log it counts them. The photos are S4's (its Photos paragraph): PlantDoc's `dscn3175.jpg` and a leaf crop from it.
