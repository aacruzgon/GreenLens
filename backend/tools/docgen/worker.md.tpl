# PDM Backend: `worker` binary

> **Status:** Draft v0.4.2, 2026-10-16. v0.4.2: the rig's compose listing gives each worker an `env_file` with its own `WORKER_TOKEN` (the worker reads no `WORKER_TOKEN_FILE`). v0.4.1: section 17 keeps only the four questions still open, since docs/vision v0.2 and contracts 5–6 now agree on the rest; section 1 drops the image-fetch difference (the skill uses 30 s too); section 12 lists the fallback service with its `stop_grace_period`. v0.4: the SIGTERM shutdown ends within `DRAIN_SECONDS`, dropping a `complete` or heartbeat call in flight (it could take 20 + 10 + 5 = 35 s, past the 30 s grace); the worker API client retries by the error body's `retryable` and `retry_after_s` (contracts 2), not by status, and waits `retry_after_s`; section 1 states the HTTP timeouts and only the two skill differences that remain. v0.3.1: why the immediate re-claim after a `204` can't spin while the API shuts down (probed). v0.3: the hosted fallback gives the guided check the same 8,000-token budget as the other calls (docs/vision README 12 #27), and the fallback row points to the full list of stripped schema keywords. v0.2: the fallback worker reads its hosted-LLM key from `LLM_API_KEY`, the name `api` and the backend README §9 use (it was `ANTHROPIC_API_KEY`). v0.1: the worker process (`--caps perception|vlm|fallback`): configuration, the heartbeat and claim loops with slots, leases and the SIGTERM drain, cancellation by drop, the VLM adapter with its per-call schema builder and decoding, the hosted fallback, per-card settings for the RX 9070 XT, and the rig's compose services; compiled on Rust 1.99.1.

