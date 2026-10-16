---
name: rust-workers
description: "The PDM worker binary (--caps perception | vlm | fallback): heartbeat, claim long poll, slots, X-Lease-Token, lease_lost, complete retries, max_runtime, SIGTERM drain, reqwest 0.13 timeouts, ort 2.0.0-rc.14 CUDA sessions, bounded CPU work. Use for any code in bins/worker."
---

# The `worker` binary

How to write and review the `worker` binary: one binary started with `--caps perception`, `--caps vlm` or `--caps fallback` ([architecture 3, Rust crates](../../../docs/architecture/README.md#rust-crates)), that pulls jobs from the API, runs them, and reports back. Versions: Rust 1.99 (edition 2024), tokio 1.53.3, tokio-util 0.7.20, reqwest 0.13.5, ort 2.0.0-rc.14, ndarray 0.17.2 ([versions.md](../rust/versions.md)).

**The project documents win**, and [contracts 5](../../../docs/architecture/contracts.md#5-worker-api-and-jobs) wins over everything else: [leases, runs and attempts](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [heartbeat](../../../docs/architecture/contracts.md#post-internalworkersheartbeat), [claim](../../../docs/architecture/contracts.md#post-internaljobsclaim), [payloads](../../../docs/architecture/contracts.md#payloads), [events](../../../docs/architecture/contracts.md#post-internaljobsidevents), [complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete) and [results](../../../docs/architecture/contracts.md#results). Also: [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning) and [authentication headers](../../../docs/architecture/contracts.md#authentication-headers), [contracts 2](../../../docs/architecture/contracts.md#2-errors) (the `409 lease_lost` body), [contracts 6](../../../docs/architecture/contracts.md#6-vlm-structured-output) (what goes to the VLM), [architecture 8.4](../../../docs/architecture/README.md#84-job-leasing-and-fallback), [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement) and [storage 3](../../../docs/storage/README.md#3-how-images-are-served). When this skill or a crate disagrees with them, follow the documents and report the disagreement.

| File | What it holds |
|---|---|
| [loop.md](loop.md) | The whole loop as one compiled and tested block: heartbeat, claim, slots, the job task, cancellation, `lease_lost`, `complete` retries, `max_runtime`, backoff, the SIGTERM drain, and thirteen tests on paused time |
| [reqwest.md](reqwest.md) | reqwest 0.13.5: the client, its timeouts, the worker API calls, reading error bodies (`lease_lost`, `retryable`, `retry_after_s`), image fetch, the VLM sidecar call; tests against a local axum server |
| [ort.md](ort.md) | ort 2.0.0-rc.14: environment, CUDA registration and the rig's driver, session and thread settings, the inference thread, terminating a run on a cancel or a drop (tested), batches up to `PERCEPTION_MAX_BATCH` |

## 1. Shape of the process

Created **once at start**, never per job:

| Thing | Shared how | Source |
|---|---|---|
| One `reqwest::Client` | Cloned into tasks; the clones share one connection pool | [Client](https://docs.rs/reqwest/0.13.5/reqwest/struct.Client.html) |
| A `Semaphore` with `slots` permits | The claim loop takes a permit before each claim; the job task holds it | [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts) |
| A `CpuPool` for CPU work | Cloned into tasks; a `Semaphore` in front of every `spawn_blocking` (section 3) | [rust-async, blocking 3](../rust-async/blocking.md#3-bounded-cpu-work-in-api), [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) |
| The ort `Environment` and every `Session` (perception only) | Owned by the inference thread ([ort.md](ort.md)) | [architecture 10](../../../docs/architecture/README.md#10-models-and-gpu-placement) |
| The running-jobs registry | Read by the heartbeat, written by job tasks | [loop.md](loop.md) |

Tasks: the **heartbeat task**, the **claim loop**, and **one task per job** spawned on a `TaskTracker` ([TaskTracker](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)). Tokens: `shutdown` (the process token of [rust-async](../rust-async/shutdown.md#6-worker)) stops the claim loop; `stop_heartbeat` is separate and cancelled only after the drain, because draining jobs still need their leases extended ([contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)); `abandon` is the parent of every job's token and is cancelled when the drain runs out of time. Token semantics: [rust-async, cancellation](../rust-async/cancellation.md#3-cancellationtoken).

## 2. The loop

Each rule is implemented and tested in [loop.md](loop.md).

1. **Heartbeat every 10 s on its own task**, listing every held job with its lease token in `current_jobs`. A job left out stops being extended and is requeued when its 2-minute lease expires. The registry entry is removed by a drop guard when the job task ends, however it ends. [contracts 5, heartbeat](../../../docs/architecture/contracts.md#post-internalworkersheartbeat), [leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)
2. **`MissedTickBehavior::Delay` on the heartbeat `interval`.** After a heartbeat call that outlasted the 10 s period, `Delay` sends one catch-up heartbeat and then spaces them 10 s from it. The default, `Burst`, fires every missed tick back to back, each a copy of the last. Give each heartbeat call a 5 s timeout, far below the 30 s after which the broker requeues all of the worker's jobs; on failure, log and keep ticking. [MissedTickBehavior](https://docs.rs/tokio/1.53.3/tokio/time/enum.MissedTickBehavior.html), [interval](https://docs.rs/tokio/1.53.3/tokio/time/fn.interval.html), [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts)

```rust
use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior, interval};

/// Ticks seen in the 4 s after a heartbeat call that took 25 s.
async fn ticks_after_a_slow_call(behavior: MissedTickBehavior) -> usize {
    let mut every = interval(Duration::from_secs(10));
    every.set_missed_tick_behavior(behavior);
    every.tick().await; // the first tick is immediate
    tokio::time::sleep(Duration::from_secs(25)).await; // the slow heartbeat
    let until = Instant::now() + Duration::from_secs(4);
    let mut ticks = 0;
    while tokio::time::timeout_at(until, every.tick()).await.is_ok() {
        ticks += 1;
    }
    ticks
}

#[tokio::test(start_paused = true)]
async fn delay_sends_one_catch_up_heartbeat_burst_sends_several() {
    assert_eq!(ticks_after_a_slow_call(MissedTickBehavior::Delay).await, 1);
    assert_eq!(ticks_after_a_slow_call(MissedTickBehavior::Burst).await, 2);
}
```

3. **`cancel_job_ids` cancels that job's `CancellationToken`, and the job stops and sends nothing more**: any later call about the job gets `409 lease_lost`. `Runner::run` gets the token and stops on it; the perception runner terminates its ONNX run (`RunOptions::terminate`) and waits for the inference thread, so the slot is freed only once the thread is. A runner that hasn't stopped `CANCEL_GRACE` (5 s, *this skill's choice*) after the cancel is dropped, and a drop guard in the inference wait terminates the run then too ([ort.md, the inference thread](ort.md#4-the-inference-thread)). [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [child_token](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html#method.child_token), [RunOptions::terminate](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate)
4. **Take a slot before claiming.** `slots.clone().acquire_owned()` first, then the claim; the `OwnedSemaphorePermit` moves into the job task and is dropped when it ends. The worker then never holds more than `slots` jobs and never polls while full; the broker enforces the same limit with `204`. Send the same `slots` in the claim and the heartbeat. [acquire_owned](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned), [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)
5. **Claim is a long poll**: `wait_s: 25` (the maximum is 30), with a request timeout longer than the poll (`wait_s + 10 s`). A `204` after the server waited means nothing arrived: claim again at once. A `204` that comes back in under half of `wait_s` didn't wait (the API shutting down, or counting this worker as full), so it backs off like an error (rule 12) and the loop can't spin on it (*this skill's choice*; the test `an_early_204_backs_off_instead_of_spinning` in [loop.md](loop.md) shows 6 to 8 claims in 60 s). [contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim), [reqwest.md](reqwest.md#2-timeouts)
6. **Every call about a job sends `X-Lease-Token`** with the job's `lease_token`, next to `Authorization: Bearer <worker token>`. [contracts 1, authentication headers](../../../docs/architecture/contracts.md#authentication-headers)
7. **`409 lease_lost` from any call drops the job silently**: no `complete`, no more events, the slot is freed. Only the body's `code` says `lease_lost`, so read it before treating the status as an error ([reqwest.md, reading errors](reqwest.md#4-reading-errors)). [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [contracts 2](../../../docs/architecture/contracts.md#2-errors)
8. **One `partial_result` per finished stage**, sent after the stage's last batch. A failed `events` call for any other reason is logged and the job goes on (`send_events` in [loop.md](loop.md)): `complete` repeats every stage, and the API keeps the `complete` version. [contracts 5, events](../../../docs/architecture/contracts.md#post-internaljobsidevents), [results](../../../docs/architecture/contracts.md#results)
9. **`complete` carries `status` `done` with `result`, or `failed` with `error: { code, message, retryable }`, and always `model_versions` and `timings_ms`.** After a timeout, a connection error, or an error body with `retryable: true` (or a `429` or `5xx` without one), send the same body again with backoff, waiting at least the body's `retry_after_s` when it has one: a repeat for a job that already completed returns `204` and changes nothing. An error body with `retryable: false` is not retried in a loop ([reqwest.md, reading errors](reqwest.md#4-reading-errors), [contracts 2](../../../docs/architecture/contracts.md#2-errors)). Stop retrying once `max_runtime_s` has passed; the lease can't outlive it. [contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete), [leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)
10. **`v != 1` fails the job with `unsupported_version`**, `retryable: false`, without running it. Decode `v`, `job_id` and `lease_token` before the payload, so a job of another version can still be failed. Decode the claim body with `decode_claim` ([rust-contract-types, the worker checks v first](../rust-contract-types/unions.md#3-the-worker-checks-v-first), region `JobHead`): only a decoded `v != 1` is `unsupported_version`. A body whose head (`v`, `job_id`, `lease_token`) doesn't decode can't be completed at all, so it is not `unsupported_version`: log it and let the lease expire. A `kind` none of the worker's capabilities claims fails like `v != 1`, before the payload is read. [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning), [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts) (which kinds each capability claims), [rust-contract-types, the worker checks v first](../rust-contract-types/unions.md#3-the-worker-checks-v-first)

    **Not in the contracts yet:** `retryable: false` for `unsupported_version` ([contracts 12, question 19](../../../docs/architecture/contracts.md#12-open-questions)), and an unknown `kind` failed as a bug with the placeholder code `unsupported_kind`, `retryable: false` ([contracts 12, questions 18 and 21](../../../docs/architecture/contracts.md#12-open-questions)).
11. **Stop a job before its `max_runtime_s`.** Take the deadline from the job's own `max_runtime_s`, not from a table in the worker (the limits are doubled on the fallback worker), measured from receipt on the monotonic clock, minus 15 s for `complete`. Past it, the runner's future is dropped (its drop guard terminates an ONNX run) and the job fails with `retryable: true`. Don't compare `lease_until` with the local clock: it is the server's time. The broker enforces `max_runtime` regardless, so a hung model loses the job either way. [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [timeout_at](https://docs.rs/tokio/1.53.3/tokio/time/fn.timeout_at.html), [tokio Instant](https://docs.rs/tokio/1.53.3/tokio/time/struct.Instant.html)

    **Not in the contracts yet:** the code `max_runtime_exceeded` for a job the worker stopped itself ([contracts 12, question 18](../../../docs/architecture/contracts.md#12-open-questions)).
12. **Back off when the API is unreachable**: 1 s doubling to a 30 s cap, each delay drawn from `[d/2, d]` so it is never near zero; reset after any answer that isn't an early `204`. When the API sent `retry_after_s`, wait at least that long. Never retry in a tight loop. *Design choice for this repo*; the tests in [loop.md](loop.md) show at most 8 claims in 60 s, and a retry 7 s after a `retry_after_s: 7`. [contracts 2](../../../docs/architecture/contracts.md#2-errors), [rand::random_range](https://docs.rs/rand/0.10.3/rand/fn.random_range.html)
13. **SIGTERM or Ctrl-C: stop claiming, drain, then stop heartbeating.** The claim loop `select!`s on `shutdown` with `biased;` and drops an open long poll. Running jobs get `DRAIN` (20 s) to finish. Jobs still running then are abandoned: their tokens are cancelled and nothing is sent, so the broker requeues them 30 s after the last heartbeat; a job's first two requeues don't count as attempts. A runner gets `CANCEL_GRACE` (5 s) to stop, so the worker exits at most 25 s after the signal. Docker Compose sends SIGKILL `stop_grace_period` after SIGTERM, 10 s by default, so every worker service in the rig's and the control plane's compose files ([architecture 12](../../../docs/architecture/README.md#12-deployment)) sets `stop_grace_period: 30s`; `infra/` doesn't exist yet. *Design choice for this repo*, the same budgets as [rust-async, shutdown 1](../rust-async/shutdown.md#1-the-sequence). [Compose file: stop_grace_period](https://docs.docker.com/reference/compose-file/services/#stop_grace_period), [rust-async, shutdown](../rust-async/shutdown.md#6-worker), [contracts 5](../../../docs/architecture/contracts.md#leases-runs-and-attempts), [ctrl_c](https://docs.rs/tokio/1.53.3/tokio/signal/fn.ctrl_c.html)
14. **A blocking call outlives its task.** Dropping the future of a `spawn_blocking` task doesn't stop the closure, and dropping the runtime waits for every started blocking task. Use `RunOptions::terminate` for ONNX Runtime ([ort.md](ort.md)) and keep other blocking work short. [spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html), [Runtime::shutdown_timeout](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Runtime.html#method.shutdown_timeout)

## 3. Slots, pools and backpressure

| `--caps` | `slots` | Where the heavy work runs | What bounds it |
|---|---|---|---|
| `perception` (RTX 3070) | 1. Batches inside the job | ONNX Runtime with CUDA, on the inference thread | One job; each model runs once per job, in batches of at most `PERCEPTION_MAX_BATCH` (default 16) |
| `vlm` (RTX 5070 Ti) | `VLM_SLOTS`, 1 or 2 (vLLM's `max_num_seqs` is the same value) | The vLLM or llama.cpp sidecar, over HTTP | The slots, and a per-call timeout under `max_runtime` |
| `fallback` (control plane) | Several, default 4 | The LLM API, over HTTPS | The slots, set from the LLM API's rate limit |

Sources: [contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts) (the `slots` table), [results](../../../docs/architecture/contracts.md#results) (batching), [contracts 12](../../../docs/architecture/contracts.md#12-open-questions) (open questions 1 and 9 fix `VLM_SLOTS` and the batch size on day 2).

- **No unbounded queue anywhere.** Work enters through a slot permit, a CPU permit, or a bounded channel (`mpsc::channel(1)` to the inference thread). Never `unbounded_channel` or a `spawn` per image without a permit. [mpsc::channel](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/fn.channel.html)
- **CPU work (decode, resize to `VLM_MAX_PIXELS`) goes through rust-async's `CpuPool`**: a semaphore in front of `spawn_blocking`, with its tests ([rust-async, blocking 3](../rust-async/blocking.md#3-bounded-cpu-work-in-api)). tokio's blocking pool grows up to a very large limit and then queues; its documentation says to bound CPU-bound work with a semaphore ([spawn_blocking](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html)). Size the pool below the core count so the runtime and ONNX Runtime's pool keep some (*design choice*). On the perception worker, decoding on the inference thread instead is also bounded ([rust-async, blocking 4](../rust-async/blocking.md#4-the-perception-worker-and-onnx-runtimes-threads)). [contracts 6, image preparation](../../../docs/architecture/contracts.md#image-preparation)

## 4. reqwest 0.13.5 in brief

Details, sources and tests: [reqwest.md](reqwest.md).

- One `Client` per process, built with `Client::builder()` so a TLS set-up failure is an error, not a panic. [reqwest.md, the client](reqwest.md#1-the-client)
- 0.13 changed the TLS default to rustls with the platform verifier. On Linux, `build()` fails when the image has no CA certificates (read in the source, not run), so every worker image installs them. [reqwest.md, the client](reqwest.md#1-the-client)
- `timeout` is the whole request, `connect_timeout` only the connect, and `read_timeout` each read, **including the wait for the response headers**. Don't set `read_timeout` on the shared client: it would kill the claim long poll and every non-streamed VLM call. Set a per-request `timeout` instead; it overrides the client's. [reqwest.md, timeouts](reqwest.md#2-timeouts)
- Read the error body of every non-2xx answer before anything else: `error_for_status` would consume it, and only the body has `lease_lost`, `retryable` and `retry_after_s`. A body that couldn't be read is a network error, not a bad body, although reqwest calls both `is_decode()`. [reqwest.md, reading errors](reqwest.md#4-reading-errors)

## 5. ort 2.0.0-rc.14 in brief

Details, sources and the compiled example: [ort.md](ort.md).

- `ort::init()...build()` once per process (a second `build()` returns an error), then `Session::builder(&env)` for each model at start. [ort.md, environment](ort.md#1-environment-and-execution-providers)
- A CUDA execution provider that fails to register is logged and skipped, and the session runs on the CPU. Add `.error_on_failure()` so the perception worker refuses to start instead. [ort.md, environment](ort.md#1-environment-and-execution-providers)
- The prebuilt binaries need CUDA 13.2, so the rig's one driver must support it for both cards; `copy-dylibs` leaves symlinks, not copies. [ort.md, CUDA on the rig](ort.md#2-cuda-on-the-rig)
- `Session::run` takes `&mut self` in rc.14. The inference thread owns the sessions, so no `Mutex` is needed. One global intra-op pool for all sessions, with spinning off. Never run inference on a runtime worker thread. [ort.md, sessions and threads](ort.md#3-sessions-and-threads)
- `RunOptions::terminate` stops a run from another thread. The inference wait calls it when the job's token fires and, through a drop guard, when the wait is dropped. [ort.md, the inference thread](ort.md#4-the-inference-thread)

## 6. Images and the VLM

- **Fetch images from each `JobImage`'s `url`** (`GET /internal/jobs/{job_id}/images/{image_id}`), with the bearer token and `X-Lease-Token`, and only from the API's own origin ([reqwest.md, reading errors](reqwest.md#4-reading-errors)). Only the images in the payload exist for the worker: payloads never list depth maps, and no route serves one. A `409 lease_lost` here drops the job like any other call. [storage 3](../../../docs/storage/README.md#3-how-images-are-served), [contracts 5, payloads](../../../docs/architecture/contracts.md#payloads), [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)
- **Every VLM call gets a timeout shorter than what is left of the job**, keeping time for `complete`; don't start a call that can't finish ([reqwest.md](reqwest.md#5-the-vlm-sidecar)). The prompt, the per-job JSON Schema and the decoding of the answer follow [contracts 6](../../../docs/architecture/contracts.md#6-vlm-structured-output); they are not this skill's subject.

## 7. Not settled by the documents

- **Not in the contracts yet:** contracts 5 names only `unsupported_version` as a failed job's code (contracts 1). The worker also sends `max_runtime_exceeded` (`retryable: true`) for a job it stopped at its soft deadline, `unsupported_kind` (`retryable: false`) for a kind it doesn't claim, and rust-errors' `model_error` placeholder for model failures ([rust-errors, jobs](../rust-errors/jobs.md)) ([contracts 12, question 18](../../../docs/architecture/contracts.md#12-open-questions)).
- **Not in the contracts yet:** `unsupported_version` is sent with `retryable: false` ([contracts 12, question 19](../../../docs/architecture/contracts.md#12-open-questions)).
- **Not in the contracts yet:** a claim dropped at shutdown may already hold a lease on the server. Nothing lists it in a heartbeat, so it is requeued when the heartbeat goes stale (30 s after the worker exits) or its lease expires, whichever comes first; there is no call to hand a lease back ([contracts 12, question 20](../../../docs/architecture/contracts.md#12-open-questions)).
- **Not in the contracts yet:** a job whose `kind` none of the worker's capabilities claims is treated as a bug and failed without running ([contracts 12, question 21](../../../docs/architecture/contracts.md#12-open-questions)).
- **The heartbeat's `gpu` field** is optional ([contracts 5, heartbeat](../../../docs/architecture/contracts.md#post-internalworkersheartbeat)); leave it out until something measures VRAM (no crate for it is pinned).

## 8. Review checklist

- [ ] One `reqwest::Client`, one ort `Environment`, every `Session` created at start; nothing loaded per job.
- [ ] Heartbeat on its own task, every 10 s, `MissedTickBehavior::Delay`, a per-call timeout well under 30 s, and every held job in `current_jobs` with its lease token.
- [ ] SIGTERM and Ctrl-C both start the drain; the heartbeat keeps running while jobs drain; its token is not a child of `shutdown`; the worker's compose service sets `stop_grace_period: 30s` (rule 13).
- [ ] `cancel_job_ids` cancels the job's token; the runner gets the token, the job sends nothing afterwards, and a runner that ignores it is dropped after `CANCEL_GRACE` (rule 3).
- [ ] A slot permit is taken before each claim and held by the job task; `slots` is the same in claim and heartbeat.
- [ ] Claim `wait_s` ≤ 30 with a request timeout longer than it; a `204` after the full wait loops at once, an early `204` backs off; no client-wide `read_timeout` (rule 5).
- [ ] `X-Lease-Token` on events, complete and image calls.
- [ ] Every error body is read before anything else; `lease_lost` drops the job without `complete`; `retryable` and `retry_after_s` decide the retry; a bad JSON body and a body that couldn't be read are told apart (rules 7 and 9).
- [ ] `complete` has `v`, `status`, `model_versions` and `timings_ms`, and is retried with backoff (at least `retry_after_s`) after network errors and retryable error bodies, until `max_runtime_s` (rule 9).
- [ ] `v != 1` fails with `unsupported_version`, and an unclaimed `kind` with `unsupported_kind`, before any work (rule 10).
- [ ] The job stops itself before `max_runtime_s`, on the monotonic clock (rule 11).
- [ ] Every retry loop sleeps with backoff; none can spin, not even on a `204` (rules 5 and 12).
- [ ] CPU work goes through the `CpuPool`; no unbounded channel or queue (section 3).
- [ ] Inference never runs on a runtime worker thread; both a cancel and a dropped inference wait reach `RunOptions::terminate` ([ort.md](ort.md#4-the-inference-thread)).
- [ ] Images come only from each `JobImage.url` on the API's origin; nothing reads depth (section 6).

## 9. Related skills

[rust](../rust/SKILL.md) (entry, versions, example checker) · [rust-async](../rust-async/SKILL.md) (tokens, tasks, shutdown, blocking work) · [rust-contract-types](../rust-contract-types/SKILL.md) (the `Job`, `CompleteJob` and event types) · [rust-errors](../rust-errors/SKILL.md) · [rust-tracing](../rust-tracing/SKILL.md) · [rust-testing](../rust-testing/SKILL.md) (paused time, fakes) · [rust-axum-api](../rust-axum-api/SKILL.md) (the broker's side of these routes) · [rust-surrealdb](../rust-surrealdb/SKILL.md) · [rust-cargo-lints](../rust-cargo-lints/SKILL.md)
