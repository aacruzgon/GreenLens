# PDM Backend: `worker` binary

> **Status:** Draft v0.5.15, 2026-10-24. v0.5.15: section 9: a damage finding names its `trace` first, and decoding drops one that names no animal or trace (vision dev-roadmap P11); the prompts are `findings@6`. v0.5.14: section 12 says how `sam-linux` runs `vlm-worker-nv` beside `vllm-nv` (vision dev-roadmap P6). v0.5.13: the `vllm-nv` comment: the FP8 fallback runs at 0.90 with nothing open on the desktop, or headless. v0.5.12: section 3 lists the stub, the test harnesses and the VLM tests' files, and section 15 the ignored harness tests: `perceive_job` runs one perception job without the API (vision dev-roadmap P2), `bench_cases` the day-2 benchmark and `record_against_live_server` the fake server's recordings. v0.5.11: `run_until` carries on with the normal shutdown when the stop signal fails, logs the error and returns it at the end. v0.5.10: a stop releases the model before `main` returns and then logs `stopped`: `run_until` drops the last handle on the worker, and on the perception worker that joins the inference thread after it has released its ONNX Runtime sessions, which fixes the 3070's exit 139 on SIGTERM (section 5). v0.5.9: section 12's `vllm-nv` example is the FP8 fallback, headless at 0.90; the default, Qwen3.5-9B AWQ, serves at 0.78 beside the 5070 Ti's desktop (D11, #13). v0.5.8: the indicators and verify prompts are `indicators@3` and `verify@3`, whose reason asks for at most 60 characters (vision vlm.md 7, issue #137). v0.5.7: the prompts are `system@2`, `findings@5`, `indicators@2` and `verify@2` (vision vlm.md 7); an `other` species or organism whose `other_name` names a listed option takes its id, a damage finding keeps `other_description` only with `other`, a repeated damage finding is dropped, and a string that reaches its limit goes back to its last full sentence or word (section 9). v0.5.6: the 3070's perception worker ran on the rig (S2): `worker-cuda` loads every perception model on CUDA and heartbeats `ort_ep: cuda`; it runs in `control-plane.compose.yml` (the 3070 is in the control-plane host), and the default log filter is `info,ort=warn` (section 12). v0.5.5: ST1 to ST5 are built and ran on a laptop (S1): with the empty knowledge base's label versions (`STUB_LABEL_SET_VERSION=d4f0f0534fb74a8d`, `STUB_CLASSIFIER_LABELS_VERSION` empty) a scan streams all five perception stages and goes on to `context`; the worker flags `--vlm-*` and `--fallback-*` have distinct ids, and the worker logs at `info` without `RUST_LOG`. v0.5.4: the adapter sends an indicators call as parallel parts when its backend limits the questions per call (`VlmBackend::max_questions`; 12 on the hosted model, because the API refuses a schema of 20 question objects as too large and a call can ask 27), each part with its own schema and prompt and retried on its own; two ignored live tests of the hosted schemas; 35 tests. v0.5.3: the perception worker reads its thresholds from the manifest with `PerceptionConfig::from_manifest`, the environment sets only the batch caps and never above the manifest's, and start-up refuses an fp16 model on `ORT_EP=cpu` or a file whose tensor names differ from the manifest (gaps G4, G6 and G11); `VLM_BASE_URL` defaults to `http://vllm-nv:8000`, the compose service's name (G7), all from [issue #12](https://github.com/aacruzgon/GreenLens/issues/12). v0.5.2: open question 3 is settled: xgrammar enforces `anyOf` with `null`. v0.5.1: section 12's image path is the registry's, `ghcr.io/aacruzgon/greenlens/worker` ([infra 3](../infra/README.md#3-images)). v0.5: section 15.1 designs stub vision (`--stub-vision`, refused in a release build without `PDM_ALLOW_STUB_VISION`), the canned `perception` result the API accepts, its marks in `model_versions` and logs, the tests, and seven work items for the roadmap's slice S1; section 17 has six new questions; no section number changes. v0.4.2: the rig's compose listing gives each worker an `env_file` with its own `WORKER_TOKEN` (the worker reads no `WORKER_TOKEN_FILE`). v0.4.1: section 17 keeps only the four questions still open, since docs/vision v0.2 and contracts 5–6 now agree on the rest; section 1 drops the image-fetch difference (the skill uses 30 s too); section 12 lists the fallback service with its `stop_grace_period`. v0.4: the SIGTERM shutdown ends within `DRAIN_SECONDS`, dropping a `complete` or heartbeat call in flight (it could take 20 + 10 + 5 = 35 s, past the 30 s grace); the worker API client retries by the error body's `retryable` and `retry_after_s` (contracts 2), not by status, and waits `retry_after_s`; section 1 states the HTTP timeouts and only the two skill differences that remain. v0.3.1: why the immediate re-claim after a `204` can't spin while the API shuts down (probed). v0.3: the hosted fallback gives the guided check the same 8,000-token budget as the other calls (docs/vision README 12 #27), and the fallback row points to the full list of stripped schema keywords. v0.2: the fallback worker reads its hosted-LLM key from `LLM_API_KEY`, the name `api` and the backend README §9 use (it was `ANTHROPIC_API_KEY`). v0.1: the worker process (`--caps perception|vlm|fallback`): configuration, the heartbeat and claim loops with slots, leases and the SIGTERM drain, cancellation by drop, the VLM adapter with its per-call schema builder and decoding, the hosted fallback, per-card settings for the RX 9070 XT, and the rig's compose services; compiled on Rust 1.99.1.