Companion to the [backend architecture](README.md), the [`vision` crate](vision.md) (the perception models and image code it uses), the [`integrations` crate](integrations.md) (its HTTP clients for the VLM server and the LLM API) and the [`domain` crate](domain.md) (the job types and the `VisionModel` port). Every listing is copied from the compiled source of `bins/worker` by the docgen tool. `cargo check -p worker` (also `--features cuda`), `cargo clippy -p worker --all-targets` and `cargo test -p worker` pass: 34 tests, among them the loop on paused time against a fake broker and the VLM adapter against a scripted backend.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [Configuration](#4-configuration)
5. [The lifecycle state machine](#5-the-lifecycle-state-machine)
6. [Leases, heartbeats and completion](#6-leases-heartbeats-and-completion)
7. [Cancellation](#7-cancellation)
8. [The VLM adapters](#8-the-vlm-adapters)
9. [The per-call schema and decoding](#9-the-per-call-schema-and-decoding)
10. [The fallback worker](#10-the-fallback-worker)
11. [Multi-vendor GPUs](#11-multi-vendor-gpus)
12. [Deployment](#12-deployment)
13. [Concurrency and lifetimes](#13-concurrency-and-lifetimes)
14. [Errors](#14-errors)
15. [Testing](#15-testing)
16. [Decisions on the contracts' open questions](#16-decisions-on-the-contracts-open-questions)
17. [Open questions](#17-open-questions)
18. [Sources](#18-sources)

---

## 1. Purpose and boundaries

One binary, three roles ([architecture 3](../architecture/README.md#rust-crates); [architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback)):

| `--caps` | Runs on | Claims | Model (`Arc<dyn VisionModel>`) | `slots` |
|---|---|---|---|---|
| `perception` | the rig, RTX 3070 | `perception` | `vision::perception::PerceptionPipeline` (ONNX Runtime, CUDA) | 1, batching inside the job |
| `vlm` | the rig, RTX 5070 Ti and RX 9070 XT (one process per card) | `vlm`, `verify`, `guided` | `VlmAdapter<OpenAiBackend>` over its own vLLM or llama.cpp sidecar | `VLM_SLOTS`, 1 or 2, per card |
| `fallback` | the control plane | every kind, 15 s after `queued_at` | `FallbackModel` (`VlmAdapter<AnthropicBackend>` + perception by the LLM) | `FALLBACK_SLOTS`, default 4 |

**What it owns:** the worker side of [contracts 5](../architecture/contracts.md#5-worker-api-and-jobs) (heartbeat, claim, events, complete, image fetch), the job lifecycle, and the VLM side of [contracts 6](../architecture/contracts.md#6-vlm-structured-output) (image preparation, per-call schemas, prompts, validation, alias mapping, box conversion, certainty).

**What it never does:** no database, no object storage, no depth maps (payloads never name one, and no route serves one). It talks only to the API over outbound HTTPS and to its own sidecar or the LLM API, so the rig needs no open ports.

**Dependency direction:** `worker → vision, integrations, domain` ([crate graph](README.md#5-crate-graph-and-dependency-rules)). The VLM adapters live here, not in `vision`, because they are HTTP clients over `integrations`; `vision` stays free of networking.

**HTTP timeouts.** The process has one `reqwest::Client`, built by `integrations::http::client` with a 600 s read timeout instead of its 60 s default ([integrations 4](integrations.md#4-shared-http-client-retries-pacing-sse)). The read timeout also covers the wait for the response headers, and a VLM call isn't streamed: the server sends nothing until the answer is complete. Each call sets its own total deadline, shorter than 600 s, and that deadline is what ends it: 240 s for a VLM call, 300 s for a hosted LLM call, `wait_s + 10` = 35 s for the claim, 30 s for an image, 10 s for `events` and `complete`, 5 s for the heartbeat.

**Where it departs from the project skills.** The `rust-workers` skill's loop (`loop.md`) is the model for `lifecycle` and `job`, with the domain's real types instead of its sketches. The skill follows this document on the `integrations` clients, the 600 s read timeout, the progress sink that cancels a job on `409 lease_lost`, the settled job error codes and the retry rules of section 6. It gives an image fetch 30 s, as `HttpWorkerApi::image` does. One difference remains: the skill backs off after a `204` that comes back in under half of `wait_s`, while the claim loop here claims again at once, because the API stops accepting connections when it shuts down (section 5, with the probe).

---

## 2. Dependencies

@@file ../Cargo.toml@@

| Crate | Why |
|---|---|
| `vision` | The perception pipeline, `OrtRuntime`, the manifest, image decode, resize and JPEG for VLM preparation |
| `integrations` | `openai_compat::OpenAiCompatClient` (vLLM, llama.cpp) and `anthropic::AnthropicClient` with `sanitize_schema`; the shared `reqwest` client builder |
| `domain` | Jobs, results, events, `VisionModel`, `ProgressSink`, the routing and job rules |
| `serde_json` with `preserve_order` | **Property order is generation order** under xgrammar ([vision vlm 8](../vision/vlm.md#8-schemas-from-the-models-side)): `reason` before `answer`, `certainty` last. Without `preserve_order`, `serde_json::Map` is a `BTreeMap` and would sort the keys alphabetically (`answer` before `reason`). Feature unification turns it on for every crate in a build that includes the worker; a test checks the order |
| `reqwest` 0.13.5 | The worker API client |
| `tokio`, `tokio-util` | Tasks, `CancellationToken`, `TaskTracker`, semaphores, paused-time tests |
| `clap` 4.6.7 (`derive`, `env`) | `--caps` and every setting from the environment |
| `secrecy` | Tokens and keys never printed (`WORKER_TOKEN`, `VLM_API_KEY`, `LLM_API_KEY`) |
| `rand` 0.10.3 | Backoff jitter |
| `anyhow` | Start-up errors in `main` only |

---

## 3. Module layout

```
bins/worker/src/
├─ main.rs               tracing, CLI, the shared HTTP client, build_model for --caps, run
├─ config.rs             Cli and its flattened groups, Caps, Timings
├─ api.rs                WorkerApi trait, HttpWorkerApi, JobLease, CallError, classify_body, decode_claim
├─ api/tests.rs          classify_body against contracts 2's error bodies
├─ lifecycle.rs          Backoff, retry_delay, Running registry, Worker<A>: heartbeat, claim loop, drain; run_until_sigterm, run_until
├─ lifecycle/tests.rs    the loop on paused time
├─ job.rs                JobRunner, run_job, complete retries, image fetch, ApiProgress, job_error
├─ vlm.rs                VlmAdapter<B> (VisionModel): analyse, verify, read_guided_check, call_validated
├─ vlm/profile.rs        AdapterProfile (box format, cell, pixel budget), CertaintyMap, CallBudget, model_versions
├─ vlm/backend.rs        VlmBackend, VlmCall<'a>, OpenAiBackend, AnthropicBackend
├─ vlm/prepare.rs        target_size, prepare, prepare_all
├─ vlm/schema.rs         the per-call JSON Schema builder
├─ vlm/prompt.rs         system, findings, indicators, verify and guided prompts
├─ vlm/decode.rs         validation, alias mapping, box conversion, truncation
├─ vlm/tests.rs          the adapter against a scripted backend
└─ fallback.rs           FallbackModel, FallbackLabels: perception on the hosted LLM
```

---

## 4. Configuration

`--caps` picks the role; everything else comes from the environment, so one image serves every card ([architecture 12](../architecture/README.md#12-deployment)).

@@items config.rs Caps Cli PerceptionArgs VlmArgs FallbackArgs Timings@@

| Variable | Default | Role | Meaning |
|---|---|---|---|
| `WORKER_ID` | | all | Names the card: `rig-3070-perception-1`, `rig-5070ti-vlm-1`, `rig-9070xt-vlm-1`, `cp-fallback-1`. Sent as `worker_id` everywhere |
| `API_BASE_URL`, `WORKER_TOKEN` | | all | The control plane and the bearer token |
| `DRAIN_SECONDS` | 20 | all | SIGTERM drain bound, and so the bound of the whole shutdown (section 5); keep `stop_grace_period` at least 10 s above it |
| `CPU_PERMITS` | 2 | all | Blocking-pool permits for decode, resize, pre-processing |
| `MODEL_MANIFEST` | `/models/manifest.toml` | perception, fallback | The manifest; the fallback reads its label lists |
| `ORT_EP`, `ORT_DEVICE_ID`, `ORT_DYLIB_PATH`, `ORT_INTRA_THREADS` | `cuda`, 0, —, 2 | perception | Section 11 |
| `PERCEPTION_MAX_BATCH`, `DETECTOR_MAX_BATCH`, `SAM_ENCODER_MAX_BATCH`, `SAM_MAX_BOXES`, `BIOCLIP_MAX_BATCH` | 16, 8, 2, 16, 16 | perception | Caps ([contracts 5, batching](../architecture/contracts.md#results)) |
| `VLM_BASE_URL`, `VLM_API_KEY`, `VLM_SERVED_NAME` | `http://vlm:8000`, —, `vlm` | vlm | The sidecar |
| `VLM_ADAPTER` | `vllm` | vlm | `vllm` or `llamacpp` |
| `VLM_MODEL`, `VLM_QUANT`, `VLM_SERVER` | | vlm | `vlm_model`, `vlm_quant`, `vlm_server` in `model_versions` |
| `VLM_BOX_FORMAT`, `VLM_CELL_PX`, `VLM_MAX_PIXELS` | `norm1000`, 32, 1,003,520 | vlm | The adapter instance ([contracts 6](../architecture/contracts.md#box-formats-per-model-adapter)) |
| `VLM_SLOTS` | 1 | vlm | Jobs held at once; the sidecar's `--max-num-seqs` (or `-np`) is the same |
| `LLM_API_KEY`, `FALLBACK_VISION_MODEL` | —, `claude-opus-5-5` | fallback | The hosted model |
| `FALLBACK_SLOTS`, `FALLBACK_MAX_LONG_SIDE`, `FALLBACK_MAX_PIXELS` | 4, 1568, 1,150,000 | fallback | Slots, and the size the API won't resize |

Start-up fails before the first claim on a missing key, a file whose hash differs from the manifest, a provider that isn't compiled in or doesn't register, or a VLM server that doesn't serve `VLM_SERVED_NAME` (`GET /v1/models`).

@@items main.rs build_model@@

---

## 5. The lifecycle state machine

```
             ┌──────────── heartbeat task (own token, stops last) ─────────────┐
start ──▶ claiming ──claim 200──▶ job task: fetching ──▶ running ──▶ completing ──▶ done
  │        ▲  │204: poll again        │cancel_job_ids / lease_lost ──▶ dropped (nothing sent)
  │        │  └─error: back off ─┘    │soft deadline (max_runtime − 15 s) ──▶ failed, retryable
  │        │                          │unsupported v / unreadable job ──▶ failed, not retryable
  │  SIGTERM ─▶ draining: no claims, jobs get DRAIN_SECONDS ──▶ abandoned (nothing sent) ──▶ exit
  └─ load model (fail fast)
```

`Worker<A>` holds the API client, the `JobRunner`, the registry of held jobs, the slot semaphore, a `TaskTracker` for job tasks and the `abandon` token that parents every job's token.

@@items lifecycle.rs Identity Worker@@
@@impl lifecycle.rs impl<A: WorkerApi> Worker<A>@@
@@items lifecycle.rs run_until_sigterm run_until@@

The claim loop takes a slot **before** each claim, so the worker never polls while full and never holds more than `slots` jobs; the permit moves into the job task and is dropped when it ends. `204` loops at once (the server already waited `wait_s`); an error backs off 1 s doubling to 30 s, each delay drawn from `[d/2, d]`. The one early `204` is the open poll when the API shuts down, and it can't turn into a spin: on the signal `axum::serve` drops its listener and disables keep-alive on open connections, so the next claim fails to connect, or gets `502` from Caddy, and backs off (✓ probe on axum 0.8.9 and reqwest 0.13.5: during a 1.7 s graceful shutdown the re-claiming client saw one `204`, then a connection error, and no claim reached the handler after the signal).

@@items lifecycle.rs Backoff retry_delay@@

**Shutdown time.** From SIGTERM to exit takes at most `DRAIN_SECONDS`, 20 s. The claim loop returns at once and drops an open long poll. Running jobs get the 20 s. At the bound, `abandon` cancels every job's token, and every await in a job task selects on that token: the model, an image fetch, a `complete` call in flight and the backoff between `complete` calls. So the tasks end at once. Then the heartbeat stops, and its `stop` token drops a call in flight too. What is left when `main` returns is a decode or resize already started on the blocking pool, which the runtime waits for, and an ONNX Runtime run, which `TerminateOnDrop` has already told to stop. Compose's `stop_grace_period: 30s` leaves 10 s for these and for Docker (section 12). Without the two cuts, a `complete` in flight at the bound (10 s deadline) and then a heartbeat in flight (5 s deadline) would run on after the drain: 20 + 10 + 5 = 35 s, past the grace, and Docker would SIGKILL the worker. A `complete` cut at the bound is either committed by the API or not. If not, the job is requeued 30 s after the last heartbeat, like any abandoned job, and the requeue is free for the job's first two. `the_whole_shutdown_ends_within_the_drain_bound` shows exactly 20 s on paused time.

---

## 6. Leases, heartbeats and completion

**The API boundary.** `WorkerApi` is used generically (`Worker<A: WorkerApi>`), never as `dyn`, so its methods return `impl Future + Send` without boxing; the explicit `Send` lets job tasks that await them run on the multi-thread runtime.

@@items api.rs JobLease CallError@@
@@impl api.rs impl CallError@@
@@items api.rs WorkerApi HttpWorkerApi ErrorBody ErrorDetail classify classify_body decode_claim@@

| Rule ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)) | Where |
|---|---|
| Heartbeat every 10 s on its own task, `MissedTickBehavior::Delay`, a 5 s timeout, every held job with its lease token | `Worker::heartbeat_loop` |
| `versions` = the model's keys + `worker_id` | `JobRunner::versions` |
| A slot before each claim; `slots` the same in claim and heartbeat | `Worker::claim_loop` |
| Claim long poll `wait_s` 25 with a request timeout of `wait_s + 10` s | `HttpWorkerApi::claim` |
| `X-Lease-Token` on events, complete and image calls | `HttpWorkerApi::leased` |
| Every error body is read before anything consumes the response: its `code` decides `lease_lost`, its `retryable` and `retry_after_s` decide the retry (table below) | `classify`, `classify_body` |
| Decode `v`, `job_id`, `lease_token` first; another `v` → `failed`, `unsupported_version`, not retryable | `ClaimedJob::decode`, `decode_claim`, `run_job` |
| Images only through `GET /internal/jobs/{id}/images/{image_id}`, each retried twice after an `Unavailable` answer | `fetch_images` |
| `complete` with `model_versions` and `timings_ms`, retried with backoff (at least `retry_after_s`) until `max_runtime_s`; a repeat is `204`; the job's token cuts it, a call in flight included | `complete_with_retry` |

**What an answer means** ([contracts 2](../architecture/contracts.md#2-errors)). Every error body has `retryable`, the server's word on whether the same request may succeed later, so the worker follows it whatever the status. Only `lease_lost` has a meaning of its own. A status alone decides only when there is no error body, as with Caddy's own `502` while the API is down.

| Answer | `CallError` | Heartbeat | Claim | Image fetch | `complete` |
|---|---|---|---|---|---|
| `409` with `code: "lease_lost"` | `LeaseLost` | — | — | drops the job | drops the job; nothing more is sent |
| An error body with `retryable: true` (`429 rate_limited`, `503 upstream_unavailable`) | `Unavailable`, with `retry_after_s` | logged; the next beat is 10 s away | backs off, at least `retry_after_s` | retried twice, at least `retry_after_s` apart | retried with backoff, at least `retry_after_s`, until `max_runtime_s` |
| A `429` or `5xx` without an error body, a connect error, a timeout, a body that couldn't be read | `Unavailable` | logged | backs off | retried twice | retried with backoff |
| An error body with `retryable: false` (`500 internal`, `429 budget_exceeded`, `400 invalid_request`, `404 not_found`), or a `4xx` without one | `Rejected` | logged | backs off like any error | fails the job with `image_fetch_failed` | logged, not retried; the job leaves the heartbeat, so its lease expires and the broker requeues it |
| `204` on a repeat `complete` | success | | | | done |

The backoffs are 1 s doubling to 30 s for the claim, 0.5 s doubling to 8 s for `complete` and 0.25 s doubling to 2 s for an image, each delay drawn from `[d/2, d]`; `retry_delay` waits the longer of that delay and `retry_after_s`.

@@items job.rs JobRunner LeaseLost job_images run_job complete_with_retry@@
@@impl job.rs impl JobRunner@@

**`model_versions` keys** ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)), as `domain::jobs::ModelVersionKey` constants:

| Key | Perception | VLM | Fallback |
|---|---|---|---|
| `worker_id` | ✓ | ✓ | ✓ |
| `vlm_model`, `vlm_server`, `vlm_box_format`, `vlm_max_pixels`, `vlm_prompts`, `vlm_certainty` | | ✓ | ✓ (`vlm_server = anthropic`) |
| `vlm_quant` | | ✓ | |
| `perception_config`, `ort_ep` | ✓ | | |
| `label_set_version`, `classifier_labels_version` | ✓: the job's in `complete`; every loaded one, comma-separated, in the heartbeat | | |
| `<model name>` (manifest entry names: `leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, `disease_head…`, `plant_probe`) | ✓ | | |

`vlm_prompts` is `system@1,findings@1,indicators@1,verify@1,guided@1`, in the contract's `name@n` form with the prompt versions of [vision vlm 7](../vision/vlm.md#7-prompts); `vlm_certainty` is `0.4/0.65/0.85`.

---

## 7. Cancellation

Three things end a job early, all through the job's `CancellationToken` (a child of `abandon`):

| Cause | How it arrives | What is sent |
|---|---|---|
| `cancel_job_ids` in a heartbeat response (a new run, a deleted scan) | `Running::cancel` | nothing |
| `409 lease_lost` on an `events` call | `ApiProgress::send` cancels the token | nothing |
| `409 lease_lost` on an image fetch | `execute` returns `LeaseLost` | nothing |
| The drain runs out at SIGTERM | `abandon.cancel()` | nothing more, and a `complete` in flight is dropped; the broker requeues after the heartbeat goes stale |
| The soft deadline (`max_runtime_s − 15 s`) | `timeout_at` around the model | `failed`, `max_runtime_exceeded`, retryable |

`run_job` `select!`s on the token and the model's future; when the token fires it returns, and **dropping the model's future is the cancellation** ([domain 9](domain.md#models)). Each adapter makes that drop effective: the perception pipeline's `TerminateOnDrop` guard terminates a running ONNX Runtime batch ([vision 6](vision.md#6-perception)); dropping a `reqwest` future closes the VLM server's connection, and vLLM aborts a request whose client disconnected (*unverified* for llama.cpp, which finishes the slot).

@@items lifecycle.rs Running Registered@@
@@items job.rs ApiProgress@@

---

## 8. The VLM adapters

`VlmAdapter<B: VlmBackend>` implements `VisionModel` for `vlm`, `verify` and `guided` jobs ([contracts 6](../architecture/contracts.md#6-vlm-structured-output); [vision vlm](../vision/vlm.md)). `B` is fixed per worker kind at compile time; only the `VisionModel` is a trait object.

@@items vlm/backend.rs VlmImage VlmCall Stop VlmReply VlmBackend OpenAiBackend AnthropicBackend@@

**The adapter instance.** Box format, cell size and pixel budget belong to the adapter instance, the model, server and preparation together ([contracts 6, box formats](../architecture/contracts.md#box-formats-per-model-adapter)):

| Instance | `box_format` | Cell | Pixels | Long side |
|---|---|---|---|---|
| Qwen3-VL-8B FP8 on vLLM (5070 Ti) or vLLM ROCm (9070 XT) | `norm_1000` | 32 | `VLM_MAX_PIXELS` (1,003,520) | — |
| The same model on llama.cpp | `norm_1000` | 32 | `VLM_MAX_PIXELS` | — |
| Qwen2.5-VL baseline | `abs_px_prepared` | 28 | `VLM_MAX_PIXELS` | — |
| The hosted LLM (fallback) | `abs_px_prepared` | 1 | 1,150,000 | 1568 |

@@items vlm/profile.rs PROMPTS CertaintyMap CallBudget VlmIdentity AdapterProfile@@

**Image preparation.** Each image is scaled (never up) to at most `VLM_MAX_PIXELS` and the long-side cap, each side rounded **down** to a multiple of the cell, then JPEG at quality 90; a 2048 × 1536 stored photo becomes 1152 × 864 (972 tokens on Qwen3). The server is started so prepared images fall inside its processor's range, so "the image as sent" is "the image as seen" ([contracts 6, image preparation](../architecture/contracts.md#image-preparation)). Preparation runs once per job on the blocking pool, so the findings and indicators calls share the same bytes, which also lets vLLM's prefix cache reuse the image tokens.

@@items vlm/prepare.rs PreparedImage target_size prepare prepare_all@@

**The calls.**

| Job | Calls | Images | Events |
|---|---|---|---|
| `vlm` | findings, then indicators | findings: the payload's `findings_images` in order; indicators: its `indicator_images` | `vlm_findings` started, partial result, completed; `vlm_indicators` started and completed, or completed with `skipped: all_measured` when no candidate has an indicator left |
| `verify` | indicators | the payload's `images`; for `new_photo` the first is labelled "new photo, taken to answer a question" | `verify` started and completed |
| `guided` | guided | the one image | none (the port has no progress sink) |

Photo routing ([contracts 5](../architecture/contracts.md#payloads)) is the API's: the payload already holds the chosen images in order (`domain::rules::routing`), and the worker keeps that order and the cap of 4 (`pick`).

@@impl vlm.rs impl<B: VlmBackend> VlmAdapter<B>@@
@@items vlm.rs pick@@

**Requests.** Greedy decoding (temperature 0, `top_p` 1, no penalties, seed 0) and `response_format` with the strict per-call schema, which vLLM and llama.cpp both take; `max_completion_tokens` on vLLM, `max_tokens` on llama.cpp (the `integrations` client's flavour decides) ([vision vlm 10](../vision/vlm.md#10-sampling-and-the-request)). Budgets: 1,500 tokens for findings and indicators, 100 for guided; 8,000 for every call on the hosted model, guided checks included, because its thinking counts toward `max_tokens`.

**Validation and the one retry.** `call_validated` treats a parse failure, a validation failure, `finish_reason: length` and a refusal alike: the call is repeated once with the same request, then the job fails with `model_error`, retryable ([contracts 6, after decoding](../architecture/contracts.md#after-decoding)). Greedy decoding makes the retry mostly deterministic, so each failure is logged with the schema name.

---

## 9. The per-call schema and decoding

The worker builds each call's JSON Schema from the payload ([contracts 6, per-job schema](../architecture/contracts.md#per-job-schema)):

| In the schema | Built as | Mapped back by |
|---|---|---|
| Images | an enum `img1` … `imgN` of the call's images | `CallImages::get` → `image_id` |
| Indicator answers | one required key per indicator, `q<n>_<slug cut to 24>` (`domain::vlm::indicator_key`), `reason` (`maxLength` 60) before `answer` | `Expected` → `indicator_id` |
| A question none of whose `image_kinds` is in the call | `answer` enum `["unsure"]` only | rejected if answered otherwise |
| Species guess | enum of `species_options` slugs + `other` | `SpeciesId` or `null` |
| Organism labels | enum of `organism_options` slugs + `other` | `OrganismId` or `null` |
| Boxes | arrays of exactly 4 integers, 0–1000 for `norm_1000`, 0–the largest prepared side for `abs_px_prepared` | `BoxFormat::to_bbox` (corners → `[x, y, w, h]`, clamped) |
| Soil readings, `other_name`, a damage box | `anyOf [schema, null]` | `None` |
| Guided | only the branch of the image's kind | rejected if the other branch comes back |

Every object has `additionalProperties: false`; length-limited strings have no `pattern` (xgrammar drops length bounds next to one, and vLLM rejects the combination ✓ per the vision research); `organisms` and `damage` have `maxItems` 10.

@@items vlm/schema.rs Limits BoxRange Question questions findings indicators guided@@
@@items vlm/prompt.rs system image_label findings indicators guided@@

**After decoding.** Strings are truncated to their limits by characters and boxes clamped for every backend, because the hosted API's schema loses the limits and a server may not enforce them. A box with no area after clamping drops an organism (which needs one) and leaves a damage finding without a region ([vision vlm 9](../vision/vlm.md#9-box-formats)). Certainty words map through the profile's `CertaintyMap`.

@@items vlm/decode.rs Invalid CallImages truncate convert_box findings Expected indicators guided@@

---

## 10. The fallback worker

The fallback worker takes any job still unclaimed 15 s after `queued_at`, with doubled `max_runtime` (the job's own `max_runtime_s` already says so), and runs it on the hosted LLM ([architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback); [vision 8](../vision/README.md#8-the-fallback-path); [vision vlm 12](../vision/vlm.md#12-the-fallback-worker)).

| Kind | What it does |
|---|---|
| `vlm`, `verify`, `guided` | The same `VlmAdapter` with the hosted profile: `abs_px_prepared` boxes on images it won't resize (≤ 1568 px a side and ≤ 1.15 MP), the same prompts with the pixel box line, `output_config.format` with the schema **sanitised** by `integrations::anthropic::sanitize_schema` (it strips every keyword the hosted API rejects, such as `maxLength`, `maxItems` and the numeric bounds, and adds `additionalProperties: false` to every object; the full list is in [contracts 6](../architecture/contracts.md#6-vlm-structured-output) and [integrations](integrations.md)), effort `low`, no `temperature`; decoding truncates and clamps afterwards. `stop_reason: max_tokens` and `refusal` are validation failures |
| `perception` with `question_id` | Completed at once with no regions and no LLM call: its only purpose is masks ([contracts 5, results](../architecture/contracts.md#results)) |
| `perception` | Up to 4 images in the findings order. One survey call (plant present, which images show leaves, the species top 3 from the payload's `label_set_version`), then one disease call over the leaf close-ups restricted to the working species' classes of the payload's `classifier_labels_version`. One whole-image `leaf` region per close-up that shows leaves (`<image_id>.1`, no mask); confidences from certainty words, never a softmax; no `image_embeddings` |

@@items fallback.rs FallbackLabels FallbackModel@@

The label lists come from the same manifest files the perception worker loads, because the payload carries only their versions; a version the fallback hasn't loaded fails with `unknown_label_version`, as on the rig.

The fallback's box format is confirmed with one live call on day 2 ([contracts 12](../architecture/contracts.md#12-open-questions), question 26).

---

## 11. Multi-vendor GPUs

The rig has three cards from two vendors ([architecture 10](../architecture/README.md#10-models-and-gpu-placement); [vision 4](../vision/README.md#4-placement-and-vram-budgets)):

| Card | Worker | Server | Settings |
|---|---|---|---|
| RTX 3070 (8 GB, sm_86) | `--caps perception`, `WORKER_ID=rig-3070-perception-1` | ONNX Runtime 1.31, CUDA 13 EP, in the worker | `ORT_EP=cuda`, `ORT_DYLIB_PATH` to the image's `libonnxruntime.so.1.31.0` |
| RTX 5070 Ti (16 GB, sm_120) | `--caps vlm`, `WORKER_ID=rig-5070ti-vlm-1` | vLLM v0.31.0 (CUDA 13) sidecar | `VLM_ADAPTER=vllm`, `VLM_SERVER=vllm@v0.31.0`, `VLM_BOX_FORMAT=norm1000`, `VLM_CELL_PX=32`, `VLM_SLOTS` from its benchmark |
| RX 9070 XT (16 GB, gfx1201) | `--caps vlm`, `WORKER_ID=rig-9070xt-vlm-1` | vLLM ROCm v0.31.0 sidecar, or llama.cpp Vulkan | `VLM_ADAPTER=vllm` (or `llamacpp`), `VLM_SERVER=vllm-rocm@v0.31.0` (or `llama.cpp-vulkan@<build>`), same box format and cell, its own `VLM_SLOTS` |

- **The VLM workers need no GPU**: they are HTTP clients of their sidecars, and one worker image serves both vendors. A scan's `vlm` job and a later `verify` or `guided` job may land on different cards; every VLM worker serves the same checkpoint, so results are comparable, not identical, and `model_versions` says which card and server produced each ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)). A worker whose `vlm_model` differs from the others is a configuration error shown on `/status`.
- **Two VLM workers race for jobs**: the claim transaction gives each job to exactly one, oldest first; nothing in the worker changes. If the 9070 XT is slower, `VLM_SLOTS=1` there lets it take the overflow.
- **ONNX Runtime on AMD** is possible but not used: perception stays on the 3070. Should it move, the same worker builds with `--features migraphx` (AMD's ORT 1.29 from `rocm/onnxruntime`, which the default `api-29` feature accepts) or `--features webgpu` (Dawn on Vulkan, no ROCm), and `ORT_EP` picks it at start ([vision 4](vision.md#4-onnx-runtime-environment-execution-providers-session-pool)). MIGraphX compiles a program per input shape, so batches would be padded to fixed sizes.

Which server runs on the 9070 XT is open ([contracts 12](../architecture/contracts.md#12-open-questions), question 25): on day 2 its boxes and indicator answers on 10 evaluation photos are compared with the 5070 Ti's.

---

## 12. Deployment

`infra/rig.compose.yml` ([architecture 12](../architecture/README.md#12-deployment)). NVIDIA services are pinned by **GPU UUID**, never by index, through the NVIDIA Container Toolkit; the AMD sidecar gets only `/dev/kfd` and **its own render node by PCI path**, never all of `/dev/dri` (which also holds the NVIDIA nodes), plus the host's numeric `render` and `video` group ids. Images are pinned by digest; the ROCm digest is from the AMD research, the CUDA image's tag and digest are confirmed on day 1.

```yaml
x-worker: &worker
  image: ghcr.io/pdm/worker@sha256:<digest>
  restart: unless-stopped
  stop_grace_period: 30s            # DRAIN_SECONDS (20), the shutdown's bound, + 10 s (section 5)
  environment: &worker-env
    API_BASE_URL: https://api.example.org

services:
  perception-worker:                # RTX 3070
    <<: *worker
    command: ["--caps", "perception"]
    env_file: rig-3070-perception-1.env   # WORKER_TOKEN (never committed)
    environment:
      <<: *worker-env
      WORKER_ID: rig-3070-perception-1
      ORT_EP: cuda
      ORT_DYLIB_PATH: /opt/onnxruntime/lib/libonnxruntime.so.1.31.0
      MODEL_MANIFEST: /models/manifest.toml
    volumes: ["models:/models:ro"]
    deploy:
      resources:
        reservations:
          devices: [{ driver: nvidia, device_ids: ["GPU-<3070 uuid>"], capabilities: [gpu] }]

  vllm-nv:                          # RTX 5070 Ti
    image: vllm/vllm-openai@sha256:<digest of v0.31.0>
    command: >
      Qwen/Qwen3-VL-8B-Instruct-FP8 --revision <sha> --served-model-name vlm
      --max-model-len 16384 --max-num-seqs 2 --max-num-batched-tokens 8192
      --gpu-memory-utilization 0.90 --enable-prefix-caching
      --limit-mm-per-prompt '{"image":{"count":4,"width":1152,"height":864},"video":0}'
      --mm-processor-kwargs '{"size":{"longest_edge":1003520,"shortest_edge":65536}}'
      --structured-outputs-config.backend xgrammar
      --structured-outputs-config.disable_any_whitespace true
      --generation-config vllm --api-key ${VLM_API_KEY}
    volumes: ["hf-cache:/root/.cache/huggingface"]
    ipc: host
    deploy:
      resources:
        reservations:
          devices: [{ driver: nvidia, device_ids: ["GPU-<5070 Ti uuid>"], capabilities: [gpu] }]

  vlm-worker-nv:
    <<: *worker
    command: ["--caps", "vlm"]
    env_file: rig-5070ti-vlm-1.env  # WORKER_TOKEN, VLM_API_KEY (never committed)
    depends_on: [vllm-nv]
    environment:
      <<: *worker-env
      WORKER_ID: rig-5070ti-vlm-1
      VLM_BASE_URL: http://vllm-nv:8000
      VLM_ADAPTER: vllm
      VLM_MODEL: Qwen/Qwen3-VL-8B-Instruct-FP8@<sha>
      VLM_QUANT: fp8_block
      VLM_SERVER: vllm@v0.31.0
      VLM_BOX_FORMAT: norm1000
      VLM_CELL_PX: "32"
      VLM_MAX_PIXELS: "1003520"
      VLM_SLOTS: "2"                # = --max-num-seqs; from the day-2 benchmark

  vllm-amd:                         # RX 9070 XT
    image: vllm/vllm-openai-rocm@sha256:749f6f3f944f12af49966ac523c1f8573e4b229594b541954bd5f879b4496b1f
    command: >                      # the same checkpoint, revision and flags as vllm-nv
      Qwen/Qwen3-VL-8B-Instruct-FP8 --revision <sha> --served-model-name vlm
      --max-num-seqs 1 ...
    devices:
      - /dev/kfd
      - /dev/dri/by-path/pci-0000:<bus>:00.0-render
    group_add: ["<host render gid>", "<host video gid>"]
    security_opt: [seccomp=unconfined]
    cap_add: [SYS_PTRACE]
    ipc: host
    environment:
      FLASH_ATTENTION_TRITON_AMD_ENABLE: "TRUE"   # no HSA_OVERRIDE_GFX_VERSION: gfx1201 is supported

  vlm-worker-amd:
    <<: *worker
    command: ["--caps", "vlm"]
    env_file: rig-9070xt-vlm-1.env  # WORKER_TOKEN, VLM_API_KEY (never committed)
    depends_on: [vllm-amd]
    environment:
      <<: *worker-env
      WORKER_ID: rig-9070xt-vlm-1
      VLM_BASE_URL: http://vllm-amd:8000
      VLM_ADAPTER: vllm
      VLM_MODEL: Qwen/Qwen3-VL-8B-Instruct-FP8@<sha>
      VLM_QUANT: fp8_block           # awq_w4a16 on both cards if FP8 doesn't run on gfx1201
      VLM_SERVER: vllm-rocm@v0.31.0
      VLM_BOX_FORMAT: norm1000
      VLM_CELL_PX: "32"
      VLM_MAX_PIXELS: "1003520"
      VLM_SLOTS: "1"

volumes: { models: {}, hf-cache: {} }
```

- **If llama.cpp serves the 9070 XT**: `ghcr.io/ggml-org/llama.cpp:server-vulkan` at a pinned build and digest, with only the render node; `-np` equal to `VLM_SLOTS`, `--image-max-tokens` at least `VLM_MAX_PIXELS / 32²` and no `--image-min-tokens`; `VLM_ADAPTER=llamacpp`; and both cards then serve the same GGUF files ([vision vlm 4](../vision/vlm.md#4-serving)). Port 8080 inside the image.
- **A second host**: if the 9070 XT goes in another machine, `infra/rig-amd.compose.yml` holds only `vllm-amd` and `vlm-worker-amd`; nothing else changes, since workers only pull.
- **The fallback worker** runs on the control plane (`infra/control-plane.compose.yml`) with `--caps fallback`, `LLM_API_KEY`, `MODEL_MANIFEST` pointing at the label files, and no GPU. Its grace period follows the same rule as the rig's services ([architecture 12](../architecture/README.md#12-deployment)): `DRAIN_SECONDS` plus 10 s, since its shutdown has the same bound.

  ```yaml
  fallback-worker:                  # infra/control-plane.compose.yml
    image: ghcr.io/pdm/worker@sha256:<digest>
    restart: unless-stopped
    stop_grace_period: 30s          # DRAIN_SECONDS (20), the shutdown's bound, + 10 s (section 5)
    env_file: fallback.env          # WORKER_TOKEN, LLM_API_KEY (never committed)
    environment:
      WORKER_CAPS: fallback
      WORKER_ID: cp-fallback-1
      API_BASE_URL: http://api:8080
      MODEL_MANIFEST: /models/manifest.toml
  ```
- **Images**: the worker image installs CA certificates (reqwest 0.13's rustls verifier fails without them) and, for perception, the CUDA 13 runtime with cuDNN 9 and ONNX Runtime 1.31's libraries next to `ORT_DYLIB_PATH` (`nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` as the base); the host has NVIDIA driver 580.65.06 or later ([vision 5](../vision/README.md#5-runtimes-and-versions)).

---

## 13. Concurrency and lifetimes

| Task | Lives | Token |
|---|---|---|
| Heartbeat | until after the drain | `stop_heartbeat` (not a child of `shutdown`, so draining jobs keep their leases); it also drops a call in flight |
| Claim loop | until SIGTERM | `shutdown`; `biased` `select!` drops an open long poll |
| One task per job, on the `TaskTracker` | until `complete` or cancellation | a child of `abandon` |
| CPU work (decode, resize, JPEG, pre-processing) | one blocking task per batch | an `OwnedSemaphorePermit` moved into the closure |
| ONNX Runtime | the inference thread (perception) | `TerminateOnDrop` |

**`'static` job tasks.** `TaskTracker::spawn` requires `F: Future + Send + 'static`: the task may outlive `Worker::start` and runs on any runtime thread. So the task's future owns everything it touches: `Arc` clones of the API and runner, the owned `ClaimedJob`, the slot permit, the `Registered` guard and the token. Inside it, `run_job(&api, &runner, …)` borrows those owned values for the task's own duration, which is fine: the borrow is of the task's state, not of the caller's.

**`impl Future + Send` in `WorkerApi` and `VlmBackend`.** Both traits are used generically, never as `dyn`, so they return `impl Future` (return-position `impl Trait` in traits) instead of a boxed future, and spell out `+ Send` because callers' futures must be `Send` to be spawned or awaited inside `#[async_trait]` methods (which box a `Send` future). Under edition 2024 the returned future captures every in-scope lifetime, here `&self` and the borrowed arguments, which is what a future that reads them needs; `VlmBackend::call<'a>` names `'a` because the future borrows both `&'a self` and the `&'a VlmCall<'a>`.

**Named lifetimes.**

| Lifetime | Where | What it borrows |
|---|---|---|
| `'a` | `VlmCall<'a>`, `VlmImage<'a>` | the call's prompts and schema and the job's prepared JPEG bytes; the backend serialises the request once and keeps none of it |
| `'c` | `CallImages<'c>` | the job's prepared images, to map aliases back while decoding |
| `'p` | `schema::Question<'p>` | the payload's indicators, to build the schema, the prompt and the expected keys without copying them |

**`Send` across `spawn_blocking`.** `prepare_all` moves owned clones of the images (each `Bytes` clone is a reference count) and of the profile into the closure; the result (`Vec<PreparedImage>`) is owned and `Send`. Nothing borrowed crosses.

**Shared state.** The registry of held jobs is a `std::sync::Mutex<HashMap<JobId, Entry>>`, never held across an `.await`; each critical section is one insert, remove or read, so a poisoned lock is still consistent (`PoisonError::into_inner`). `Registered`'s `Drop` removes a job from the heartbeat list however its task ends, before its slot is freed.

---

## 14. Errors

| Failure | `complete` | Retryable |
|---|---|---|
| `v` other than 1, or a `v: 1` job whose kind or payload this build can't read | `unsupported_version` | no |
| A label list or classifier mapping not loaded (`vision::VisionError::UnknownLabelVersion`, found by downcasting the model error's source) | `unknown_label_version` | no |
| A model failure, a VLM server or LLM API error, output invalid twice | `model_error` | yes (`ModelError::retryable`) |
| The adapter can't do the kind (`ModelError::Unsupported`) | `model_error` | no |
| An image fetch that fails after its retries | `image_fetch_failed` | yes |
| The soft deadline | `max_runtime_exceeded` | yes |
| `409 lease_lost` anywhere, `cancel_job_ids`, the drain | nothing is sent | |

@@items job.rs job_error@@

The message is the error and its `source()` chain, `": "`-joined ([rust-errors](#18-sources)). `JobErrorCode` keeps unknown codes as `Other(String)`, so an older API reading a newer worker's code doesn't fail to decode.

---

## 15. Testing

| Test | Shows |
|---|---|
| `holds_at_most_slots_jobs` | With 2 slots and hung models, exactly 2 claims in 60 s |
| `a_finished_job_completes_with_versions_and_timings` | `done` with `worker_id` in `model_versions` and the kind in `timings_ms` |
| `lease_lost_from_a_progress_event_drops_the_job` | A `409 lease_lost` on `events` cancels the job's token |
| `heartbeat_lists_jobs_and_cancels_the_listed_ones` | `current_jobs` carries the lease; `cancel_job_ids` cancels the token |
| `another_version_fails_with_unsupported_version` | `failed`, `unsupported_version`, not retryable, model never runs |
| `an_image_that_cannot_be_fetched_fails_the_job_retryably` | `image_fetch_failed`, retryable |
| `complete_is_retried_after_a_timeout` | Two failed `complete`s, then one lands |
| `a_hung_model_is_stopped_before_max_runtime` | `max_runtime_exceeded` at 105 s for `max_runtime_s: 120` |
| `shutdown_abandons_jobs_after_the_drain_bound` | The drain waits exactly 20 s, sends nothing, frees the slot |
| `claim_errors_back_off_instead_of_spinning` | 6–8 claims in 60 s against an unreachable API |
| `claim_errors_wait_for_retry_after` | Claims at 0, 20 and 40 s after errors with `retry_after_s: 20` |
| `complete_waits_for_retry_after` | After a `503` with `retry_after_s: 7`, the next `complete` goes out 7 s later, not after the 0.25–0.5 s backoff |
| `a_complete_in_flight_does_not_hold_up_the_drain` | A `complete` that never answers is dropped at the 20 s bound, and the slot is freed |
| `stopping_the_heartbeat_drops_a_call_in_flight` | The heartbeat loop ends at once, not after the call's 5 s |
| `the_whole_shutdown_ends_within_the_drain_bound` | `run_until` with a hung `complete` and a hung heartbeat returns exactly 20 s after the signal |
| `classify_body` tests (5) | `lease_lost` only from a `409` with that code; `retryable: true` → `Unavailable` with `retry_after_s`; `retryable: false` → `Rejected` whatever the status (`500 internal`, `429 budget_exceeded`); without an error body only `429` and `5xx` are retried |
| `a_cut_off_answer_is_retried_once_then_succeeds` | `finish_reason: length`, then a valid answer; `vlm_indicators` skipped with `all_measured` |
| `two_invalid_answers_fail_with_a_retryable_model_error` | Unparseable, then a refusal → `InvalidOutput`, retryable |
| `images_are_prepared_on_the_cell_grid` | A 64 × 48 image becomes 64 × 32 on the 32-pixel grid |
| schema, decode, prepare unit tests | `reason` before `answer` in the serialised schema; `unsure`-only questions; payload enums and box bounds; one guided branch; aliases, boxes, truncation and certainty mapped; a zero-area organism box drops the organism; unknown species and missing keys fail; Qwen3 cells and the pixel budget; the hosted 1568-pixel cap |

All loop tests run on paused time (`start_paused = true`) against a fake `WorkerApi`; the fake answers an empty claim like a 25 s long poll, so the clock can advance. Contract tests of `HttpWorkerApi` against a live `api` belong to the end-to-end tests ([api](api.md)). The day-2 benchmark ([vision evaluation](../vision/evaluation.md)) checks the adapters against the real servers: valid output, xgrammar's enforcement of `maxLength` and `maxItems`, boxes drawn back on the prepared images, and the 9070 XT against the 5070 Ti.

---

## 16. Decisions on the contracts' open questions

The questions of [contracts 12](../architecture/contracts.md#12-open-questions) that the worker settles. Contracts.md isn't edited here; the coordinator applies them.

| Question | Decision | Where |
|---|---|---|
| 18, job error codes | `unsupported_version` (no), `unknown_label_version` (no), `model_error` (yes; no when the adapter can't do the kind), `image_fetch_failed` (yes), and **`max_runtime_exceeded`** (yes) for a job the worker stopped at its soft deadline, as the skills assumed. Added to `domain::jobs::JobErrorCode`, which keeps unknown strings as `Other` | sections 6, 14; `domain::jobs` |
| 19, is `unsupported_version` retryable? | No: the same worker can't succeed on a retry, and a newer worker gets the job only when it is requeued after its attempts | `run_job` |
| 20, lease hand-back at shutdown | Confirm the wait: no hand-back call. At SIGTERM the claim loop drops an open long poll; a lease the server granted meanwhile is requeued when the heartbeat goes stale (30 s) | section 5 |
| 21, an unknown job `kind` | The claim already filters by the declared `caps`. If one slips through (an older worker, a newer API), the worker reads the head (`v`, `job_id`, `lease_token`) and completes `failed` with `unsupported_version`, not retryable, naming the problem in the message, instead of leaving it to expire | `decode_claim`, `run_job` |
| 23, a missing `X-Lease-Token` | The worker always sends it on every call about a job; whatever the API answers for a missing one (`400 invalid_request` in the skills) is a bug the worker logs and never retries (`CallError::Rejected`). The status itself is the API's decision | `HttpWorkerApi::leased`, `classify` |

---

## 17. Open questions

1. **`organism_options` without recognition text** ([vision vlm 7](../vision/vlm.md#7-prompts)) is still a proposal ([contracts open question 36](../architecture/contracts.md#12-open-questions)).
2. **The fallback's runtime.** Opus 5.5 always thinks, so every hosted call gets 8,000 output tokens ([contracts 6](../architecture/contracts.md#after-decoding)). Whether calls that long finish within the fallback's doubled `max_runtime` is measured on day 2 ([contracts open question 26](../architecture/contracts.md#12-open-questions)).
3. **`anyOf` with `null`** for nullable values in the schema is *unverified* on xgrammar 0.2.7 (the day-2 schema check, [contracts open question 31](../architecture/contracts.md#12-open-questions)).
4. **Cancellation on llama.cpp**: whether a dropped connection frees the slot is *unverified* ([contracts open question 33](../architecture/contracts.md#12-open-questions)).

**Resolved with docs/vision v0.2.** Contracts 5 and 6 and [vision 6](../vision/README.md#6-model-files-versions-and-pinning) and [vision vlm 11–12](../vision/vlm.md#11-validation-retries-and-versions) now agree on what this section used to list: the `model_versions` value formats (underscores, `@` versions, commas between prompts, slashes between certainty values); per-model keys named by their manifest entry, with SAM as `sam_encoder` and `sam_decoder`; `label_set_version` and `classifier_labels_version` as keys, comma-separated in the heartbeat; boxes with no area; box bounds for pixel formats; the hosted model's 1,568-pixel and 1.15-megapixel caps; the fallback loading its label files from the manifest; JPEG at quality 90 with each side rounded down; the 8,000-token hosted budget; and `BoxFormat`'s comment, which names `norm_1000` as the Qwen3-VL format and `abs_px_prepared` as the fallback's.

---

## 18. Sources

**Rust language**
- The Book: [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [17 async and await](https://doc.rust-lang.org/book/ch17-00-async-await.html) · [15.3 `Drop`](https://doc.rust-lang.org/book/ch15-03-drop.html)
- The Reference: [`impl Trait` in traits](https://doc.rust-lang.org/reference/types/impl-trait.html) · [dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)
- [Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · [Async Book](https://rust-lang.github.io/async-book/)
- tokio 1.53.3: [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) · [`select!` fairness](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#fairness) · [`MissedTickBehavior`](https://docs.rs/tokio/1.53.3/tokio/time/enum.MissedTickBehavior.html) · [`timeout_at`](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout_at.html) · [`Semaphore::acquire_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned) · [paused time](https://docs.rs/tokio/1.53.3/tokio/attr.test.html#configure-the-runtime-to-start-with-time-paused)
- tokio-util 0.7.20: [`CancellationToken`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html) · [`TaskTracker`](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)

**Crates and servers**
- [reqwest 0.13.5](https://docs.rs/reqwest/0.13.5/reqwest/) · [clap 4.6.7 derive](https://docs.rs/clap/4.6.7/clap/_derive/index.html) · [serde_json `preserve_order`](https://docs.rs/serde_json/1.0.154/serde_json/#features) · [secrecy 0.10.3](https://docs.rs/secrecy/0.10.3/secrecy/)
- [vLLM structured outputs](https://docs.vllm.ai/en/stable/features/structured_outputs.html) · [vLLM Docker deployment (ROCm)](https://docs.vllm.ai/en/latest/deployment/docker.html) · [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md) · [Anthropic structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs) · [Anthropic vision coordinates](https://platform.claude.com/docs/en/build-with-claude/vision-coordinates)
- [ROCm Docker](https://rocm.docs.amd.com/projects/install-on-linux/en/latest/how-to/docker.html) · [NVIDIA Container Toolkit with Compose](https://docs.docker.com/compose/how-tos/gpu-support/)

**Project documents, research and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [domain](domain.md) · [vision](vision.md) · [integrations](integrations.md) · [api](api.md)
- [Computer-vision design](../vision/README.md): [VLM](../vision/vlm.md), [models](../vision/models.md), [evaluation](../vision/evaluation.md)
- The backend research notes (APIs, AMD RX 9070 XT, model decisions)
- Skills in the main checkout's `.claude/skills/`: `rust-workers` (`SKILL.md`, `loop.md`, `reqwest.md`, `ort.md`), `rust-async`, `rust-errors` (`jobs.md`), `rust-testing`, `rust` (`versions.md`)

External links were taken from the research notes, which opened them on 2026-10-15; facts marked ✓ were compiled or run there or in the scratch workspace.