Companion to the [backend architecture](README.md), the [`vision` crate](vision.md) (the perception models and image code it uses), the [`integrations` crate](integrations.md) (its HTTP clients for the VLM server and the LLM API) and the [`domain` crate](domain.md) (the job types and the `VisionModel` port). Every listing is copied from the compiled source of `bins/worker` by the docgen tool. `cargo check -p worker` (also `--features cuda`), `cargo clippy -p worker --all-targets` and `cargo test -p worker` pass: 35 tests, among them the loop on paused time against a fake broker and the VLM adapter against a scripted backend.

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
├─ stub.rs               StubModel, StubConfig: canned perception results (section 15.1)
├─ stub/tests.rs         the stub's tests (section 15.1.5)
├─ harness.rs            perceive_job (ignored): one perception job without the API, through run_job
├─ vlm.rs                VlmAdapter<B> (VisionModel): analyse, verify, read_guided_check, call_validated
├─ vlm/profile.rs        AdapterProfile (box format, cell, pixel budget), CertaintyMap, CallBudget, model_versions
├─ vlm/backend.rs        VlmBackend, VlmCall<'a>, OpenAiBackend, AnthropicBackend
├─ vlm/prepare.rs        target_size, prepare, prepare_all
├─ vlm/schema.rs         the per-call JSON Schema builder
├─ vlm/prompt.rs         system, findings, indicators, verify and guided prompts
├─ vlm/decode.rs         validation, alias mapping, box conversion, truncation
├─ vlm/tests.rs          the adapter against a scripted backend
├─ vlm/tests/record.rs   record_against_live_server (ignored): the worker's calls recorded for the fake server; stored
├─ vlm/tests/bench.rs    bench_cases (ignored): the day-2 benchmark's driver
├─ vlm/tests/live.rs     live_indicators_worst_case, live_findings (ignored): the hosted schemas
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
| `PERCEPTION_MAX_BATCH`, `DETECTOR_MAX_BATCH`, `SAM_ENCODER_MAX_BATCH`, `SAM_MAX_BOXES`, `BIOCLIP_MAX_BATCH` | 16, 8, 2, 16, 16 | perception | Caps ([contracts 5, batching](../architecture/contracts.md#results)). They change speed, not results, and none may exceed its model's `max_batch` (`max_boxes`) in the manifest; every threshold comes from the manifest's `[model.config]` ([vision 5](vision.md#5-the-model-manifest)) |
| `VLM_BASE_URL`, `VLM_API_KEY`, `VLM_SERVED_NAME` | `http://vllm-nv:8000`, —, `vlm` | vlm | The sidecar |
| `VLM_ADAPTER` | `vllm` | vlm | `vllm` or `llamacpp` |
| `VLM_MODEL`, `VLM_QUANT`, `VLM_SERVER` | | vlm | `vlm_model`, `vlm_quant`, `vlm_server` in `model_versions` |
| `VLM_BOX_FORMAT`, `VLM_CELL_PX`, `VLM_MAX_PIXELS` | `norm1000`, 32, 1,003,520 | vlm | The adapter instance ([contracts 6](../architecture/contracts.md#box-formats-per-model-adapter)) |
| `VLM_SLOTS` | 1 | vlm | Jobs held at once; the sidecar's `--max-num-seqs` (or `-np`) is the same |
| `LLM_API_KEY`, `FALLBACK_VISION_MODEL` | —, `claude-opus-5-5` | fallback | The hosted model |
| `FALLBACK_SLOTS`, `FALLBACK_MAX_LONG_SIDE`, `FALLBACK_MAX_PIXELS` | 4, 1568, 1,150,000 | fallback | Slots, and the size the API won't resize |
| `WORKER_STUB_VISION`, `PDM_ALLOW_STUB_VISION`, `STUB_*` | off, unset | perception | Canned results for the walking skeleton; refused in a release build without the allow variable ([section 15.1](#151-stub-vision)) |

Start-up fails before the first claim on a missing key, a missing `[model.config]` threshold, a batch cap above the manifest's, an ONNX file whose tensor names differ from its entry, an fp16 model with `ORT_EP=cpu`, a file whose hash differs from the manifest, a provider that isn't compiled in or doesn't register, or a VLM server that doesn't serve `VLM_SERVED_NAME` (`GET /v1/models`).

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

**Shutdown time.** From SIGTERM to exit takes at most `DRAIN_SECONDS`, 20 s. The claim loop returns at once and drops an open long poll. Running jobs get the 20 s. At the bound, `abandon` cancels every job's token, and every await in a job task selects on that token: the model, an image fetch, a `complete` call in flight and the backoff between `complete` calls. So the tasks end at once. Then the heartbeat stops, and its `stop` token drops a call in flight too. What is left when `main` returns is a decode or resize already started on the blocking pool, which the runtime waits for, and an ONNX Runtime run, which `TerminateOnDrop` has already told to stop and whose end the inference thread's join waits for. Compose's `stop_grace_period: 30s` leaves 10 s for these and for Docker (section 12). Without the two cuts, a `complete` in flight at the bound (10 s deadline) and then a heartbeat in flight (5 s deadline) would run on after the drain: 20 + 10 + 5 = 35 s, past the grace, and Docker would SIGKILL the worker. A `complete` cut at the bound is either committed by the API or not. If not, the job is requeued 30 s after the last heartbeat, like any abandoned job, and the requeue is free for the job's first two. `the_whole_shutdown_ends_within_the_drain_bound` shows exactly 20 s on paused time.

**Releasing the model before exit.** `run_until` takes the worker by value, and every task holding a clone has ended when it returns, so the worker and its model drop there; `main` then logs `stopped` and returns 0. The order matters on the perception worker: the model owns the inference thread and its CUDA sessions, and returning from `main` calls libc `exit`, which unloads the CUDA libraries without waiting for other threads. The thread used to be detached, so it was still releasing sessions during `exit`: on 2026-10-20 the 3070's worker logged `shutting down: no new claims` and died with SIGSEGV (exit 139) 0.7 s later, holding no job and logging nothing more, while the CPU worker stopped cleanly. Dropping the last `Inference` now joins the thread after it has released the sessions and the environment ([vision 6](vision.md#the-inference-thread)). `run_until_releases_the_model_before_it_returns` checks that the model is gone when `run_until` returns, even with a job abandoned at the bound.

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

`vlm_prompts` is `system@2,findings@6,indicators@3,verify@3,guided@1`, in the contract's `name@n` form with the prompt versions of [vision vlm 7](../vision/vlm.md#7-prompts); `vlm_certainty` is `0.4/0.65/0.85`.

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

@@items vlm/backend.rs VlmImage VlmCall Stop Tokens VlmReply VlmBackend OpenAiBackend AnthropicBackend MAX_QUESTIONS_PER_CALL@@

**Large indicators calls on the hosted model.** Anthropic compiles each structured-output schema into a grammar and refuses one that grows too large ("The compiled grammar is too large"). Live on 2026-10-16, with 9-character keys and 3 images, 16 question objects passed and 20 failed; with full-length keys (`q27_` + 24 slug characters) and 4 images, 18 passed and 20 failed; and at 27, dropping each question's `image` or `reason` didn't help. The number of question objects is what counts, so 12 leaves a margin of 6. A call can ask 27 (9 candidates × 3 discriminating indicators, [contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)). So `VlmBackend::max_questions` says how many questions one call may carry: `MAX_QUESTIONS_PER_CALL` (12) for `AnthropicBackend`, no limit for `OpenAiBackend`. `VlmAdapter::indicators` splits a longer list into parts of at most that many, in question order, and sends them in parallel. Each part has its own schema and a prompt that lists only its questions, keeps the question keys of the whole list, and goes through `call_validated` on its own, so a cut-off or invalid part is asked again without the others. The answers are joined in question order, which is what one call would have returned. A worst-case scan on the hosted model makes 3 calls, each sending the images again, and up to 6 if every part needs its retry. vLLM and llama.cpp take the whole list in one call.

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
| A damage finding's `trace` | a plain string (`maxLength` 60), the first property, before `pattern`, so the model names the animal or the trace it left before it picks a pattern; no union, so the hosted schema stays within its 16 | not passed on: decoding drops a finding without one |
| Guided | only the branch of the image's kind | rejected if the other branch comes back |

Every object has `additionalProperties: false`; length-limited strings have no `pattern` (xgrammar drops length bounds next to one, and vLLM rejects the combination ✓ per the vision research); `organisms` and `damage` have `maxItems` 10.

@@items vlm/schema.rs Limits BoxRange Question questions findings indicators guided@@
@@items vlm/prompt.rs system image_label findings indicators guided@@

**After decoding.** Strings are truncated to their limits by characters and boxes clamped for every backend, because the hosted API's schema loses the limits and a server may not enforce them. A string that reaches its limit was most likely stopped there by `maxLength` mid-word, sometimes in another language ([vision vlm 4](../vision/vlm.md#4-serving)), so `truncate` takes it back to its last full sentence, or failing that its last full word; a shorter string is kept as it is. A box with no area after clamping drops an organism (which needs one) and leaves a damage finding without a region ([vision vlm 9](../vision/vlm.md#9-box-formats)). A damage finding whose `trace` names no animal and nothing an animal left is dropped, and logged at debug with its pattern and trace: an empty trace (or a missing one), a negated one ("no insect visible") or one made only of disease, colour, shape, plant-part and joining words ("brown spots with a yellow halo"). Qwen3.5 reports a tomato's early-blight spots as `leaf_mines` and FP8 a corn's blight as `chewed_margins` whatever the prompt's wording, so a disease lesion reported as damage would add an organism candidate from outside the set, a `verify` job and a `pest_or_damage` status; the trace is the structural check (task P11 of [vision dev-roadmap](../vision/dev-roadmap.md), [vision vlm 4](../vision/vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran)). The trace stays in the worker: `DamageFinding` has no field for it, since nothing downstream reads it. A damage finding with the same pattern, image, region and `other_description` as an earlier one is dropped, because a model in a loop can fill the list with one finding. An `other` species or organism whose `other_name` is a listed option's id or name (case, surrounding spaces and `_` for a space don't matter) takes that option's id: the day-1 check got `other` with `other_name: "apple"` while `apple` was listed, and a prompt that asks for the id silences Qwen3.5's soil readings ([vision vlm 4](../vision/vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran)). `other_name` and `other_description` are kept only with `other`, so a known species, organism or damage pattern carries no free text. Certainty words map through the profile's `CertaintyMap`.

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

The fallback's box format was confirmed by a live run on 2026-10-23 ([contracts 12](../architecture/contracts.md#12-open-questions), question 26; [report](../../ml/training/eval/reports/p9-fallback-2026-10-23.md)): Opus 5.5 answers in pixels of the image as sent.

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

**On the rig today** (S2, 2026-10-20): the RTX 3070 is in the control-plane host, so `perception-worker` runs in `infra/control-plane.compose.yml` behind the `perception` profile, with `API_BASE_URL=http://api:8080` ([infra 4](../infra/README.md#infracontrol-planecomposeyml)). On `sam-linux`, `vlm-worker-nv` runs in `infra/rig.compose.yml` beside `vllm-nv` and claims through the control plane's public URL; it takes `VLM_MODEL` (as `VLM_MODEL@VLM_REVISION`), `VLM_SLOTS` (as `VLM_MAX_NUM_SEQS`), `VLM_MAX_PIXELS` and `VLM_API_KEY` from the sidecar's own settings in `infra/.env`, and `VLM_QUANT` is `awq_w4a16` for Qwen3.5-9B AWQ ([infra 4](../infra/README.md#infrarigcomposeyml), vision dev-roadmap P6). The listing below is the single-host layout; the second rig host runs only the VLM services. Without `RUST_LOG` the worker logs at `info,ort=warn`: ONNX Runtime logs each session's graph optimisation at `info` through `ort`, about 7,600 lines when this worker starts. Its first start loaded all five perception models on CUDA in 2.5 s and used 902 MiB on the card before any job.

`infra/rig.compose.yml` ([architecture 12](../architecture/README.md#12-deployment)). NVIDIA services are pinned by **GPU UUID**, never by index, through the NVIDIA Container Toolkit; the AMD sidecar gets only `/dev/kfd` and **its own render node by PCI path**, never all of `/dev/dri` (which also holds the NVIDIA nodes), plus the host's numeric `render` and `video` group ids. Images are pinned by digest; the ROCm digest is from the AMD research, the CUDA image's tag and digest are confirmed on day 1.

```yaml
x-worker: &worker
  image: ghcr.io/aacruzgon/greenlens/worker@sha256:<digest>
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

  vllm-nv:                          # RTX 5070 Ti, here the FP8 fallback at 0.90 (nothing open on the desktop, or headless);
                                    # the default, Qwen3.5-9B AWQ, serves at 0.78 beside its desktop (D11)
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
    image: ghcr.io/aacruzgon/greenlens/worker@sha256:<digest>
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
| `questions_beyond_the_backend_limit_go_in_parts_each_retried_alone` | A backend taking 2 questions a call gets 5 as parts of 2, 2 and 1, each schema with only its keys; the cut-off part alone is asked again; the answers come back in question order |
| `live_indicators_worst_case`, `live_findings` (ignored) | Against the real API, no images: 27 questions with full-length keys and 4 images in parts of 12, every one answered (`LIVE_QUESTIONS` and `LIVE_PER_CALL` probe the limit); the findings schema accepted |
| `perceive_job` (ignored) | One perception job without the API (vision dev-roadmap P2): the model from `build_model` and the worker's environment, the job through `run_job` with its soft deadline, the photos stored as the bucket holds them; writes the `complete` body, stage times, a summary per image and, with `nvidia-smi`, peak VRAM. `PERCEPTION_HARNESS_PHOTOS` or `PERCEPTION_HARNESS_JOB` in, `PERCEPTION_HARNESS_OUT` out |
| harness unit tests (3) | `kind=path` pairs become a payload in upload order with every stage; a job file reads as its payload and photos; a stub run through `run_job` leaves its `complete` body and events with the file-backed API |
| `bench_cases`, `record_against_live_server` (ignored) | Against a VLM server: the day-2 benchmark's cases through `analyse` with each call's time and finish (`VLM_BENCH_*`); the worker's own calls recorded for the fake server (`VLM_RECORD_*`) |
| schema, decode, prepare unit tests | `reason` before `answer` in the serialised schema; `unsure`-only questions; payload enums and box bounds; one guided branch; aliases, boxes, truncation and certainty mapped; a zero-area organism box drops the organism; unknown species and missing keys fail; Qwen3 cells and the pixel budget; the hosted 1568-pixel cap |

The stub's tests are in [section 15.1.5](#1515-tests). All loop tests run on paused time (`start_paused = true`) against a fake `WorkerApi`; the fake answers an empty claim like a 25 s long poll, so the clock can advance. Contract tests of `HttpWorkerApi` against a live `api` belong to the end-to-end tests ([api](api.md)). The day-2 benchmark ([vision evaluation](../vision/evaluation.md)) checks the adapters against the real servers: valid output, xgrammar's enforcement of `maxLength` and `maxItems`, boxes drawn back on the prepared images, and the 9070 XT against the 5070 Ti.

### 15.1 Stub vision

The roadmap's first slice (S1, [roadmap 5](roadmap.md#5-s1-walking-skeleton)) runs a scan from the app through the API and the broker to a worker and back over SSE, before any GPU or model file exists. The worker has only the real perception path and the hosted fallback, so S1 has nothing to claim a `perception` job ([roadmap gap 2](roadmap.md#13-gaps-found-while-writing-this-roadmap)). **Stub vision** is a `VisionModel` that answers `perception` jobs with canned, valid results. It is a development tool. It is off by default, it is refused in a release build unless the operator says so twice, and it marks everything it produces.

ST1 to ST5 of section 15.1.7 are compiled and tested (`bins/worker/src/stub.rs`, `StubArgs` and `check_stub_allowed` in `config.rs`); ST6 and ST7 are not. The listings labelled "Proposed (not compiled)" are the design as first written.

#### 15.1.1 Turning it on

| Setting | Name | Default | Meaning |
|---|---|---|---|
| Flag | `--stub-vision` (env `WORKER_STUB_VISION`) | off | Use `StubModel` instead of the real model for `--caps perception` |
| Allow variable | `PDM_ALLOW_STUB_VISION=1` | unset | Required in a release build (`cfg!(debug_assertions)` is false). Debug builds and `cargo test` don't need it |
| `STUB_LABEL_SET_VERSION`, `STUB_CLASSIFIER_LABELS_VERSION` | env only | none, **required** | The label versions the stub reports in its heartbeat (comma-separated when several). Set them to the values the API queues, which are the knowledge base's `label_set_version` and the classifier mapping's version |
| `STUB_SPECIES` | env only | `tomato` | Comma-separated species slugs used as candidates (section 15.1.2) |
| `STUB_PROBLEMS` | env only | empty | Comma-separated problem slugs the disease stage may add below `healthy` |
| `STUB_PLANT_PRESENT` | env only | `true` | `false` makes the plant check fail, to exercise `no_plant_found` |
| `STUB_STAGE_DELAY_MS` | env only | 150 | Pause inside each stage, so the SSE stream shows the stages in order and a test can kill the worker mid-job. `10000` leaves a window of about a minute for the lease-expiry check |

**The rules, all checked before the first claim** (a refusal exits with a message that names the rule; nothing is claimed):

1. `--stub-vision` is valid only with `--caps perception`. With `vlm` or `fallback` it is refused: a VLM job is the real VLM's or the hosted model's, and a canned `VlmResult` would put invented findings into a diagnosis.
2. In a release build it is refused unless `PDM_ALLOW_STUB_VISION=1`. The flag alone isn't enough, so a flag left in a compose file by mistake can't start it, and the rig's `infra/rig.compose.yml` never sets the variable.
3. Both `STUB_*_VERSION` variables must be set; an empty `STUB_CLASSIFIER_LABELS_VERSION` is a value, and is the one an empty knowledge base gives (✓ ran on a laptop, 2026-10-17). The API queues a `perception` job only when a live perception or fallback worker reports the job's label versions in its heartbeat; with live workers of which none has both, submit fails with `503 upstream_unavailable` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)). The stub can't read a manifest (section 15.1.4), so the operator names the versions.
4. `MODEL_MANIFEST`, `ORT_*` and the `*_MAX_BATCH` variables are ignored. The manifest isn't loaded, no ONNX Runtime is initialised, and no GPU or model file is needed.

```text
// Proposed (not compiled): bins/worker/src/config.rs
#[derive(Debug, Args)]
pub struct StubArgs {
    #[arg(long, env = "WORKER_STUB_VISION")]
    pub stub_vision: bool,
    #[arg(long, env = "PDM_ALLOW_STUB_VISION")]
    pub allow_stub_vision: bool,
    #[arg(long, env = "STUB_LABEL_SET_VERSION")]
    pub label_set_version: Option<String>,
    #[arg(long, env = "STUB_CLASSIFIER_LABELS_VERSION")]
    pub classifier_labels_version: Option<String>,
    #[arg(long, env = "STUB_SPECIES", value_delimiter = ',', default_value = "tomato")]
    pub species: Vec<String>,
    #[arg(long, env = "STUB_PROBLEMS", value_delimiter = ',')]
    pub problems: Vec<String>,
    #[arg(long, env = "STUB_PLANT_PRESENT", default_value_t = true)]
    pub plant_present: bool,
    #[arg(long, env = "STUB_STAGE_DELAY_MS", default_value_t = 150)]
    pub stage_delay_ms: u64,
}

/// A pure function, so a test can pass `release` instead of changing its own build.
pub fn check_stub_allowed(cli: &Cli, release: bool) -> Result<(), StubRefused> { /* rules 1 to 3 */ }
```

`build_model` calls `check_stub_allowed(&cli, !cfg!(debug_assertions))` first and returns `StubModel::new(StubConfig::from(&cli.stub))` when the flag is on. It logs one `warn!` line, `stub vision is on: results are canned`, and the worker's tracing span carries `stub_vision = true` for the life of the process.

#### 15.1.2 What a stub result contains

`StubModel::perceive` reads only the payload: image ids, kinds and sizes, the stage list, `species_id`, `skip_species_identification` and `regions`. It never looks at pixels. It runs the stages in the payload's `stages`, in the contract's order, and for each one sends `stage_started`, a `partial_result` and `stage_completed`, with a pause of `STUB_STAGE_DELAY_MS` before the last of the three. `complete` then repeats every stage's output, so the API finds nothing to change ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)). Every number goes through `Confidence::new` and `BBox::new`, which reject anything the API's decoding would.

| Field | Stub value |
|---|---|
| `plant_present` | Set only when `plant_check` is in `stages`: `STUB_PLANT_PRESENT` (default `true`) |
| `regions` | Only with `segmentation`. Per image, numbered from 1 as `<image_id>.<n>`: `whole_plant` gives one `plant` region, box `x 0.10, y 0.10, w 0.80, h 0.80`; `leaf_closeup` and `leaf_underside` give one `leaf` region, box `0.15, 0.15, 0.70, 0.70`; `soil_surface` gives one `soil` region, box `0.00, 0.50, 1.00, 0.50`; `flower_fruit` gives none. Confidence 0.90. **No `mask`**: masks are optional, and the API measures only what needs no mask. A question photo's job (`question_id` set) gets the same rule for its one image |
| `species` | Only with `species`, and only when the payload has no `species_id` and doesn't set `skip_species_identification`. Up to three candidates from `STUB_SPECIES`, in order, with scores 0.62, 0.21 and 0.09 (the top one is a medium confidence, so a plant with only this stage counts as readable, [contracts 3](../architecture/contracts.md#3-scan-rest-api)). `species_id` is the slug, `label` is `"<slug> (stub)"`. A slug the knowledge base doesn't know is the operator's mistake: the API keeps `knowledge_ref` and the candidate stays unmatched |
| `disease` | Only with `disease`. For each leaf region of this job (or of the payload's `regions` when segmentation isn't re-run) on a `leaf_closeup` or `leaf_underside` image, one entry with its `region_id`. `top` is `healthy` at 0.70, then the first two `STUB_PROBLEMS` at 0.20 and 0.10, in that order, so it is sorted and has at most three entries. The working species is the payload's `species_id` or the first candidate; the stub doesn't restrict `top` to its labels, because it has no label file |
| `disease_skipped` | `no_leaves_found` when `disease` is requested and there is no leaf region and no close-up. Never `species_not_supported` |
| `image_embeddings` | **Absent**, always. The field is optional. A made-up vector stored with an embedding model name would corrupt the similar-case search the day a real one is stored beside it. The `image_embedding` stage is reported complete with `skipped: { reason: "stub" }` |

`analyse`, `verify` and `read_guided_check` return `ModelError::Unsupported`, so a `vlm`, `verify` or `guided` job that reaches the stub (it can't: the claim filters by `caps`) would complete `failed`, `model_error`, not retryable.

**What the API checks, and why a stub result passes.** The API decodes `result` as the job's recorded kind (`JobResult::decode`), and a result that doesn't decode is stored as a failure with `model_error` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)). Decoding rejects a confidence outside 0 to 1 and a box outside the image. Nothing else is checked: `perception_writes` copies regions, candidates and `top` entries into observations, and no code reads `image_embeddings` yet. The one check that depends on the worker is the label-version gate in rule 3 above. The stub passes it only because the operator set the versions.

#### 15.1.3 How a stub result is recognised

A stub result must never be taken for a real one, in a log or in stored data.

| Where | Mark |
|---|---|
| `complete.model_versions` and the heartbeat's `versions` | `stub: "1"`, `perception_config: "stub"`, and `stub` as the value of `leaf_detector`, `bioclip` and `disease_head`. `worker_id` is added by `JobRunner` as for every worker. `label_set_version` and `classifier_labels_version` are the job's own, as for a real worker. No `ort_ep` |
| Stored observations | `model_version` is `stub`: the API takes it from `vlm_model`, then from `perception_config` (`complete.model_versions`) |
| Candidate labels | `"<slug> (stub)"` |
| `WORKER_ID` | By convention `dev-stub-perception-1`; the compose file for S1 uses it |
| Logs | The `warn!` at start-up, and `stub_vision = true` on the worker's span, so every line has it |
| Stage event | The `image_embedding` skip reason `stub` |

The API can't tell a stub from a real worker by anything but `model_versions`. Whether it should refuse them in production is open (section 17).

#### 15.1.4 What the stub never does

- **No network.** `StubModel` holds no `reqwest` client, no `integrations` client and no `ort` session. The worker loop's own calls to the API (heartbeat, claim, events, image fetch, `complete`) are unchanged, and that includes the image fetch: it exercises the API's worker image route, which S1 wants. The stub ignores the bytes and never decodes them.
- **No files.** It reads no manifest, no label file and no model file, and writes nothing. Its only inputs are the payload and the environment.
- **No randomness and no clock.** Equal payloads give equal results, so a test can compare them and the smoke script can assert on them. The only wait is `tokio::time::sleep`, which a cancellation drops like any other `.await`.
- **No other kind.** It answers `perception` and nothing else (section 15.1.1, rule 1).

#### 15.1.5 Tests

| Test | Shows |
|---|---|
| `stub_is_refused_in_release_without_the_allow_variable` | `check_stub_allowed(cli, true)` fails; with `allow_stub_vision` it passes; with `release = false` it passes without it |
| `stub_is_refused_with_vlm_and_fallback_caps` | Rule 1, for both caps |
| `stub_needs_both_label_versions` | A missing `STUB_LABEL_SET_VERSION` or `STUB_CLASSIFIER_LABELS_VERSION` is refused, naming the variable |
| `stub_regions_follow_the_image_kinds` | A full scan's five photo kinds give the plant, leaf, leaf, soil regions and none for `flower_fruit`, ids `<image>.1`, no mask |
| `stub_result_decodes_as_the_jobs_kind` | The serialised result passes `JobResult::decode(JobKind::Perception, …)` for every subset of `stages`, and every box and confidence is in range |
| `stub_follows_the_payloads_species_choice` | With `species_id` or `skip_species_identification` there is no `species`; disease uses the given species; without either, three candidates in descending score |
| `stub_disease_is_sorted_and_skips_without_leaves` | `top` has at most three entries in descending order; no leaf regions and no close-up give `disease_skipped: no_leaves_found` |
| `stub_never_sends_embeddings` | `image_embeddings` is `None`, and the `image_embedding` stage is skipped with reason `stub` |
| `stub_marks_every_version` | `model_versions` has `stub: "1"`, `perception_config: "stub"` and the three model keys as `stub` |
| `stub_module_has_no_io` | The module's source (`include_str!`) contains none of `reqwest`, `std::fs`, `tokio::fs`, `std::net`, `ort`, `rand` or `SystemTime` |
| `a_stub_job_completes_through_the_loop` | The existing loop test with a fake `WorkerApi` and a `StubModel`: `done`, `stub` in `model_versions`, the stage events in order |
| `a_cancelled_stub_job_stops_at_its_delay` | With `STUB_STAGE_DELAY_MS = 10_000` on paused time, a cancel after 3 s ends the job and sends nothing |

One more test belongs to `pipeline`, since the stub's output is its input: `a_stub_perception_result_stores_its_observations`. A fixture of the stub's full result for a five-photo scan goes through `job_completed`, and the stored observations carry `model_version: "stub"` and the `Retrieve` claim.

**The S1 exit check these enable.** With a stub worker on the control plane, the smoke script ([roadmap 5](roadmap.md#5-s1-walking-skeleton)) creates a garden, uploads photos, submits and follows the stream through the plant check, segmentation, species, disease and `image_embedding` stages. It then drops the connection and resumes with `Last-Event-ID`. With `STUB_STAGE_DELAY_MS=10000`, killing the worker mid-job requeues the job once its lease runs out, and a second stub worker finishes it. Restarting `api` during the scan and resuming replays each missed event once. These are the first two exit checks and the third.

#### 15.1.6 Where the stub stops

The scan goes on to the `retrieve` step and queues a `vlm` job. On the control plane, the fallback worker takes that job 15 s after `queued_at`, with a real `LLM_API_KEY` and a real cost. Without a key the scan waits at `analysing` with the `vlm` job queued. S1's exit checks hold either way, because they end at the perception stages and the requeue. A canned VLM result is not offered (rule 1); open question 7 asks whether the smoke test needs one.

#### 15.1.7 Work items

All are part of S1's work item 1 ([roadmap 5](roadmap.md#5-s1-walking-skeleton)). Each item's exit check is a test or a command.

| # | Work | Exit check |
|---|---|---|
| ST1 ✓ done | `StubArgs`, `check_stub_allowed`, the `build_model` branch, the start-up `warn!` and span field | The three refusal tests pass; `worker --caps perception --stub-vision` with the allow variable starts in a release build on a machine with no GPU and no `/models` |
| ST2 ✓ done | `StubModel` and its result builder | `stub_regions_follow_the_image_kinds`, `stub_result_decodes_as_the_jobs_kind`, `stub_follows_the_payloads_species_choice`, `stub_disease_is_sorted_and_skips_without_leaves` and `stub_never_sends_embeddings` pass; `cargo clippy -p worker --all-targets` is clean |
| ST3 ✓ done | The marks of section 15.1.3 and `stub_module_has_no_io` | `stub_marks_every_version` and `stub_module_has_no_io` pass; the heartbeat of a started stub lists both label versions |
| ST4 ✓ done | The stage events and the delay | `a_stub_job_completes_through_the_loop` and `a_cancelled_stub_job_stops_at_its_delay` pass on paused time |
| ST5 ✓ done | `pipeline`'s fixture test, and a check that the stub's output decodes against `contracts.md` | `a_stub_perception_result_stores_its_observations` passes |
| ST6 | A `perception-stub-worker` service in S1's `control-plane.compose.yml` (`--caps perception --stub-vision`, `PDM_ALLOW_STUB_VISION=1`, the `STUB_*` variables, its own `env_file` with `WORKER_TOKEN`) | The worker's first heartbeat shows on the API; a submitted scan's perception job is claimed within a second and the stage events reach the stream |
| ST7 | The kill, restart and resume checks of section 15.1.5 in the S1 smoke script | S1's first three exit checks pass against the deployed control plane |

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
2. **The fallback's runtime.** Opus 5.5 always thinks, so every hosted call gets 8,000 output tokens ([contracts 6](../architecture/contracts.md#after-decoding)). Settled on 2026-10-23 ([contracts open question 26](../architecture/contracts.md#12-open-questions), [report](../../ml/training/eval/reports/p9-fallback-2026-10-23.md)): at effort low a call used at most 469 output tokens and 9.6 s, and a `vlm` job at most 15.5 s against the fallback's 585 s.
3. **`anyOf` with `null`** for nullable values in the schema. Settled on 2026-10-16 ([contracts open question 31](../architecture/contracts.md#12-open-questions), [vision vlm 4](../vision/vlm.md#4-serving)): xgrammar in vLLM v0.31.0 produces both branches and blocks values outside the schema. The non-null soil branch needs the prompt to show a reading's shape.
4. **Cancellation on llama.cpp**: whether a dropped connection frees the slot is *unverified* ([contracts open question 33](../architecture/contracts.md#12-open-questions)).
5. **Should `api` refuse stub results in production?** Today only `model_versions.stub` tells them apart. A setting such as `API_REJECT_STUB_VISION=true` on the deployed control plane, once S1 is over, would fail a `complete` that carries the key. The alternative is to rely on the release guard and the compose files alone (section 15.1.3).
6. **Is the `stub` key a contracts change?** The `model_versions` table in [contracts 5](../architecture/contracts.md#post-internaljobsidcomplete) lists keys by model entry. Adding `stub` and the value `stub` for `perception_config` needs a row there. The coordinator applies it.
7. **Does S1 need a canned `vlm` result?** Section 11.6 stops at the `vlm` job, and the fallback worker would answer it for a real LLM key. If the smoke test must reach `planned` without a key, a stub for `vlm`, `verify` and `guided` is a second flag with the same guard. It would need findings that name only knowledge-base ids, and a plan the scoring step accepts. It stays out until S1 shows the need.
8. **Where the label versions come from.** The stub gets them from two variables because the heartbeat needs them before any job, and it reads no file. Reading only the two version strings from the manifest's header would be a smaller burden for the operator, but it is a file read. They stay variables unless the S1 compose file shows them going stale after a knowledge reseed.
9. **Does the skeleton build need ONNX Runtime at all?** If the S1 worker image is built without the `cuda` feature and with `load-dynamic`, the stub starts with no `libonnxruntime`. This is *unverified*: ST1's exit check on a machine with no GPU settles it.
10. **Species and `image_embedding` events.** Section 11.2 sends no events for `species` when the payload names the species, and a `skipped: stub` for `image_embedding`. Whether `PerceptionPipeline` does the same is not compared yet; ST4 compares the event order of both.

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
