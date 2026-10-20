# PDM Backend: `api` binary

> **Status:** Draft v0.7.15, 2026-10-20. v0.7.15: `outbound` parses Soil Landscapes of Canada once at start and gives it to `SoilGrids`, so a Canadian garden where SoilGrids has no data is `ready` after one call (a test runs the wired adapter for a Toronto garden); a GPS garden whose reverse lookup fails is created with the time zone's label instead of `503` ([integrations 8, 9](integrations.md#8-soilgrids)). v0.7.14: `ApiError::chain` gives the log form of an error handled outside a request (its code and message, then the cause's `source()` chain), so a failed follow-up or scheduled task, the weather-cell and `SoilGrids` warnings and `ApiError`'s `Debug` log the cause, not only "store failed" or the user's message; `POST /plants`, `POST /plants/{id}/species`, a new plant of `POST /scans` and of `PUT /gardens/{id}/map` answer `400 invalid_request` for a species the knowledge base doesn't hold (the store's check, contracts 9); `tests/pipeline.rs` drives the real pipeline over the fake store (`state_with_pipeline`) for `complete`'s catch-up stage events and species confirmation (section 21). v0.7.13: `farm_sync/tests_era.rs` has T-E4.5's Slack half (a `day2` Slack sync, then the 180-day cleanup) and E4.3's resync against Era's Drive simulator, and Slack's re-read starts at the newest message seen when that is older than its 7 days. v0.7.12: a farm record is embedded and hashed under a line naming its author and title (`chunk::embedded_text`, EON gap G12), and a source's stored state carries `text_version`, so a state from before it starts afresh (keeping `failing`) and every record is read and embedded again once. v0.7.11: the modules are a library target (`lib.rs`) and `main.rs` the binary, so `backend/tools/era-eval` (docs/EON E8) can run the real farm sync; `connect`, `farm_sync`, `sealing`, `config`, `router`, `scheduler` and `wiring` are public, the rest stay private, the library has no doc tests, and the signal handler's test moved into `main.rs` (section 3). v0.7.10: the farm sync reads Google Drive (roadmap E4.3): `FarmSources` holds `DriveSource` for owners' connectors and, with `ERA_GOOGLE_BASE`, for the `era` ones; the sync renews Google's access token before a run when it has under 5 minutes left and once a run after Google refuses it, storing each grant through `connect::refresh_and_store`, the compare-and-set the source picker now shares (`ConnectVendors::refresh` takes `force`); an `era` connector is never renewed (section 16); after its audit, a page that lists the whole source (`FarmPage::listed`) makes the sync delete the source's other records, `TooLarge` from an adapter stops with `too_large`, the renewal before a run gives way to shutdown, and one Drive page can overrun the 3-minute budget (section 16). v0.7.9: the account deletion finishes the user's `deleting` connectors through `deleting_connectors_of_user`, so one in a garden being deleted no longer blocks `DELETE $user`, and a connect lost to a deletion is answered as not found; `complete` runs its work on the tracker in a `connect` route group with a 60 s timeout, so a dropped request neither keeps a claim nor orphans tokens, and a panic releases the claim; a final write that fails ambiguously re-reads the row before revoking; the pending cleanup leaves claims younger than `CONNECT_CLAIM_HOLD`; without `CONNECTOR_KEY` an account cascade deletes the user's connectors unrevoked (sections 7, 11, 16, 17). v0.7.8: the connect and deletion branches share one path: `DeletionRunner::finish_connector` is the only revoke-then-delete (the owner's `DELETE /connectors/{id}` spawns it; `revoke.rs` and `connect::disconnect` are gone), `sealing::StoredTokens` the only reader of a sealed token, and `ConnectVendors` the only revoke seam; a token another live `vendor` connector of the same Slack team or Google account holds isn't revoked; each revoke runs in a task of its own; a demotion starts the task only when an owner was demoted; `complete` claims the state before the exchange and revokes tokens it can't store; a refreshed Google token is stored with a compare-and-set; the callback passes on only a short vendor error code (sections 11, 17). v0.7.7: connector deletion (roadmap E3.6): a garden's cascade, an owner's demotion, removal or leaving, and account deletion delete the connectors that owner created; the store marks them `deleting` in its transaction, and the deletion runner unseals each token, revokes it through the `TokenRevoker` seam (`revoke.rs`, Slack's `auth.revoke` and Google's `revoke`; never for an `era` connector) after that commit, then deletes the records and the row, resumed at start while the mark remains; the deletion runner opens tokens with `unseal` too (section 17). v0.7.6: farm connectors connect (roadmap E3.3–E3.5): `connect.rs` signs the OAuth state (`OAuthState`, an HMAC-SHA256 under `ConnectorKey::state_key` over the connector id, the nonce and the expiry), seals the tokens as `StoredTokens` (Slack's bot token as its plain string, Google's grant as JSON `{access_token, refresh_token, expires_at}`), and holds the `ConnectVendors` seam that `wiring/connect.rs` implements over the Slack and Drive clients on the vendors' own hosts; the seven connector routes are built (section 11), refuse an `era` connector with `409 invalid_state`, and the `cleanup` task deletes pending connectors after 10 minutes (section 16). v0.7.5: the farm sync's rounds decide turns and `synced_at`, never who is read (section 16): a round gives every chosen source a turn, a page asked whatever follows (`Rotation::turned`), the budget left then goes to the passes in progress, and the round ends with `Synced` only when every source is caught up, `RoundEnded` when one has pages left or a failed last turn (`SourceState::failing`, which keeps `last_error`), and `NoSources` for a connector without sources; older-item checks need 140 s left, and a page can still overrun the budget by several paced calls. The farm sync's audit fixes (section 16): the sources rotate per run (`state::Rotation`, stored under `SYNC_STATE_KEY`), so each run starts after the last source the previous one read, a new source joins the round in progress, and the run reports `Synced` (setting `synced_at`) only when every chosen source has finished its pass in the round; older-thread checks are two per connector run (`PROBES_PER_RUN`), none on a first pass, and a page is offered only as many as 70 s each of the budget left covers after 70 s for the page; the garden's record count is read before each page is stored; a page of a source dropped meanwhile doesn't count toward the record limit; seeding reports a garden or a connector being deleted (`Seeded::GardenDeleting`, `Seeded::ConnectorDeleting`); shutdown interrupts a page in flight (the budget is still checked between pages), a `429`'s wait is kept between 60 s and 1 h, and seeding repairs an existing demo connector (token sealed again, status and `last_error` reset) and warns when the fixed id lives in another garden. v0.7.4: start-up seeds the demo garden's two `era` connectors after resuming deletions (roadmap E4.6), by fixed id, into the garden `ERA_DEMO_GARDEN_ID` names, with their tokens sealed under `CONNECTOR_KEY` (sections 4, 5 and 16). v0.7.3: the farm sync (roadmap E4.1, E4.2, E4.4, E4.5): `farm_sync.rs` runs the `connectors` task every minute when connectors are on (`AppState::farm_sync`), at most 4 due connectors a run side by side, over the `FarmSource` adapters by kind and endpoint (`FarmSources`): pages from each source's saved cursor, chunks of at most 2,000 characters with a SHA-256 `text_hash`, only changed chunks embedded, the daily 7-day re-read that deletes what the vendor no longer has, the 10,000-record limit, and `last_error` and the next attempt by failure; `cleanup` deletes Slack messages older than 180 days (section 16). v0.7.2: farm connectors are configured (roadmap E3.0–E3.2): `CONNECTOR_KEY` (64 hex characters) is the one switch, read through `AppState::connector_key`; without it the list answers `available: false` and the owner routes `409 invalid_state`, and with it the list comes from the store. The Slack and Google OAuth clients (each pair together or not at all), `CONNECTOR_RETURN_URL` and the Era bases are read for the routes and seeding to come. `sealing.rs` seals vendor tokens with AES-256-GCM (`aes-gcm` 0.11.1), a random nonce per seal and the connector id as associated data. v0.7.1: the farm connector routes are declared (roadmap E1.4): `GET /gardens/{id}/connectors` answers `available: false`, and the owner routes and the public OAuth callback answer `409 invalid_state` until connectors are configured; `Access<R, table::Connector>` resolves a connector's garden. v0.7: shared memories are recorded in `SurrealDB` Agent Memory: the optional `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and `AGENT_MEMORY_API_KEY` (all three or none), `Ports.beliefs`, the new `GET /plants/{id}/beliefs` (never a 5xx), the `beliefs` scheduler task every 10 s (the only writer to Agent Memory: queued erases, then unsynced shared memories), deletions that queue their erase in their first transaction, and `POST /memories` taking a `topic`; 42 tests. v0.6.5: `RUSTFS_BUCKET` defaults to `pdm-images`, the bucket storage 2 creates, not `pdm`. v0.6.4: section 4 proposes that start-up step 3 applies the numbered migrations and the generated `knowledge.surql` after the schema, and that step 6 stops on an embedder that differs from the knowledge set's. v0.6.3: section 7 says `backend/openapi.json` is committed and a test fails when it falls behind the types. v0.6.2: section 21 names `gen_fake_store.py` at its place in the repository, `backend/tools/apigen/`. v0.6.1: worker tokens are kept as SHA-256 digests and compared digest to digest in constant time, so a token's length no longer shows in the comparison time. v0.6: a live scan stream re-reads the state after 15 s without an event, so a resting point reached without one and a deleted scan close it; image bodies end on the shutdown token; the claim group has the 1 MiB JSON limit (it had axum's 2 MiB default); a signal handler that fails to install no longer shuts the API down; invite accepts are held to at most 10 in any minute (a burst of 5, then one every 12 s); the replay-completeness claim is corrected; the section 1 skills table is gone, since the skills follow this design; the fake-store generator moves to `backend/tools/apigen/`. v0.5: the scan stream ends on a duplicate flagged `rests` (a resting event stored during the replay used to leave it open), the timeout's bare `503` gets `Retry-After` from `render_errors`, the global body cap is 20 MiB + 128 KiB (not 24 MiB), the `api` container's `stop_grace_period` is 190 s, the scheduler's `TASKS` listing is complete, and comments cite the settled contract sections. v0.4: the generated fake store now delegates every store method, default-bodied ones included, and four handler tests cover the v0.3 paths: late worker events dropped with `204`, a tombstoned account's token answered `401`, a second `DELETE /me` starting nothing, and the `SoilGrids` retry time by failure. v0.3: the cross-crate fixes: every `RustFS` delete goes through one `blob::DeleteQueue`, drained at shutdown for at least one full attempt; the store's `Notify` is the claim long poll's only wake-up; `complete` fails a job for good once; late `events` after `complete` are dropped with `204`; `Pipeline::sweep` owns the question continuation; `SoilGrids` retries 15 minutes after an outage and 7 days after no data; account deletion starts with a tombstone's compare-and-set that refuses the deleted account's tokens and resumes the Clerk deletion after a restart. v0.2: the trigger rules read `domain::rules::weather::WeatherThresholds` from `Config::weather`, the one instance also given to Open-Meteo and the pipeline, and one text embedder serves every consumer now that `TextEmbedder::embed` takes an `EmbedMode`. v0.1: the `api` binary designed down to every handler, extractor, layer and background task: start-up and shutdown, configuration, the layer stack, auth and membership proofs, idempotency, the error mapping, every endpoint of contracts v0.9, uploads, the resumable scan event stream, the chat stream, the job broker, the scheduler, the deletion runner and the signed media routes, compiled on Rust 1.99.1 against axum 0.8.9, tower-http 0.7.1 and utoipa 6, and wired to the real `store`, `blob`, `knowledge`, `vision`, `integrations`, `agent` and `pipeline` crates.

Companion to the [backend architecture](README.md), the [`domain` crate](domain.md) whose ports and types `api` wires, and [domain-types](domain-types.md) for every wire shape. Every listing below is copied from the compiled source of `bins/api` by the docgen tool. `cargo check -p api`, `cargo clippy -p api --all-targets` (no warning in `api`) and `cargo test -p api` pass: 42 tests, among them handler tests that drive the real router with `tower::ServiceExt::oneshot`. `cargo check --workspace` passes, the binary links, and `api openapi` writes the document.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [Start-up and shutdown](#4-start-up-and-shutdown)
5. [Configuration](#5-configuration)
6. [Application state, wiring and the seams](#6-application-state-wiring-and-the-seams)
7. [The router and the layer stack](#7-the-router-and-the-layer-stack)
8. [Errors](#8-errors)
9. [Authentication and access](#9-authentication-and-access)
10. [Idempotency](#10-idempotency)
11. [Endpoint index](#11-endpoint-index)
12. [Uploads](#12-uploads)
13. [Scan events: writer, hub and stream](#13-scan-events-writer-hub-and-stream)
14. [Chat streaming and confirmation cards](#14-chat-streaming-and-confirmation-cards)
15. [The job broker and the worker API](#15-the-job-broker-and-the-worker-api)
16. [The scheduler](#16-the-scheduler)
17. [Deletion](#17-deletion)
18. [Media](#18-media)
19. [Rate limits and budgets](#19-rate-limits-and-budgets)
20. [Concurrency, lifetimes and generics](#20-concurrency-lifetimes-and-generics)
21. [Testing](#21-testing)
22. [Decisions on the contracts' open questions](#22-decisions-on-the-contracts-open-questions)
23. [Open questions](#23-open-questions)
24. [Sources](#24-sources)

---

## 1. Purpose and boundaries

**What it owns** ([architecture 3](../architecture/README.md#3-components), [architecture 9](../architecture/README.md#9-api-surface)):

- every public endpoint of the app and the worker API, with the OpenAPI document generated from the same handlers;
- authentication (Clerk session tokens, worker tokens, the Svix webhook) and the membership check on every garden-scoped call, as the proof values of [domain 8](domain.md#8-authorization-proofs);
- the mapping of every error to the contracts 2 body, idempotency keys, rate limits;
- uploads: multipart streaming, the CPU image pipeline on blocking threads, blob puts, records;
- the per-scan event writer, the SSE hub and the resumable scan stream, and the chat stream;
- the job broker's long poll and the worker routes;
- the scheduler (weather and alerts, plan triggers, sweeps, requeues, cleanup, soil retries, orphans, push receipts) and the deletion runner;
- the HMAC signer of image URLs and the routes that stream images from `RustFS`.

**What it never does.** It holds no pipeline logic: retrieval, scoring, fusion, plan writing and measurement are `pipeline`'s, reached through its entry points; chat turns are `agent`'s. It writes no SQL (`store`), no S3 call (`blob`), no outbound HTTP (`integrations`). Only `main.rs` uses `anyhow`; only `wiring.rs` names a concrete adapter.

**Who calls it.** The app (REST and two SSE streams), the workers (the worker API), Clerk through Svix (the webhook), and the operator (`/healthz`, `/status`, `api openapi`).

**The project skills.** `.claude/skills/rust-axum-api` and its siblings follow this document. Access checks are extractors (`AuthUser`, `Access<R, T>`, `ConvAccess<R, T>`, `WorkerAuth`, `JobLease`), so the `Membership<R>` proof a repository call needs comes out of the extractor, typed by the role the handler names. A malformed path id is `404 not_found` (section 22, question 14). A request timeout is `503 upstream_unavailable` with `upstream: "api"`, retryable (question 17). The writer, the broadcast channel and the plan lock retire independently (section 13; question 22). `AppState` holds only `Arc<dyn Port>`s, so SQL stays in `store` and S3 in `blob`. Where a skill and this document disagree, this document wins.

---

## 2. Dependencies

```toml
[package]
name = "api"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

# The modules are a library so `backend/tools/era-eval` can run the farm sync (src/lib.rs); the
# binary is main.rs. Its doc comments hold shell listings, not doc tests.
[lib]
doctest = false

[features]
default = []
# HEIC uploads through the system libheif (>= 1.19); the api image builds with it.
heic = ["vision/heic"]

[dependencies]
# the workspace: domain for every type and port; the library crates only in wiring.rs
domain.workspace = true
store.workspace = true
blob.workspace = true
integrations.workspace = true
vision.workspace = true
knowledge.workspace = true
agent.workspace = true
pipeline.workspace = true
# HTTP
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
utoipa.workspace = true
utoipa-axum.workspace = true
governor.workspace = true
serde_path_to_error.workspace = true
# async
tokio.workspace = true
tokio-util.workspace = true
tokio-stream.workspace = true
async-stream.workspace = true
async-trait.workspace = true
futures.workspace = true
bytes.workspace = true
# data
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
ulid.workspace = true
uuid.workspace = true
# crypto and secrets
hmac.workspace = true
aes-gcm.workspace = true
sha2.workspace = true
secrecy.workspace = true
rand.workspace = true
data-encoding.workspace = true
aws-lc-rs.workspace = true
# errors and logs
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
domain = { workspace = true, features = ["mock", "test-util"] }
# The farm sync's tests: the embedded engine (kv-mem, tests only) and Era's harness.
surrealdb = { workspace = true, features = ["kv-mem"] }
integrations = { workspace = true, features = ["era-sim"] }
http-body-util.workspace = true
mockall.workspace = true
tokio = { workspace = true, features = ["test-util"] }
# The Drive sync's tests: Google's API mocked over HTTP.
wiremock.workspace = true
```

The `heic` feature turns on `vision`'s HEIC decoder, which links the system libheif (1.19 or newer). The `api` image builds with it ([infra 3](../infra/README.md#3-images)); without it, a HEIC upload is refused as an unsupported type.

| Dependency | Why |
|---|---|
| `domain` | Every type and port |
| `store`, `blob`, `knowledge`, `vision`, `integrations`, `agent`, `pipeline` | Named in `wiring.rs` only, plus the re-exported seam types in `seams.rs` (section 6) |
| `axum` (`macros`, `multipart`) | Routing, extractors, SSE, multipart |
| `tower`, `tower-http` | `ServiceBuilder` and the layers: request id, trace, CORS, compression, catch-panic, timeout, body limit |
| `utoipa`, `utoipa-axum` | `#[utoipa::path]`, `OpenApiRouter`, `routes!` |
| `governor` | Keyed rate limiters |
| `serde_path_to_error` | The JSON path of a field that failed to decode, for `details.field` (axum already depends on it) |
| `tokio`, `tokio-util`, `tokio-stream`, `async-stream`, `futures` | Runtime, `CancellationToken`, `TaskTracker`, `stream!`, `StreamExt` |
| `hmac`, `sha2` | Signed URLs; idempotency fingerprints |
| `aes-gcm` (`zeroize`) | Sealing the farm connectors' vendor tokens (section 5, sealing) |
| `secrecy` | `SecretString` in the configuration |
| `rand`, `data-encoding` | Invite codes: 128 bits from `rand::rng()`, base32 |
| `aws-lc-rs` | Constant-time comparison of worker tokens |
| `anyhow`, `tracing-subscriber` | `main.rs` only: start-up errors, JSON logs |
| `http-body-util`, `mockall` (dev) | Reading bodies in handler tests; the domain mocks |

---

## 3. Module layout

```
bins/api/src/
├─ lib.rs            the modules, as a library: main.rs and backend/tools/era-eval use it; only
│                    connect, farm_sync, sealing, config, router, scheduler and wiring are public
├─ main.rs           start-up order, serve, signals, drain; `api openapi`
├─ config.rs         Config from the environment, SecretString
├─ wiring.rs         the only module naming concrete adapters; builds Ports and AppState
├─ state.rs          AppState, Ports, signer(), member(), spawn_follow_up()
├─ seams.rs          the pipeline, agent and vision APIs as traits (ScanWriter, PipelineApi, ChatTurns, ImageProcessing)
├─ router.rs         OpenApiRouter groups, the global layer stack, /openapi.json
├─ openapi.rs        the document's frame and security schemes
├─ error.rs          ApiError, ErrorParts, render_errors, from_status, panic_response, From impls
├─ reply.rs          Wire<T>: every JSON success response; replays
├─ extract.rs        ApiJson, ApiQuery, ApiPath, ApiMultipart, path_param, RequestIdHeader
│  ├─ auth.rs        AuthUser, WorkerAuth, JobLease
│  ├─ access.rs      Access<R, T>, ConvAccess<R, T>, Scoped, ConvScoped
│  └─ idempotency.rs Begin, Begun, begin(), IdemJson<T>, canonical_json_hash
├─ limits.rs         governor limiters
├─ workers.rs        worker tokens
├─ signing.rs        HmacSigner (UrlSigner), PhotoIndex (SpeciesPhotos)
├─ sealing.rs        ConnectorKey: AES-256-GCM seal and unseal; StoredTokens, the sealed tokens
├─ connect.rs        the consent URLs, ConnectVendors (the vendor seam: exchange, refresh, sources, revoke), refresh_and_store
│  └─ oauth.rs       OAuthState: the signed OAuth state
├─ uploads.rs        multipart reading, fingerprint, the CPU pipeline, blob puts
├─ events.rs
│  ├─ hub.rs         EventHub: one broadcast channel per watched scan
│  ├─ writer.rs      ScanWriters: per-scan mpsc actors, plan locks; implements ScanWriter
│  └─ stream.rs      scan_stream: subscribe, replay, live, close
├─ broker.rs         JobBroker: Notify-woken claim long poll
├─ push.rs           Pusher: who gets which push
├─ scheduler.rs      the background tasks
│  └─ triggers.rs    the trigger rules (frost, heat, humid stretch, heavy rain, after rain, dry soil)
├─ farm_sync.rs      FarmSync: the connectors task's body over the FarmSource adapters; FarmSources
│  ├─ chunk.rs       chunks of at most 2,000 characters, their embedded text and text_hash
│  ├─ state.rs       SourceState: a source's stored cursor around the adapter's own; Rotation: the round, its turns and the next start
│  ├─ seed.rs        start-up seeding of the demo garden's era connectors
│  ├─ tests.rs       the sync on the embedded store with a scripted source
│  └─ tests_era.rs   the Slack and Drive syncs against Era's simulators (ignored by default)
├─ deletion.rs       DeletionRunner: cascades (objects, Agent Memory, records), account deletion, resume
├─ media.rs          signature check, object streaming
├─ routes.rs
│  └─ routes/        scans, gardens, plants, species, tasks, account, memories, chat, connectors,
│                    internal, media, ops
└─ tests.rs          handler tests
   ├─ tests/fake_store.rs  generated: a Store of per-area mockall mocks
   ├─ tests/beliefs.rs     the beliefs route and the deletion runner's Agent Memory step
   ├─ tests/connect.rs     the connect flow and the owner routes on, with a mocked ConnectVendors
   ├─ tests/connector_deletion.rs  connectors deleted with their garden or their owner: revoke order, failures, resume
   └─ tests/connectors.rs  the connector routes while connectors are off, and the list
```

---

## 4. Start-up and shutdown

```rust
/// Start-up in order: tracing; configuration; the ports (store with schema, blob, adapters,
/// models, the species photo index); resumed deletions; the demo garden's `era` connectors; the
/// scheduler; then serve until a signal, and drain.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        let json = serde_json::to_string_pretty(&router::openapi())?;
        std::io::stdout().write_all(json.as_bytes())?;
        return Ok(());
    }
    let config = config::Config::from_env().context("configuration")?;
    init_tracing(config.log_json);
    tracing::info!(bind = %config.bind, "starting");

    let shutdown = CancellationToken::new();
    let tasks = TaskTracker::new();
    let drain = config.shutdown_drain;
    let bind = config.bind;
    let state = wiring::build(config, shutdown.clone(), tasks.clone()).await?;

    let resumed = state.deletions.resume().await.context("resume deletions")?;
    if resumed > 0 {
        tracing::info!(resumed, "deletions resumed");
    }
    farm_sync::seed::at_start(&state).await;
    scheduler::start(&state);
    let deletes = state.deletes.clone();
    tokio::spawn(signal(shutdown.clone()));

    let listener = tokio::net::TcpListener::bind(bind).await.context("bind")?;
    // Graceful shutdown waits for every open connection: the SSE streams and the claim
    // long-poll end on the token, so this returns soon after the signal.
    axum::serve(listener, router::app(state))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .context("serve")?;

    // Background tasks get `SHUTDOWN_DRAIN_S`. Pending RustFS deletes get at least one full
    // attempt (the client's 120 s operation timeout) even when that drain is shorter, because an
    // interrupted delete can leak objects on RustFS 1.0.1 (storage 4).
    tasks.close();
    let delete_drain = drain.max(blob::client::OPERATION_TIMEOUT);
    let (tasks_waited, deletes_waited) = tokio::join!(
        tokio::time::timeout(drain, tasks.wait()),
        tokio::time::timeout(delete_drain, deletes.drain()),
    );
    if tasks_waited.is_err() {
        tracing::warn!(
            ?drain,
            "background tasks still running at exit; deletions resume on restart"
        );
    }
    if deletes_waited.is_err() {
        tracing::warn!(
            in_flight = deletes.in_flight(),
            "object deletes still running at exit"
        );
    }
    tracing::info!("stopped");
    Ok(())
}

/// JSON logs by default, `RUST_LOG` filtering (default `info`).
fn init_tracing(json: bool) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().flatten_event(true).init();
    } else {
        builder.init();
    }
}

/// Cancels the token on Ctrl-C or SIGTERM (Docker's stop signal). A handler that fails to
/// install is logged and waits forever, so it can't end the process the moment it starts; the
/// other signal, or Docker's SIGKILL, still stops it.
async fn signal(shutdown: CancellationToken) {
    let ctrl_c = until_signal("ctrl_c", tokio::signal::ctrl_c());
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => until_signal("sigterm", std::future::ready(Err(e))).await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
    tracing::info!("shutdown signal");
    shutdown.cancel();
}

/// Waits for a signal. If its handler can't be installed, logs that and never returns.
async fn until_signal(name: &str, signal: impl Future<Output = std::io::Result<()>>) {
    if let Err(error) = signal.await {
        tracing::error!(signal = name, %error, "signal handler not installed");
        std::future::pending::<()>().await;
    }
}
```

**The order** ([architecture 12](../architecture/README.md#12-deployment)):

| Step | What | Fails how |
|---|---|---|
| 1 | Tracing: JSON logs, `RUST_LOG` (default `info`) | — |
| 2 | Configuration from the environment (section 5) | Exits naming the variable |
| 3 | `SurrealDB`: apply `schema.surql` (idempotent, [database 1](../database/README.md#1-version-and-setup)) over a short-lived root connection, then connect and sign in as the `api` database user; load the knowledge index (`knowledge::SurrealKnowledgeStore::load`). Proposed: the same root connection then applies the migrations, `store::SurrealStore::migrate` ([store 4](store.md#4-connecting-and-the-schema)): new `NNNN_<slug>.surql` files in order, then `knowledge.surql` when its hash changed | Exits: nothing works without them. Proposed: also exits, naming the file, on a failed file, an edited or missing recorded file, or a file merged out of order |
| 4 | `RustFS` client ([storage 5](../storage/README.md#5-rust-client)) | Exits |
| 5 | Outbound adapters: one `reqwest` client shared by Clerk, Svix, Open-Meteo, `SoilGrids` (with the embedded Soil Landscapes of Canada lookup, parsed once), the geocoder (its time-zone boundaries parsed on a blocking thread) and Expo Push | Exits on a bad secret or a malformed embedded lookup |
| 6 | Models on CPU from `{MODELS_DIR}/manifest.toml`, every file's hash checked, on a blocking thread: the text embedder (one instance; each call names its `EmbedMode`), the upload pipeline's face detector, and the measurer with its colour spot segmenter. Proposed: then compare the embedder's `<name>@<version>` and dimension with `knowledge_meta:current` | Exits: face blurring fails closed, so uploads can't run without it. Proposed: also exits on an embedder that differs from the knowledge set's, since query vectors would not match the stored ones |
| 7 | The species photo index from `KnowledgeRepo::load_knowledge`; the product catalogue for the dose and plan checks | Exits |
| 8 | `api`'s own pieces: hub, broker, writers, pusher, deletion runner; then the pipeline and the agent, which need the writer | — |
| 9 | Resume cascades marked `deleting_at`, and account deletions whose tombstone isn't finished (section 17) | Exits on a store error |
| 10 | With connectors on and `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` and `ERA_DEMO_GARDEN_ID` set, seed the demo garden's two `era` connectors by fixed id (`farm_sync::seed`, section 16) | Logged, not fatal: a garden that doesn't exist yet is skipped until the next start |
| 11 | Start the scheduler's tasks on the `TaskTracker` (section 16) | — |
| 12 | Bind and serve | Exits |

`seed-upload` has already finished when `api` starts (`depends_on` with `service_completed_successfully`, [architecture 12](../architecture/README.md#12-deployment)), so the migration that references the species photos and the photo index see them.

**The migrations (proposed).** The design is in [database 1](../database/README.md#1-version-and-setup) ("Applying the migrations") and the crate's side in [store 4](store.md#4-connecting-and-the-schema). Production runs one `api`; each file's transaction checks the hash it expects, so a second process applying the same file at once rolls back and moves on. ✓ On the pinned image a 21 MB knowledge file of 4,000 chunks applies in 2.2 s on empty tables and 6–7 s over existing rows, so an unchanged file is skipped rather than re-applied. A failed migration exits the process; Compose restarts it into the same failure until a fixed build ships, and the files applied before it stay applied ([database 7](../database/README.md#7-operations)).

**Shutdown.** One `CancellationToken` for the process, cancelled on Ctrl-C or SIGTERM. A handler that fails to install is logged and waits forever (`until_signal`), so it can't shut the API down the moment it starts (✓ `a_signal_handler_that_fails_to_install_never_fires`). `axum::serve(..).with_graceful_shutdown(token.cancelled_owned())` stops accepting and then waits for every open connection; it returns only when the last one closes. So every long response ends on the token: the scan stream and the chat stream through `.take_until(token.cancelled_owned())`, the claim long poll through its `select!` (it answers `204` and the worker reconnects), and image bodies through `take_until` in `media::stream_object`, since `TimeoutLayer` bounds only the response head and a slow download would otherwise hold shutdown open (the client sees a short body and fetches the image again; ✓ `an_image_body_ends_on_the_shutdown_token`). Without that, one open SSE stream keeps the process up forever (✓ verified in the HTTP research probe: `completed = false` without `take_until`, true with it). `cancelled_owned()` returns an owned, `'static` future, which is what `with_graceful_shutdown` and `take_until` need.

Then `tracker.close()` and `tracker.wait()` with `SHUTDOWN_DRAIN_S` (30 s) as the deadline, and at the same time `DeleteQueue::drain` with `max(SHUTDOWN_DRAIN_S, 120 s)` as its deadline: pending `RustFS` deletes get at least one full attempt (the client's 120-second operation timeout, `blob::client::OPERATION_TIMEOUT`) even when the rest of the drain is shorter, because an interrupted delete can leak objects ([storage 4](../storage/README.md#4-writes-and-deletes)). The tracker holds the scheduler's loops (which end on the token), the writer actors (which drain and stop on the token), spawned follow-ups, and deletion cascades. Cascades don't watch the token, except an account cascade's Clerk retries, which stop on it (the tombstone resumes them); a cascade still running at the deadline is cut off with the process and resumes on the next start from `deleting_at` or the tombstone. Its object deletes belong to the delete queue, not to the cascade, so cutting the cascade off never interrupts one.

**The container's grace period.** Docker Compose sends SIGKILL `stop_grace_period` after SIGTERM, 10 s by default ([Compose file reference](https://docs.docker.com/reference/compose-file/services/#stop_grace_period)), so the `api` service sets `stop_grace_period: 190s`: the slowest in-flight request (60 s, `SLOW_TIMEOUT`), then the delete drain (`max(SHUTDOWN_DRAIN_S, 120 s)`), plus 10 s of margin. A `SHUTDOWN_DRAIN_S` above 120 s raises it by the difference.

---

## 5. Configuration

```rust
/// Everything `api` reads from the environment. Built by [`Config::from_env`].
#[derive(Debug, Clone)]
pub struct Config {
    /// `API_BIND`.
    pub bind: SocketAddr,
    /// `API_PUBLIC_BASE`: the origin prefixed to every signed image URL.
    pub public_base: String,
    /// `API_WEB_ORIGINS`: the web build's origins, for CORS and Clerk's `azp`.
    pub web_origins: Vec<String>,
    /// `API_URL_SIGNING_KEY`: at least 32 bytes, the HMAC key of signed URLs ([storage 3]).
    pub url_signing_key: SecretString,
    /// `WORKER_TOKENS`: `worker_id=token` pairs, comma separated ([contracts 1]).
    pub worker_tokens: BTreeMap<WorkerId, SecretString>,
    /// `MODELS_DIR`: the face detector and the text embedder (`vision`).
    pub models_dir: String,
    /// `UPLOAD_CPU_PERMITS`: concurrent upload pipelines on `spawn_blocking`.
    pub upload_permits: usize,
    /// `MEASURE_CPU_PERMITS`: concurrent measurement stages (the pipeline's).
    pub measure_permits: usize,
    /// `REQUEST_TIMEOUT_S`: the response-head timeout of ordinary routes.
    pub request_timeout: Duration,
    /// `RATE_PER_MINUTE`: the general per-user request limit.
    pub rate_per_minute: u32,
    /// `SHUTDOWN_DRAIN_S`: how long shutdown waits for background tasks.
    pub shutdown_drain: Duration,
    /// `DIAGNOSIS_MARGIN`: the fusion margin of contracts 7 (open question 3).
    pub diagnosis_margin: i32,
    /// `LOG_FORMAT`: `json` (default) or `pretty`.
    pub log_json: bool,
    /// The weather thresholds: the one instance handed to Open-Meteo (`frost_risk`), the
    /// pipeline (fusion) and the scheduler (triggers). The defaults; not read from the
    /// environment.
    pub weather: WeatherThresholds,
    /// The database.
    pub database: DatabaseConfig,
    /// The object store.
    pub storage: StorageConfig,
    /// The auth provider.
    pub clerk: ClerkConfig,
    /// Outbound providers.
    pub providers: ProviderConfig,
    /// Farm connectors.
    pub connectors: ConnectorSettings,
}

/// `SurrealDB` connection ([database 1], [database 6]).
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// `SURREAL_URL`, such as `ws://surrealdb:8000`. The namespace and database are the
    /// schema's own (`pdm` / `main`, `store::client`).
    pub url: String,
    /// `SURREAL_USER`: the database-level user `api` runs as.
    pub user: String,
    /// `SURREAL_PASS`.
    pub password: SecretString,
    /// `SURREAL_ROOT_USER`: applies `schema.surql` (idempotent) on every start, over a
    /// short-lived connection; `api` can't run its `DEFINE NAMESPACE` itself.
    pub root_user: String,
    /// `SURREAL_ROOT_PASS`.
    pub root_password: SecretString,
}

/// `RustFS` connection ([storage 5]).
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// `RUSTFS_ENDPOINT`.
    pub endpoint: String,
    /// `RUSTFS_BUCKET`.
    pub bucket: String,
    /// `RUSTFS_ACCESS_KEY`.
    pub access_key: String,
    /// `RUSTFS_SECRET_KEY`.
    pub secret_key: SecretString,
}

/// Clerk ([frontend 8], what the backend needs from Clerk).
#[derive(Debug, Clone)]
pub struct ClerkConfig {
    /// `CLERK_ISSUER`: the instance's Frontend API URL, the `iss` claim; the JWKS is
    /// `{issuer}/.well-known/jwks.json`.
    pub issuer: String,
    /// `CLERK_SECRET_KEY`: Backend API key, for account deletion.
    pub secret_key: SecretString,
    /// `CLERK_WEBHOOK_SECRET`: the Svix endpoint secret (`whsec_…`).
    pub webhook_secret: SecretString,
}

/// `SurrealDB` Agent Memory, where shared memories are mirrored (architecture 7, memory). All
/// three variables or none.
#[derive(Debug, Clone)]
pub struct AgentMemorySettings {
    /// `AGENT_MEMORY_URL`: the service's origin.
    pub url: String,
    /// `AGENT_MEMORY_CONTEXT_ID`: the context every call belongs to.
    pub context_id: String,
    /// `AGENT_MEMORY_API_KEY`.
    pub api_key: SecretString,
}

/// The three Agent Memory variables, set together or not at all.
const AGENT_MEMORY_VARS: [&str; 3] = [
    "AGENT_MEMORY_URL",
    "AGENT_MEMORY_CONTEXT_ID",
    "AGENT_MEMORY_API_KEY",
];

/// Outbound providers, handed to the `integrations` and `agent` constructors.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// `LLM_API_KEY`.
    pub llm_api_key: SecretString,
    /// `LLM_CHAT_MODEL`: chat and plan writing ([architecture 10]); the client's default
    /// when unset.
    pub llm_chat_model: Option<String>,
    /// `LLM_CHEAP_MODEL`: summaries, memory extraction, off-topic replies, the classifier
    /// fallback; the client's default when unset.
    pub llm_cheap_model: Option<String>,
    /// `JEV_API_KEY` and `JEV_URL`: the optional text classifier ([architecture 7]); the LLM
    /// fallback when unset.
    pub jev: Option<(String, SecretString)>,
    /// `EXPO_ACCESS_TOKEN`: only when enhanced push security is on.
    pub expo_access_token: Option<SecretString>,
    /// `OPEN_METEO_API_KEY`: only for Open-Meteo's commercial endpoint.
    pub open_meteo_api_key: Option<SecretString>,
    /// `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and `AGENT_MEMORY_API_KEY`: shared
    /// memories are mirrored into Agent Memory when all three are set; none set turns it off.
    pub agent_memory: Option<AgentMemorySettings>,
}

/// A vendor's OAuth client for the farm connectors ([connectors 5]): both variables or neither.
///
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
#[derive(Debug, Clone)]
pub struct OAuthClient {
    /// `SLACK_CLIENT_ID` or `GOOGLE_CLIENT_ID`: public, it goes in the consent URL.
    pub client_id: String,
    /// `SLACK_CLIENT_SECRET` or `GOOGLE_CLIENT_SECRET`: sent only in the code exchange.
    pub client_secret: SecretString,
}

/// The two Slack variables, set together or not at all.
const SLACK_VARS: [&str; 2] = ["SLACK_CLIENT_ID", "SLACK_CLIENT_SECRET"];

/// The two Google variables, set together or not at all.
const GOOGLE_VARS: [&str; 2] = ["GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"];

/// Farm connectors ([connectors 4], [connectors 5]). `key` is the one switch: without
/// `CONNECTOR_KEY` connectors are off (decision 9 of docs/EON), whatever else is set.
///
/// [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
#[derive(Debug, Clone)]
pub struct ConnectorSettings {
    /// `CONNECTOR_KEY`: 64 hex characters (32 bytes, `openssl rand -hex 32`), the AES-256-GCM
    /// key that seals vendor tokens. Unset turns connectors off.
    pub key: Option<ConnectorKey>,
    /// `SLACK_CLIENT_ID` and `SLACK_CLIENT_SECRET`.
    pub slack: Option<OAuthClient>,
    /// `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`: a Web application client.
    pub google: Option<OAuthClient>,
    /// `CONNECTOR_RETURN_URL` (default `pdm://connectors`): where the OAuth callback's `302`
    /// sends the app, as `{return_url}/{connector id}/callback?code&state`. The callback
    /// itself is `{API_PUBLIC_BASE}/connectors/oauth/callback`.
    pub return_url: String,
    /// `ERA_SLACK_BASE`: Era's Slack simulator, for start-up seeding of the demo garden's
    /// `era` connectors (roadmap E4.6). Never used by a route (decision 8).
    pub era_slack_base: Option<String>,
    /// `ERA_GOOGLE_BASE`: Era's Google Drive simulator, as above.
    pub era_google_base: Option<String>,
    /// `ERA_DEMO_GARDEN_ID`: the demo garden (E9.0 creates it through the API, so its id is
    /// known only after) that start-up seeding gives the two `era` connectors.
    pub era_demo_garden: Option<GardenId>,
}
```

```rust
/// All three Agent Memory variables, or none; a partial set names the first missing one.
fn agent_memory<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
) -> Result<Option<AgentMemorySettings>, ConfigError> {
    let [url, context, key] = AGENT_MEMORY_VARS.map(|name| env.get(name));
    match (url, context, key) {
        (None, None, None) => Ok(None),
        (Some(url), Some(context_id), Some(key)) => Ok(Some(AgentMemorySettings {
            url: url.trim().trim_end_matches('/').to_owned(),
            context_id: context_id.trim().to_owned(),
            api_key: SecretString::from(key),
        })),
        _ => {
            let missing = AGENT_MEMORY_VARS
                .into_iter()
                .find(|name| env.get(name).is_none())
                .unwrap_or(AGENT_MEMORY_VARS[0]);
            Err(ConfigError::Invalid {
                name: missing,
                problem: "AGENT_MEMORY_URL, AGENT_MEMORY_CONTEXT_ID and AGENT_MEMORY_API_KEY \
                          are set together or not at all"
                    .to_owned(),
            })
        }
    }
}

/// The connector settings; a partial vendor pair names its first missing variable.
fn connectors<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
) -> Result<ConnectorSettings, ConfigError> {
    let key = env
        .get("CONNECTOR_KEY")
        .map(|value| {
            ConnectorKey::from_hex(&value).map_err(|e| ConfigError::Invalid {
                name: "CONNECTOR_KEY",
                problem: e.to_string(),
            })
        })
        .transpose()?;
    let return_url = env
        .or("CONNECTOR_RETURN_URL", "pdm://connectors")
        .trim()
        .trim_end_matches('/')
        .to_owned();
    if !return_url_ok(&return_url) {
        return Err(ConfigError::Invalid {
            name: "CONNECTOR_RETURN_URL",
            problem: "expected an https URL or an app scheme (not http, javascript, data, file or vbscript), with no query, such as pdm://connectors"
                .to_owned(),
        });
    }
    Ok(ConnectorSettings {
        key,
        slack: oauth_client(env, SLACK_VARS)?,
        google: oauth_client(env, GOOGLE_VARS)?,
        return_url,
        era_slack_base: http_base(env, "ERA_SLACK_BASE")?,
        era_google_base: http_base(env, "ERA_GOOGLE_BASE")?,
        era_demo_garden: env
            .get("ERA_DEMO_GARDEN_ID")
            .map(|value| {
                value.trim().parse().map_err(|_| ConfigError::Invalid {
                    name: "ERA_DEMO_GARDEN_ID",
                    problem: "expected a garden id (a ULID)".to_owned(),
                })
            })
            .transpose()?,
    })
}

/// A vendor's client id and secret, both or neither; a partial pair names the missing one.
fn oauth_client<F: Fn(&str) -> Option<String>>(
    env: &Env<F>,
    [id_var, secret_var]: [&'static str; 2],
) -> Result<Option<OAuthClient>, ConfigError> {
    match (env.get(id_var), env.get(secret_var)) {
        (None, None) => Ok(None),
        (Some(id), Some(secret)) => Ok(Some(OAuthClient {
            client_id: id.trim().to_owned(),
            client_secret: SecretString::from(secret),
        })),
        (id, _) => Err(ConfigError::Invalid {
            name: if id.is_none() { id_var } else { secret_var },
            problem: format!("{id_var} and {secret_var} are set together or not at all"),
        }),
    }
}
```

| Variable | Default | Meaning |
|---|---|---|
| `API_BIND` | `0.0.0.0:8080` | Listen address |
| `API_PUBLIC_BASE` | required | The origin prefixed to every signed image URL, such as `https://api.example.org` |
| `API_WEB_ORIGINS` | empty | The web build's origins, comma separated: CORS and Clerk's `azp` |
| `API_URL_SIGNING_KEY` | required, ≥ 32 bytes | HMAC key of signed URLs |
| `WORKER_TOKENS` | required | `worker_id=token` pairs, each token ≥ 32 bytes |
| `MODELS_DIR` | `/models` | Holds `manifest.toml` and the face detector, text embedder and tokenizer it lists |
| `UPLOAD_CPU_PERMITS` | 2 | Upload pipelines running at once on blocking threads |
| `MEASURE_CPU_PERMITS` | 2 | Measurement stages at once (the pipeline's `cpu_permits`) |
| `REQUEST_TIMEOUT_S` | 10 | Response-head timeout of ordinary routes |
| `RATE_PER_MINUTE` | 120 | The general per-user request limit (a placeholder: the documents give none) |
| `SHUTDOWN_DRAIN_S` | 30 | How long shutdown waits for background tasks; pending `RustFS` deletes always get at least 120 s. Keep the service's `stop_grace_period` at `max(this, 120) + 70` seconds (190 s by default) |
| `DIAGNOSIS_MARGIN` | 2 | The fusion margin ([contracts 12](../architecture/contracts.md#12-open-questions), question 3) |
| `LOG_FORMAT` | `json` | `pretty` for development |
| `RUST_LOG` | `info` | `tracing` filter |
| `SURREAL_URL` | `ws://surrealdb:8000` | The namespace and database are the schema's own (`pdm`, `main`) |
| `SURREAL_USER` | `api` | The database user `api` runs as |
| `SURREAL_PASS` | required | Secret |
| `SURREAL_ROOT_USER` | `root` | Applies `schema.surql` on every start, over a short-lived connection |
| `SURREAL_ROOT_PASS` | required | Secret |
| `RUSTFS_ENDPOINT` | `http://rustfs:9000` | |
| `RUSTFS_BUCKET` | `pdm-images` | The bucket of [storage 2](../storage/README.md#2-bucket-keys-and-access) |
| `RUSTFS_ACCESS_KEY` | required | `api`'s own storage identity ([storage 2](../storage/README.md#2-bucket-keys-and-access)) |
| `RUSTFS_SECRET_KEY` | required | Secret |
| `CLERK_ISSUER` | required | The Frontend API URL (`iss`); the JWKS is `{issuer}/.well-known/jwks.json` |
| `CLERK_SECRET_KEY` | required | Backend API, for account deletion |
| `CLERK_WEBHOOK_SECRET` | required | Svix endpoint secret, `whsec_…` |
| `LLM_API_KEY` | required | The agent's and the pipeline's LLM |
| `LLM_CHAT_MODEL` | the client's default | Chat and plan writing ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)) |
| `LLM_CHEAP_MODEL` | the client's default | Memory extraction, off-topic replies, the classifier fallback |
| `JEV_API_KEY`, `JEV_URL` | unset | The optional classifier; both or neither; the LLM classifier otherwise |
| `EXPO_ACCESS_TOKEN` | unset | Only with enhanced push security |
| `OPEN_METEO_API_KEY` | unset | Only for Open-Meteo's commercial endpoint |
| `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID`, `AGENT_MEMORY_API_KEY` | unset | `SurrealDB` Agent Memory, where shared memories are mirrored ([architecture 7, memory](../architecture/README.md#memory)). All three or none: a partial set stops start-up with `ConfigError::Invalid` naming the first missing variable. The key is a `SecretString`; unset turns the mirror off (`NoBeliefs`) |
| `CONNECTOR_KEY` | unset | The farm connectors' sealing key ([connectors 4](../connectors/README.md#4-data)): 32 bytes written as 64 hex characters, either case (`openssl rand -hex 32`, as [infra 6](../infra/README.md#6-secrets-and-configuration) generates every secret). Anything else stops start-up without quoting the value. Unset turns connectors off (decision 9 of [docs/EON](../EON/README.md#10-decisions-blockers-and-waiting-items)) |
| `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET` | unset | The Slack app's OAuth client ([connectors 5](../connectors/README.md#5-connecting-and-tokens)). Both or neither: a partial pair stops start-up naming the missing one. The secret is a `SecretString` |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | unset | The Google Web application OAuth client; both or neither, as above |
| `CONNECTOR_RETURN_URL` | `pdm://connectors` | Where `GET /connectors/oauth/callback` sends the app: `302` to `{this}/{connector id}/callback?code&state`. A scheme is required; a query or fragment is refused. The callback itself is `{API_PUBLIC_BASE}/connectors/oauth/callback` |
| `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` | unset | Era's Slack and Drive simulators, `http(s)://` origins, for start-up seeding of the demo garden's `era` connectors (roadmap E4.6) and the adapters that read them. No route reads them (decision 8) |
| `ERA_DEMO_GARDEN_ID` | unset | The demo garden's id (a ULID), which seeding gives the two `era` connectors. E9.0 creates the garden through the API, so the deploy sets it afterwards; until the garden exists, seeding logs and skips |

Every secret is a `SecretString`: `Debug` prints `[REDACTED]` (the test checks the signing key never appears in the formatted config). `Config::from_lookup` takes any variable source, so tests build a config from a map.

**Sealing.** `CONNECTOR_KEY` parses into a `ConnectorKey`, which holds the expanded AES-256-GCM key (wiped on drop through `aes-gcm`'s `zeroize` feature) and prints as `ConnectorKey([REDACTED])`. `seal` draws a fresh 96-bit nonce from the OS for every call, so sealing one token twice gives two values, and writes `nonce (12 bytes) || ciphertext || tag (16 bytes)` into `domain::ports::Sealed`, whose `Debug` prints only the length. The associated data is the connector's id as it is written on the wire, so a sealed value opens only on its own connector ([connectors 4](../connectors/README.md#4-data)). `unseal` returns the plaintext as a `SecretSlice<u8>`, wiped on drop; a wrong key, another connector's id or any changed byte is the same opaque `UnsealError::Rejected`. It lives in `api` because every caller does: the connect routes (E3.3, E3.4), the `connectors` scheduler task (E4.1) and start-up seeding (E4.6). The `FarmSource` adapters in `integrations` get an unsealed `VendorToken` and never see the key, and `domain` holds only the opaque bytes, so neither depends on a cipher. The connect routes seal Google's PKCE verifier (`SealPurpose::Pkce`) and, after the exchange, the vendor's tokens (`SealPurpose::Token`) as `sealing::StoredTokens`: Slack's bot token as its plain string, Google's grant as JSON `{access_token, refresh_token?, expires_at?}`, read back as JSON exactly when the plaintext starts with `{`; the routes, the deletion runner and the sync all read it through `StoredTokens::unseal`. The deletion runner opens a deleted connector's tokens to revoke them (E3.6, section 17). Start-up seeding seals the demo connectors' tokens as plain strings (E4.6).

```rust
/// `CONNECTOR_KEY`: 32 bytes, written as 64 hex characters (`openssl rand -hex 32`, the way every
/// other secret is generated, [infra 6]). Holds the expanded AES-256 key, which is wiped on drop;
/// `Debug` never prints it.
///
/// [infra 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/infra/README.md#6-secrets-and-configuration
#[derive(Clone)]
pub struct ConnectorKey {
    cipher: Arc<Aes256Gcm>,
    /// The OAuth state's HMAC key, derived from the raw key (E3.3, [connectors 5]).
    state_key: Arc<SecretBox<[u8; 32]>>,
}

/// Why `CONNECTOR_KEY` was refused. The message never quotes the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// Not hexadecimal.
    #[error("expected 64 hex characters (openssl rand -hex 32)")]
    NotHex,
    /// Hexadecimal, but not 32 bytes.
    #[error("expected 32 bytes (64 hex characters), got {0} bytes")]
    Length(usize),
}

/// Why a sealed value didn't open. Deliberately opaque: AES-GCM can't tell a wrong key, a wrong
/// connector and a changed byte apart, and saying more would help no one but an attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnsealError {
    /// Shorter than a nonce and a tag: not something [`ConnectorKey::seal`] wrote.
    #[error("the sealed value is too short")]
    TooShort,
    /// The tag didn't verify: another key, another connector, or a changed byte.
    #[error("the sealed value doesn't open under this key for this connector")]
    Rejected,
}

/// A connector's vendor tokens, as they are sealed in `connector.secret` with
/// [`SealPurpose::Token`]: the one reader and writer of that value, for the connect routes, the
/// deletion runner's revoke (E3.6) and the sync (E4). Two forms:
///
/// - **A plain string**, the token's own bytes: Slack's `xoxb-…` bot token, which has neither a
///   refresh token nor an expiry, and a seeded connector's fixed token (roadmap E4.6).
/// - **A JSON object** `{"access_token", "refresh_token"?, "expires_at"?}`: Google's grant.
///   `expires_at` is RFC 3339. A revoke prefers `refresh_token`, which ends the whole grant.
///
/// [`seal`](Self::seal) writes the plain string exactly when there is no refresh token and no
/// expiry; [`unseal`](Self::unseal) reads a value that starts with `{` as JSON, anything else
/// as a plain token.
#[derive(Clone)]
pub struct StoredTokens {
    /// Slack's `xoxb-…` bot token, or Google's access token.
    pub access_token: SecretString,
    /// Google's refresh token.
    pub refresh_token: Option<SecretString>,
    /// When Google's access token expires.
    pub expires_at: Option<Timestamp>,
}
```

```rust
impl ConnectorKey {
    /// Parses the variable's value: 64 hex characters, either case, surrounding space ignored.
    pub fn from_hex(value: &str) -> Result<Self, KeyError> {
        let mut bytes = data_encoding::HEXLOWER_PERMISSIVE
            .decode(value.trim().as_bytes())
            .map_err(|_| KeyError::NotHex)?;
        let key = <[u8; 32]>::try_from(bytes.as_slice()).map(Self::from_bytes);
        let len = bytes.len();
        bytes.zeroize();
        key.map_err(|_| KeyError::Length(len))
    }

    /// A key from its 32 bytes. The caller wipes its own copy.
    pub fn from_bytes(mut bytes: [u8; 32]) -> Self {
        let mut key = Key::<Aes256Gcm>::from(bytes);
        let cipher = Aes256Gcm::new(&key);
        key.zeroize();
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&bytes)
            .expect("HMAC accepts a key of any length");
        mac.update(STATE_KEY_LABEL);
        let mut derived: [u8; 32] = mac.finalize().into_bytes().into();
        bytes.zeroize();
        let state_key = SecretBox::new(Box::new(derived));
        derived.zeroize();
        Self {
            cipher: Arc::new(cipher),
            state_key: Arc::new(state_key),
        }
    }

    /// The key that signs the OAuth `state` (E3.3): HMAC-SHA256 of the raw key under a fixed
    /// label, so it differs from the AES key and rotating `CONNECTOR_KEY` rotates both.
    pub fn state_key(&self) -> &[u8; 32] {
        self.state_key.expose_secret()
    }

    /// Seals `plaintext` for `connector`, with a fresh random nonce, so sealing the same token
    /// twice gives two different values.
    pub fn seal(&self, purpose: SealPurpose, connector: ConnectorId, plaintext: &[u8]) -> Sealed {
        let nonce = Nonce::generate();
        let mut body = Vec::with_capacity(plaintext.len() + TAG_LEN);
        body.extend_from_slice(plaintext);
        self.cipher
            .encrypt_in_place(
                &nonce,
                associated_data(purpose, connector).as_bytes(),
                &mut body,
            )
            .expect(
                "AES-GCM refuses only a message over 64 GiB, and a token is a few hundred bytes",
            );
        let mut sealed = Vec::with_capacity(NONCE_LEN + body.len());
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&body);
        Sealed(sealed.into())
    }

    /// Opens a value sealed for `connector`. The plaintext comes back as a secret, wiped when
    /// dropped. The deletion runner opens tokens to revoke them (E3.6); the routes (E3.4) and
    /// the sync (E4.1) open them too.
    pub fn unseal(
        &self,
        purpose: SealPurpose,
        connector: ConnectorId,
        sealed: &Sealed,
    ) -> Result<SecretSlice<u8>, UnsealError> {
        if sealed.0.len() < NONCE_LEN + TAG_LEN {
            return Err(UnsealError::TooShort);
        }
        let (nonce, body) = sealed.0.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce).map_err(|_| UnsealError::TooShort)?;
        let mut body = body.to_vec();
        let opened = self.cipher.decrypt_in_place(
            &nonce,
            associated_data(purpose, connector).as_bytes(),
            &mut body,
        );
        if opened.is_err() {
            body.zeroize();
            return Err(UnsealError::Rejected);
        }
        Ok(SecretSlice::from(body))
    }
}
```

**One switch.** `AppState::connector_key` (section 6) is the only place that decides whether connectors are on. The list route asks it and answers `available: false`; each of the seven write routes calls `require_connectors` first and answers `409 invalid_state` when it is off; the `connectors` task and seeding ask it too; `wiring.rs` logs the decision at start, with a warning when vendor or Era settings are set without the key.

---

## 6. Application state, wiring and the seams

```rust
/// The domain ports, built by `wiring.rs` from the library crates.
#[derive(Clone)]
pub struct Ports {
    /// `store`: every repository (`Store` is their supertrait).
    pub store: Arc<dyn Store>,
    /// `store`: memories. The `beliefs` task records the shared ones in Agent Memory.
    pub memory: Arc<dyn MemoryStore>,
    /// `integrations`: `SurrealDB` Agent Memory, or `NoBeliefs` when it isn't configured.
    pub beliefs: Arc<dyn BeliefStore>,
    /// `knowledge`: profiles, species list, chunk search.
    pub knowledge: Arc<dyn KnowledgeStore>,
    /// `blob`: `RustFS`.
    pub blob: Arc<dyn BlobStore>,
    /// `integrations`: Clerk JWKS.
    pub auth: Arc<dyn AuthVerifier>,
    /// `integrations`: Svix verification.
    pub webhooks: Arc<dyn WebhookVerifier>,
    /// `integrations`: Open-Meteo.
    pub weather: Arc<dyn WeatherProvider>,
    /// `integrations`: `SoilGrids`.
    pub soil: Arc<dyn SoilProvider>,
    /// `integrations`: the geocoder.
    pub geocoder: Arc<dyn Geocoder>,
    /// `integrations`: Expo Push.
    pub notifier: Arc<dyn Notifier>,
    /// `vision`: the text embedder, on CPU.
    pub embedder: Arc<dyn TextEmbedder>,
    /// `api`: the HMAC signer of image URLs.
    pub signer: Arc<dyn UrlSigner>,
    /// `api`: the species photo index, loaded from the knowledge base at start.
    pub photos: Arc<dyn SpeciesPhotos>,
    /// System time.
    pub clock: Arc<dyn Clock>,
    /// ULIDs.
    pub ids: Arc<dyn IdGen>,
}

/// Everything a handler, an extractor or a background task can reach.
pub struct AppState {
    /// The configuration.
    pub config: Arc<Config>,
    /// The domain ports.
    pub ports: Ports,
    /// `pipeline::Pipeline`.
    pub pipeline: Arc<dyn PipelineApi>,
    /// `agent::ChatAgent`.
    pub chat: Arc<dyn ChatTurns>,
    /// `vision::upload::UploadProcessor`.
    pub images: Arc<dyn ImageProcessing>,
    /// One broadcast channel per scan with live subscribers.
    pub hub: Arc<EventHub>,
    /// The per-scan writer actors; the pipeline's `ScanWriter`.
    pub writers: Arc<ScanWriters>,
    /// The claim long-poll's wake-up.
    pub broker: Arc<JobBroker>,
    /// Keyed rate limiters.
    pub limits: RateLimits,
    /// Bounds the upload pipelines running on `spawn_blocking`.
    pub upload_permits: Arc<Semaphore>,
    /// Worker bearer tokens.
    pub workers: WorkerTokens,
    /// Garden, plant, scan and account cascades.
    pub deletions: DeletionRunner,
    /// The farm connectors' sync and their adapters; `None` when connectors are off (no
    /// `CONNECTOR_KEY`), so the `connectors` task and start-up seeding don't run.
    pub farm_sync: Option<Arc<FarmSync>>,
    /// The one path to `RustFS` deletes ([storage 4]): detached, never cancelled, retried, and
    /// drained at shutdown for at least one full attempt. Shared with `deletions`.
    pub deletes: blob::DeleteQueue,
    /// The farm connectors' vendor calls (the code exchange, the source picker, the revoke), on
    /// the vendors' fixed hosts; a seam like `pipeline`, so route tests mock it.
    pub connect: Arc<dyn ConnectVendors>,
    /// Cancelled once on SIGTERM / Ctrl-C.
    pub shutdown: CancellationToken,
    /// Every background task and spawned follow-up; waited on at shutdown.
    pub tasks: TaskTracker,
}

/// The router's state type.
pub type SharedState = Arc<AppState>;
```

```rust
impl AppState {
    /// A signer for one response or one event send: it borrows the signer and photo ports from
    /// `self` for `'_`, and every URL it makes expires 15 minutes from now.
    pub fn signer(&self) -> Signer<'_> {
        Signer::new(
            &*self.ports.signer,
            &*self.ports.photos,
            self.ports.clock.now(),
        )
    }

    /// The membership check for a garden id from a body or query ([`authz::check_membership`]).
    pub async fn member<R: MinRole>(
        &self,
        user: &CurrentUser,
        garden: GardenId,
    ) -> Result<Membership<R>, ApiError> {
        Ok(authz::check_membership::<R>(&*self.ports.store, user, garden).await?)
    }

    /// Whether farm connectors are on, and the key that seals their tokens: the one switch
    /// (decision 9 of docs/EON). `None` without `CONNECTOR_KEY`: the list answers
    /// `available: false`, every owner write `409 invalid_state`, and the `connectors` scheduler
    /// task (E4.1) and start-up seeding (E4.6) do nothing ([`Self::farm_sync`] is `None`).
    pub fn connector_key(&self) -> Option<&ConnectorKey> {
        self.config.connectors.key.as_ref()
    }

    /// Spawns a follow-up on the tracker; failures are logged, the sweep recovers them.
    pub fn spawn_follow_up(self: &Arc<Self>, follow_up: Option<crate::seams::FollowUp>) {
        if let Some(follow_up) = follow_up {
            let state = Arc::clone(self);
            self.tasks.spawn(async move {
                if let Err(error) = state.pipeline.follow_up(follow_up).await {
                    tracing::warn!(?follow_up, error = %crate::error::chain(&error), "follow-up failed; the sweep retries it");
                }
            });
        }
    }
}
```

**One state, no generics.** `AppState` holds every port as `Arc<dyn Trait>` (convention 3); handlers get `State<Arc<AppState>>`. `Store` is the supertrait of the 17 repositories, so `state.ports.store.create_scan(..)` and `state.ports.store.append_event(..)` go through one `Arc<dyn Store>`; where a component needs one area (the broker needs `JobRepo`), the `Arc<dyn Store>` is upcast to `Arc<dyn JobRepo>` (trait upcasting, stable since Rust 1.86).

**The seams.** Three library APIs aren't domain ports: the pipeline's entry points and its `ScanWriter` port, the agent's chat turn, and the vision crate's upload pipeline. `api` holds each as a trait object of its own (`PipelineApi`, `ChatTurns`, `ImageProcessing`), so a handler test swaps it for a fake exactly like a domain port; `wiring.rs` implements those traits for `pipeline::Pipeline`, `agent::ChatAgent` and `vision::upload::UploadProcessor` (a local trait on a foreign type, which the orphan rule allows). The types that cross are the crates' own, re-exported (`pub use pipeline::{FollowUp, Handled, PhotoInfo, PipelineError, …}`), and each seam method has the pipeline method's exact signature: `PipelineError` is the pipeline's enum (`LabelsNotLoaded { label_set_version, classifier_labels_version }`, `PlanWriter(agent::PlanWriterError)`), and `photo_answer_added` is synchronous and infallible as in the pipeline. The continuation of an unanswered question isn't a seam method: `sweep` owns it (section 16).

```rust
/// The pipeline's entry points (`pipeline::Pipeline`). All return quickly except
/// [`follow_up`](Self::follow_up), which `api` spawns.
#[async_trait]
pub trait PipelineApi: Send + Sync {
    /// Runs a step or question step and its hand-overs.
    async fn follow_up(&self, follow_up: FollowUp) -> Result<(), PipelineError>;
    /// `POST /scans/{id}/submit`.
    async fn submit(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
    ) -> Result<SubmitOutcome, PipelineError>;
    /// `POST /plants/{id}/species`.
    async fn species_confirmed(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<Plant<Stored>, PipelineError>;
    /// `POST /scans/{id}/answers`.
    async fn answer(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        answer: AnswerRequest,
    ) -> Result<Handled<Scan<Stored>>, PipelineError>;
    /// Before `add_image`, for an upload with `question_id`.
    async fn photo_answer_start(
        &self,
        scan: ScanId,
        question: QuestionId,
        photo: PhotoInfo,
    ) -> Result<PhotoAnswerStart, PipelineError>;
    /// After `add_image`, with `AddedImage.photo_answer`: a `measuring` start comes back as a
    /// follow-up; a `segmenting` start has its job queued already.
    fn photo_answer_added(&self, claim: QuestionStepClaim) -> Option<FollowUp>;
    /// `POST /internal/jobs/{id}/events`.
    async fn worker_events(
        &self,
        lease: &Lease,
        job: LeasedJob,
        events: Vec<WorkerEvent>,
    ) -> Result<(), PipelineError>;
    /// `POST /internal/jobs/{id}/complete`.
    async fn job_completed(
        &self,
        lease: &Lease,
        job: LeasedJob,
        complete: CompleteJob,
    ) -> Result<Handled<CompletionOutcome>, PipelineError>;
    /// A job out of attempts (from `complete` or the requeue sweep).
    async fn job_failed_for_good(&self, job: LeasedJob) -> Result<Option<FollowUp>, PipelineError>;
    /// The sweep: stale steps, stalled question steps, and the 10-minute continuation of
    /// unanswered questions (its one owner; the scheduler's `sweep` task calls it).
    async fn sweep(&self) -> Result<Vec<FollowUp>, PipelineError>;
    /// A confirmed `revise_plan`: plan lock, `write_revision`, then `plan_superseded`.
    async fn store_revision(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        write: PlanWrite,
    ) -> Result<ActionPlan, PipelineError>;
}

/// `TurnStream::into_parts()`, with the events boxed: `'static` and `Send`, so they can move
/// into the response body.
pub struct TurnParts {
    /// The events. Dropping this stream stops the LLM call.
    pub events: BoxStream<'static, ChatEvent>,
    /// Resolved by the turn task once it stored the message.
    pub outcome: oneshot::Receiver<TurnOutcome>,
}

/// The agent's chat turn and the pure preview application (`agent::ChatAgent`,
/// `agent::tools::revise::apply_preview`).
#[async_trait]
pub trait ChatTurns: Send + Sync {
    /// Validation, budget, routing, storing the user message; then the stream.
    async fn start_turn(&self, request: TurnRequest) -> Result<TurnParts, TurnError>;

    /// The plan a confirmed `revise_plan` card stores.
    fn apply_preview(
        &self,
        base: &ActionPlan,
        preview: &Preview,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<RevisedPlan, PreviewError>;
}

/// The CPU upload pipeline (`vision::upload::UploadProcessor`). Synchronous and blocking: `api`
/// calls it inside `spawn_blocking`, behind a `Semaphore`.
pub trait ImageProcessing: Send + Sync {
    /// A scan photo with its optional depth.
    fn process(&self, input: UploadInput<'_>) -> Result<ProcessedUpload, UploadError>;
    /// A chat image: no quality check, no depth.
    fn process_chat(&self, file: &[u8]) -> Result<ProcessedImage, UploadError>;
}
```

The pipeline's `ScanWriter` port, which `api` implements (section 13), is the pipeline crate's:

```rust
#[async_trait::async_trait]
pub trait ScanWriter: Send + Sync {
    async fn emit(&self, scan: ScanId, run: Run, events: Vec<ScanEventBody<Stored>>)
        -> Result<Vec<ScanEvent<Stored>>, StoreError>;
    async fn plan_lock(&self, scan: ScanId) -> PlanGuard;
}
```

It has no wake-up method: the store signals the claim long poll itself after every write that queues a job (section 15).

**Wiring.** `wiring.rs` is the only module that names concrete types. It implements the seams by delegation:

```rust
#[async_trait]
impl ChatTurns for agent::ChatAgent {
    async fn start_turn(&self, request: TurnRequest) -> Result<TurnParts, TurnError> {
        let (events, outcome) = agent::ChatAgent::start_turn(self, request)
            .await?
            .into_parts();
        Ok(TurnParts {
            events: events.boxed(),
            outcome,
        })
    }

    fn apply_preview(
        &self,
        base: &ActionPlan,
        preview: &Preview,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<RevisedPlan, PreviewError> {
        agent::tools::revise::apply_preview(base, preview, action, now)
    }
}
```

and builds everything in the order of section 4:

```rust
/// The system clock.
pub struct SystemClock;

/// ULIDs from `ulid` 3 (`Ulid::generate`).
pub struct UlidGen;

/// The CPU models `api` loads: the text embedder, the upload pipeline (face detector) and the
/// measurer, from `{MODELS_DIR}/manifest.toml`, every file's hash checked.
struct Models {
    /// The text embedder, shared by `api` (garden notes), the pipeline (memory summaries) and
    /// the agent (searches and memories); each call names its `EmbedMode`.
    embedder: Arc<dyn TextEmbedder>,
    images: Arc<dyn ImageProcessing>,
    measurer: Arc<dyn pipeline::ImageMeasurer>,
}

/// Loads the ONNX sessions on a blocking thread: they take seconds.
async fn load_models(models_dir: &str) -> anyhow::Result<Models> {
    let manifest_path = Path::new(models_dir).join("manifest.toml");
    tokio::task::spawn_blocking(move || -> anyhow::Result<Models> {
        let runtime =
            vision::runtime::OrtRuntime::init(&vision::runtime::RuntimeConfig::cpu("api"))?;
        let manifest = vision::manifest::Manifest::load(&manifest_path)?;
        let embedder = vision::embed::ArcticEmbedder::load(
            &runtime,
            &manifest,
            vision::embed::EmbedderConfig::default(),
        )?;
        let images = vision::upload::UploadProcessor::load(
            &runtime,
            &manifest,
            vision::upload::UploadConfig::default(),
        )?;
        let spots = Arc::new(vision::measure::spots::ColourSpotSegmenter::default());
        let measurer =
            vision::measure::Measurer::new(vision::measure::MeasurementConfig::default(), spots);
        Ok(Models {
            embedder: Arc::new(embedder),
            images: Arc::new(images),
            measurer: Arc::new(pipeline::measure::VisionMeasurer::new(Arc::new(measurer))),
        })
    })
    .await?
}

/// The database side: the schema applied as root, then the `api` user's connection, the
/// repositories, memories and the knowledge index.
struct Databases {
    store: Arc<dyn Store>,
    memory: Arc<dyn MemoryStore>,
    knowledge_repo: Arc<dyn KnowledgeRepo>,
    knowledge: Arc<dyn KnowledgeStore>,
    jobs_queued: Arc<tokio::sync::Notify>,
}

async fn connect_databases(config: &Config) -> anyhow::Result<Databases> {
    let db = &config.database;
    store::SurrealStore::apply_schema(&db.url, &db.root_user, db.root_password.expose_secret())
        .await
        .context("apply schema.surql")?;
    let surreal = Arc::new(
        store::SurrealStore::connect(&store::StoreConfig {
            url: db.url.clone(),
            username: db.user.clone(),
            password: db.password.expose_secret().to_owned(),
        })
        .await
        .context("SurrealDB")?,
    );
    // The store signals this after every write that queues or requeues a job commits; the
    // broker's claim long poll waits on it, and nothing else wakes it.
    let jobs_queued = surreal.jobs_queued();
    let knowledge_repo: Arc<dyn KnowledgeRepo> = surreal.clone();
    let knowledge: Arc<dyn KnowledgeStore> = Arc::new(
        knowledge::SurrealKnowledgeStore::load(Arc::clone(&knowledge_repo))
            .await
            .context("knowledge base")?,
    );
    Ok(Databases {
        store: surreal.clone(),
        memory: surreal,
        knowledge_repo,
        knowledge,
        jobs_queued,
    })
}

/// The outbound adapters, over one shared HTTP client.
struct Outbound {
    auth: Arc<dyn AuthVerifier>,
    webhooks: Arc<dyn WebhookVerifier>,
    auth_admin: Arc<dyn AuthAdmin>,
    weather: Arc<dyn WeatherProvider>,
    soil: Arc<dyn SoilProvider>,
    geocoder: Arc<dyn Geocoder>,
    notifier: Arc<dyn Notifier>,
    chat_model: Arc<dyn ChatModel>,
    classifier: Arc<dyn TextClassifier>,
    beliefs: Arc<dyn BeliefStore>,
    connect: Arc<dyn ConnectVendors>,
    farm: FarmSources,
}

#[expect(
    clippy::too_many_lines,
    reason = "every outbound adapter is built here, one after another, over one HTTP client"
)]
async fn outbound(config: &Config, clock: &Arc<dyn Clock>) -> anyhow::Result<Outbound> {
    let http = client(&HttpConfig::default()).context("HTTP client")?;
    let mut verifier = ClerkVerifierConfig::new(config.clerk.issuer.clone());
    verifier.allowed_azp.clone_from(&config.web_origins);
    // The time-zone boundaries are a few megabytes to parse: off the async threads.
    let zones = Arc::new(tokio::task::spawn_blocking(TimeZoneFinder::default).await?);
    // Soil Landscapes of Canada, for the Canadian points where SoilGrids has no data.
    let landscapes = Arc::new(SoilLandscapes::embedded().context("Soil Landscapes of Canada")?);
    let defaults = ModelIds::default();
    let strong = config
        .providers
        .llm_chat_model
        .clone()
        .unwrap_or_else(|| defaults.chat.clone());
    let cheap = config
        .providers
        .llm_cheap_model
        .clone()
        .unwrap_or_else(|| defaults.memory_extraction.clone());
    let models = ModelIds {
        chat: strong.clone(),
        plan_writer: strong,
        memory_extraction: cheap.clone(),
        off_topic: cheap.clone(),
        classifier: cheap,
        ..defaults
    };
    let llm = AnthropicClient::new(
        http.clone(),
        AnthropicConfig {
            models,
            ..AnthropicConfig::new(config.providers.llm_api_key.clone())
        },
    );
    let chat_model: Arc<dyn ChatModel> = Arc::new(agent::model::AnthropicChat::new(llm));
    let classifier: Arc<dyn TextClassifier> = match &config.providers.jev {
        Some((base_url, api_key)) => Arc::new(JevClassifier::new(
            http.clone(),
            JevConfig {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                timeout: Duration::from_secs(2),
            },
        )),
        None => Arc::new(agent::model::LlmClassifier::new(
            Arc::clone(&chat_model),
            256,
        )),
    };
    let connect: Arc<dyn ConnectVendors> = Arc::new(connect::VendorClients::new(
        |c| SlackClient::new(http.clone(), c),
        |c| DriveClient::new(http.clone(), c),
        &config.connectors,
        Arc::clone(clock),
    ));
    let farm = farm_sources(
        config,
        |c| SlackClient::new(http.clone(), c),
        |c| DriveClient::new(http.clone(), c),
    );
    let beliefs: Arc<dyn BeliefStore> = match agent_memory(config) {
        Some(c) => Arc::new(AgentMemory::new(http.clone(), c)),
        None => Arc::new(NoBeliefs),
    };
    Ok(Outbound {
        auth: Arc::new(ClerkVerifier::new(http.clone(), verifier)),
        webhooks: Arc::new(
            SvixVerifier::new(&config.clerk.webhook_secret).context("CLERK_WEBHOOK_SECRET")?,
        ),
        auth_admin: Arc::new(ClerkAdmin::new(
            http.clone(),
            ClerkAdminConfig {
                base_url: "https://api.clerk.com/v1".to_owned(),
                secret_key: config.clerk.secret_key.clone(),
                timeout: Duration::from_secs(10),
            },
        )),
        weather: Arc::new(OpenMeteo::new(
            http.clone(),
            OpenMeteoConfig {
                api_key: config.providers.open_meteo_api_key.clone(),
                thresholds: config.weather,
                ..OpenMeteoConfig::default()
            },
            Arc::clone(clock),
        )),
        soil: Arc::new(
            SoilGrids::new(http.clone(), SoilGridsConfig::default(), Arc::clone(clock))
                .with_local_fallback(landscapes),
        ),
        geocoder: Arc::new(CompositeGeocoder::new(
            http.clone(),
            GeocoderConfig::default(),
            zones,
        )),
        notifier: Arc::new(ExpoPush::new(
            http,
            ExpoPushConfig {
                access_token: config.providers.expo_access_token.clone(),
                ..ExpoPushConfig::default()
            },
        )),
        chat_model,
        classifier,
        beliefs,
        connect,
        farm,
    })
}

/// The sync when connectors are on (decision 9 of docs/EON): the `connectors` task and
/// start-up seeding run only with it. It renews Google's tokens through `vendors`, the same
/// instance as the routes, on Google's own token endpoint.
fn farm_sync(
    config: &Config,
    ports: &Ports,
    vendors: Arc<dyn ConnectVendors>,
    sources: FarmSources,
) -> Option<Arc<FarmSync>> {
    config.connectors.key.clone().map(|key| {
        Arc::new(FarmSync::new(
            Arc::clone(&ports.store),
            Arc::clone(&ports.embedder),
            Arc::clone(&ports.clock),
            key,
            vendors,
            sources,
            SyncConfig::default(),
        ))
    })
}

/// The farm connectors' adapters: the real Slack, with Tier 1 pacing, and Google Drive for
/// owners' connectors; Era's simulators, without pacing or back-off, for the seeded `era` ones
/// when `ERA_SLACK_BASE` and `ERA_GOOGLE_BASE` are set.
fn farm_sources(
    config: &Config,
    slack_client: impl Fn(SlackConfig) -> SlackClient,
    drive_client: impl Fn(DriveConfig) -> DriveClient,
) -> FarmSources {
    let slack = |c: SlackConfig| Arc::new(SlackSource::new(slack_client(c)));
    let drive = |c: DriveConfig| Arc::new(DriveSource::new(drive_client(c)));
    let mut sources = FarmSources::default()
        .with(ConnectorEndpoint::Vendor, slack(SlackConfig::default()))
        .with(ConnectorEndpoint::Vendor, drive(DriveConfig::default()));
    if let Some(base) = &config.connectors.era_slack_base {
        sources = sources.with(
            ConnectorEndpoint::Era,
            slack(SlackConfig::era(base.clone())),
        );
    }
    if let Some(base) = &config.connectors.era_google_base {
        sources = sources.with(
            ConnectorEndpoint::Era,
            drive(DriveConfig::era(base.clone())),
        );
    }
    sources
}

/// Agent Memory's settings when it is configured, logging whether it is on; the key is never
/// logged.
fn agent_memory(config: &Config) -> Option<AgentMemoryConfig> {
    let Some(m) = &config.providers.agent_memory else {
        tracing::info!("agent memory off: AGENT_MEMORY_URL is not set");
        return None;
    };
    tracing::info!(url = %m.url, context = %m.context_id, "agent memory on");
    Some(AgentMemoryConfig::new(
        m.url.clone(),
        m.context_id.clone(),
        m.api_key.clone(),
    ))
}

/// Logs whether farm connectors are on, which vendors and simulators are configured, and a
/// warning when vendor or Era settings are set without `CONNECTOR_KEY`, which turns them off.
/// Neither the key nor a client secret is logged.
fn log_connectors(config: &Config) {
    let c = &config.connectors;
    let (slack, google) = (c.slack.is_some(), c.google.is_some());
    let era = c.era_slack_base.is_some() || c.era_google_base.is_some();
    if c.key.is_none() {
        if slack || google || era {
            tracing::warn!(
                slack,
                google,
                era,
                "farm connectors off: CONNECTOR_KEY is not set, so the other connector settings are unused"
            );
        } else {
            tracing::info!("farm connectors off: CONNECTOR_KEY is not set");
        }
        return;
    }
    tracing::info!(
        slack = c.slack.as_ref().map(|s| s.client_id.as_str()),
        google = c.google.as_ref().map(|g| g.client_id.as_str()),
        return_url = %c.return_url,
        era_slack = c.era_slack_base.is_some(),
        era_google = c.era_google_base.is_some(),
        "farm connectors on"
    );
}

/// Builds every port and the state, in order: the databases (schema as root, then connect as
/// `api`; the knowledge index), the object store, the outbound adapters, the CPU models, the
/// species photo index, then `api`'s own pieces and the pipeline and agent that need the writer.
#[expect(
    clippy::too_many_lines,
    reason = "every port and in-process piece is built here, in start-up order"
)]
pub async fn build(
    config: Config,
    shutdown: CancellationToken,
    tasks: TaskTracker,
) -> anyhow::Result<Arc<AppState>> {
    let config = Arc::new(config);
    let dbs = connect_databases(&config).await?;
    let blob = blob_store(&config);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Arc<dyn IdGen> = Arc::new(UlidGen);
    let out = outbound(&config, &clock).await?;
    log_connectors(&config);
    let models = load_models(&config.models_dir).await.context("models")?;
    let snapshot = dbs
        .knowledge_repo
        .load_knowledge()
        .await
        .context("species photos")?;
    let products = Arc::new(agent::safety::ProductCatalogue::new(
        &dbs.knowledge.products().await?,
    ));
    let store = dbs.store;
    let ports = Ports {
        store: Arc::clone(&store),
        memory: dbs.memory,
        beliefs: Arc::clone(&out.beliefs),
        knowledge: dbs.knowledge,
        blob: Arc::clone(&blob),
        auth: out.auth,
        webhooks: out.webhooks,
        weather: out.weather,
        soil: out.soil,
        geocoder: out.geocoder,
        notifier: Arc::clone(&out.notifier),
        embedder: models.embedder,
        signer: Arc::new(HmacSigner::new(
            config.url_signing_key.clone(),
            config.public_base.clone(),
        )),
        photos: Arc::new(PhotoIndex::new(snapshot.species_photos)),
        clock: Arc::clone(&clock),
        ids,
    };
    let farm_sync = farm_sync(&config, &ports, Arc::clone(&out.connect), out.farm);
    let hub = Arc::new(EventHub::default());
    let broker = Arc::new(JobBroker::new(
        store.clone(),
        Arc::clone(&clock),
        dbs.jobs_queued,
    ));
    let pusher = Pusher::new(Arc::clone(&store), out.notifier);
    let writers = Arc::new(ScanWriters::new(
        Arc::clone(&store),
        Arc::clone(&clock),
        Arc::clone(&hub),
        pusher.clone(),
        tasks.clone(),
        shutdown.clone(),
    ));
    let pipeline = pipeline_api(
        &config,
        &ports,
        &writers,
        models.measurer,
        &out.chat_model,
        &products,
    );
    let chat = chat_turns(
        &ports,
        out.chat_model,
        out.classifier,
        products,
        tasks.clone(),
    );
    // The one delete queue: every RustFS delete in `api` goes through it, and `main` drains it.
    let deletes = blob::DeleteQueue::new(Arc::clone(&blob));
    let mut deletions = DeletionRunner::new(
        store,
        deletes.clone(),
        out.auth_admin,
        pusher,
        clock,
        tasks.clone(),
        shutdown.clone(),
    );
    // Connectors on: the deletion runner revokes through the routes' own vendor seam.
    if let Some(key) = &config.connectors.key {
        deletions = deletions.with_revoke(key.clone(), Arc::clone(&out.connect));
    }
    Ok(Arc::new(AppState {
        limits: RateLimits::new(config.rate_per_minute),
        upload_permits: Arc::new(Semaphore::new(config.upload_permits.max(1))),
        workers: WorkerTokens::new(config.worker_tokens.clone()),
        config,
        ports,
        pipeline,
        chat,
        images: models.images,
        hub,
        writers,
        broker,
        deletions,
        deletes,
        connect: out.connect,
        farm_sync,
        shutdown,
        tasks,
    }))
}
```

**Agent Memory.** `outbound` builds `integrations::agent_memory::AgentMemory` over the shared `reqwest` client when the three variables are set, and `NoBeliefs` otherwise, logging which at start (the URL and context, never the key). `Ports.beliefs` holds it, and goes to the agent's and the pipeline's deps for their reads. No request writes to Agent Memory: the `beliefs` task records what the requests left unsynced or queued (section 16; [agent 14](agent.md#beliefs-in-agent-memory)).

**One embedder, two modes.** The text embedder (snowflake-arctic-embed-xs) embeds queries and documents differently, and every `TextEmbedder::embed` call names its `EmbedMode` ([domain 9](domain.md#9-boundary-traits)). So `api` loads one embedder and shares one `Arc<dyn TextEmbedder>` (`Ports.embedder`): garden notes are embedded as `Document`, the pipeline's memory summaries as `Document`, and the agent embeds the user's message and its searches as `Query` and the memories it writes as `Document`.

The two synchronous ports `api` implements itself:

```rust
/// HMAC-SHA256 over `SigningInput::canonical()` (`"GET\n{path}\n{exp}"`), lowercase hex
/// (`domain::url`; domain open question 11). The key is `API_URL_SIGNING_KEY`.
pub struct HmacSigner {
    key: SecretString,
    public_base: String,
}

/// The species reference photos, held in memory and refreshed only by a restart (the knowledge
/// base changes only with a migration). Loaded from `KnowledgeRepo::load_knowledge` at start.
pub struct PhotoIndex(HashMap<SpeciesId, Vec<SpeciesPhoto>>);
```

```rust
impl UrlSigner for HmacSigner {
    fn signature(&self, input: &SigningInput) -> String {
        hex(&self.mac(input).finalize().into_bytes())
    }

    fn verify(&self, input: &SigningInput, sig: &str) -> bool {
        // `verify_slice` compares in constant time and checks the length.
        unhex(sig).is_some_and(|bytes| self.mac(input).verify_slice(&bytes).is_ok())
    }

    fn public_base(&self) -> &str {
        &self.public_base
    }
}
```

---

## 7. The router and the layer stack

Every route is registered through utoipa's `OpenApiRouter` and `routes!`, which reads each handler's `#[utoipa::path]` and hands axum the same path string, so the spec can't drift from the routes. Paths use axum 0.8's `{id}` captures (`/:id` panics at router build). `routes!(a, b)` takes several handlers only when they share one path.

```rust
/// The claim long-poll waits up to 30 s ([contracts 5]); its group's timeout leaves 5 s more.
pub const CLAIM_TIMEOUT: Duration = Duration::from_secs(35);

/// Uploads run the CPU pipeline; worker completions carry large results; a connect's
/// `complete` waits on two vendor calls.
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(60);

/// Worker result bodies (perception masks and embeddings for every image).
pub const WORKER_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// JSON bodies of the app and claim routes. Each group sets its own limit, since a group without
/// one gets axum's 2 MiB default.
pub const JSON_BODY_LIMIT: usize = 1024 * 1024;

/// Every route, grouped. A group's `.layer` wraps only the routes registered in it, which is why
/// the upload and worker routes are built as their own `OpenApiRouter`s and merged.
pub fn routes(request_timeout: Duration) -> OpenApiRouter<SharedState> {
    let app = OpenApiRouter::new()
        .routes(routes!(scans::create_scan))
        .routes(routes!(scans::get_scan, scans::delete_scan))
        .routes(routes!(scans::delete_image, media::scan_image))
        .routes(routes!(media::scan_mask))
        .routes(routes!(scans::submit))
        .routes(routes!(scans::answer))
        .routes(routes!(scans::feedback))
        .routes(routes!(scans::events))
        .routes(routes!(gardens::list_gardens, gardens::create_garden))
        .routes(routes!(
            gardens::get_garden,
            gardens::update_garden,
            gardens::delete_garden
        ))
        .routes(routes!(gardens::garden_weather))
        .routes(routes!(gardens::list_members))
        .routes(routes!(gardens::update_member, gardens::remove_member))
        .routes(routes!(gardens::list_invites, gardens::create_invite))
        .routes(routes!(gardens::revoke_invite))
        .routes(routes!(gardens::accept_invite))
        .routes(routes!(gardens::get_map, gardens::put_map))
        .routes(routes!(plants::list_plants, plants::create_plant))
        .routes(routes!(
            plants::get_plant,
            plants::update_plant,
            plants::delete_plant
        ))
        .routes(routes!(plants::timeline))
        .routes(routes!(plants::plant_beliefs))
        .routes(routes!(plants::measurements))
        .routes(routes!(plants::confirm_species))
        .routes(routes!(species::list_species))
        .routes(routes!(species::get_species))
        .routes(routes!(media::species_photo))
        .routes(routes!(tasks::list_tasks, tasks::create_task))
        .routes(routes!(tasks::update_task, tasks::delete_task))
        .routes(routes!(
            account::get_me,
            account::update_me,
            account::delete_me
        ))
        .routes(routes!(account::register_push_token))
        .routes(routes!(account::remove_push_token))
        .routes(routes!(memories::list_memories, memories::create_memory))
        .routes(routes!(memories::share_memory))
        .routes(routes!(memories::delete_memory))
        .routes(routes!(chat::create_conversation, chat::list_conversations))
        .routes(routes!(chat::get_conversation))
        .routes(routes!(media::chat_image))
        .routes(routes!(chat::send_message))
        .routes(routes!(chat::message_feedback))
        .routes(routes!(chat::confirm_pending_action))
        .routes(routes!(chat::reject_pending_action))
        .routes(routes!(
            connectors::list_connectors,
            connectors::create_connector
        ))
        .routes(routes!(connectors::oauth_callback))
        .routes(routes!(connectors::available_sources))
        .routes(routes!(connectors::set_sources))
        .routes(routes!(connectors::sync_connector))
        .routes(routes!(connectors::delete_connector))
        .routes(routes!(ops::healthz))
        .routes(routes!(ops::status))
        .routes(routes!(ops::auth_webhook))
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(timeout(request_timeout));
    let uploads = OpenApiRouter::new()
        .routes(routes!(scans::upload_image))
        .routes(routes!(chat::upload_chat_image))
        .layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT))
        .layer(timeout(SLOW_TIMEOUT));
    // `complete` exchanges a code in two vendor calls (Slack's 20 s deadline each, Google's
    // 30 s), so it gets the slow routes' timeout; its work runs on in a task when even that
    // passes (contracts 9, farm connectors).
    let connect = OpenApiRouter::new()
        .routes(routes!(connectors::complete_connector))
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(timeout(SLOW_TIMEOUT));
    let claim = OpenApiRouter::new()
        .routes(routes!(internal::claim))
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .layer(timeout(CLAIM_TIMEOUT));
    let worker = OpenApiRouter::new()
        .routes(routes!(internal::heartbeat))
        .routes(routes!(internal::job_events))
        .routes(routes!(internal::complete))
        .routes(routes!(internal::job_image))
        .layer(DefaultBodyLimit::max(WORKER_BODY_LIMIT))
        .layer(timeout(SLOW_TIMEOUT));
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(app)
        .merge(uploads)
        .merge(connect)
        .merge(claim)
        .merge(worker)
}
```

**Groups.** Auth is in the extractors, so groups differ only in what a `.layer` on them sets. `OpenApiRouter::layer` wraps every route registered *so far* in that router, so each group is its own `OpenApiRouter`, layered, then merged (✓ the research probe found that a body limit on a shared router reached every route):

| Group | Routes | Timeout | Body limit |
|---|---|---|---|
| app | everything else, both SSE streams, media, ops, the webhook | `REQUEST_TIMEOUT_S` (10 s) | 1 MiB |
| uploads | `POST /scans/{id}/images`, `POST /conversations/{id}/images` | 60 s | 12 MiB + 2 × 4 MiB + 128 KiB |
| connect | `POST /connectors/{id}/complete` (two vendor calls, 20 s or 30 s deadlines each; section 11) | 60 s | 1 MiB |
| claim | `POST /internal/jobs/claim` | 35 s (the long poll waits up to 30 s) | 1 MiB |
| worker | heartbeat, events, complete, job images | 60 s | 16 MiB |

`TimeoutLayer` bounds only the time to the response **head** (✓ research probe: an SSE body kept flowing past a 1 s layer), so it is safe on the SSE routes. `TimeoutLayer::with_status_code` is used because `TimeoutLayer::new` is deprecated since tower-http 0.6.7.

```rust
/// `req_<ULID>` request ids ([contracts 2], `request_id`).
#[derive(Debug, Clone, Copy)]
pub struct MakeReqId;

/// CORS for the web build ([frontend 17]): the configured origins only, exactly the four request
/// headers, and `x-request-id` and `retry-after` readable by the app.
pub fn cors(origins: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static("last-event-id"),
        ])
        .expose_headers([HeaderName::from_static("x-request-id"), header::RETRY_AFTER])
        .max_age(Duration::from_secs(600))
}

/// The whole application: the routes, `/openapi.json`, the fallback and the global stack, in
/// request order (`ServiceBuilder` applies top to bottom, the first layer seeing the request first):
///
/// | Layer | Why there |
/// |---|---|
/// | drop a client's `x-request-id`, set `req_<ULID>`, propagate it | first, so the span, every layer and every error body see one id, and every response carries it |
/// | `TraceLayer` | outside everything that answers, so the span records the final status |
/// | `CorsLayer` | answers preflights (which carry no `Authorization`) before anything else runs |
/// | `CompressionLayer` | after the error renderer has written its body; skips `text/event-stream` (frames would sit in the encoder) and images |
/// | `render_errors` | writes every error body; everything below it, tower-http's own responses included, gets an `ErrorBody` |
/// | `CatchPanicLayer` | inside the renderer, so a panic's `internal` body gets its `request_id` |
/// | `RequestBodyLimitLayer` | the global byte cap; each group's `DefaultBodyLimit` is no larger and answers with its own number |
pub fn app(state: SharedState) -> Router {
    let cors = cors(&state.config.web_origins);
    let (router, api) = routes(state.config.request_timeout).split_for_parts();
    let spec = Json(api);
    let compression = CompressionLayer::new().compress_when(
        DefaultPredicate::new().and(NotForContentType::const_new("text/event-stream")),
    );
    router
        .route("/openapi.json", get(move || async move { spec.clone() }))
        .fallback(|| async { crate::error::ApiError::not_found() })
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .map_request(|mut request: Request| {
                    request.headers_mut().remove("x-request-id");
                    request
                })
                .layer(SetRequestIdLayer::x_request_id(MakeReqId))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &Request| {
                            let request_id = request
                                .extensions()
                                .get::<RequestId>()
                                .and_then(|id| id.header_value().to_str().ok())
                                .unwrap_or_default();
                            // The matched route, never the raw path: ids and push tokens stay out of logs.
                            let route = request
                                .extensions()
                                .get::<axum::extract::MatchedPath>()
                                .map_or("unmatched", axum::extract::MatchedPath::as_str);
                            tracing::info_span!("request", %request_id, method = %request.method(), route)
                        })
                        .on_response(DefaultOnResponse::new().level(Level::INFO)),
                )
                .layer(cors)
                .layer(compression)
                .layer(middleware::from_fn(render_errors))
                .layer(CatchPanicLayer::custom(panic_response))
                .layer(RequestBodyLimitLayer::new(WORKER_BODY_LIMIT.max(UPLOAD_BODY_LIMIT))),
        )
}
```

**Why that order.** `ServiceBuilder` applies its layers top to bottom: the first one sees the request first and the response last.

| # | Layer | Why there |
|---|---|---|
| 1–3 | drop a client-sent `x-request-id`; set `req_<ULID>`; propagate it to the response | First, so the trace span, every layer and every error body see one id, and every response, preflights and errors included, carries it ([contracts 2](../architecture/contracts.md#2-errors), `request_id`). A client can't choose the id that ends up in our logs |
| 4 | `TraceLayer`, span `request` with `request_id`, method and the **matched route** | Outside everything that answers, so the span records the final status. The matched route, never the raw path: ids and push tokens stay out of the logs |
| 5 | `CorsLayer` | Answers every preflight itself (a preflight carries no `Authorization`). The configured origins only, never `Any`; exactly `Authorization`, `Content-Type`, `Idempotency-Key` and `Last-Event-ID` ([frontend 17](../frontend/README.md#17-builds-environments-and-delivery)); `x-request-id` and `retry-after` exposed so the web build can read them |
| 6 | `CompressionLayer` with `DefaultPredicate` and `NotForContentType("text/event-stream")` | Outside the error renderer, so error bodies are compressed too. tower-http 0.7.1's default predicate already skips SSE, gRPC, `image/*` and tiny bodies (✓ research probe); the explicit SSE rule keeps the intent if the default changes, because a compressor buffers SSE frames |
| 7 | `render_errors` | Writes every error body (section 8); everything inside it, tower-http's own responses included, gets an `ErrorBody` |
| 8 | `CatchPanicLayer::custom(panic_response)` | Inside the renderer, so a panic's `internal` body gets its `request_id` |
| 9 | `RequestBodyLimitLayer` (20 MiB + 128 KiB) | The global byte cap, `WORKER_BODY_LIMIT.max(UPLOAD_BODY_LIMIT)`: the upload limit (12 MiB photo, two 4 MiB depth maps, 128 KiB for the meta and multipart overhead), which is larger than the worker routes' 16 MiB. Each group's `DefaultBodyLimit` is no larger and answers with its own number |

`/openapi.json` serves the generated document (utoipa 6 emits OpenAPI 3.1.0); `api openapi` prints the same document for `backend/openapi.json`, from which `openapi-typescript` generates the app's types ([frontend 9](../frontend/README.md#9-api-layer)). The file is committed, and the test `the_committed_openapi_document_matches_the_types` fails when it no longer matches what the `domain` types generate, so a contract change can't leave the app's side behind; regenerate it with `cargo run -p api -- openapi > openapi.json` in `backend/`. ✓ Generated: 50 paths; each `Page<T>` instantiation gets its own component (`Page_Garden`, `Page_Plant`, …), which the research probe had left unverified. The frame:

```rust
/// The document's frame. `OpenAPI` 3.1 (utoipa 6's default); every path needs the Clerk bearer
/// token unless it declares `security(())` (signed media, ops, the webhook) or is a worker route.
#[derive(OpenApi)]
#[openapi(
    info(title = "PDM API", description = "Plant, crop and garden health monitoring."),
    modifiers(&BearerAuth),
    security(("clerk" = [])),
    components(schemas(domain::chat::AnchorType, domain::records::task::TaskState)),
    tags(
        (name = "scans", description = "contracts 3 and 4"),
        (name = "gardens", description = "contracts 9"),
        (name = "members", description = "contracts 9, members and invites"),
        (name = "map", description = "contracts 9, garden map"),
        (name = "plants", description = "contracts 9, plants"),
        (name = "species", description = "contracts 9, species"),
        (name = "tasks", description = "contracts 9, tasks"),
        (name = "account", description = "contracts 9, account"),
        (name = "memories", description = "contracts 8, memory"),
        (name = "chat", description = "contracts 8"),
        (name = "connectors", description = "farm connectors, docs/connectors 5 and 11"),
        (name = "media", description = "signed image URLs, storage 3"),
        (name = "worker", description = "contracts 5; worker token"),
        (name = "ops", description = "contracts 9, ops and auth webhook"),
    )
)]
pub struct ApiDoc;
```

---

## 8. Errors

Every non-2xx response carries the `ErrorBody` of [contracts 2](../architecture/contracts.md#2-errors). `IntoResponse` can't see the request, so it can't know the `request_id`; the body is therefore written in one place.

```rust
/// An error response: a contracts 2 code with its details, a message safe to show, `retryable`,
/// `retry_after_s`, and, for the log only, the cause. Boxed, so a `Result<T, ApiError>` stays
/// one pointer wider than `T` however large the details get.
pub struct ApiError(Box<Inner>);

/// What [`render_errors`] needs to write the body, left in the response's extensions.
#[derive(Debug, Clone)]
pub struct ErrorParts {
    kind: ErrorKind,
    message: Cow<'static, str>,
    retryable: bool,
    retry_after_s: Option<u32>,
    /// The cause chain, rendered once, for the log line of a `5xx`.
    cause: Option<String>,
}
```

```rust
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let inner = *self.0;
        let mut response = status.into_response();
        if let Some(seconds) = inner.retry_after_s {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response.extensions_mut().insert(ErrorParts {
            cause: inner.cause.as_deref().map(|e| chain(e)),
            kind: inner.kind,
            message: inner.message,
            retryable: inner.retryable,
            retry_after_s: inner.retry_after_s,
        });
        response
    }
}
```

```rust
/// The middleware that writes every error body. It sits inside the request-id layers and the
/// trace span, and outside everything that can answer with an error.
pub async fn render_errors(request: Request, next: Next) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("req_unknown")
        .to_owned();
    let mut response = next.run(request).await;
    let parts = match response.extensions_mut().remove::<ErrorParts>() {
        Some(parts) => parts,
        None if response.status().as_u16() < 400 || is_json(&response) => return response,
        None => {
            // axum's 404/405 and rejections, tower-http's 413 and timeouts.
            let error = from_status(response.status());
            *response.status_mut() = error.status();
            response.headers_mut().remove(header::ALLOW);
            let inner = *error.0;
            ErrorParts {
                cause: None,
                kind: inner.kind,
                message: inner.message,
                retryable: inner.retryable,
                retry_after_s: inner.retry_after_s,
            }
        }
    };
    if let Some(cause) = &parts.cause {
        // contracts 2: `internal` is logged with request_id, as a field of the event itself.
        if parts.kind.code() == ErrorCode::Internal {
            tracing::error!(request_id, error = %cause, "internal error");
        } else {
            tracing::warn!(request_id, code = %parts.kind.code(), error = %cause, "request failed");
        }
    }
    let body = ErrorBody {
        error: ErrorObject {
            kind: parts.kind,
            message: parts.message.into_owned(),
            retryable: parts.retryable,
            retry_after_s: parts.retry_after_s,
            request_id,
        },
    };
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Here, not only in `into_response`, so the errors built from a bare status (the timeout's
    // 503) carry the header too.
    if let Some(seconds) = body.error.retry_after_s {
        headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    }
    headers.remove(header::CONTENT_LENGTH);
    *response.body_mut() = Body::from(bytes);
    response
}

/// The error for a response no `ApiError` made, chosen from its status.
pub fn from_status(status: StatusCode) -> ApiError {
    match status.as_u16() {
        401 => ApiError::unauthenticated(),
        // An unknown path and a known path with the wrong method both look like an unknown ID.
        404 | 405 => ApiError::not_found(),
        413 => ApiError::new(ErrorKind::UploadTooLarge {
            max_bytes: IMAGE_MAX_BYTES,
        }),
        415 => ApiError::new(ErrorKind::UnsupportedMediaType {
            accepted: ACCEPTED_MEDIA_TYPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }),
        400 | 422 => ApiError::invalid_request("body", "the request body could not be read"),
        // Only the timeout layer answers 503 without an `ApiError` (contracts 2, timeouts).
        503 => ApiError::new(ErrorKind::UpstreamUnavailable {
            upstream: "api".to_owned(),
        })
        .message("The request took too long. Please try again.")
        .retry_after(5),
        _ => ApiError::new(ErrorKind::Internal),
    }
}

/// `CatchPanicLayer::custom`: a panic becomes `500 internal`, rendered by [`render_errors`].
#[expect(
    clippy::needless_pass_by_value,
    reason = "CatchPanicLayer::custom passes the payload by value"
)]
pub fn panic_response(panic: Box<dyn Any + Send + 'static>) -> Response {
    let what = panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned());
    ApiError::internal(format!("handler panicked: {what}")).into_response()
}
```

| Step | Who | What |
|---|---|---|
| 1 | `SetRequestIdLayer` | Puts `RequestId` (`req_<ULID>`) in the request's extensions |
| 2 | `ApiError::into_response` | The status, `Retry-After` when there is one, and an `ErrorParts` in the **response** extensions; the cause chain rendered once, for the log |
| 3 | `render_errors` | Reads `RequestId` on the way in; on the way out, if the response holds `ErrorParts`, writes the JSON with that `request_id`, and logs a `5xx` cause with `request_id` as a field of the event itself (a filter that drops the span would drop the span's fields) |
| 4 | `render_errors`, fallback | A non-2xx response with no `ErrorParts` and no JSON body (axum's 404 and 405 and rejections, tower-http's 413 and timeouts) gets the body `from_status` picks. `render_errors` writes `Retry-After` from the body's `retry_after_s` on both paths, so the timeout's bare `503` gets `Retry-After: 5` like any other `503` (✓ `a_timeout_503_carries_retry_after_like_any_other`). A stored idempotent replay of a `4xx` is already JSON, so it is left alone |

`ApiError` is a boxed struct: a `Result<T, ApiError>` is one pointer wider than `T`, however large the details (clippy's `result_large_err` flagged 83 sites before the box). Its `kind` is `domain::error::ErrorKind`, the code with its typed details, so every contracts 2 code exists exactly once and can't be paired with the wrong details.

**The mapping table** (every lower error to one code, the `From` impls in `error.rs`):

| HTTP | `code` | When, in `api` | From |
|---|---|---|---|
| 400 | `invalid_request` | Malformed JSON, a missing, wrong-typed or **unknown** field (`details.field` is the JSON path from `serde_path_to_error`); a bad query parameter; a bad header (`Idempotency-Key`, `Last-Event-ID`, `X-Lease-Token`, named as the field); a JSON body over 1 MiB (`field: "body"`); a rule serde can't express; a place the geocoder can't find (`field: "location.postal_code"` and the like); map pins naming unknown plants (`problem: "unknown_plant"`, `plant_ids`) | `JsonRejection`, `QueryRejection`, `MultipartError`, `DomainError::InvalidRequest`, `DomainError::UnknownPlants` |
| 401 | `unauthenticated` | No bearer token, a token the `AuthVerifier` refuses; a worker token that matches no worker, or whose worker isn't the body's `worker_id`; a webhook whose Svix signature or timestamp fails | `AuthError::InvalidToken`, `AuthError::InvalidWebhook` |
| 403 | `forbidden` | A member below the route's role, with `required_role` | `AccessError::Forbidden`, `DomainError::Forbidden` |
| 403 | `invalid_signature` | A signed image URL expired, too far ahead, or tampered | `SignatureError` |
| 404 | `not_found` | An unknown id, a malformed path id, a record being deleted, a non-member, someone else's conversation or private memory, an unknown path or method | `AccessError::NotFound`, `StoreError::NotFound`, `PathRejection`, `BlobError::NotFound` |
| 409 | `invalid_state` | The state machine refuses (`status`, `allowed`); a non-retryable failed scan resubmitted (`hint: "new_scan"`); a stale plan (`current_plan_id`); a stale map (`current_map_version`); a card whose plant or neighbour was deleted (`hint`); a plan or alert task deleted | `DomainError::InvalidState`, `DomainError::Transition`, `Conflict::StalePlan`, `Conflict::StaleMap`, `Conflict::AlreadyExists` |
| 409 | `request_in_progress` | Same `Idempotency-Key` still running; `retry_after_s: 2` | `IdempotencyBegin::InProgress` |
| 409 | `lease_lost` | A worker call about a requeued, expired, cancelled or older-run job, with `job_id` | `Conflict::LeaseLost` |
| 413 | `upload_too_large` | A photo over 12 MiB (`max_bytes: 12582912`); a body over the global cap (20 MiB + 128 KiB) | the upload reader, `MultipartError` 413, the body-limit layer |
| 415 | `unsupported_media_type` | Not JPEG, PNG, WebP or HEIC by its declared type or its bytes; an undecodable HEIC | `UploadError::UnsupportedMedia` |
| 422 | `image_quality` | The blur or exposure check failed; `problem`, `blur_score`, `exposure`, `hint`; `retryable: true` (a retake succeeds) | `UploadError::Quality` |
| 422 | `missing_required_shot` | Submit without `whole_plant` | `DomainError::MissingRequiredShot` |
| 422 | `too_many_images` | A kind at its limit | `DomainError::TooManyImages` |
| 422 | `idempotency_conflict` | The same key with another fingerprint | `IdempotencyBegin::Conflict` |
| 429 | `rate_limited` | A governor limit; `retry_after_s` rounded up, never 0 | `limits::check` |
| 429 | `budget_exceeded` | The daily scan limit (in `create_scan`'s transaction) or the chat token budget (the agent's `start_turn`); `retryable: false`, `budget`, `resets_at` | `DomainError::BudgetExceeded` |
| 500 | `internal` | A bug: a query error, a panic, a join error, a blob refusal | `StoreError::Query`, `BlobError::Rejected`, `ModelError` other than `Unavailable` |
| 503 | `processing_unavailable` | Face blurring or encoding failed, or the photo's blob put failed; nothing stored; `retry_after_s: 2` | `UploadError::FaceBlur`, `UploadError::Encode` |
| 503 | `upstream_unavailable` | `SurrealDB` (`database`), `RustFS` (`storage`), Clerk's JWKS (`auth`), the LLM, Open-Meteo, the geocoder; no live worker has the run's label versions (`perception`); a request past its timeout (`api`). `retryable: true`, `retry_after_s` | `StoreError::Unavailable`, `Conflict::Retries`, `BlobError::Unavailable`, `ProviderError`, `ModelError::Unavailable`, `AuthError::Unavailable`, `PipelineError::LabelsNotLoaded`, the timeout layer |

`Conflict::ClaimLost` and `Conflict::QuestionStepLost` are the pipeline's own: a step that loses its claim stops silently, so they reach HTTP only through a bug, as `409 invalid_state`.

**Worker job errors are not HTTP errors**: a failed job travels in the `complete` body ([contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)), and the worker gets `204`.

---

## 9. Authentication and access

### Extractors and the state generic

Every extractor implements axum 0.8's `FromRequestParts<S>`, whose method is a native `async fn` in the impl (the trait declares `fn … -> impl Future<Output = …> + Send`, return-position `impl Trait` in a trait, stable since Rust 1.75; no `#[async_trait]`). Two bounds appear on every impl:

- **`Arc<AppState>: FromRef<S>`**: the extractor takes what it needs out of any router state that can produce an `Arc<AppState>`. On the real router `S` *is* `Arc<AppState>`, and axum's blanket `impl<T: Clone> FromRef<T> for T` provides the impl; a test router whose state contains one would provide its own. So an extractor doesn't fix the router's state type.
- **`S: Send + Sync`**: the trait's future must be `Send` (axum runs handlers on tokio's multi-threaded runtime), and the future holds `&S` across `.await`, which is `Send` only if `S: Sync`.

No lifetime is spelled: `parts: &mut Parts` and `state: &S` are elided, and the returned future borrows both, which the trait's signature allows.

### `AuthUser`

```rust
/// A signed-in user. Verifies the bearer token through the `AuthVerifier` port and maps its
/// `sub` to a user, creating the user on first sight (`authz::authenticate`: an upsert on
/// `auth_id`, so two first requests racing each other agree; the `user.created` webhook
/// creates nothing, api 22 question 24). A deleted account's token, still valid, is refused
/// with `401` while its tombstone exists, so it never creates the user again. Then the general
/// per-user rate limit. The user is cached in the request's extensions, so a second extractor
/// on the same request (an access extractor and the idempotency extractor) doesn't verify twice.
#[derive(Debug, Clone)]
pub struct AuthUser(pub CurrentUser);
```

```rust
impl<S> FromRequestParts<S> for AuthUser
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<CurrentUser>() {
            return Ok(Self(user.clone()));
        }
        let app = Arc::<AppState>::from_ref(state);
        let token = extract::bearer(parts).ok_or_else(ApiError::unauthenticated)?;
        let user = authz::authenticate(
            &*app.ports.auth,
            &*app.ports.store,
            token,
            app.ports.clock.now(),
        )
        .await?;
        limits::check(&app.limits.per_user, user.id())?;
        parts.extensions.insert(user.clone());
        Ok(Self(user))
    }
}
```

`domain::authz::authenticate` verifies the token through the `AuthVerifier` port (Clerk's JWKS, `exp`, `nbf`, `iss`, `azp` against the web origins) and maps `sub` to a user with `IdentityRepo::upsert_user`, creating the user on first sight: this request is how a user record is made (section 22, question 24). A deleted account's session token stays valid at Clerk for up to its lifetime (60 s by default); while the account's tombstone exists (section 17), `upsert_user` refuses the auth id with `StoreError::NotFound`, checked on the read path and inside the create's transaction, and `authenticate` turns that into `AuthError::AccountDeleted`: `401 unauthenticated`, never a new, empty user. Then the general rate limit, keyed by the user, so an unauthenticated caller can't fill the limiter's key map. The `CurrentUser` is cached in the request's extensions, so `Access` and `IdemJson` on the same request don't verify twice.

### Membership proofs: `Access<R, T>`

```rust
/// Where a record's garden is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locate {
    /// The id is the garden.
    Garden(domain::id::GardenId),
    /// The store looks up the record's garden, skipping records being deleted.
    Resource(GardenResource),
}

/// A table whose records belong to a garden, so a route can be scoped by its `{id}`.
/// Implemented here for the domain's table markers (a local trait on foreign types is allowed by
/// the orphan rule).
pub trait Scoped: Table + Sized + Send + Sync {
    /// How to find the garden of `id`.
    fn locate(id: Id<Self>) -> Locate;
}

/// The caller, the record in `{id}`, and a proof of at least role `R` in its garden.
///
/// - `R: MinRole` is the minimum role, checked at run time against the stored role and carried
///   in the proof's type.
/// - `T: Scoped` is the table of the `{id}` capture (`Access<Editor, table::Scan>` reads a
///   `ScanId`).
#[derive(Debug, Clone)]
pub struct Access<R: MinRole, T: Scoped> {
    /// The caller.
    pub user: CurrentUser,
    /// The proof.
    pub member: Membership<R>,
    /// The record.
    pub id: Id<T>,
}
```

```rust
impl<S, R, T> FromRequestParts<S> for Access<R, T>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
    R: MinRole,
    T: Scoped,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        let id: Id<T> = extract::path_param(parts, state, "id").await?;
        let app = Arc::<AppState>::from_ref(state);
        let store: &dyn Store = &*app.ports.store;
        let member = match T::locate(id) {
            Locate::Garden(garden) => authz::check_membership::<R>(store, &user, garden).await?,
            Locate::Resource(resource) => authz::check_access::<R>(store, &user, resource).await?,
        };
        Ok(Self { user, member, id })
    }
}
```

The role is a type parameter, so the role a route needs is written in its signature (`access: Access<Editor, table::Scan>`), and the repository call it makes only compiles with a proof of at least that role ([domain 8](domain.md#8-authorization-proofs)). `R: MinRole` is checked at run time against the stored role by `authz::check_membership` / `check_access` and carried in `Membership<R>`; `T: Scoped` is the table of the `{id}` capture, which says how to find its garden: the id itself for `table::Garden`, a `GardenResource` lookup for the others (which skips records being deleted). `Scoped` is a local trait implemented for the domain's table markers, which the orphan rule allows.

| Outcome | Status |
|---|---|
| The `{id}` isn't a ULID | `404` |
| No such record, being deleted, or the caller isn't a member | `404`, the same body ✓ (`a_non_member_gets_the_same_404_as_an_unknown_scan`) |
| A member below `R` | `403 forbidden` with `required_role` ✓ (`a_viewer_is_403_with_the_required_role`) |

A garden id in a body or query (`POST /scans`, `GET /plants?garden_id=`) is checked in the handler with `state.member::<R>(&user, garden)`, after the body is parsed. A route whose role depends on the request (removing a member: an owner, or the member themselves) takes `Access<Viewer, …>` and calls the proof's run-time `require::<Owner>()`. A handler that holds `Membership<Editor>` and calls a method that wants `Membership<Viewer>` downgrades with `at_least()` at compile time.

### Conversations: `ConvAccess<R, T>`

```rust
/// A table whose records belong to a conversation: the conversation itself, its pending
/// actions and its messages.
///
/// The method returns `impl Future + Send` rather than being an `async fn`: an `async fn` in a
/// trait has no `Send` bound on its future, and the extractor that awaits it must be `Send`
/// (axum's `FromRequestParts` future is `+ Send`). Implementations may still be written as
/// `async fn`, because their futures are `Send`.
pub trait ConvScoped: Table + Sized + Send + Sync {
    /// The conversation `id` belongs to, or `None` when it names nothing.
    fn conversation(
        store: &dyn Store,
        id: Id<Self>,
    ) -> impl Future<Output = Result<Option<ConversationId>, StoreError>> + Send;
}

/// The caller, the record in `{id}`, and a proof that the caller owns its conversation and is a
/// member of its garden with at least `R` ([contracts 8], access). Anyone else gets `404`.
#[derive(Debug, Clone)]
pub struct ConvAccess<R: MinRole, T: ConvScoped> {
    /// The caller.
    pub user: CurrentUser,
    /// The proof.
    pub owner: ConversationOwner<R>,
    /// The record.
    pub id: Id<T>,
}
```

```rust
impl ConvScoped for table::Conversation {
    // No lookup: a ready future instead of an `async fn` with nothing to await.
    fn conversation(
        _: &dyn Store,
        id: ConversationId,
    ) -> impl Future<Output = Result<Option<ConversationId>, StoreError>> + Send {
        std::future::ready(Ok(Some(id)))
    }
}
```

`ConvScoped::conversation` returns `impl Future + Send` instead of being an `async fn`: an `async fn` in a trait has no `Send` bound on its future, and the extractor that awaits it must be `Send`. Implementations may still be written as `async fn` (their futures are `Send`, and an impl may refine the declared return type). The conversation's own impl does no lookup, so it returns `std::future::ready(..)`, as clippy's `unused_async_trait_impl` suggests. The pending-action and message impls look the conversation up, then `authz::check_conversation::<R>` checks owner **and** membership ([contracts 8](../architecture/contracts.md#access)); anyone else gets `404`.

### Workers

```rust
/// A worker, identified by its bearer token ([`crate::workers::WorkerTokens`]).
#[derive(Debug, Clone)]
pub struct WorkerAuth(pub WorkerId);

/// A worker's hold on the job in the path: the worker token, the `{id}` capture and
/// `X-Lease-Token` ([contracts 5]). The store compares all three inside its write, so a stale
/// token is `409 lease_lost` there, never a separate read here.
#[derive(Debug, Clone)]
pub struct JobLease(pub Lease);
```

```rust
/// The configured tokens, kept only as SHA-256 digests.
#[derive(Clone)]
pub struct WorkerTokens(Vec<(WorkerId, [u8; 32])>);

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    out.copy_from_slice(digest::digest(&digest::SHA256, bytes).as_ref());
    out
}
```

One bearer token per worker process, mapped to its `WorkerId`. The configured tokens are kept as SHA-256 digests, made once at start; the bearer is hashed and its digest compared in constant time with every configured digest (✓ `identifies_only_exact_tokens`). Comparing raw tokens would leak their length, because `verify_slices_are_equal` returns at once when the lengths differ (aws-lc-rs 1.18.2, `constant_time.rs`), while two digests are always 32 bytes. A claim or heartbeat whose body names another worker is `401`. `JobLease` adds the `{id}` capture and `X-Lease-Token`; a missing or malformed header is `400 invalid_request` naming it (question 23). The lease isn't checked here: the store compares token, worker and status inside the write's transaction, so a stale lease is `409 lease_lost` from the write itself, never from a separate read that a requeue could race.

### The auth webhook

`POST /webhooks/auth` reads the raw body as `Bytes` and verifies it through the `WebhookVerifier` port (Svix's HMAC over `id.timestamp.body`, constant time, every `v1` entry, the timestamp within 5 minutes either way) before anything parses it (section 11, `auth_webhook`).

---

## 10. Idempotency

The create calls of [contracts 1](../architecture/contracts.md#idempotency) accept `Idempotency-Key`, stored as `idempotency_key:[user, route, key]` for 24 hours ([database 4](../database/README.md#idempotency-keys-)).

```rust
/// What `begin` decided.
pub enum Begin {
    /// Run the handler; finish the record afterwards.
    Fresh(Begun),
    /// The stored response of an earlier, finished request.
    Replay(Response),
}

/// A started (or absent) idempotency record. Finish it with [`Begun::finish`],
/// [`Begun::complete`] or [`Begun::release`]; if it is dropped unfinished (a panic, a cancelled
/// request), its `Drop` releases the key in a spawned task, so a retry runs again instead of
/// getting `409 request_in_progress` until the key expires.
pub struct Begun {
    scope: Option<IdempotencyScope>,
    repo: Arc<dyn IdempotencyRepo>,
    request_id: String,
}
```

```rust
impl Begun {
    /// A record for a request without the header.
    fn none(repo: Arc<dyn IdempotencyRepo>, request_id: String) -> Self {
        Self {
            scope: None,
            repo,
            request_id,
        }
    }

    /// Stores the handler's result when contracts 1 allows it, and returns the response.
    pub async fn finish<T: Serialize + ToSchema>(
        mut self,
        result: Result<Wire<T>, ApiError>,
    ) -> Response {
        let Some(scope) = self.scope.take() else {
            return result.into_response();
        };
        let stored = match &result {
            Ok(wire) => wire.stored().ok(),
            Err(error) if error.code().is_stored_for_idempotency() => {
                serde_json::to_value(error.body(&self.request_id))
                    .ok()
                    .map(|body| StoredResponse {
                        status: error.code().http_status(),
                        body,
                    })
            }
            Err(_) => None,
        };
        let outcome = match stored {
            Some(response) => self.repo.finish(&scope, response).await,
            None => self.repo.release(&scope).await,
        };
        if let Err(error) = outcome {
            tracing::warn!(error = %error, "idempotency record not finished");
        }
        result.into_response()
    }

    /// Stores a response built outside a `Wire` (the chat replay, `200 { message }`).
    pub async fn complete(mut self, response: StoredResponse) {
        if let Some(scope) = self.scope.take()
            && let Err(error) = self.repo.finish(&scope, response).await
        {
            tracing::warn!(error = %error, "idempotency record not finished");
        }
    }

    /// Releases the key: the next request with it runs again.
    pub async fn release(mut self) {
        if let Some(scope) = self.scope.take()
            && let Err(error) = self.repo.release(&scope).await
        {
            tracing::warn!(error = %error, "idempotency key not released");
        }
    }
}
```

```rust
impl Drop for Begun {
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let repo = Arc::clone(&self.repo);
            runtime.spawn(async move {
                let _ = repo.release(&scope).await;
            });
        }
    }
}
```

```rust
/// Starts the record for `(user, method, path, key)` with `fingerprint`.
pub async fn begin(
    state: &AppState,
    user: UserId,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    request_id: String,
    fingerprint: &str,
) -> Result<Begin, ApiError> {
    let repo: Arc<dyn IdempotencyRepo> = state.ports.store.clone();
    let Some(raw) = headers.get("idempotency-key") else {
        return Ok(Begin::Fresh(Begun::none(repo, request_id)));
    };
    let key: IdempotencyKey = raw
        .to_str()
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::invalid_request("Idempotency-Key", "not a UUID"))?;
    let scope = IdempotencyScope {
        user,
        route: format!("{method} {path}"),
        key,
    };
    let expires_at = state.ports.clock.now().plus(IDEMPOTENCY_TTL);
    match repo.begin(&scope, fingerprint, expires_at).await? {
        IdempotencyBegin::Started => Ok(Begin::Fresh(Begun {
            scope: Some(scope),
            repo,
            request_id,
        })),
        IdempotencyBegin::Replay(stored) => Ok(Begin::Replay(reply::replay(stored))),
        IdempotencyBegin::InProgress => {
            Err(ApiError::new(ErrorKind::RequestInProgress).retry_after(IN_PROGRESS_RETRY_S))
        }
        IdempotencyBegin::Conflict => Err(ApiError::new(ErrorKind::IdempotencyConflict)),
    }
}

/// SHA-256 of a JSON value's canonical form: object keys sorted at every level, no whitespace.
/// Written by hand rather than through `serde_json::to_vec`, whose key order depends on whether
/// any crate in the build turned on `serde_json/preserve_order`.
pub fn canonical_json_hash(value: &serde_json::Value) -> String {
    fn write(hasher: &mut Sha256, value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                hasher.update(b"{");
                for key in keys {
                    write(hasher, &serde_json::Value::String(key.clone()));
                    hasher.update(b":");
                    write(hasher, &map[key]);
                    hasher.update(b",");
                }
                hasher.update(b"}");
            }
            serde_json::Value::Array(items) => {
                hasher.update(b"[");
                for item in items {
                    write(hasher, item);
                    hasher.update(b",");
                }
                hasher.update(b"]");
            }
            scalar => hasher.update(scalar.to_string().as_bytes()),
        }
    }
    let mut hasher = Sha256::new();
    write(&mut hasher, value);
    hex_digest(hasher)
}
```

| Case | Answer |
|---|---|
| No header | Runs normally; nothing is stored |
| New key | Runs; a `2xx` or a deterministic `4xx` (`400`, `403`, `404`, `413`, `415`, `422`, `ErrorCode::is_stored_for_idempotency`) is stored with its status and body; anything else (`409`, `429`, `5xx`) releases the key, so a retry runs again |
| Same key, same fingerprint, finished | The stored status and body, as they were (✓ `a_repeated_key_replays_the_stored_response_without_running_the_handler`: `create_scan` is never called) |
| Same key, same fingerprint, still running | `409 request_in_progress`, `retryable`, `retry_after_s: 2`, `Retry-After: 2` (✓ test) |
| Same key, different fingerprint | `422 idempotency_conflict` |
| Chat message | Completed: stored as `200 { message }` when the turn task reports it (section 14). Interrupted or failed: the key is released |

**The scope** is `(user, "METHOD /actual/path", key)`: the path with its ids, so the same key on two scans is two records.

**Fingerprints.** A JSON body's is a SHA-256 of its canonical form: keys sorted at every level, written by hand rather than with `serde_json::to_vec`, whose key order depends on whether any crate in the build turns on `serde_json/preserve_order` (feature unification would change it silently). A body that isn't JSON is hashed raw. A multipart body's covers its fields and file bytes, never the raw body, whose boundary changes on every retry (section 12).

**A stored error** carries the `request_id` of the request that produced it: the replay is the original response, as contracts 1 says.

**Never stuck.** A handler that panics, times out (the timeout layer drops its future) or loses its client drops its `Begun` unfinished; `Drop` releases the key in a spawned task (an `async` call can't run in `Drop`), so the retry runs instead of getting `409 request_in_progress` for 24 hours.

```rust
/// A JSON body under an idempotency record: the extractor reads the body, fingerprints it,
/// begins the record, then deserialises `T`. A replay or a refusal is the rejection, so the
/// handler runs only for a fresh request.
pub struct IdemJson<T> {
    /// The caller (the extractor authenticates first, to scope the key).
    pub user: domain::authz::CurrentUser,
    /// The body.
    pub body: T,
    /// The record to finish.
    pub begun: Begun,
}

/// `IdemJson`'s rejection: an error, or the stored response of a finished request.
pub enum IdemRejection {
    /// An error response.
    Error(ApiError),
    /// The replay.
    Replay(Response),
}
```

```rust
impl<S, T> FromRequest<S> for IdemJson<T>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = IdemRejection;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (mut parts, body) = request.into_parts();
        let AuthUser(user) = AuthUser::from_request_parts(&mut parts, state).await?;
        let app = Arc::<AppState>::from_ref(state);
        let request_id = extract::request_id(&parts);
        let bytes = axum::body::to_bytes(body, JSON_MAX_BYTES)
            .await
            .map_err(|_| ApiError::invalid_request("body", "at most 1 MiB of JSON"))?;
        let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
        let fingerprint = if let Ok(value) = &parsed {
            canonical_json_hash(value)
        } else {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            hex_digest(hasher)
        };
        let begun = match begin(
            &app,
            user.id(),
            &parts.method,
            parts.uri.path(),
            &parts.headers,
            request_id,
            &fingerprint,
        )
        .await?
        {
            Begin::Fresh(begun) => begun,
            Begin::Replay(response) => return Err(IdemRejection::Replay(response)),
        };
        let decoded = parsed
            .map_err(|e| ApiError::invalid_request("body", e.to_string()))
            .and_then(|value| {
                serde_path_to_error::deserialize::<_, T>(value).map_err(|e| {
                    let path = e.path().to_string();
                    let field = if path == "." { "body".to_owned() } else { path };
                    ApiError::invalid_request(field, e.inner().to_string())
                })
            });
        match decoded {
            Ok(body) => Ok(Self { user, body, begun }),
            // A malformed body is a deterministic 400: stored like any other.
            Err(error) => Err(IdemRejection::Replay(
                begun
                    .finish::<domain::garden::ops::Healthz>(Err(error))
                    .await,
            )),
        }
    }
}
```

`IdemJson<T>` is a body extractor (`FromRequest`, so the last argument), generic over the body type `T: DeserializeOwned + Send`. It authenticates first (the key is per user), fingerprints, begins the record, and only then decodes `T`; a replay or a refusal is its *rejection*, an `IdemRejection` that is either an error or the stored `Response`, so the handler runs only for a fresh request. A body that fails to decode is a deterministic `400`, stored like any other.

---

## 11. Endpoint index

Every endpoint of [architecture 9](../architecture/README.md#9-api-surface) and the contracts. "Role" is the minimum role in the garden, or who may call. "Idem." marks the create calls that take `Idempotency-Key`.

| Method | Path | Handler | Role | Idem. | Contract |
|---|---|---|---|---|---|
| POST | `/scans` | `scans::create_scan` | editor (body `garden_id`) | ✓ | [3](../architecture/contracts.md#post-scans) |
| GET | `/scans/{id}` | `scans::get_scan` | viewer | | [3](../architecture/contracts.md#get-scansid) |
| DELETE | `/scans/{id}` | `scans::delete_scan` | editor; `pending` only | | [3](../architecture/contracts.md#delete-scansid) |
| POST | `/scans/{id}/images` | `scans::upload_image` | editor | ✓ | [3](../architecture/contracts.md#post-scansidimages) |
| DELETE | `/scans/{id}/images/{image_id}` | `scans::delete_image` | editor; `pending` only | | [3](../architecture/contracts.md#delete-scansidimagesimage_id) |
| GET | `/scans/{id}/images/{image_id}` | `media::scan_image` | signed URL | | [3](../architecture/contracts.md#get-scansidimagesimage_id-and-get-scansidmasksmask_id) |
| GET | `/scans/{id}/masks/{mask_id}` | `media::scan_mask` | signed URL | | [3](../architecture/contracts.md#get-scansidimagesimage_id-and-get-scansidmasksmask_id) |
| POST | `/scans/{id}/submit` | `scans::submit` | editor | (idempotent by rule) | [3](../architecture/contracts.md#post-scansidsubmit) |
| POST | `/scans/{id}/answers` | `scans::answer` | editor | ✓ | [3](../architecture/contracts.md#post-scansidanswers) |
| POST | `/scans/{id}/feedback` | `scans::feedback` | reviewer | ✓ | [3](../architecture/contracts.md#post-scansidfeedback) |
| GET | `/scans/{id}/events` | `scans::events` | viewer | | [4](../architecture/contracts.md#4-scan-events-sse) |
| GET, POST | `/gardens` | `gardens::list_gardens`, `create_garden` | signed in | POST ✓ | [9](../architecture/contracts.md#gardens) |
| GET, PATCH, DELETE | `/gardens/{id}` | `get_garden`, `update_garden`, `delete_garden` | viewer, owner, owner | | [9](../architecture/contracts.md#gardens) |
| GET | `/gardens/{id}/weather` | `gardens::garden_weather` | viewer | | [9](../architecture/contracts.md#gardens) |
| GET | `/gardens/{id}/members` | `gardens::list_members` | viewer | | [9](../architecture/contracts.md#members-and-invites) |
| PATCH, DELETE | `/gardens/{id}/members/{user_id}` | `update_member`, `remove_member` | owner; owner or self | | [9](../architecture/contracts.md#members-and-invites) |
| GET, POST | `/gardens/{id}/invites` | `list_invites`, `create_invite` | owner | POST ✓ | [9](../architecture/contracts.md#members-and-invites) |
| DELETE | `/gardens/{id}/invites/{invite_id}` | `gardens::revoke_invite` | owner | | [9](../architecture/contracts.md#members-and-invites) |
| POST | `/invites/{code}/accept` | `gardens::accept_invite` | signed in; 10/min | | [9](../architecture/contracts.md#members-and-invites) |
| GET, PUT | `/gardens/{id}/map` | `get_map`, `put_map` | viewer, editor | PUT ✓ | [9](../architecture/contracts.md#garden-map) |
| GET, POST | `/plants` | `plants::list_plants`, `create_plant` | viewer, editor | POST ✓ | [9](../architecture/contracts.md#plants) |
| GET, PATCH, DELETE | `/plants/{id}` | `get_plant`, `update_plant`, `delete_plant` | viewer, editor, owner | | [9](../architecture/contracts.md#plants) |
| GET | `/plants/{id}/timeline` | `plants::timeline` | viewer | | [9](../architecture/contracts.md#plants) |
| GET | `/plants/{id}/beliefs` | `plants::plant_beliefs` | viewer | | [8](../architecture/contracts.md#memory) |
| GET | `/plants/{id}/measurements` | `plants::measurements` | viewer | | [9](../architecture/contracts.md#plants) |
| POST | `/plants/{id}/species` | `plants::confirm_species` | editor | | [3](../architecture/contracts.md#post-plantsidspecies) |
| GET | `/species` | `species::list_species` | signed in | | [9](../architecture/contracts.md#species) |
| GET | `/species/{id}` | `species::get_species` | signed in | | [9](../architecture/contracts.md#species) |
| GET | `/species/{id}/images/{hash}` | `media::species_photo` | signed URL | | [9](../architecture/contracts.md#species) |
| GET, POST | `/tasks` | `tasks::list_tasks`, `create_task` | viewer, editor | POST ✓ | [9](../architecture/contracts.md#tasks) |
| PATCH, DELETE | `/tasks/{id}` | `update_task`, `delete_task` | editor | | [9](../architecture/contracts.md#tasks) |
| GET, PATCH, DELETE | `/me` | `account::get_me`, `update_me`, `delete_me` | signed in | | [9](../architecture/contracts.md#account) |
| POST | `/me/push-tokens` | `account::register_push_token` | signed in | | [9](../architecture/contracts.md#account) |
| DELETE | `/me/push-tokens/{token}` | `account::remove_push_token` | signed in | | [9](../architecture/contracts.md#account) |
| GET, POST | `/memories` | `memories::list_memories`, `create_memory` | viewer, editor | POST ✓ | [8](../architecture/contracts.md#memory) |
| POST | `/memories/{id}/share` | `memories::share_memory` | editor, the author | | [8](../architecture/contracts.md#memory) |
| DELETE | `/memories/{id}` | `memories::delete_memory` | the author; owner for shared | | [8](../architecture/contracts.md#memory) |
| GET, POST | `/conversations` | `chat::list_conversations`, `create_conversation` | viewer | POST ✓ | [8](../architecture/contracts.md#endpoints) |
| GET | `/conversations/{id}` | `chat::get_conversation` | conversation owner | | [8](../architecture/contracts.md#endpoints) |
| POST | `/conversations/{id}/images` | `chat::upload_chat_image` | conversation owner | ✓ | [8](../architecture/contracts.md#endpoints) |
| GET | `/conversations/{id}/images/{image_id}` | `media::chat_image` | signed URL | | [8](../architecture/contracts.md#endpoints) |
| POST | `/conversations/{id}/messages` | `chat::send_message` | conversation owner | ✓ | [8](../architecture/contracts.md#sending-a-message) |
| POST | `/messages/{id}/feedback` | `chat::message_feedback` | conversation owner | | [8](../architecture/contracts.md#endpoints) |
| POST | `/pending-actions/{id}/confirm` | `chat::confirm_pending_action` | conversation owner, editor | | [8](../architecture/contracts.md#pending-actions-confirmation-cards) |
| POST | `/pending-actions/{id}/reject` | `chat::reject_pending_action` | conversation owner, editor | | [8](../architecture/contracts.md#pending-actions-confirmation-cards) |
| GET, POST | `/gardens/{id}/connectors` | `connectors::list_connectors`, `create_connector` | viewer, owner | | [connectors 11](../connectors/README.md#11-changes-to-other-documents) |
| GET | `/connectors/oauth/callback` | `connectors::oauth_callback` | none; the signed `state` | | [connectors 5](../connectors/README.md#5-connecting-and-tokens) |
| POST | `/connectors/{id}/complete` | `connectors::complete_connector` | owner who started it | | [connectors 5](../connectors/README.md#5-connecting-and-tokens) |
| GET | `/connectors/{id}/available-sources` | `connectors::available_sources` | owner | | [connectors 11](../connectors/README.md#11-changes-to-other-documents) |
| PUT | `/connectors/{id}/sources` | `connectors::set_sources` | owner | | [connectors 11](../connectors/README.md#11-changes-to-other-documents) |
| POST | `/connectors/{id}/sync` | `connectors::sync_connector` | owner; one in 5 minutes | | [connectors 6](../connectors/README.md#6-sync) |
| DELETE | `/connectors/{id}` | `connectors::delete_connector` | owner | | [connectors 8](../connectors/README.md#8-privacy-and-security) |
| POST | `/internal/workers/heartbeat` | `internal::heartbeat` | worker token | | [5](../architecture/contracts.md#post-internalworkersheartbeat) |
| POST | `/internal/jobs/claim` | `internal::claim` | worker token | | [5](../architecture/contracts.md#post-internaljobsclaim) |
| POST | `/internal/jobs/{id}/events` | `internal::job_events` | worker token + lease | | [5](../architecture/contracts.md#post-internaljobsidevents) |
| POST | `/internal/jobs/{id}/complete` | `internal::complete` | worker token + lease | | [5](../architecture/contracts.md#post-internaljobsidcomplete) |
| GET | `/internal/jobs/{id}/images/{image_id}` | `internal::job_image` | worker token + lease | | [5](../architecture/contracts.md#5-worker-api-and-jobs) |
| POST | `/webhooks/auth` | `ops::auth_webhook` | Svix signature | | [9](../architecture/contracts.md#auth-webhook) |
| GET | `/healthz` | `ops::healthz` | none | | [9](../architecture/contracts.md#ops) |
| GET | `/status` | `ops::status` | none | | [9](../architecture/contracts.md#ops) |
| GET | `/openapi.json` | `router::app` | none | | [contracts 1](../architecture/contracts.md#source-of-truth) |

**Responses go through `Wire<T>`**, whose bound is `T: Serialize + utoipa::ToSchema`. Only the signed form of a URL-bearing type implements `ToSchema` ([domain 5](domain.md#5-signed-url-typing)), so a handler that tries to return `Scan<Stored>` doesn't compile; every handler that returns a record signs it first with `state.signer()`.

```rust
/// A status and a JSON body.
#[derive(Debug, Clone)]
pub struct Wire<T> {
    status: StatusCode,
    body: T,
}

/// A stored response replayed as-is ([contracts 1]: "the stored response is returned with its
/// original status").
pub fn replay(stored: StoredResponse) -> Response {
    let status = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);
    (status, Json(stored.body)).into_response()
}
```

### Handlers of note

A thin handler is one extractor and one repository call; most are. These carry logic worth reading. Scans:

```rust
/// `POST /scans`: a scan in `pending`, with a new plant when `new_plant` is given. Counts toward
/// the daily scan limit (30), in the scan's transaction (`429 budget_exceeded`).
#[utoipa::path(post, path = "/scans", tag = "scans",
    request_body = CreateScanRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 201, body = Scan), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn create_scan(
    State(state): State<SharedState>,
    request: IdemJson<CreateScanRequest>,
) -> Response {
    let IdemJson { user, body, begun } = request;
    let result = async {
        let editor = state.member::<Editor>(&user, body.garden_id).await?;
        let plant = match body.plant()? {
            ScanPlant::Existing(id) => NewScanPlant::Existing(id),
            ScanPlant::New(plant) => NewScanPlant::New {
                id: state.ports.ids.id(),
                plant,
            },
        };
        let scan = state
            .ports
            .store
            .create_scan(
                editor,
                NewScan {
                    id: state.ports.ids.id(),
                    plant,
                    created_by: user.id(),
                    created_at: state.ports.clock.now(),
                },
            )
            .await?;
        Ok(Wire::created(scan.sign((), &state.signer())))
    }
    .await;
    begun.finish(result).await
}

/// `POST /scans/{id}/submit`: no body. `202` when a run starts, `200` when nothing changes
/// (submit is idempotent); `409 invalid_state` with `hint: "new_scan"` after a non-retryable
/// failure; `422 missing_required_shot`.
#[utoipa::path(post, path = "/scans/{id}/submit", tag = "scans",
    params(("id" = String, Path, description = "Scan id")),
    responses((status = 202, body = Scan), (status = 200, body = Scan), (status = 409, body = ErrorBody),
        (status = 422, body = ErrorBody), (status = 503, body = ErrorBody)))]
pub async fn submit(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
) -> Result<Wire<Scan>, ApiError> {
    let outcome = state.pipeline.submit(access.member, access.id).await?;
    let status = if outcome.started {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok(Wire::with_status(
        status,
        outcome.scan.sign((), &state.signer()),
    ))
}

/// `POST /scans/{id}/answers`: an indicator question's answer; rescored in the background.
#[utoipa::path(post, path = "/scans/{id}/answers", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = AnswerRequest,
    responses((status = 202, body = Scan), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn answer(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
    request: IdemJson<AnswerRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = async {
        let handled = state
            .pipeline
            .answer(access.member, access.id, body)
            .await?;
        state.spawn_follow_up(handled.follow_up);
        Ok(Wire::accepted(handled.value.sign((), &state.signer())))
    }
    .await;
    begun.finish(result).await
}

/// `GET /scans/{id}/events`: the resumable SSE stream ([`scan_stream`]). Everything that can
/// fail with an `ErrorBody` happens before the first byte: authentication, membership (`404` for
/// non-members) and `Last-Event-ID` parsing.
#[utoipa::path(get, path = "/scans/{id}/events", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Last-Event-ID" = Option<u64>, Header, description = "The last seq the app applied")),
    responses((status = 200, content_type = "text/event-stream", body = ScanEvent,
        description = "`id: seq`, `event: type`, `data: ScanEvent`; `: keepalive` every 15 s"),
        (status = 404, body = ErrorBody)))]
pub async fn events(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Scan>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>> + Send + use<>>, ApiError>
{
    let after = match headers.get("last-event-id") {
        None => EventSeq::ZERO,
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .map(EventSeq)
            .ok_or_else(|| ApiError::invalid_request("Last-Event-ID", "not a sequence number"))?,
    };
    let stream = scan_stream(state, access.member, access.id, after);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE).text("keepalive")))
}
```

The garden map: shape rules without the database (`GardenMapInput::validate`: counts, simple polygons, distances, one pin per plant), then the store checks `base_version`, the plants and deletions in one transaction ([database 4](../database/README.md#garden-map-versions-)); a stale version is `409` with `current_map_version`, unknown plants `400` with every one of them:

```rust
/// `PUT /gardens/{id}/map`: a new version, atomic; `409 invalid_state` with
/// `current_map_version` when `base_version` is stale, `400` with every unknown plant. Editor.
#[utoipa::path(put, path = "/gardens/{id}/map", tag = "map", params(("id" = String, Path),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = GardenMapInput,
    responses((status = 200, body = GardenMap), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn put_map(
    State(state): State<SharedState>,
    access: Access<Editor, table::Garden>,
    request: IdemJson<GardenMapInput>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = async {
        // Shape rules (counts, polygons, distances, one pin per plant) are checked without the
        // database; the store then checks the version, the plants and deletions in one
        // transaction (database 4, garden map versions).
        let upload = body.validate()?;
        let new_plant_ids: Vec<PlantId> = upload
            .pins
            .iter()
            .filter(|pin| matches!(pin.plant, PinPlant::New(_)))
            .map(|_| state.ports.ids.id())
            .collect();
        let write = MapWrite {
            upload,
            new_plant_ids,
            created_at: state.ports.clock.now(),
        };
        Ok(Wire::ok(
            state.ports.store.put_map(access.member, write).await?,
        ))
    }
    .await;
    begun.finish(result).await
}

/// `DELETE /gardens/{id}/members/{user_id}`: an owner removing anyone, or a member leaving
/// (the role check depends on the request, so it is the run-time `require`). Their chats in the
/// garden go with them; the last owner can't leave (`409`).
#[utoipa::path(delete, path = "/gardens/{id}/members/{user_id}", tag = "members",
    params(("id" = String, Path), ("user_id" = String, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn remove_member(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
    ApiPath(ids): ApiPath<(GardenId, UserId)>,
) -> Result<StatusCode, ApiError> {
    let target = ids.1;
    if target != access.user.id() {
        access.member.require::<Owner>()?;
    }
    let removed = state
        .ports
        .store
        .remove_member(access.member, target)
        .await?;
    let runner = state.deletions.clone();
    let garden = removed.garden;
    let started = domain::ports::DeletionStarted {
        target: domain::ports::DeletionTarget::Member {
            garden,
            user: target,
        },
        prefixes: removed.chat_prefixes,
        keys: Vec::new(),
        notify: Vec::new(),
        garden_name: None,
    };
    runner.spawn(started);
    Ok(StatusCode::NO_CONTENT)
}

/// A new invite code: 128 random bits as 26 base32 characters ([contracts 9]; rand 0.10's
/// `ThreadRng`, a CSPRNG reseeded from the OS).
pub fn new_invite_code() -> InviteCode {
    let mut bytes = [0_u8; 16];
    rand::rng().fill(&mut bytes[..]);
    // reason: 16 bytes always encode to 26 characters of A-Z2-7, the code's exact format.
    BASE32_NOPAD
        .encode(&bytes)
        .parse()
        .unwrap_or_else(|_| unreachable!("16 random bytes encode to a valid invite code"))
}
```

Locations are geocoded once, at the precision the user chose, with the time zone ([architecture 8.5](../architecture/README.md#85-garden-setup)):

```rust
/// Geocodes a location input once ([architecture 8.5]): coordinates are kept at the precision
/// the user chose and named by reverse geocoding; a postal code or city is forward geocoded. The
/// time zone comes with it. An unknown postal code or city is `400 invalid_request`.
async fn resolve(
    state: &AppState,
    input: GardenLocationInput,
) -> Result<ResolvedLocation, ApiError> {
    let query = input.query()?;
    let precision = query.precision();
    let field = match &query {
        LocationQuery::Coordinates { .. } => "location.lat",
        LocationQuery::PostalCode(_) => "location.postal_code",
        LocationQuery::City(_) => "location.city",
    };
    let geocoded = match &query {
        LocationQuery::Coordinates { at, .. } => {
            state
                .ports
                .geocoder
                .geocode(GeocodeQuery::Coordinates(*at))
                .await
        }
        LocationQuery::PostalCode(code) => {
            state
                .ports
                .geocoder
                .geocode(GeocodeQuery::PostalCode(code))
                .await
        }
        LocationQuery::City(city) => state.ports.geocoder.geocode(GeocodeQuery::City(city)).await,
    }
    .map_err(|e| match e {
        domain::ports::ProviderError::NotFound { .. } => {
            ApiError::invalid_request(field, "no place found for this location")
        }
        other => other.into(),
    })?;
    let coordinates = integrations::geocode::round_for(
        match &query {
            LocationQuery::Coordinates { at, .. } => *at,
            _ => geocoded.coordinates,
        },
        precision,
    );
    Ok(ResolvedLocation {
        coordinates,
        precision,
        label: geocoded.label,
        time_zone: geocoded.time_zone,
    })
}
```

The webhook:

```rust
/// `POST /webhooks/auth`: Clerk through Svix. The raw body is verified (signature over
/// `id.timestamp.body`, timestamp within 5 minutes either way) **before** it is parsed; a bad
/// signature or timestamp is `401 unauthenticated` (contracts 9, auth webhook). `user.deleted` starts the
/// account cascade. `user.created` is acknowledged and changes nothing: the user record is made
/// by the first authenticated request, and Clerk doesn't guarantee event order, so a late
/// `user.created` must never recreate a deleted user. Idempotent: `204` every time, also for
/// event types it doesn't handle and for a signed body the adapter can't read (Svix would retry
/// it forever).
#[utoipa::path(post, path = "/webhooks/auth", tag = "ops", security(()),
    params(("svix-id" = String, Header), ("svix-timestamp" = String, Header), ("svix-signature" = String, Header)),
    request_body(content = String, content_type = "application/json", description = "Clerk's event, Svix-signed"),
    responses((status = 204), (status = 401, body = ErrorBody)))]
pub async fn auth_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    };
    let signed = WebhookHeaders {
        id: header("svix-id"),
        timestamp: header("svix-timestamp"),
        signature: header("svix-signature"),
    };
    let Some(event) = state
        .ports
        .webhooks
        .verify(signed, &body, state.ports.clock.now())?
    else {
        return Ok(StatusCode::NO_CONTENT);
    };
    // A cascade already running for this account (a `DELETE /me`) makes this a no-op.
    if event.event_type == AuthEventType::UserDeleted
        && let Some(user) = state.ports.store.user_by_auth(&event.data.id).await?
    {
        state.deletions.start_account(user, event.data.id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /healthz`: `200 { ok: true }` when `SurrealDB` and `RustFS` answer; `503` otherwise.
#[utoipa::path(get, path = "/healthz", tag = "ops", security(()),
    responses((status = 200, body = Healthz), (status = 503, body = ErrorBody)))]
pub async fn healthz(State(state): State<SharedState>) -> Result<Wire<Healthz>, ApiError> {
    state.ports.store.status(state.ports.clock.now()).await?;
    // A listing of an empty prefix: one cheap round trip to RustFS.
    let probe = ObjectPrefix::garden(domain::id::Id::from_ulid(ulid::Ulid::nil()));
    state.ports.blob.list(&probe).await?;
    Ok(Wire::ok(Healthz { ok: BoolLit }))
}
```

A plant's beliefs: Agent Memory's current beliefs of the plant and its garden, and the plant's superseded ones, read together under the adapter's entity-read deadline. Agent Memory off, failing or slow answers `200` with `available: false`, never a `5xx`, because the screen has the memories list to fall back on:

```rust
/// How long `GET /plants/{id}/beliefs` waits for Agent Memory: the adapter's own deadline of an
/// entity read, for both reads together.
const BELIEFS_TIMEOUT: Duration = Duration::from_secs(3);

/// `GET /plants/{id}/beliefs`: what is believed about the plant now (its current beliefs and its
/// garden's) and its superseded beliefs, newest first, from Agent Memory. Shared memories only,
/// so any member. Never a 5xx: Agent Memory off, failing or slower than 3 s is
/// `available: false` with empty lists.
#[utoipa::path(get, path = "/plants/{id}/beliefs", tag = "plants", params(("id" = String, Path)),
    responses((status = 200, body = PlantBeliefs), (status = 404, body = ErrorBody)))]
pub async fn plant_beliefs(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Plant>,
) -> Result<Wire<PlantBeliefs>, ApiError> {
    let unavailable = PlantBeliefs {
        available: false,
        current: Vec::new(),
        history: Vec::new(),
    };
    let beliefs = &state.ports.beliefs;
    if !beliefs.enabled() {
        return Ok(Wire::ok(unavailable));
    }
    let garden = access.member.garden();
    let plant = access.id;
    let subject = BeliefSubject {
        garden,
        plant: Some(plant),
    };
    let reads = async {
        tokio::try_join!(
            beliefs.current(garden, Some(plant)),
            beliefs.history(subject)
        )
    };
    match tokio::time::timeout(BELIEFS_TIMEOUT, reads).await {
        Ok(Ok((current, history))) => Ok(Wire::ok(PlantBeliefs {
            available: true,
            current,
            history: history
                .into_iter()
                .filter(|b| b.superseded_at.is_some())
                .collect(),
        })),
        Ok(Err(e)) => {
            tracing::warn!(%plant, error = %crate::error::chain(&e), "plant beliefs unavailable");
            Ok(Wire::ok(unavailable))
        }
        Err(_) => {
            tracing::warn!(%plant, "plant beliefs timed out");
            Ok(Wire::ok(unavailable))
        }
    }
}
```

**Farm connectors** ([connectors 5](../connectors/README.md#5-connecting-and-tokens); roadmap E3.1–E3.4 of [docs/EON](../EON/README.md)). Without `CONNECTOR_KEY` (`AppState::connector_key`, section 5) connectors are off: the list answers `available: false` without asking the store, and everything else `409 invalid_state` after its access checks, not `503`, which clients retry (decision 9). With the key:

- **Starting.** `POST /gardens/{id}/connectors` (owner) creates a `pending` connector whose `oauth` holds the nonce, the app's `device_hash` and, for Google, the PKCE verifier sealed with `SealPurpose::Pkce`; it answers `201` with the vendor's consent URL. A vendor without its OAuth client (`SLACK_CLIENT_ID`, `GOOGLE_CLIENT_ID`) is `409 invalid_state`, and so is a third connector in a garden (the store's limit).
- **The state** is `<connector id>.<nonce>.<expiry in unix seconds>.<mac>`: four URL-safe fields, the MAC an HMAC-SHA256 under `ConnectorKey::state_key` over the first three as written, checked in constant time before anything else is read. A state works only while the pending connector still holds its nonce and expiry and no `complete` has claimed it (`oauth.claimed_at`); the exchange clears them, so a tampered, expired, claimed or used state is `400 invalid_request` on `state`, at the callback and at `complete`.
- **The callback** is public: it checks the state, stores nothing, and answers `302` to `{CONNECTOR_RETURN_URL}/{id}/callback` with the code (or the vendor's `error`, when the owner declined, passed on only when it is 1 to 64 characters of `[a-z_]` and as `vendor_error` otherwise) and the state, with `Cache-Control: no-store` and `Referrer-Policy: no-referrer`.
- **`complete`** (owner) checks the state against the `{id}` in the path, then the device: anyone but the owner who started it, or a `device_verifier` whose SHA-256 isn't the stored hash, gets `404 not_found`, and the pending connector is deleted (`finish_connector` on the tracker). Then it claims the state (`claim_connect`, a compare-and-set on `pending`, the nonce, the expiry and no earlier claim), so of two concurrent `complete`s only one exchanges the code and the other gets the used-state `400`. Then it exchanges the code: a code the vendor refuses (the client's `AuthRefused`) is `400 invalid_request` on `code`; our own request rejected (the client's `Refused`, a misconfigured OAuth client) is `500`; an outage is `503 upstream_unavailable` with `upstream: "slack"` or `"google_drive"`. A failed exchange gave no tokens, so `release_connect` clears the claim and a retry can claim again. The tokens are sealed and the store's compare-and-set on the claimed nonce activates the connector; when it loses (the connector was deleted or cleaned up during the exchange) the answer is the used-state `400` or `404`, and the tokens nothing stores are revoked in the background (`DeletionRunner::revoke_tokens`, with the shared-account rule). Any other store error may have come after the commit applied, so `connect_lost` reads the row again: holding the same sealed value, the connect worked and is `200`; otherwise the tokens are revoked; and when the read fails too, they are left alone and logged.

  **Not cancelled with its request.** The handler spawns the work on the tracker and only awaits its `JoinHandle`, so the route's timeout (the `connect` group's 60 s) or a client that goes away drops the handler, not the exchange: the task still stores the tokens, revokes them or releases the claim, and finishes the idempotency record, so a retry with the same `Idempotency-Key` gets `409 request_in_progress` until it is done and then its answer. The claimed part (exchange, then `complete_connect`) is a task of its own: a panic in it comes back as a `JoinError`, which releases the claim and answers `500`.
- **Sources.** `available-sources` first renews a Google access token with under 5 minutes left through `connect::refresh_and_store`, which the sync uses too: `ConnectVendors::refresh`, then `replace_secret`, a compare-and-set against the sealed value it read (of two refreshes at once one is stored, and the other call uses its own token); then it asks the vendor (channels where the bot is a member, or the Google account's folders); a refused token makes the connector `reauth_needed` and answers `409 invalid_state` with that status. `PUT sources` takes at most 10 channels or 1 folder, each id from the vendor's list (its name comes from there), stores them, then deletes each dropped source's records, 500 at a time, before answering with the fresh record count.
- **Sync now** is the store's compare-and-set (decision 12): `202`, or `429 rate_limited` with `retry_after_s` up to the next allowed instant.
- **Disconnect.** `DELETE` has the store mark the connector `deleting` (the list and the sync skip it) and answers `202`; a task on the tracker runs `DeletionRunner::finish_connector` (section 17), the one revoke-then-delete path, which every cascade uses too.
- **Demo connectors** (`endpoint: "era"`) refuse every owner route with `409 invalid_state` (decision 11). No route takes a URL: the consent URLs are constants, and `wiring/connect.rs` builds the Slack and Drive clients with their default configurations, the vendors' own hosts (decision 8).

```rust
/// The answer of every connector write while connectors are off.
fn connectors_off() -> ApiError {
    ApiError::invalid_state(InvalidStateDetails::default())
        .message("Farm connectors aren't available on this server yet.")
}

/// `GET /gardens/{id}/connectors`: the garden's connectors with their sources, last sync and
/// record counts. Any member. `available: false` with an empty list while connectors are off.
#[utoipa::path(get, path = "/gardens/{id}/connectors", tag = "connectors", params(("id" = String, Path)),
    responses((status = 200, body = ConnectorList), (status = 404, body = ErrorBody)))]
pub async fn list_connectors(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Garden>,
) -> Result<Wire<ConnectorList>, ApiError> {
    // Off without `CONNECTOR_KEY` (the one switch, `AppState::connector_key`): the store isn't
    // asked.
    if state.connector_key().is_none() {
        return Ok(Wire::ok(ConnectorList::unavailable()));
    }
    let connectors = state.ports.store.connectors(access.member).await?;
    Ok(Wire::ok(ConnectorList {
        available: true,
        connectors,
    }))
}

/// `POST /gardens/{id}/connectors`: starts a connect, `pending`, and returns the vendor's consent
/// URL. Owner. `409 invalid_state` while connectors are off, when this server has no OAuth
/// client for the vendor, or when the garden already has 2 connectors.
#[utoipa::path(post, path = "/gardens/{id}/connectors", tag = "connectors",
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body = CreateConnectorRequest,
    responses((status = 201, body = CreateConnectorResponse), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_connector(
    State(state): State<SharedState>,
    access: Access<Owner, table::Garden>,
    request: IdemJson<CreateConnectorRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let result = start_connect(&state, &access, body).await;
    begun.finish(result).await
}
```

```rust
/// A connect's signed state, as the pending connector's `oauth` field remembers it.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthState {
    /// The pending connector.
    pub connector: ConnectorId,
    /// 128 random bits, also stored in the pending connector's `oauth.nonce`.
    pub nonce: String,
    /// When the state stops working: 10 minutes after the start, whole seconds.
    pub expires_at: Timestamp,
}

/// Why a state was refused. Every one is `400 invalid_request` on `state`; the variants exist
/// for the log and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    /// Not four fields of the expected shapes.
    #[error("the state is malformed")]
    Malformed,
    /// The MAC doesn't verify: tampered, or signed under another `CONNECTOR_KEY`.
    #[error("the state's signature doesn't verify")]
    Signature,
    /// Past its expiry.
    #[error("the state has expired")]
    Expired,
}
```

```rust
/// Slack's consent page ([connectors 5]). A constant: no configuration can move it.
///
/// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
pub const SLACK_AUTHORIZE_URL: &str = "https://slack.com/oauth/v2/authorize";

/// Google's consent page. A constant, like Slack's.
pub const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

/// A vendor call that failed. The vendor's response body is never kept ([connectors 8]).
///
/// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
#[derive(Debug, thiserror::Error)]
pub enum VendorError {
    /// This server has no OAuth client for the vendor.
    #[error("{0} isn't configured")]
    NotConfigured(ConnectorKind),
    /// The vendor refused the owner's code or token (the client's `AuthRefused`): a used,
    /// expired or wrong code at the exchange (Slack's `invalid_code`, Google's `invalid_grant`);
    /// a revoked token, or a missing scope, afterwards. The owner fixes it by connecting again.
    #[error("{vendor} refused the code or token ({code})")]
    AuthRefused {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's short error code.
        code: String,
    },
    /// The vendor rejected our own request (the client's `Refused` or `NotFound`): a
    /// misconfigured OAuth client (Slack's `bad_redirect_uri`, `invalid_client_id`,
    /// `bad_client_secret`; Google's `invalid_client`) or another refusal no owner can fix. A
    /// server-side error, never the owner's `400`.
    #[error("{vendor} rejected the request ({code})")]
    Rejected {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's short error code.
        code: String,
    },
    /// Down, timing out, throttling or answering something unreadable.
    #[error("{vendor} is unavailable")]
    Unavailable {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's `Retry-After`, when it throttled.
        retry_after_s: Option<u32>,
        /// The client's error, without any body.
        #[source]
        source: Option<domain::ports::BoxError>,
    },
}

/// Everything the connector routes ask Slack or Google, on the vendors' fixed hosts. `wiring.rs`
/// implements it over `integrations`' `SlackClient` and `DriveClient`; the route tests mock it.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ConnectVendors: Send + Sync {
    /// Exchanges an authorization code (Slack's `oauth.v2.access` then `auth.test`; Google's
    /// token endpoint with the PKCE verifier, then `about.get`). Never retried: a code works
    /// once.
    async fn exchange(
        &self,
        kind: ConnectorKind,
        code: &str,
        pkce_verifier: Option<SecretString>,
        redirect_uri: &str,
    ) -> Result<Exchanged, VendorError>;

    /// Google's tokens with a new access token when the stored one has less than 5 minutes
    /// left, or at once with `force` (the vendor just refused it), and a refresh token is there
    /// to renew it; `None` when they are fine as they are (always for Slack, whose bot token
    /// doesn't expire). Callers go through [`refresh_and_store`], which stores what comes back
    /// with `FarmRecordStore::replace_secret`, a compare-and-set against the sealed value it
    /// read, so two refreshes at once keep one.
    async fn refresh(
        &self,
        kind: ConnectorKind,
        tokens: &StoredTokens,
        force: bool,
    ) -> Result<Option<StoredTokens>, VendorError>;

    /// The channels the bot is a member of, or the folders the Google account can open, with
    /// the access token as given: refresh it first.
    async fn available_sources(
        &self,
        kind: ConnectorKind,
        tokens: &StoredTokens,
    ) -> Result<Vec<ConnectorSource>, VendorError>;

    /// Revokes the tokens: Slack's `auth.revoke`; Google's `revoke` with the refresh token,
    /// which ends the whole grant of that account to this OAuth client, or the access token
    /// when there is none. A token the vendor already forgot counts as revoked. Never call it
    /// directly for a stored connector: `DeletionRunner::finish_connector` decides whether a
    /// shared account's token may go.
    async fn revoke(&self, kind: ConnectorKind, tokens: &StoredTokens) -> Result<(), VendorError>;
}

/// A connector's tokens renewed through `vendors` ([`ConnectVendors::refresh`]): Google's
/// access token when it has less than 5 minutes left, or at once with `force`, for the source
/// picker and the sync. A renewed grant is stored over `sealed`, the value the caller unsealed,
/// with `replace_secret`, a compare-and-set, and `sealed` then holds the new value, so a second
/// renewal in the same run compares against it. When the stored value changed meanwhile
/// (another refresh stored first, or the connector was disconnected, deleted or reconnected),
/// nothing is stored and the caller still uses its own token, which is as good; a store
/// failure is logged and leaves the same. Returns the tokens to use: the renewed ones, or
/// `tokens` as they were.
pub async fn refresh_and_store(
    vendors: &dyn ConnectVendors,
    store: &dyn Store,
    key: &ConnectorKey,
    (id, kind): (ConnectorId, ConnectorKind),
    sealed: &mut Option<Sealed>,
    (tokens, force): (StoredTokens, bool),
) -> Result<StoredTokens, VendorError> {
    let Some(fresh) = vendors.refresh(kind, &tokens, force).await? else {
        return Ok(tokens);
    };
    if let Some(read) = sealed.clone() {
        let new = fresh.seal(key, id);
        match store.replace_secret(id, read, new.clone()).await {
            Ok(true) => {
                tracing::debug!(connector = %id, "refreshed token stored");
                *sealed = Some(new);
            }
            Ok(false) => {
                tracing::debug!(connector = %id, "refreshed token not stored: the connector changed or went meanwhile");
            }
            Err(error) => {
                tracing::warn!(connector = %id, error = %error, "refreshed token not stored");
            }
        }
    }
    Ok(fresh)
}
```

---

## 12. Uploads

```rust
/// `depth_meta` is a small JSON record.
pub const DEPTH_META_MAX_BYTES: u64 = 64 * 1024;

/// The upload routes' `DefaultBodyLimit`: the photo, two depth maps, the meta and multipart
/// overhead.
pub const UPLOAD_BODY_LIMIT: usize = 12 * 1024 * 1024 + 2 * 4 * 1024 * 1024 + 128 * 1024;

/// A file part.
#[derive(Debug, Clone)]
pub struct FilePart {
    /// Its bytes.
    pub bytes: Bytes,
    /// Its declared content type.
    pub content_type: Option<String>,
}

/// A depth part: absent, read, or dropped because it was over its cap.
#[derive(Debug, Clone, Default)]
pub enum DepthPart {
    /// Not sent.
    #[default]
    Absent,
    /// Read.
    Read(Bytes),
    /// Over 4 MiB: not buffered past the cap.
    TooLarge,
}

/// The parts of an upload, read with their caps.
#[derive(Debug, Clone, Default)]
pub struct UploadForm {
    /// `kind`.
    pub kind: Option<String>,
    /// `file`.
    pub file: Option<FilePart>,
    /// `question_id`.
    pub question_id: Option<String>,
    /// `depth`.
    pub depth: DepthPart,
    /// `depth_confidence`.
    pub depth_confidence: DepthPart,
    /// `depth_meta`.
    pub depth_meta: Option<Bytes>,
}

/// Reads one field chunk by chunk, stopping at `max` bytes, so nothing past the cap is ever
/// buffered.
async fn read_capped(
    field: &mut axum::extract::multipart::Field<'_>,
    max: u64,
) -> Result<Capped, ApiError> {
    let mut buffer = BytesMut::new();
    while let Some(chunk) = field.chunk().await? {
        if (buffer.len() + chunk.len()) as u64 > max {
            // Drain the rest of the field so the next one can be read.
            while field.chunk().await?.is_some() {}
            return Ok(Capped::Over);
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(Capped::Read(buffer.freeze()))
}

/// Streams the multipart body into an [`UploadForm`]. Unknown fields are skipped.
pub async fn read_form(mut multipart: axum::extract::Multipart) -> Result<UploadForm, ApiError> {
    let mut form = UploadForm::default();
    while let Some(mut field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "file" => {
                let content_type = field.content_type().map(str::to_owned);
                match read_capped(&mut field, IMAGE_MAX_BYTES).await? {
                    Capped::Read(bytes) => {
                        form.file = Some(FilePart {
                            bytes,
                            content_type,
                        });
                    }
                    Capped::Over => {
                        return Err(ApiError::new(ErrorKind::UploadTooLarge {
                            max_bytes: IMAGE_MAX_BYTES,
                        }));
                    }
                }
            }
            "depth" | "depth_confidence" => {
                let part = match read_capped(&mut field, DEPTH_MAX_BYTES).await? {
                    Capped::Read(bytes) => DepthPart::Read(bytes),
                    Capped::Over => DepthPart::TooLarge,
                };
                if name == "depth" {
                    form.depth = part;
                } else {
                    form.depth_confidence = part;
                }
            }
            "depth_meta" => match read_capped(&mut field, DEPTH_META_MAX_BYTES).await? {
                Capped::Read(bytes) => form.depth_meta = Some(bytes),
                Capped::Over => form.depth_meta = Some(Bytes::new()),
            },
            "kind" | "question_id" => {
                let Capped::Read(bytes) = read_capped(&mut field, 128).await? else {
                    return Err(ApiError::invalid_request(name, "too long"));
                };
                let value = text(&bytes, &name)?;
                if name == "kind" {
                    form.kind = Some(value);
                } else {
                    form.question_id = Some(value);
                }
            }
            _ => while field.chunk().await?.is_some() {},
        }
    }
    Ok(form)
}
```

```rust
impl UploadForm {
    /// The idempotency fingerprint ([contracts 1]): every field name with a hash of its value,
    /// in a fixed order, never the raw body, whose boundary changes on each retry. A part over its
    /// cap counts by its marker, so a retry with the same oversized depth matches.
    pub fn fingerprint(&self) -> String {
        fn part(hasher: &mut Sha256, name: &str, value: Option<&[u8]>) {
            hasher.update(name.as_bytes());
            hasher.update(b"=");
            match value {
                Some(bytes) => {
                    hasher.update(Sha256::digest(bytes));
                }
                None => hasher.update(b"-"),
            }
            hasher.update(b"\n");
        }
        fn depth(d: &DepthPart) -> Option<&[u8]> {
            match d {
                DepthPart::Absent => None,
                DepthPart::Read(b) => Some(b),
                DepthPart::TooLarge => Some(b"<too_large>"),
            }
        }
        let mut hasher = Sha256::new();
        part(&mut hasher, "kind", self.kind.as_deref().map(str::as_bytes));
        part(
            &mut hasher,
            "question_id",
            self.question_id.as_deref().map(str::as_bytes),
        );
        part(
            &mut hasher,
            "file",
            self.file.as_ref().map(|f| f.bytes.as_ref()),
        );
        part(&mut hasher, "depth", depth(&self.depth));
        part(
            &mut hasher,
            "depth_confidence",
            depth(&self.depth_confidence),
        );
        part(&mut hasher, "depth_meta", self.depth_meta.as_deref());
        hex_digest(hasher)
    }

    fn file(&self) -> Result<&FilePart, ApiError> {
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| ApiError::invalid_request("file", "required"))?;
        // The declared type is checked here; the pipeline sniffs the bytes too.
        if let Some(declared) = &file.content_type
            && declared != "application/octet-stream"
            && !accepted_media_type(declared)
        {
            return Err(ApiError::new(ErrorKind::UnsupportedMediaType {
                accepted: ACCEPTED_MEDIA_TYPES
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            }));
        }
        Ok(file)
    }
}
```

**Streaming with caps.** Each part is read chunk by chunk and stops at its cap, so nothing past it is ever buffered: the photo at 12 MiB (`413 upload_too_large`, `max_bytes: 12582912`; contracts 2's "12 MB" read as MiB, domain open question 13), each depth map at 4 MiB (over it, the depth is dropped with `too_large` and the photo kept: a depth problem never fails the upload, [contracts 3](../architecture/contracts.md#post-scansidimages)), `depth_meta` at 64 KiB. The route's `DefaultBodyLimit` (all parts plus overhead) answers first for a body that is too big as a whole. The idempotency record begins only after the whole form is read, because the fingerprint covers every part.

```rust
/// `POST /scans/{id}/images`: see [`uploads`]. The multipart body is read whole (within its
/// caps) before the idempotency record begins, because the fingerprint covers every part.
#[utoipa::path(post, path = "/scans/{id}/images", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    request_body(content_type = "multipart/form-data", description = "kind, file, question_id?, depth?, depth_confidence?, depth_meta?"),
    responses((status = 201, body = UploadImageResponse), (status = 409, body = ErrorBody),
        (status = 413, body = ErrorBody), (status = 415, body = ErrorBody), (status = 422, body = ErrorBody),
        (status = 503, body = ErrorBody)))]
pub async fn upload_image(
    State(state): State<SharedState>,
    access: Access<Editor, table::Scan>,
    headers: HeaderMap,
    request_id: RequestIdHeader,
    ApiMultipart(multipart): ApiMultipart,
) -> Result<Response, ApiError> {
    let form = uploads::read_form(multipart).await?;
    let path = format!("/scans/{}/images", access.id);
    let begun = match idempotency::begin(
        &state,
        access.user.id(),
        &axum::http::Method::POST,
        &path,
        &headers,
        request_id.0,
        &form.fingerprint(),
    )
    .await?
    {
        Begin::Fresh(begun) => begun,
        Begin::Replay(response) => return Ok(response),
    };
    let result = uploads::store_scan_image(&state, access.member, access.id, form).await;
    Ok(begun.finish(result).await)
}
```

```rust
/// Runs the CPU pipeline on a blocking thread, behind the upload semaphore. The closure owns its
/// buffers (`'static`, as `spawn_blocking` requires) and lends them to the pipeline for the call:
/// `UploadInput<'_>` borrows from the closure's own locals.
async fn process(
    state: &Arc<AppState>,
    kind: ImageKind,
    form: &UploadForm,
) -> Result<crate::seams::ProcessedUpload, ApiError> {
    let file = form.file()?.bytes.clone();
    let depth = match &form.depth {
        DepthPart::Read(d) => Some((
            d.clone(),
            match &form.depth_confidence {
                DepthPart::Read(c) => Some(c.clone()),
                _ => None,
            },
            form.depth_meta.clone().unwrap_or_default(),
        )),
        DepthPart::Absent | DepthPart::TooLarge => None,
    };
    let permit = Arc::clone(&state.upload_permits)
        .acquire_owned()
        .await
        .map_err(ApiError::internal)?;
    let images = Arc::clone(&state.images);
    let processed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let input = UploadInput {
            kind,
            file: &file,
            depth: depth.as_ref().map(|(d, c, m)| DepthInput {
                depth: d,
                confidence: c.as_deref(),
                meta: m,
            }),
        };
        images.process(input)
    })
    .await??;
    Ok(processed)
}

/// `POST /scans/{id}/images` after the idempotency record began.
pub async fn store_scan_image(
    state: &Arc<AppState>,
    editor: Membership<Editor>,
    scan: ScanId,
    form: UploadForm,
) -> Result<Wire<UploadImageResponse>, ApiError> {
    let kind = parse_kind(form.kind.as_deref())?;
    let question: Option<QuestionId> = form
        .question_id
        .as_deref()
        .map(|q| {
            q.parse()
                .map_err(|_| ApiError::invalid_request("question_id", "not an id"))
        })
        .transpose()?;
    let early_depth_problem =
        matches!(form.depth, DepthPart::TooLarge).then_some(DepthProblem::TooLarge);
    let processed = process(state, kind, &form).await?;

    let garden = editor.garden();
    let image: ImageId = state.ports.ids.id();
    let key = ObjectKey::scan_image(garden, scan, image);
    state
        .ports
        .blob
        .put(&key, processed.image.webp.clone(), ContentType::Webp)
        .await
        .map_err(|e| {
            ApiError::new(ErrorKind::ProcessingUnavailable)
                .retry_after(2)
                .cause(e)
        })?;

    let (stored_depth, depth_outcome) = match processed.depth {
        Some(Ok(d)) => store_depth(state, garden, scan, image, d).await,
        Some(Err(problem)) => (None, Some(dropped(problem))),
        None => (None, early_depth_problem.map(dropped)),
    };

    let photo_answer = match question {
        Some(q) => Some(
            state
                .pipeline
                .photo_answer_start(
                    scan,
                    q,
                    PhotoInfo {
                        image_id: image,
                        kind,
                        width: processed.image.width,
                        height: processed.image.height,
                        has_depth: stored_depth.is_some(),
                    },
                )
                .await?,
        ),
        None => None,
    };
    let added = state
        .ports
        .store
        .add_image(
            editor,
            scan,
            NewScanImage {
                id: image,
                kind,
                question,
                width: processed.image.width,
                height: processed.image.height,
                storage_key: key,
                quality: ImageQuality {
                    blur_score: processed.quality.blur_score,
                    exposure: processed.quality.exposure,
                },
                depth: stored_depth,
                photo_answer_start: photo_answer,
                uploaded_at: state.ports.clock.now(),
            },
        )
        .await?;
    // A guided job or a question photo's segmentation job was queued in `add_image`'s
    // transaction; the store woke the claims after its commit.
    if let Some(claim) = added.photo_answer {
        state.spawn_follow_up(state.pipeline.photo_answer_added(claim));
    }
    let response = UploadImageResponse {
        image: added.image,
        quality: processed.quality,
        depth: depth_outcome,
    };
    Ok(Wire::created(response.sign(scan, &state.signer())))
}

/// Puts the depth map and its confidence map. Any failure deletes what was written and reports
/// `storage_error`; the photo stays ([storage 4], uploads).
async fn store_depth(
    state: &AppState,
    garden: domain::id::GardenId,
    scan: ScanId,
    image: ImageId,
    depth: ProcessedDepth,
) -> (Option<StoredDepth>, Option<DepthOutcome>) {
    let blob = &state.ports.blob;
    let depth_key = ObjectKey::depth(garden, scan, image);
    let confidence_key = depth
        .confidence_png
        .as_ref()
        .map(|_| ObjectKey::depth_confidence(garden, scan, image));
    let mut written = Vec::new();
    let mut result = blob
        .put(&depth_key, depth.depth_png.clone(), ContentType::Png)
        .await;
    if result.is_ok() {
        written.push(depth_key.clone());
        if let (Some(key), Some(png)) = (&confidence_key, &depth.confidence_png) {
            result = blob.put(key, png.clone(), ContentType::Png).await;
            if result.is_ok() {
                written.push(key.clone());
            }
        }
    }
    match result {
        Ok(()) => (
            Some(StoredDepth {
                depth_key,
                confidence_key,
                meta: depth.meta,
                width: depth.width,
                height: depth.height,
            }),
            Some(DepthOutcome::Stored { stored: BoolLit }),
        ),
        Err(error) => {
            tracing::warn!(%scan, %image, error = %error, "depth not stored");
            if !written.is_empty() {
                // Through the delete queue: detached, never cancelled, retried (storage 4). The
                // outcome isn't needed; the queue logs a delete that gave up, and the orphan
                // sweep finds the keys, since no record points to them.
                drop(state.deletes.submit(blob::DeleteJob::Keys(written)));
            }
            (None, Some(dropped(DepthProblem::StorageError)))
        }
    }
}
```

| Step | What | On failure |
|---|---|---|
| 1 | `kind`, `question_id`, the declared media type | `400`; `415` |
| 2 | The `vision` CPU pipeline on `spawn_blocking`, behind `UPLOAD_CPU_PERMITS`: sniff and decode (HEIC included), blur and exposure check (every scan kind), EXIF orientation applied, metadata stripped, faces blurred, resized (`stored_long_side`: 2048 px or more when the depth survives validation), WebP; depth checked, rotated with the photo, intrinsics and gravity transformed, face boxes zeroed in the depth and confidence maps, 16-bit PNG | `422 image_quality` (nothing stored, ✓ `an_upload_that_fails_the_quality_check_is_422_and_never_stored`); `415`; face blurring or encoding failed: `503 processing_unavailable`, **fail closed** |
| 3 | Put the photo with `If-None-Match: *` under `gardens/{garden}/scans/{scan}/{image}.webp` | `503 processing_unavailable` |
| 4 | Put the depth map, then the confidence map | The depth is dropped with `storage_error`; whatever was written is submitted to the delete queue (section 17); the photo stays |
| 5 | A question photo: `Pipeline::photo_answer_start` decides segment-first or measure-now, and `add_image` records it with the image in one transaction, the segmentation job included, so a crash never leaves an `answering` question with nothing in flight | |
| 6 | `ScanRepo::add_image`: kind limits, the scan's status and the question's state checked in the transaction; a guided check's `guided` job queued in it | `409 invalid_state`, `422 too_many_images`; the objects are left for the orphan sweep |
| 7 | Hand a question photo to `Pipeline::photo_answer_added` and spawn its follow-up (a job queued in step 6 already woke the claims: the store signals after its commit) | Logged; the sweep recovers a `measuring` step |
| 8 | `201 UploadImageResponse`, signed | |

**Why `spawn_blocking` and a semaphore.** Decoding a 12 MiB HEIC, running the face detector and encoding WebP take hundreds of milliseconds of CPU; on an async worker thread they would stall every other request on it. `spawn_blocking` moves the work to tokio's blocking pool, whose default ceiling is 512 threads, so the `Semaphore` (`acquire_owned`, the permit moved into the closure and dropped when it ends) bounds how many pipelines run at once to the cores the box has.

**Why the closure owns and the pipeline borrows.** `spawn_blocking` needs a `'static` closure: it may outlive the handler's stack. So the handler moves owned `Bytes` (reference-counted, cheap to clone) into the closure, and inside it builds `UploadInput<'_>` borrowing those locals for the length of one synchronous call. The borrowed form is the vision crate's (convention 4: borrowed views for hot, synchronous work); the owned form is what crosses the task boundary.

**HEIC** is decoded by the vision crate when built with libheif ([contracts 12](../architecture/contracts.md#12-open-questions), question 8); an undecodable HEIC is `415`. Chat images use `process_chat` (no quality check, no depth) and the same blob and record steps:

```rust
/// `POST /conversations/{id}/images` after the idempotency record began: the same pipeline
/// without the quality check or depth ([contracts 8]: "depth parts are ignored").
pub async fn store_chat_image(
    state: &Arc<AppState>,
    owner: ConversationOwner<Viewer>,
    form: UploadForm,
) -> Result<Wire<ChatImageUploaded>, ApiError> {
    let file = form.file()?.bytes.clone();
    let permit = Arc::clone(&state.upload_permits)
        .acquire_owned()
        .await
        .map_err(ApiError::internal)?;
    let images = Arc::clone(&state.images);
    let processed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        images.process_chat(&file)
    })
    .await??;
    let conversation = owner.conversation();
    let image: ChatImageId = state.ports.ids.id();
    let key = ObjectKey::chat_image(owner.membership().garden(), conversation, image);
    state
        .ports
        .blob
        .put(&key, processed.webp, ContentType::Webp)
        .await
        .map_err(|e| {
            ApiError::new(ErrorKind::ProcessingUnavailable)
                .retry_after(2)
                .cause(e)
        })?;
    let id = state
        .ports
        .store
        .add_chat_image(
            owner,
            NewChatImage {
                id: image,
                width: processed.width,
                height: processed.height,
                storage_key: key,
                uploaded_at: state.ports.clock.now(),
            },
        )
        .await?;
    let url = state.signer().url(&ImagePath::ChatImage {
        conversation,
        image: id,
    });
    Ok(Wire::created(ChatImageUploaded { image_id: id, url }))
}
```

---

## 13. Scan events: writer, hub and stream

[Contracts 4](../architecture/contracts.md#4-scan-events-sse) asks for one serialised writer per scan that assigns `seq`, stores, then broadcasts; a stream that subscribes, replays after `Last-Event-ID`, and goes live; URLs signed on send; and closing at the resting points.

### The writer

```rust
/// An actor with nothing to do for this long stops; the next event starts a new one.
pub const WRITER_IDLE: Duration = Duration::from_secs(60);

/// A writer command.
enum Command {
    Emit {
        run: Run,
        events: Vec<ScanEventBody<Stored>>,
        reply: Reply,
    },
}

/// A running actor: its generation (so a retiring actor removes only its own entry) and sender.
struct Handle {
    generation: u64,
    tx: mpsc::Sender<Command>,
}

/// What the actors share.
struct Inner {
    actors: Mutex<HashMap<ScanId, Handle>>,
    plan_locks: Mutex<HashMap<ScanId, Arc<tokio::sync::Mutex<()>>>>,
    generation: AtomicU64,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    hub: Arc<EventHub>,
    pusher: Pusher,
    tasks: TaskTracker,
    shutdown: CancellationToken,
}

/// Every scan's writer. Cheap to clone (one `Arc`).
#[derive(Clone)]
pub struct ScanWriters {
    inner: Arc<Inner>,
}
```

```rust
/// The actor: one command at a time, until idle or shut down; then it removes its own entry,
/// closes its channel and drains what was already queued, so no accepted command is lost.
async fn run_actor(
    inner: Arc<Inner>,
    scan: ScanId,
    generation: u64,
    mut rx: mpsc::Receiver<Command>,
) {
    loop {
        let next = tokio::select! {
            command = rx.recv() => command,
            () = tokio::time::sleep(WRITER_IDLE) => None,
            () = inner.shutdown.cancelled() => None,
        };
        match next {
            Some(command) => handle(&inner, scan, command).await,
            None => break,
        }
    }
    {
        let mut actors = inner.actors.lock().unwrap_or_else(PoisonError::into_inner);
        if actors
            .get(&scan)
            .is_some_and(|h| h.generation == generation)
        {
            actors.remove(&scan);
        }
    }
    rx.close();
    while let Some(command) = rx.recv().await {
        handle(&inner, scan, command).await;
    }
}

/// Appends then broadcasts each event in order. The last event of the call is broadcast after
/// the scan's stream state is read, flagged `rests` when that state is a resting point.
async fn append_all(
    inner: &Inner,
    scan: ScanId,
    run: Run,
    events: Vec<ScanEventBody<Stored>>,
) -> Result<Vec<ScanEvent<Stored>>, StoreError> {
    let count = events.len();
    let mut stored = Vec::with_capacity(count);
    for (index, body) in events.into_iter().enumerate() {
        let push = Pusher::scan_reason(&body);
        let event = inner
            .store
            .append_event(
                scan,
                NewEvent {
                    run,
                    ts: inner.clock.now(),
                    body,
                },
            )
            .await?;
        let rests = if index + 1 == count && may_rest(&event.body) {
            inner.store.stream_state(scan).await?.at_rest()
        } else {
            false
        };
        inner.hub.publish(
            scan,
            Published {
                event: event.clone(),
                rests,
            },
        );
        if let Some(reason) = push {
            let pusher = inner.pusher.clone();
            inner
                .tasks
                .spawn(async move { pusher.to_scan_creator(scan, reason).await });
        }
        stored.push(event);
    }
    Ok(stored)
}

/// The events after which a scan can be at rest ([contracts 4], when the stream closes).
fn may_rest(body: &ScanEventBody<Stored>) -> bool {
    matches!(
        body,
        ScanEventBody::PlanReady { .. }
            | ScanEventBody::PlanSuperseded { .. }
            | ScanEventBody::Diagnosis { .. }
            | ScanEventBody::QuestionResolved { .. }
            | ScanEventBody::GuidedCheckDone { .. }
            | ScanEventBody::Failed { .. }
    )
}
```

```rust
#[async_trait]
impl ScanWriter for ScanWriters {
    async fn emit(
        &self,
        scan: ScanId,
        run: Run,
        events: Vec<ScanEventBody<Stored>>,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError> {
        let mut events = events;
        // At most twice: a send can fail only when the actor retired between the lookup and
        // the send, and the second lookup then starts a new actor.
        for _ in 0..2 {
            let (reply, answer) = oneshot::channel();
            match self
                .sender(scan)
                .send(Command::Emit { run, events, reply })
                .await
            {
                Ok(()) => {
                    return answer.await.unwrap_or_else(|e| {
                        Err(StoreError::Query {
                            what: "scan writer",
                            source: BoxError::from(e),
                        })
                    });
                }
                Err(mpsc::error::SendError(Command::Emit { events: back, .. })) => events = back,
            }
        }
        Err(StoreError::Query {
            what: "scan writer",
            source: "the writer kept retiring".into(),
        })
    }

    async fn plan_lock(&self, scan: ScanId) -> PlanGuard {
        let lock = Arc::clone(
            self.inner
                .plan_locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(scan)
                .or_default(),
        );
        PlanGuard::new(lock.lock_owned().await)
    }
}
```

- **One actor per active scan**, fed by a bounded `mpsc` channel (64 commands). Every event of a scan goes through `ScanWriter::emit`, from the pipeline (worker results, answers, the sweep, species changes) and from `api` itself (the claim's `queued → analysing`). For each event the actor calls `EventRepo::append_event`, which increments `scan.last_event_seq` and creates `scan_event:[scan, seq]` in one transaction ([database 4](../database/README.md#scan-events-a-gap-free-seq-)), then publishes. Because `seq` comes from the database, ordering doesn't depend on the actor's lifetime: a writer recreated later continues the sequence.
- **Resting points.** After the last event of each `emit` call, if that event can end a resting point (`plan_ready`, `plan_superseded`, `diagnosis`, `question_resolved`, `guided_check_done`, `failed`), the actor reads `EventRepo::stream_state` and flags the event `rests` when the scan is at rest: `planned` or `failed`, no question open or `answering`, no guided check `analysing`, **and no step claim held** (`step_claimed`, the pipeline's addition, so a late answer's `question_resolved`, emitted while `score` runs, doesn't close the stream before the new diagnosis and plan). The pipeline emits each resting point's final events in one call (agreed by message).
- **Pushes.** The writer sees every event, so the push rule of [contracts 9](../architecture/contracts.md#push-notifications) lives here (`Pusher::scan_reason`): `plan_ready`, `plan_superseded` (not for a chat revision), `question` and `failed` go to the scan's creator while still a member, in a spawned task.
- **Plan writes** of a scan run one at a time under `plan_lock`: an owned guard of a per-scan `tokio::sync::Mutex<()>`, separate from the event channel so a 10-second plan-writer call never delays event delivery ([contracts 7](../architecture/contracts.md#actionplan), re-planning).

**Lifecycle (question 22).** Three pieces, each with its own rule, so none keeps another alive: the actor starts on a scan's first `emit` and retires after 60 s without a command or on shutdown, removing only its own entry (a generation number), then closing its channel and draining what was queued, so no accepted command is lost; an `emit` that races the retirement gets its command back from the failed send and starts a new actor. The broadcast channel lives in the hub (below). The plan lock is swept when nobody holds or waits for it (`Arc::strong_count == 1`). Settled with the pipeline author.

### The hub

```rust
/// What the writer broadcasts: the stored event, and whether the scan was at rest after the
/// call that produced it ([contracts 4], when the stream closes). `rests` is set only on the last
/// event of a writer call.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    /// The stored event: image and mask ids, no URLs. Each stream signs it as it sends.
    pub event: ScanEvent<Stored>,
    /// The stream closes after sending this event.
    pub rests: bool,
}

/// One broadcast sender per scan.
#[derive(Default)]
pub struct EventHub {
    channels: Mutex<HashMap<ScanId, broadcast::Sender<Arc<Published>>>>,
}
```

One `tokio::sync::broadcast` channel per scan with live subscribers, in process, which assumes one `api` instance ([architecture 12](../architecture/README.md#operations)). A subscriber creates the channel; a publish to a scan without one is a no-op (nobody to tell; the event is stored). The writer looks the sender up under the map's lock for every publish and never caches it, and the idle sweep removes a channel under the same lock only when `receiver_count() == 0`, so a sweep can never strand a subscriber on a channel the writer no longer uses. A full channel overwrites its oldest value; a slow receiver gets `Lagged` and replays from the database.

### The stream

```rust
/// `GET /scans/{id}/events`: the resumable SSE stream ([`scan_stream`]). Everything that can
/// fail with an `ErrorBody` happens before the first byte: authentication, membership (`404` for
/// non-members) and `Last-Event-ID` parsing.
#[utoipa::path(get, path = "/scans/{id}/events", tag = "scans",
    params(("id" = String, Path, description = "Scan id"),
        ("Last-Event-ID" = Option<u64>, Header, description = "The last seq the app applied")),
    responses((status = 200, content_type = "text/event-stream", body = ScanEvent,
        description = "`id: seq`, `event: type`, `data: ScanEvent`; `: keepalive` every 15 s"),
        (status = 404, body = ErrorBody)))]
pub async fn events(
    State(state): State<SharedState>,
    access: Access<Viewer, table::Scan>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>> + Send + use<>>, ApiError>
{
    let after = match headers.get("last-event-id") {
        None => EventSeq::ZERO,
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .map(EventSeq)
            .ok_or_else(|| ApiError::invalid_request("Last-Event-ID", "not a sequence number"))?,
    };
    let stream = scan_stream(state, access.member, access.id, after);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE).text("keepalive")))
}
```

```rust
/// How long a live stream waits without an event before it reads the scan's state again. Some
/// changes reach a resting point without an event (a step claim released with nothing to say),
/// and deleting a scan emits none, so only a fresh read can close those streams.
pub const REST_CHECK: Duration = Duration::from_secs(15);

/// The stream body. It owns everything it uses (`'static`): the `Arc<AppState>`, the proof
/// (`Copy`), the scan id and the receiver, because axum moves the body into hyper's connection
/// task. In order:
///
/// 1. subscribe to the scan's broadcast first, so an event stored during the replay is in the
///    replay or in the receiver, never in neither;
/// 2. read the stream state, then replay every stored event after `last`; state first, so every
///    event stored before the read is replayed. A step commits its state before it emits its
///    last events, so a stream can see the scan at rest without them and close early; the app
///    refetches the scan on every close, which covers that gap;
/// 3. at rest: end after the replay (a resting scan replays then closes);
/// 4. live: drop `seq <= last`, but end on such a duplicate when it is flagged `rests`: the
///    resting event was stored after the state read and already went out in the replay, so
///    waiting for another event would keep a resting scan's stream open; a `seq` that skips
///    ahead, or `Lagged`, goes back to 2, and so does [`REST_CHECK`] without an event;
/// 5. end after an event flagged `rests`, when the scan is gone (being deleted: `stream_state`
///    answers `NotFound`), when the channel closes, on a store error (the app
///    reconnects with `Last-Event-ID`), or on shutdown: `take_until` ends the stream when the
///    token fires, without which `axum::serve`'s graceful shutdown would wait on it forever.
///
/// Each event is signed as it is sent ([contracts 4]: URLs signed on send, live or replayed).
pub fn scan_stream(
    state: Arc<AppState>,
    member: Membership<Viewer>,
    scan: ScanId,
    after: EventSeq,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let shutdown = state.shutdown.clone();
    let events = async_stream::stream! {
        let mut live = state.hub.subscribe(scan);
        let mut last = after;
        'replay: loop {
            let at_rest = match state.ports.store.stream_state(scan).await {
                Ok(s) => s.at_rest(),
                Err(StoreError::NotFound) => {
                    tracing::debug!(%scan, "event stream ended: scan gone");
                    return;
                }
                Err(error) => {
                    tracing::warn!(%scan, error = %error, "event stream ended: state unreadable");
                    return;
                }
            };
            let stored = match state.ports.store.events_after(member, scan, last).await {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!(%scan, error = %error, "event stream ended: replay failed");
                    return;
                }
            };
            for event in stored {
                if event.seq <= last {
                    continue;
                }
                last = event.seq;
                yield Ok(to_sse(&state, event));
            }
            if at_rest {
                return;
            }
            loop {
                // `recv` is cancel-safe: a timed-out wait loses no message.
                let Ok(received) = tokio::time::timeout(REST_CHECK, live.recv()).await else {
                    continue 'replay;
                };
                match received {
                    Ok(published) => {
                        let seq = published.event.seq;
                        if seq <= last {
                            if published.rests {
                                return;
                            }
                            continue;
                        }
                        if seq > last.next() {
                            continue 'replay;
                        }
                        last = seq;
                        yield Ok(to_sse(&state, published.event.clone()));
                        if published.rests {
                            return;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue 'replay,
                    Err(RecvError::Closed) => return,
                }
            }
        }
    };
    events.take_until(shutdown.cancelled_owned())
}

/// `id: {seq}`, `event: {type}`, `data: {json}`, in the contract's order. The `Signer` borrows
/// the state's signing ports for the length of this call only.
fn to_sse(state: &AppState, event: ScanEvent<Stored>) -> Event {
    let signed = event.sign((), &state.signer());
    Event::default()
        .id(signed.seq.to_string())
        .event(signed.event_name())
        .json_data(&signed)
        .unwrap_or_else(|_| Event::default().comment("unserialisable event"))
}
```

The handler checks everything that can fail with an error body before the first byte: authentication, membership (`404` for non-members, [architecture 4](../architecture/README.md#authorization-model)) and `Last-Event-ID`. Then the body:

1. **Subscribe first**, so an event stored while the replay runs is in the replay or in the receiver, never in neither.
2. **Read the stream state, then replay** every stored event after `last` (a key-range read, `scan_event:[$scan, $after + 1]..=[$scan, ..]`). State first, so every event stored before the read is in the replay. That doesn't make the replay complete up to the resting point: a pipeline step commits its state and then emits its last events ([pipeline 10](pipeline.md#10-events)), so a stream that reads the state between the two sees the scan at rest, replays without those events (`plan_ready`, for example) and closes. The app covers the gap: on every close it refetches `GET /scans/{id}` ([frontend 10](../frontend/README.md#10-live-streams)), which already holds what the step committed, and push notifications go out from the writer either way.
3. **At rest**: end after the replay; a scan at rest replays and closes (✓ `a_scan_at_rest_replays_and_closes`).
4. **Live**: drop `seq <= last`, except that a duplicate flagged `rests` ends the stream: that resting event was stored after the state read, so it already went out in the replay, and waiting for another event would leave the stream of a resting scan open. A `seq` that skips ahead (`seq > last + 1`) or `Lagged` goes back to step 2, so the client never sees a gap. So does `REST_CHECK` (15 s) without an event: some changes reach a resting point without one (a step claim released with nothing to say), and deleting a scan emits none, so only a fresh state read can close those streams. It costs each open stream one state read and one empty range read every 15 s.
5. **End** after an event flagged `rests`, when the scan is gone (`stream_state` reads only scans without `deleting_at`, so a scan being deleted answers `NotFound`; a reconnect then gets `404`), when the channel closes, on a store error (logged; the status is already 200, and the app reconnects with `Last-Event-ID`), or on shutdown through `take_until`.

✓ `sse_resume_replays_after_last_event_id_then_goes_live_without_duplicates`: with `Last-Event-ID: 1`, the replay sends 2 and 3; a live repeat of 3 is dropped; 4 and 5 follow, and the stream ends after 5. ✓ `a_resting_event_stored_during_the_replay_still_ends_the_stream`: the state read says `analysing`, the replay sends 2 and the resting 3, and the live copy of 3, flagged `rests`, ends the stream. ✓ `a_resting_point_reached_without_an_event_closes_the_stream` and `deleting_the_scan_closes_its_stream`: a live stream with no events re-reads the state after 15 s and closes, at rest after the replay, or on `NotFound`.

**Signing on send.** Stored events hold image and mask ids; `to_sse` signs each one as it is sent, live or replayed, so a replay hours later carries fresh 15-minute URLs and species photos are filled in from `species_id` then ([contracts 4](../architecture/contracts.md#transport)). The `Signer<'_>` borrows the signer and photo ports from the `AppState` the generator owns, for the length of one call: it is built, used for one event's `sign`, and dropped before the next `.await`, so the stream holds no borrow across a suspension point.

**What axum writes** (✓ research probe): `content-type: text/event-stream`, `cache-control: no-cache` (which a tunnel needs, [architecture 12](../architecture/README.md#12-deployment)); `Event::default().id(..).event(..).json_data(..)` writes `id:`, `event:`, `data:` in that order; `KeepAlive::new().interval(15 s).text("keepalive")` writes `: keepalive` after 15 idle seconds. No response-body timeout is set on this route.

---

## 14. Chat streaming and confirmation cards

```rust
/// `POST /conversations/{id}/messages`: the agent's turn as SSE, events without `id:` lines (not
/// resumable). Errors before the stream starts (budget, validation) are ordinary error bodies.
///
/// The stream is the agent's `'static` event stream mapped to SSE frames. If the client goes
/// away, hyper drops the body, which drops the agent's receiver; the turn task sees its next send
/// fail, stops the LLM call and stores the message with `interrupted: true`. Shutdown ends the
/// stream through `take_until`, the same way.
///
/// The idempotency record finishes when the turn task reports its outcome: a completed message is
/// stored as the replay `200 { message }`; an interrupted or failed one releases the key, so the
/// app's "resend", with a new key, runs again.
#[utoipa::path(post, path = "/conversations/{id}/messages", tag = "chat", request_body = SendMessageRequest,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header, description = "UUID per user action")),
    responses((status = 200, content_type = "text/event-stream", body = ChatEvent,
            description = "`event: name`, `data: ChatEvent`; a replay is `application/json` MessageReplay"),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)))]
pub async fn send_message(
    State(state): State<SharedState>,
    access: ConvAccess<Viewer, table::Conversation>,
    request: IdemJson<SendMessageRequest>,
) -> Response {
    let IdemJson { body, begun, .. } = request;
    let turn = state
        .chat
        .start_turn(TurnRequest {
            owner: access.owner,
            message: body,
            shutdown: state.shutdown.child_token(),
        })
        .await;
    let parts = match turn {
        Ok(parts) => parts,
        Err(error) => return begun.finish::<MessageReplay>(Err(error.into())).await,
    };
    let conversation: ConversationId = access.id;
    let outcome = parts.outcome;
    let finisher = Arc::clone(&state);
    state.tasks.spawn(async move {
        match outcome.await {
            Ok(TurnOutcome::Completed(message)) => {
                let replay = MessageReplay {
                    message: message.sign(conversation, &finisher.signer()),
                };
                match serde_json::to_value(&replay) {
                    Ok(body) => begun.complete(StoredResponse { status: 200, body }).await,
                    Err(_) => begun.release().await,
                }
            }
            _ => begun.release().await,
        }
    });
    let frames = parts
        .events
        .map(|event| {
            Ok::<_, Infallible>(
                Event::default()
                    .event(event.event_name())
                    .json_data(&event)
                    .unwrap_or_else(|_| Event::default().comment("unserialisable event")),
            )
        })
        .take_until(state.shutdown.clone().cancelled_owned());
    Sse::new(frames)
        .keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE).text("keepalive"))
        .into_response()
}
```

- **Before the stream** (validation, the token budget, routing, storing the user message) errors are ordinary `ErrorBody` responses, from the agent's `start_turn` ([contracts 8](../architecture/contracts.md#sending-a-message)).
- **The stream** is the agent's `'static` event stream mapped to frames without `id:` lines (`Event::default().event(name).json_data(&event)`): not resumable.
- **Disconnect.** When the client goes away, hyper drops the response body, which drops the agent's receiver; the turn task sees its next send fail (or `tx.closed()` resolve while it waits on the model), stops the LLM call and stores the message with `interrupted: true`. A guard held in a closure would not do: with edition 2021+ disjoint captures, a `move` closure that never mentions the guard doesn't capture it (research probe), which is why the agent detects the drop through the channel.
- **Shutdown** ends the stream through `take_until`, which interrupts the turn the same way.
- **Idempotency.** The turn task resolves a oneshot with its outcome once the message is stored; a spawned task finishes the record: `Completed` stores the signed `200 { message }` replay, anything else releases the key, so the app's "resend", with a new key, runs again. The app tells a replay from a stream by its `Content-Type` ([frontend 10](../frontend/README.md#chat-messages)).

**Confirmation cards** ([contracts 8](../architecture/contracts.md#pending-actions-confirmation-cards)):

```rust
/// `POST /pending-actions/{id}/confirm`: the conversation's owner, an editor or owner of the
/// garden.
///
/// | State | Result |
/// |---|---|
/// | `confirmed` (a retry) | `200` with the first result |
/// | `rejected`, `expired`, or past `expires_at` | `409 invalid_state` with `status` |
/// | `proposed`, a task card | the task is created (source `chat`) or completed; a plant deleted since: `409` with `hint: "plant_removed"` |
/// | `proposed`, a plan card | under the scan's plan lock: the preview applied to the current plan exactly as shown, stored as a new version with reason `chat_revision`; a base that is no longer current: `409` with `current_plan_id`; a neighbour deleted since: `409` with `hint: "neighbour_removed"`. Never retried |
#[utoipa::path(post, path = "/pending-actions/{id}/confirm", tag = "chat", params(("id" = String, Path)),
    responses((status = 200, body = ConfirmResponse), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody)))]
pub async fn confirm_pending_action(
    State(state): State<SharedState>,
    access: ConvAccess<Editor, table::PendingAction>,
) -> Result<Wire<ConfirmResponse>, ApiError> {
    let record = state
        .ports
        .store
        .pending_action(access.owner, access.id)
        .await?;
    let action = record.action;
    let now = state.ports.clock.now();
    match action.status {
        PendingStatus::Confirmed => {
            return Ok(Wire::ok(ConfirmResponse {
                pending_action: action,
                result: record.result.unwrap_or_default(),
            }));
        }
        PendingStatus::Rejected | PendingStatus::Expired => {
            return Err(ApiError::invalid_state(InvalidStateDetails {
                status: Some(action.status.as_str().to_owned()),
                allowed: Some(vec![PendingStatus::Proposed.as_str().to_owned()]),
                ..InvalidStateDetails::default()
            }));
        }
        PendingStatus::Proposed if action.expires_at <= now => {
            return Err(ApiError::invalid_state(InvalidStateDetails {
                status: Some(PendingStatus::Expired.as_str().to_owned()),
                ..InvalidStateDetails::default()
            }));
        }
        PendingStatus::Proposed => {}
    }
    let editor = access.owner.membership();
    let result = match (&action.tool, &action.preview) {
        (WriteTool::CreateTask, Preview::Task { task }) => {
            let created = confirm_new_task(&state, editor, access.user.id(), task, now).await?;
            ConfirmResult {
                task: Some(created),
                plan: None,
            }
        }
        (WriteTool::CompleteTask, Preview::Task { task }) => {
            let patch = UpdateTaskRequest {
                done: Some(true),
                dismissed: None,
                title: None,
                detail: None,
                due: None,
                repeat: None,
            };
            let done = state
                .ports
                .store
                .update_task(editor, task.id, patch, None, now)
                .await?;
            ConfirmResult {
                task: Some(done),
                plan: None,
            }
        }
        (WriteTool::RevisePlan, Preview::Plan { base_plan_id, .. }) => {
            let plan = confirm_revision(&state, editor, *base_plan_id, &action, now).await?;
            ConfirmResult {
                task: None,
                plan: Some(plan),
            }
        }
        _ => {
            return Err(ApiError::internal(
                "pending action tool and preview disagree",
            ));
        }
    };
    let confirmed = state
        .ports
        .store
        .mark_confirmed(access.owner, access.id, result.clone(), now)
        .await?;
    Ok(Wire::ok(ConfirmResponse {
        pending_action: confirmed,
        result,
    }))
}

/// The plan card: read the base plan, apply the preview exactly as shown, resolve its citations,
/// and hand the write to the pipeline, which takes the scan's plan lock, compares and sets
/// `scan.plan` (`409` with `current_plan_id` when the base isn't current), and emits
/// `plan_superseded` with reason `chat_revision`.
async fn confirm_revision(
    state: &SharedState,
    editor: domain::authz::Membership<Editor>,
    base_plan_id: domain::id::PlanId,
    action: &domain::chat::PendingAction,
    now: domain::wire::Timestamp,
) -> Result<ActionPlan, ApiError> {
    let base = state
        .ports
        .store
        .plan(editor.at_least(), base_plan_id)
        .await?;
    let revised = state
        .chat
        .apply_preview(&base, &action.preview, action.id, now)
        .map_err(|e| match e {
            PreviewError::BaseMismatch { current, .. } => {
                ApiError::invalid_state(InvalidStateDetails {
                    current_plan_id: Some(current),
                    ..InvalidStateDetails::default()
                })
            }
            PreviewError::NotAPlan => ApiError::internal(e),
        })?;
    let citations = state
        .ports
        .knowledge
        .citations(&revised.citation_ids)
        .await?;
    // The base's farm records that the revised steps still cite (connectors 7).
    let farm_records = base
        .farm_records
        .iter()
        .filter(|c| {
            revised
                .steps
                .iter()
                .any(|s| s.farm_citations.contains(&c.record_id))
        })
        .cloned()
        .collect();
    let plan = ActionPlan {
        id: state.ports.ids.id(),
        scan_id: base.scan_id,
        plant_id: base.plant_id,
        version: base.version + 1,
        supersedes: Some(base.id),
        provisional: base.provisional,
        diagnosis_id: base.diagnosis_id,
        open_question: base.open_question.clone(),
        constraints: revised.constraints,
        summary: revised.summary,
        steps: revised.steps,
        watch_for: revised.watch_for,
        citations,
        farm_records,
        task_ids: Vec::new(),
        llm_model: base.llm_model.clone(),
        prompt_version: base.prompt_version.clone(),
        created_at: now,
    };
    let write = PlanWrite {
        plan,
        reason: PlanReason::ChatRevision,
        expected: Some(base.id),
        tasks: Vec::new(),
        triggers: Vec::new(),
    };
    state
        .pipeline
        .store_revision(editor, base.scan_id, write)
        .await
        .map_err(|e| match ApiError::from(e) {
            error if error.code() == domain::error::ErrorCode::NotFound => ApiError::from(
                domain::error::DomainError::invalid_state_hint(InvalidStateHint::NeighbourRemoved),
            ),
            error => error,
        })
}
```

The confirm on a plan card applies the preview to its base plan exactly as the card showed it (`agent::tools::revise::apply_preview`, pure), resolves the citations, and hands the write to `Pipeline::store_revision`, which takes the scan's plan lock and compares and sets `scan.plan`: a base that lost the race to an automatic re-plan is `409 invalid_state` with `current_plan_id`, never retried. A step naming a neighbour deleted since is `409` with `hint: "neighbour_removed"`; a task card whose plant was deleted is `409` with `hint: "plant_removed"`. A confirmed card confirmed again returns the first result.

---

## 15. The job broker and the worker API

```rust
/// The fallback poll interval of the long-poll.
pub const CLAIM_POLL: Duration = Duration::from_secs(1);

/// The claim long poll over the store's `Notify`.
pub struct JobBroker {
    notify: Arc<Notify>,
    jobs: Arc<dyn JobRepo>,
    clock: Arc<dyn Clock>,
}
```

```rust
impl JobBroker {
    /// A broker over the job repository, waiting on `notify`: the store's own `Notify`
    /// (`SurrealStore::jobs_queued`). The store calls `notify_waiters`, which wakes every
    /// waiting claim (each re-runs its claim query and one wins) and stores no permit, so a
    /// claim that starts later doesn't wake for nothing.
    pub fn new(jobs: Arc<dyn JobRepo>, clock: Arc<dyn Clock>, notify: Arc<Notify>) -> Self {
        Self {
            notify,
            jobs,
            clock,
        }
    }

    /// The long poll: claim at once if a job is eligible, else wait for a wake-up, the 1-second
    /// poll, the deadline (`None`: `204`) or shutdown (`None`, so the worker reconnects to the
    /// next process).
    ///
    /// The `Notified` future is created and `enable`d before each claim query, so a job queued
    /// between the query and the wait still wakes this claim.
    pub async fn claim(
        &self,
        worker: &WorkerId,
        caps: &[Capability],
        slots: u32,
        wait: Duration,
        shutdown: &CancellationToken,
    ) -> Result<Option<ClaimedJob>, StoreError> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let now = self.clock.now();
            if let Some(job) = self
                .jobs
                .claim_job(worker, slots, claim_policy(caps, now), now)
                .await?
            {
                return Ok(Some(job));
            }
            tokio::select! {
                () = &mut notified => {}
                () = tokio::time::sleep(CLAIM_POLL) => {}
                () = tokio::time::sleep_until(deadline) => return Ok(None),
                () = shutdown.cancelled() => return Ok(None),
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }
}
```

**The long poll** ([database 4](../database/README.md#job-claim-with-slots-), [architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback)): claim at once if a job is eligible (the store's transaction: the worker's `slots`, its kinds, `queued_at` 15 s back for the fallback, the photo-answer hold); otherwise wait for a wake-up, the 1-second fallback poll, the deadline (`204`) or shutdown (`204`, and the worker reconnects to the next process). `Notify::notify_waiters` wakes **every** waiting claim, each re-runs its query and one wins (the transaction's re-check makes the losers conflict and retry onto the next job), and it stores no permit, so a claim that starts later doesn't wake for nothing. The `Notified` future is created and `enable`d *before* each query, so a job queued between the query and the wait still wakes it. ✓ `an_empty_queue_answers_none_at_the_deadline_after_polling` (paused time: 3 queries in a 3-second wait).

**One wake-up path.** The `Notify` is the store's own (`SurrealStore::jobs_queued()`), and only the store signals it, after the commit of every write that queues or requeues a job: `enqueue_job`, step commits and run starts, `add_image` (a guided or segmentation job), a retryable failure in `complete_job`, and `requeue_stale` when any job went back to `queued` ([store 14](store.md#14-jobs-and-workers)). Neither `api` nor the pipeline signals it by hand (there is no `ScanWriter::job_queued` or `JobBroker::job_queued`), so a new write path can't forget to, and the stale-lease requeue wakes waiting claims like every other.

```rust
/// `POST /internal/workers/heartbeat`: every 10 s. Extends the listed leases (only the worker's
/// own, never past `claimed_at + max_runtime`), records `versions` for `/status`, and answers
/// the jobs to stop.
#[utoipa::path(post, path = "/internal/workers/heartbeat", tag = "worker", security(("worker" = [])), request_body = Heartbeat,
    responses((status = 200, body = HeartbeatResponse), (status = 401, body = ErrorBody)))]
pub async fn heartbeat(
    State(state): State<SharedState>,
    WorkerAuth(worker): WorkerAuth,
    ApiJson(body): ApiJson<Heartbeat>,
) -> Result<Wire<HeartbeatResponse>, ApiError> {
    if body.worker_id != worker {
        return Err(ApiError::unauthenticated().message("the token belongs to another worker"));
    }
    Ok(Wire::ok(
        state
            .ports
            .store
            .heartbeat(body, state.ports.clock.now())
            .await?,
    ))
}

/// `POST /internal/jobs/claim`: the long poll, up to `wait_s` (default 25, at most 30). `200 Job`,
/// or `204` when nothing arrived (also on shutdown, so the worker reconnects). A claim that moved
/// the scan from `queued` to `analysing` emits that `status_changed` here.
#[utoipa::path(post, path = "/internal/jobs/claim", tag = "worker", security(("worker" = [])), request_body = ClaimRequest,
    responses((status = 200, body = Job), (status = 204), (status = 401, body = ErrorBody)))]
pub async fn claim(
    State(state): State<SharedState>,
    WorkerAuth(worker): WorkerAuth,
    ApiJson(body): ApiJson<ClaimRequest>,
) -> Result<Response, ApiError> {
    if body.worker_id != worker {
        return Err(ApiError::unauthenticated().message("the token belongs to another worker"));
    }
    let wait = std::time::Duration::from_secs(u64::from(body.wait_s()));
    let claimed = state
        .broker
        .claim(&worker, &body.caps, body.slots, wait, &state.shutdown)
        .await?;
    let Some(claimed) = claimed else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    if claimed.started_analysing
        && let Some(run) = claimed.job.run
    {
        let event = ScanEventBody::StatusChanged {
            from: ScanStatus::Queued,
            to: ScanStatus::Analysing,
        };
        if let Err(error) = state
            .writers
            .emit(claimed.job.scan_id, run, vec![event])
            .await
        {
            tracing::warn!(job = %claimed.job.job_id, error = %error, "status_changed not emitted");
        }
    }
    Ok((StatusCode::OK, Json(claimed.job)).into_response())
}

/// `POST /internal/jobs/{id}/events`: stage events and partial results, turned into scan events
/// by the pipeline (a partial result's findings, or `finding_retracted` when it changed).
/// `measurement` is never a worker stage: `api` reports it itself. Events that arrive after the
/// job completed with the same token (a late batch) get `204` and are dropped, never forwarded:
/// the result is final and its events were sent at `complete`.
#[utoipa::path(post, path = "/internal/jobs/{id}/events", tag = "worker", security(("worker" = [])), request_body = WorkerEvents,
    params(("id" = String, Path), ("X-Lease-Token" = String, Header)),
    responses((status = 204), (status = 409, body = ErrorBody)))]
pub async fn job_events(
    State(state): State<SharedState>,
    JobLease(lease): JobLease,
    ApiJson(body): ApiJson<WorkerEvents>,
) -> Result<StatusCode, ApiError> {
    let Some(job) = state
        .ports
        .store
        .live_lease(&lease, state.ports.clock.now())
        .await?
    else {
        tracing::debug!(job = %lease.job, "events after complete dropped");
        return Ok(StatusCode::NO_CONTENT);
    };
    let events = body
        .events
        .into_iter()
        .filter(|e| {
            !matches!(
                e,
                domain::jobs::WorkerEvent::StageStarted {
                    stage: Stage::Measurement
                }
            )
        })
        .collect();
    state.pipeline.worker_events(&lease, job, events).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /internal/jobs/{id}/complete`: the result, read by the job's recorded kind (contracts
/// v0.9), or a failure. `204` also for a repeat with the same token after completion. A job out
/// of attempts fails for good inside `Pipeline::job_completed` (the scan, the question or the
/// guided check, by kind), whose follow-up comes back in `handled`: it is applied once.
#[utoipa::path(post, path = "/internal/jobs/{id}/complete", tag = "worker", security(("worker" = [])), request_body = CompleteJob,
    params(("id" = String, Path), ("X-Lease-Token" = String, Header)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn complete(
    State(state): State<SharedState>,
    JobLease(lease): JobLease,
    ApiJson(body): ApiJson<CompleteJob>,
) -> Result<StatusCode, ApiError> {
    // `check_lease` also accepts a job already completed with this token, so a retried
    // `complete` reaches `complete_job`, which answers `AlreadyCompleted` (contracts 5: `204`,
    // nothing changes).
    let job = state
        .ports
        .store
        .check_lease(&lease, state.ports.clock.now())
        .await?;
    let handled = state.pipeline.job_completed(&lease, job, body).await?;
    state.spawn_follow_up(handled.follow_up);
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /internal/jobs/{id}/images/{image_id}`: a job's image, authorised by its lease: only an
/// image the job's payload names, only while the lease holds.
#[utoipa::path(get, path = "/internal/jobs/{id}/images/{image_id}", tag = "worker",
    params(("id" = String, Path), ("image_id" = String, Path), ("X-Lease-Token" = String, Header)),
    responses((status = 200, content_type = "image/webp"), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn job_image(
    State(state): State<SharedState>,
    JobLease(lease): JobLease,
    ApiPath(ids): ApiPath<(JobId, ImageId)>,
) -> Result<Response, ApiError> {
    let object = state
        .ports
        .store
        .job_image(&lease, ids.1, state.ports.clock.now())
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}
```

| Route | Notes |
|---|---|
| heartbeat | The body's `worker_id` must be the token's. `JobRepo::heartbeat` extends each listed lease to 2 minutes from now, only for this worker's own jobs, never past `claimed_at + max_runtime`; records `versions` for `/status`; answers `cancel_job_ids` |
| claim | `wait_s` defaults to 25, capped at 30. A claim that moved the scan to `analysing` emits `status_changed` through the writer. Kinds are filtered by the worker's declared capabilities in the store's query |
| events | `JobRepo::live_lease` first: a live lease goes on; a job already completed with the same token answers `204` and the events are **dropped**, never forwarded (the result is final and its findings went out at `complete`; a late batch would resurrect stage progress or partial findings the final result replaced); anything else is `409 lease_lost`. Then the pipeline turns stage events and partial results into scan events and finding diffs; a `guided` job's stage events go out on the scan's current run. `measurement` is never a worker stage |
| complete | The result is read by the job's **recorded** kind (contracts v0.9, `JobResult::decode`), in the pipeline. `check_lease` accepts a job already completed with the same token, so a retry after a timeout reaches `complete_job`, which answers `AlreadyCompleted`: `204`, nothing changes. A job out of attempts fails for good inside `Pipeline::job_completed`, by kind (`final_failure`: the scan, the question or the guided check), exactly once; the handler only spawns the follow-up it returns |
| job images | Authorised by the lease: only an image the job's payload names, only while the lease holds; streamed like the signed media |

**Label versions** (contracts v0.9, `unknown_label_version`): the broker never queues a `perception` job naming label versions no live perception or fallback worker reports. The check is the pipeline's, when it builds the job (`JobRepo::live_workers`), and answers `503 upstream_unavailable` (`upstream: "perception"`, retryable) when live workers exist and none has both versions.

---

## 16. The scheduler

```rust
/// One scheduled task: its name (the log field), interval and body.
pub struct Task {
    /// For the logs.
    pub name: &'static str,
    /// Time between the start of two runs.
    pub every: Duration,
    /// Delay before the first run, so start-up isn't one burst of queries.
    pub first_after: Duration,
}

/// Every task of the table in api.md section 16. The 10-minute question continuation is part
/// of `sweep` (`Pipeline::sweep` owns it), not a task of its own.
pub const TASKS: [Task; 10] = [
    Task {
        name: "requeue",
        every: Duration::from_secs(5),
        first_after: Duration::from_secs(5),
    },
    Task {
        name: "sweep",
        every: Duration::from_secs(30),
        first_after: Duration::from_secs(10),
    },
    Task {
        name: "idle",
        every: Duration::from_mins(1),
        first_after: Duration::from_mins(1),
    },
    Task {
        name: "beliefs",
        every: Duration::from_secs(10),
        first_after: Duration::from_secs(20),
    },
    Task {
        name: "cleanup",
        every: Duration::from_mins(10),
        first_after: Duration::from_mins(1),
    },
    Task {
        name: "weather",
        every: Duration::from_hours(1),
        first_after: Duration::from_secs(30),
    },
    Task {
        name: "soil",
        every: Duration::from_mins(15),
        first_after: Duration::from_mins(2),
    },
    Task {
        name: "connectors",
        every: Duration::from_mins(1),
        first_after: Duration::from_mins(1),
    },
    Task {
        name: "receipts",
        every: Duration::from_mins(15),
        first_after: Duration::from_mins(15),
    },
    Task {
        name: "orphans",
        every: Duration::from_hours(24),
        first_after: Duration::from_hours(1),
    },
];

/// Starts every task on the tracker. The farm connectors' `connectors` task starts only when
/// connectors are on ([`AppState::farm_sync`], decision 9 of docs/EON).
pub fn start(state: &Arc<AppState>) {
    for task in &TASKS {
        match task.name {
            "requeue" => every(state, task, requeue),
            "sweep" => every(state, task, sweep),
            "idle" => every(state, task, idle),
            "beliefs" => every(state, task, beliefs),
            "cleanup" => every(state, task, cleanup),
            "weather" => every(state, task, weather_and_triggers),
            "soil" => every(state, task, soil),
            "connectors" => {
                if state.farm_sync.is_some() {
                    every(state, task, connectors);
                }
            }
            "receipts" => every(state, task, receipts),
            _ => every(state, task, orphans),
        }
    }
}

/// Runs `body` every `task.every` until shutdown. `F` is a plain function from the state to a
/// future; both are `Send + 'static` because the loop is spawned and owns them.
fn every<F, Fut>(state: &Arc<AppState>, task: &Task, body: F)
where
    F: Fn(Arc<AppState>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), ApiError>> + Send + 'static,
{
    let state = Arc::clone(state);
    let (name, period, first) = (task.name, task.every, task.first_after);
    let shutdown = state.shutdown.clone();
    state.tasks.clone().spawn(async move {
        let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + first, period);
        // A run that overran doesn't cause a burst of catch-up runs.
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticks.tick() => {}
            }
            if let Err(error) = body(Arc::clone(&state)).await {
                tracing::warn!(task = name, error = %error.chain(), "scheduled task failed");
            }
        }
    });
}
```

```rust
/// Shared memories per run of the belief sweep.
const BELIEF_SYNC_BATCH: u32 = 50;

/// The belief sweep, the only writer to Agent Memory (architecture 7, memory): erases the
/// queued subjects, then records the unsynced shared memories. Every 10 s, so a new shared
/// memory reaches Agent Memory within seconds and a deleted one is erased as fast. With Agent
/// Memory off it only empties the erase queue.
async fn beliefs(state: Arc<AppState>) -> Result<(), ApiError> {
    let p = &state.ports;
    let report = agent::beliefs::sync_pending(
        p.memory.as_ref(),
        p.beliefs.as_ref(),
        p.clock.as_ref(),
        BELIEF_SYNC_BATCH,
    )
    .await?;
    if report != agent::beliefs::SweepReport::default() {
        tracing::info!(
            forgotten = report.forgotten,
            recorded = report.recorded,
            failed = report.failed,
            "belief sweep"
        );
    }
    Ok(())
}
```

Each task is a loop on the process `TaskTracker`: `interval_at` with a first delay (so start-up isn't one burst of queries), `MissedTickBehavior::Delay` (a run that overran doesn't cause catch-up runs), and a `select!` on the shutdown token. A failed run is logged and the next tick tries again. `every` is generic over the task body: `F: Fn(Arc<AppState>) -> Fut` and `Fut: Future<Output = Result<(), ApiError>>`, both `Send + 'static` because the loop is spawned and owns them; plain `async fn`s are passed directly.

| Task | Interval | What | Database pattern |
|---|---|---|---|
| `requeue` | 5 s | Expired leases and jobs of workers silent for 30 s back to `queued` with `queued_at` reset; a job out of attempts fails for good through the pipeline; the store wakes the claims after its commit | `UPDATE job SET status = 'queued' … WHERE status = 'leased' AND lease_until < time::now()` on the `job_lease` index ([database 4](../database/README.md#job-claim-with-slots-)) |
| `sweep` | 30 s | `Pipeline::sweep`, the one owner of recovery and continuation: stale steps (2 min; 5 for `plan`) and question photos stuck in `measuring` for 2 min, re-claimed and re-run; questions unanswered for 10 minutes, continued with the best candidate marked uncertain (skipped while a photo for it is analysing). Follow-ups spawned | Compare-and-set on the old stamp and run ([database 4](../database/README.md#steps-between-jobs-)); the `score` claim |
| `idle` | 60 s | Limiter keys (`retain_recent`, `shrink_to_fit`), idle broadcast channels, unheld plan locks | In memory |
| `beliefs` | 10 s | `agent::beliefs::sync_pending(…, 50)`, the only writer to Agent Memory: erases the subjects queued in `belief_forget` (then re-marks their remaining shared memories unsynced), then records the unsynced shared memories, oldest first. A failure is logged and retried next run without holding up the rest; a failed erase holds back the records under its subject. The counts are logged when any is above 0. With Agent Memory off it only empties the queue | `belief_forget` (`belief_forgets`, `finish_belief_forget`); `memory.belief_synced_at` (`unsynced_shared`, `mark_synced`) |
| `cleanup` | 10 min | Expired idempotency keys, weather older than 2 hours, expired pending actions, finished jobs older than 7 days, account tombstones 24 h after Clerk confirmed the deletion (`TOMBSTONE_KEEP`), farm connectors still `pending` 10 minutes after their connect started (`PENDING_CONNECT_TTL`, `delete_expired_connects`), except one a `complete` claimed less than 10 minutes ago, whose exchange may still be running (`CONNECT_CLAIM_HOLD`), synced Slack messages older than 180 days (`farm_sync::delete_old_messages`; Drive documents stay while they are under the folder) | Batches of 500; `weather_cache` by key range ([database 4](../database/README.md#cleanup-no-ttl)) |
| `weather` | 1 h | Group gardens by cell, fetch each missing cell once (batches of 50, a failed batch retried once, then cell by cell, `integrations::open_meteo::fetch_cells`), cache them; fire forecast alerts (task + push to every member) and plan triggers (task on the rule's plant) | `weather_cache:[hour, cell]`; tasks deduplicated by the unique `alert_key` ([database 4](../database/README.md#alert-dedupe-)) |
| `soil` | 15 min | `SoilGrids` for due gardens, at most 4 a run (its limit is 5 calls a minute): `pending`, or `unavailable` past `soil_retry_at`. A failure stores `unavailable` with the next attempt `soil_retry_after` it ([contracts 9](../architecture/contracts.md#gardens)): 15 minutes after an outage (timeout, connection error, `5xx`, `429`), 7 days after `NotFound` (null pixels at the point, no soil in Soil Landscapes of Canada, and null at the 8 nearby probes). So a null Canadian point inside SLC costs one call and is `ready`, a null-pixel garden outside it costs its 9 paced calls once a week, not every 15 minutes; an outage stops at the first call. `POST /gardens` makes the first attempt the same way (`fetch_soil_profile`) | `garden.soil_profile`, `garden.soil_retry_at`; `gardens_due_soil_profile`, `soil_profile_failed` |
| `connectors` | 1 min | Only when connectors are on. `FarmSync::run_due`: at most 4 due connectors (`active`, `next_sync_at` passed, not stopped by `too_large` or `source_missing`), synced side by side; each is due again 15 minutes after a sync that finished (below) | `connector_due` index (`due_connectors`), `finish_sync`, `farm_record` upserts by `(connector, vendor_id, n)` |
| `receipts` | 15 min | Expo push receipts: forget `DeviceNotRegistered` tokens (`Notifier::check_receipts`) | `push_token` deletes |
| `orphans` | 24 h | Objects older than an hour under each garden's prefix that no record points to, deleted through the delete queue ([storage 4](../storage/README.md#4-writes-and-deletes)) | `DeletionRepo::unreferenced_keys` over every key field |

```rust
/// `SoilGrids` retries ([contracts 9], the soil profile): at most 4 due gardens a run, inside
/// its 5 calls a minute ([architecture 6.4]). A garden is due when `pending`, or `unavailable`
/// past its `soil_retry_at`: 15 minutes after an outage, 7 days after no data. A null-pixel
/// garden costs its 9 paced calls (the point and 8 probes) once a week, not every 15 minutes;
/// an outage stops at the first call.
async fn soil(state: Arc<AppState>) -> Result<(), ApiError> {
    let now = state.ports.clock.now();
    for (garden, at) in state.ports.store.gardens_due_soil_profile(now, 4).await? {
        fetch_soil_profile(&state, garden, at).await?;
    }
    Ok(())
}

/// One `SoilGrids` attempt for a garden, from `POST /gardens` or the `soil` task: the profile,
/// or `unavailable` with the next attempt `soil_retry_after` the failure.
pub async fn fetch_soil_profile(
    state: &AppState,
    garden: GardenId,
    at: domain::records::weather::Coordinates,
) -> Result<(), ApiError> {
    match state.ports.soil.soil_profile(at).await {
        Ok(profile) => {
            state
                .ports
                .store
                .set_soil_profile(garden, SoilProfileStatus::Ready, Some(profile))
                .await?;
        }
        Err(error) => {
            let wait = soil_retry_after(&error);
            tracing::info!(%garden, error = %crate::error::chain(&error), retry_in_s = wait.as_secs(), "SoilGrids failed; retried later");
            let retry_at = state.ports.clock.now().plus(wait);
            state
                .ports
                .store
                .soil_profile_failed(garden, retry_at)
                .await?;
        }
    }
    Ok(())
}
```

```rust
/// Weather by cell, forecast alerts and plan triggers ([architecture 8.3]): group the gardens by
/// cell, fetch each missing cell once in requests of up to 50 (a failed batch is retried once,
/// then cell by cell), cache it, then fire the rules. Alert tasks are deduplicated by their
/// unique `alert_key`, so overlapping runs create each alert once.
async fn weather_and_triggers(state: Arc<AppState>) -> Result<(), ApiError> {
    let now = state.ports.clock.now();
    let hour = hour_of(now);
    let cells = state.ports.store.gardens_by_cell().await?;
    let mut weather: HashMap<WeatherCell, CellWeather> = HashMap::new();
    let mut missing = Vec::new();
    for cell in &cells {
        match state.ports.store.cached(cell.cell, hour).await? {
            Some(w) => {
                weather.insert(cell.cell, w);
            }
            None => missing.push(CellRequest {
                cell: cell.cell,
                time_zone: cell.time_zone.clone(),
            }),
        }
    }
    fetch_missing_weather(&state, &missing, hour, &mut weather).await?;
    let thresholds = state.config.weather;
    let pusher = Pusher::new(state.ports.store.clone(), state.ports.notifier.clone());
    let mut fired_by_garden: HashMap<GardenId, Vec<_>> = HashMap::new();
    for cell in &cells {
        let Some(w) = weather.get(&cell.cell) else {
            continue;
        };
        let fired = triggers::fired(w, &thresholds);
        for &garden in &cell.gardens {
            fired_by_garden.insert(garden, fired.clone());
            for &(trigger, date) in &fired {
                let (title, detail) = triggers::alert_text(trigger);
                let task = NewTask {
                    id: state.ports.ids.id(),
                    plant: None,
                    plan: None,
                    plan_step: None,
                    scan: None,
                    title: title.to_owned(),
                    detail: Some(detail.to_owned()),
                    due: Some(date),
                    repeat: None,
                    source: TaskSource::ForecastAlert,
                    trigger: Some(TaskTrigger {
                        kind: trigger,
                        date,
                    }),
                    alert_key: Some(AlertKey::Forecast {
                        garden,
                        trigger,
                        date,
                    }),
                    created_by: None,
                    created_at: now,
                };
                if let Some(created) = state.ports.store.create_alert_task(garden, task).await? {
                    let members = pusher.members(garden).await;
                    let payload = PushPayload::Alert {
                        v: V1,
                        garden_id: garden,
                        task_id: created.id,
                        trigger,
                    };
                    pusher.send(&members, title, detail, payload).await;
                }
            }
        }
    }
    create_plan_trigger_tasks(&state, &fired_by_garden, now).await?;
    Ok(())
}
```

### The farm sync

```rust
/// A connector is synced again this long after a sync that finished, or after an outage
/// ([connectors 6]; the `soil` task's pattern).
///
/// [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
pub const SYNC_EVERY: Duration = Duration::from_mins(15);

/// The re-read of a vendor's trailing window runs this long after the last.
pub const REREAD_EVERY: Duration = Duration::from_hours(24);

/// Older-item checks ([`FarmPageRequest::probe_limit`]) one connector's run may make, over all
/// its sources: under Slack's Tier 1 pacing each is a minute of the `conversations.replies`
/// pacer, which the threads that changed need too.
pub const PROBES_PER_RUN: u32 = 2;

/// The sync's settings.
#[derive(Debug, Clone, Copy)]
pub struct SyncConfig {
    /// Due connectors taken per run.
    pub batch: u32,
    /// How long one connector reads in a run before it saves its cursor and leaves the rest
    /// for the next run (checked between pages: under Slack's Tier 1 pacing a page can wait a
    /// minute per call).
    pub run_budget: Duration,
    /// Records a garden may hold.
    pub max_records: u32,
}

/// The adapters, by vendor and endpoint: the real vendor for owners' connectors, Era's
/// simulators for the seeded demo ones. Cheap to clone.
#[derive(Clone, Default)]
pub struct FarmSources(HashMap<(ConnectorKind, ConnectorEndpoint), Arc<dyn FarmSource>>);

/// How one connector's run ended, for the logs and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// The round ended with every source caught up: `synced_at` is set.
    Synced,
    /// The round ended, but a source has pages left (a long first pass, a re-read) or its last
    /// turn failed: no `synced_at`. The next round starts at once while a pass is in progress,
    /// and 15 minutes later when only a failing source is behind.
    RoundEnded,
    /// Sources wait for their turn in the round, for the next run.
    PagesLeft,
    /// The connector has no source chosen: nothing read, tried again after [`SYNC_EVERY`].
    NoSources,
    /// Stopped with this code.
    Failed(SyncErrorCode),
    /// No adapter for its kind and endpoint here; tried again later.
    NoAdapter,
    /// The store or the embedder failed; tried again after [`SYNC_EVERY`].
    Internal,
}

/// The sync. One per process, built only when connectors are on.
pub struct FarmSync {
    store: Arc<dyn Store>,
    embedder: Arc<dyn TextEmbedder>,
    clock: Arc<dyn Clock>,
    key: ConnectorKey,
    /// Renews Google's access tokens, on the vendor's own token endpoint.
    vendors: Arc<dyn ConnectVendors>,
    sources: FarmSources,
    config: SyncConfig,
}
```

```rust
impl FarmSync {
    /// The sync over these ports.
    pub fn new(
        store: Arc<dyn Store>,
        embedder: Arc<dyn TextEmbedder>,
        clock: Arc<dyn Clock>,
        key: ConnectorKey,
        vendors: Arc<dyn ConnectVendors>,
        sources: FarmSources,
        config: SyncConfig,
    ) -> Self {
        Self {
            store,
            embedder,
            clock,
            key,
            vendors,
            sources,
            config,
        }
    }

    /// The `connectors` task: syncs at most [`SyncConfig::batch`] due connectors, side by side,
    /// and returns how each ended. A connector's failure is logged and stored; only a failure
    /// to list the due connectors is an error.
    pub async fn run_due(
        &self,
        shutdown: &CancellationToken,
    ) -> Result<Vec<(ConnectorId, RunEnd)>, ApiError> {
        let due = self
            .store
            .due_connectors(self.clock.now(), self.config.batch)
            .await?;
        let runs = due.into_iter().map(|row| async move {
            let id = row.connector.id;
            (id, self.sync_connector(row, shutdown).await)
        });
        Ok(futures::future::join_all(runs).await)
    }

    /// One connector's run, from its row as [`FarmRecordStore::due_connectors`] returned it.
    pub async fn sync_connector(&self, row: ConnectorRow, shutdown: &CancellationToken) -> RunEnd {
        let id = row.connector.id;
        let kind = row.connector.kind;
        let span = tracing::info_span!("farm_sync", connector = %id, kind = kind.as_str());
        let end = tracing::Instrument::instrument(self.run(row, shutdown), span.clone()).await;
        let _entered = span.enter();
        match end {
            RunEnd::Synced
            | RunEnd::RoundEnded
            | RunEnd::PagesLeft
            | RunEnd::NoSources
            | RunEnd::NoAdapter => {
                tracing::info!(end = ?end, "farm sync");
            }
            RunEnd::Failed(code) => {
                tracing::warn!(last_error = code.as_str(), "farm sync stopped");
            }
            RunEnd::Internal => {}
        }
        end
    }

    async fn run(&self, row: ConnectorRow, shutdown: &CancellationToken) -> RunEnd {
        let id = row.connector.id;
        let due_at = row.next_sync_at;
        let now = self.clock.now();
        let Some(source) = self.sources.get(row.connector.kind, row.connector.endpoint) else {
            let outcome = SyncOutcome {
                due_at,
                cursors: row.cursors.clone(),
                synced_at: None,
                next_sync_at: now.plus(SYNC_EVERY),
                last_error: row.last_error,
            };
            return self.finish(id, outcome, RunEnd::NoAdapter).await;
        };
        let opened = self.tokens(&row);
        let (Some(tokens), Some(account)) = (opened, row.connector.account.clone()) else {
            // A token that doesn't open (another key) or a connector without its account:
            // only reconnecting fixes it.
            return self
                .stop(
                    &row,
                    row.cursors.clone(),
                    &Halt::Vendor(refused(row.connector.kind)),
                )
                .await;
        };
        if row.connector.sources.is_empty() {
            let outcome = SyncOutcome {
                due_at,
                cursors: row.cursors.clone(),
                synced_at: None,
                next_sync_at: now.plus(SYNC_EVERY),
                last_error: None,
            };
            return self.finish(id, outcome, RunEnd::NoSources).await;
        }
        let renewable = row.connector.endpoint == ConnectorEndpoint::Vendor;
        // Shutdown interrupts a renewal in flight: nothing is read, and the connector stays due.
        let grant = tokio::select! {
            biased;
            () = shutdown.cancelled() => None,
            grant = self.grant(&row, tokens, renewable) => Some(grant),
        };
        let Some(grant) = grant else {
            return self.leave_due(&row, now).await;
        };
        let grant = match grant {
            Ok(grant) => grant,
            Err(e) => return self.stop(&row, row.cursors.clone(), &Halt::Vendor(e)).await,
        };
        let run = Run {
            id,
            kind: row.connector.kind,
            garden: row.connector.garden_id,
            source: source.as_ref(),
            grant: Mutex::new(grant),
            renewable,
            account,
            now,
            deadline: tokio::time::Instant::now() + self.config.run_budget,
        };
        let mut cursors = row.cursors.clone();
        let read = self
            .read_round(&run, &row.connector.sources, &mut cursors, shutdown)
            .await;
        let end_now = self.clock.now();
        let (synced_at, next, end) = match read {
            Ok(Round::Ended { caught_up: true }) => {
                (Some(end_now), end_now.plus(SYNC_EVERY), RunEnd::Synced)
            }
            Ok(Round::Ended { caught_up: false }) => {
                // A pass still in progress (a long first pass, a re-read) goes on at once in the
                // next round, so a busy channel's first sync isn't held to one run in 15
                // minutes; only a failing source waits the 15 minutes.
                let in_pass = row.connector.sources.iter().any(|s| {
                    let state = SourceState::parse(cursors.get(&s.id));
                    state.more && !state.failing
                });
                let next = if in_pass {
                    end_now
                } else {
                    end_now.plus(SYNC_EVERY)
                };
                (None, next, RunEnd::RoundEnded)
            }
            Ok(Round::TurnsLeft) => (None, end_now, RunEnd::PagesLeft),
            Err(Stop::Internal(e)) => return self.internal(&row, cursors, e).await,
            Err(Stop::Vendor(e)) => return self.stop(&row, cursors, &Halt::Vendor(e)).await,
            Err(Stop::TooLarge) => return self.stop(&row, cursors, &Halt::TooLarge).await,
        };
        // A source whose last turn failed keeps the connector's error until a page of it comes
        // through, though this run didn't ask it.
        let failing = row
            .connector
            .sources
            .iter()
            .any(|s| SourceState::parse(cursors.get(&s.id)).failing);
        let outcome = SyncOutcome {
            due_at,
            cursors,
            synced_at,
            next_sync_at: next,
            last_error: if failing { row.last_error } else { None },
        };
        self.finish(id, outcome, end).await
    }

    /// The connector's tokens, unsealed through [`StoredTokens`], which reads both forms the
    /// connect routes seal: Slack's bot token as its plain string, Google's grant as JSON.
    fn tokens(&self, row: &ConnectorRow) -> Option<StoredTokens> {
        let sealed = row.secret.as_ref()?;
        StoredTokens::unseal(&self.key, row.connector.id, sealed).ok()
    }

    /// A run that read nothing and leaves the connector due, as it was, for the next run.
    async fn leave_due(&self, row: &ConnectorRow, now: Timestamp) -> RunEnd {
        let outcome = SyncOutcome {
            due_at: row.next_sync_at,
            cursors: row.cursors.clone(),
            synced_at: None,
            next_sync_at: now,
            last_error: row.last_error,
        };
        self.finish(row.connector.id, outcome, RunEnd::PagesLeft)
            .await
    }

    /// The run's tokens: a connector of the vendor's own endpoint with a refresh token has
    /// Google's access token renewed first when it has less than 5 minutes left.
    async fn grant(
        &self,
        row: &ConnectorRow,
        tokens: StoredTokens,
        renewable: bool,
    ) -> Result<Grant, FarmSourceError> {
        let mut sealed = row.secret.clone();
        let tokens = if renewable && tokens.refresh_token.is_some() {
            let kind = row.connector.kind;
            connect::refresh_and_store(
                self.vendors.as_ref(),
                self.store.as_ref(),
                &self.key,
                (row.connector.id, kind),
                &mut sealed,
                (tokens, false),
            )
            .await
            .map_err(|e| renewal_error(kind, e))?
        } else {
            tokens
        };
        Ok(Grant {
            tokens,
            sealed,
            renewed: false,
        })
    }

    /// One page from the vendor. A refused token is renewed once a run and the page asked
    /// again ([`Self::renew`]).
    async fn read_page(
        &self,
        run: &Run<'_>,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        let refused = match run.source.read_page(&run.token(), request.clone()).await {
            Err(e @ FarmSourceError::AuthRefused { .. }) => e,
            other => return other,
        };
        match self.renew(run).await? {
            Some(token) => run.source.read_page(&token, request).await,
            None => Err(refused),
        }
    }

    /// After a refused token: Google's grant renewed at once and stored, once a run, for a
    /// connector of the vendor's own endpoint that has a refresh token. `None` when it can't be
    /// (Slack, an `era` connector, no refresh token, renewed already in this run). A refused
    /// refresh is [`FarmSourceError::AuthRefused`].
    async fn renew(&self, run: &Run<'_>) -> Result<Option<VendorToken>, FarmSourceError> {
        let (tokens, mut sealed) = {
            let mut grant = run
                .grant
                .lock()
                .expect("the grant's lock is never poisoned");
            if !run.renewable || grant.renewed || grant.tokens.refresh_token.is_none() {
                return Ok(None);
            }
            grant.renewed = true;
            (grant.tokens.clone(), grant.sealed.clone())
        };
        let fresh = connect::refresh_and_store(
            self.vendors.as_ref(),
            self.store.as_ref(),
            &self.key,
            (run.id, run.kind),
            &mut sealed,
            (tokens, true),
        )
        .await
        .map_err(|e| renewal_error(run.kind, e))?;
        let token = VendorToken::new(fresh.access_token.expose_secret());
        let mut grant = run
            .grant
            .lock()
            .expect("the grant's lock is never poisoned");
        grant.tokens = fresh;
        grant.sealed = sealed;
        Ok(Some(token))
    }

    /// One run's share of the round in progress ([`Rotation`]). First the sources that haven't
    /// had their turn in the round, in order from where the last run stopped: each reads until
    /// its pass ends, the budget runs out or shutdown comes, and asking it for a page is its
    /// turn, even when the page fails. Then, once every source has had its turn, the budget
    /// left goes to the sources with pages left (a long first pass, a re-read), in order. A
    /// vendor, limit or store error stops the run. Stores each source's state and the rotation
    /// in `cursors`, whatever the end.
    async fn read_round(
        &self,
        run: &Run<'_>,
        sources: &[ConnectorSource],
        cursors: &mut BTreeMap<String, SyncCursor>,
        shutdown: &CancellationToken,
    ) -> Result<Round, Stop> {
        let mut states: Vec<SourceState> = sources
            .iter()
            .map(|s| SourceState::parse(cursors.get(&s.id)))
            .collect();
        let mut rotation = Rotation::parse(cursors.get(SYNC_STATE_KEY));
        rotation
            .turned
            .retain(|id| sources.iter().any(|s| &s.id == id));
        let all_turned = |r: &Rotation| sources.iter().all(|s| r.turned.contains(&s.id));
        if all_turned(&rotation) {
            // The sources still waiting were dropped: the next round starts.
            rotation.round = rotation.round.saturating_add(1);
            rotation.turned.clear();
        }
        let n = sources.len();
        let start = rotation
            .next
            .as_ref()
            .and_then(|id| sources.iter().position(|s| &s.id == id))
            .unwrap_or(0);
        let mut probes = PROBES_PER_RUN;
        // The last source this run asked the vendor for a page of.
        let mut last: Option<usize> = None;
        let mut read: Result<Read, Stop> = Ok(Read::Done);
        // The turns, then the budget left for the passes in progress.
        for second in [false, true] {
            if second && !all_turned(&rotation) {
                break;
            }
            let from = if second {
                last.map_or(start, |i| i + 1)
            } else {
                start
            };
            for i in (0..n).map(|k| (from + k) % n) {
                let src = &sources[i];
                let state = &mut states[i];
                let waits = if second {
                    state.more
                } else {
                    !rotation.turned.contains(&src.id)
                };
                if !waits {
                    continue;
                }
                let mut asked = false;
                read = self
                    .read_source(run, src, state, (&mut probes, &mut asked), shutdown)
                    .await;
                if asked {
                    rotation.turned.insert(src.id.clone());
                    last = Some(i);
                }
                if matches!(read, Err(Stop::Vendor(_))) {
                    state.failing = true;
                }
                cursors.insert(src.id.clone(), state.to_cursor());
                if !matches!(read, Ok(Read::Done)) {
                    break;
                }
            }
            if !matches!(read, Ok(Read::Done)) {
                break;
            }
        }
        // The next run starts after the last source this one asked for a page, so a source
        // that takes the budget (a long first pass) or keeps failing never holds up the
        // others for more than its turn; one left mid-pass resumes from its cursor. When the
        // round ends, the next one starts one source further on still, so the sources that
        // come first in a run, which get its older-item checks, change from round to round.
        let ended = all_turned(&rotation);
        if let Some(i) = last {
            let step = if ended { 2 } else { 1 };
            rotation.next = Some(sources[(i + step) % n].id.clone());
        }
        if ended {
            rotation.round = rotation.round.saturating_add(1);
            rotation.turned.clear();
        }
        cursors.insert(SYNC_STATE_KEY.to_owned(), rotation.to_cursor());
        read?;
        Ok(if ended {
            Round::Ended {
                caught_up: states.iter().all(|s| !s.more && !s.failing),
            }
        } else {
            Round::TurnsLeft
        })
    }

    /// Reads one source's pages until its pass ends, the budget runs out or shutdown comes.
    /// `probes` follows the run's older-item checks left; `asked` is set once a page was asked
    /// for.
    async fn read_source(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        state: &mut SourceState,
        (probes, asked): (&mut u32, &mut bool),
        shutdown: &CancellationToken,
    ) -> Result<Read, Stop> {
        let window = run.source.reread_window();
        loop {
            if shutdown.is_cancelled() || tokio::time::Instant::now() >= run.deadline {
                return Ok(Read::PagesLeft);
            }
            if state.reread.is_none()
                && !state.more
                && let Some(window) = window
                && state
                    .reread_at
                    .is_some_and(|at| run.now >= at.plus(REREAD_EVERY))
            {
                state.reread = Some(Reread {
                    from: run.now.minus(window),
                    seen: std::collections::BTreeSet::new(),
                });
            }
            // No checks on a source's first pass, which reads every item in full, and only as
            // many as the run's budget left covers.
            let first_pass = state.reread_at.is_none();
            let remaining = run
                .deadline
                .saturating_duration_since(tokio::time::Instant::now());
            let request = FarmPageRequest {
                account: run.account.clone(),
                source: src.clone(),
                cursor: state.vendor_cursor(),
                oldest: run.now.minus(FARM_RECORD_KEEP),
                reread_from: state.reread.as_ref().map(|r| r.from),
                probe_limit: if first_pass {
                    0
                } else {
                    probes_offered(remaining, *probes)
                },
            };
            // Shutdown interrupts a page in flight (a Tier 1 page can wait a minute per call);
            // nothing of it is stored yet, and it isn't the source's turn. The budget is not
            // raced: it is checked between pages.
            let page = tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(Read::PagesLeft),
                page = self.read_page(run, request) => page,
            };
            *asked = true;
            let page = page.map_err(Stop::Vendor)?;
            *probes = probes.saturating_sub(page.probes);
            // A resync's deletions go first, so the records it drops don't count against the
            // garden's limit when the page is stored; every item of the page is in `listed`.
            if let Some(listed) = &page.listed {
                self.delete_unlisted(run, src, listed).await?;
            }
            self.store_page(run, src, &page).await?;
            state.failing = false;
            // Only ids in the window are kept: a re-read that catches up from older (after a
            // week or more without a sync) would otherwise store every id of the catch-up. A
            // kept id carries no time, so it counts when the source has a record of it in the
            // window, which is all the deletion at the re-read's end looks at.
            if let Some(reread) = state.reread.as_mut() {
                let from = reread.from;
                reread.seen.extend(
                    page.items
                        .iter()
                        .filter(|i| i.at >= from)
                        .map(|i| i.vendor_id.clone()),
                );
                if !page.kept.is_empty() {
                    let window: std::collections::HashSet<String> = self
                        .store
                        .source_vendor_ids_since(run.id, src.id.clone(), from)
                        .await?
                        .into_iter()
                        .collect();
                    reread
                        .seen
                        .extend(page.kept.iter().filter(|v| window.contains(*v)).cloned());
                }
            }
            let SyncCursor(cursor) = page.cursor;
            state.vendor = Some(cursor);
            state.more = page.more;
            if !page.more {
                if let Some(reread) = state.reread.take() {
                    self.delete_unseen(run, src, &reread).await?;
                    state.reread_at = Some(run.now);
                } else if state.reread_at.is_none() {
                    // The first pass read the whole 180 days, the re-read window included.
                    state.reread_at = Some(run.now);
                }
                return Ok(Read::Done);
            }
        }
    }

    /// At a re-read's end: the source's records in the window that the pass didn't see are
    /// gone at the vendor. The window starts a second late, so a message on the boundary
    /// second, which the vendor's exclusive `oldest` may leave out, is never deleted.
    async fn delete_unseen(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        reread: &Reread,
    ) -> Result<(), Stop> {
        let stored = self
            .store
            .source_vendor_ids_since(
                run.id,
                src.id.clone(),
                reread.from.plus(Duration::from_secs(1)),
            )
            .await?;
        let gone: Vec<String> = stored
            .into_iter()
            .filter(|v| !reread.seen.contains(v))
            .collect();
        if !gone.is_empty() {
            let deleted = self.store.delete_records(run.id, gone).await?;
            tracing::info!(source = %src.id, deleted, "records gone at the vendor deleted");
        }
        Ok(())
    }

    /// After a page that listed the whole source ([`FarmPage::listed`]): the source's records
    /// whose vendor id the listing didn't hold are gone at the vendor.
    async fn delete_unlisted(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        listed: &[String],
    ) -> Result<(), Stop> {
        let listed: std::collections::HashSet<&str> = listed.iter().map(String::as_str).collect();
        let stored = self
            .store
            .source_vendor_ids_since(
                run.id,
                src.id.clone(),
                Timestamp::from(chrono::DateTime::UNIX_EPOCH),
            )
            .await?;
        let gone: Vec<String> = stored
            .into_iter()
            .filter(|v| !listed.contains(v.as_str()))
            .collect();
        if !gone.is_empty() {
            let deleted = self.store.delete_records(run.id, gone).await?;
            tracing::info!(source = %src.id, deleted, "records no longer listed deleted");
        }
        Ok(())
    }

    /// Chunks, hashes, embeds the changed chunks and stores one page; deletes what it reports
    /// gone.
    async fn store_page(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        page: &FarmPage,
    ) -> Result<(), Stop> {
        // One item per vendor id; a later copy wins.
        let mut items: BTreeMap<&str, &domain::ports::FarmItem> = BTreeMap::new();
        for item in &page.items {
            items.insert(item.vendor_id.as_str(), item);
        }
        let mut records: Vec<NewFarmRecord> = Vec::new();
        let mut changed: Vec<(usize, String)> = Vec::new();
        let mut shrunk: Vec<(String, u32)> = Vec::new();
        let mut new = 0_u32;
        if !items.is_empty() {
            let stored = self
                .store
                .record_hashes(run.id, items.keys().map(|v| (*v).to_owned()).collect())
                .await?;
            let mut hashes: HashMap<(&str, u32), &str> = HashMap::new();
            let mut held: HashMap<&str, u32> = HashMap::new();
            for (id, hash) in &stored {
                hashes.insert((&id.vendor_id, id.n), hash);
                let n = held.entry(&id.vendor_id).or_default();
                *n = (*n).max(id.n + 1);
            }
            'items: for (vendor_id, item) in &items {
                let texts = chunk::chunks(item.kind, &item.text, &item.title);
                let mut item_records = Vec::with_capacity(texts.len());
                let mut item_changed = Vec::new();
                let mut item_new = 0;
                for (n, text) in texts.into_iter().enumerate() {
                    let n = u32::try_from(n).unwrap_or(u32::MAX);
                    let Ok(id) = FarmRecordId::new(run.id, vendor_id, n) else {
                        tracing::warn!(source = %src.id, "an item with an unusable id skipped");
                        continue 'items;
                    };
                    // The hash covers what is embedded: the author and title, then the text.
                    let embedded = chunk::embedded_text(&item.title, item.author.as_deref(), &text);
                    let hash = chunk::text_hash(&embedded);
                    let old = hashes.get(&(*vendor_id, n)).copied();
                    if old.is_none() {
                        item_new += 1;
                    }
                    if old != Some(hash.as_str()) {
                        item_changed.push((records.len() + item_records.len(), embedded));
                    }
                    item_records.push(NewFarmRecord {
                        id,
                        kind: item.kind,
                        source: src.id.clone(),
                        title: item.title.clone(),
                        author: item.author.clone(),
                        link: item.link.clone(),
                        text,
                        text_hash: hash,
                        vector: None,
                        at: item.at,
                    });
                }
                let chunks = u32::try_from(item_records.len()).unwrap_or(u32::MAX);
                if held.get(vendor_id).is_some_and(|&h| h > chunks) {
                    shrunk.push(((*vendor_id).to_owned(), chunks));
                }
                new += item_new;
                changed.extend(item_changed);
                records.extend(item_records);
            }
        }
        // The garden's count is read for each page, not once a run: up to 4 connectors sync
        // side by side, and another connector of the garden may have stored records since.
        // Two pages stored at once can still pass the limit, by at most a page each.
        if new > 0 {
            let count = self.store.garden_record_count(run.garden).await?;
            if count.saturating_add(new) > self.config.max_records {
                return Err(Stop::TooLarge);
            }
        }
        self.embed(&mut records, &changed).await?;
        if !records.is_empty() {
            self.store.upsert_records(run.id, records).await?;
        }
        for (vendor_id, from) in shrunk {
            self.store
                .delete_chunks_from(run.id, vendor_id, from)
                .await?;
        }
        if !page.removed.is_empty() {
            self.store
                .delete_records(run.id, page.removed.clone())
                .await?;
        }
        Ok(())
    }

    /// Embeds the changed chunks, each given by its index in `records` and its embedded text,
    /// in `Document` mode, in batches; the others keep their stored vector.
    async fn embed(
        &self,
        records: &mut [NewFarmRecord],
        changed: &[(usize, String)],
    ) -> Result<(), Stop> {
        if changed.is_empty() {
            return Ok(());
        }
        let model = self.embedder.model().name;
        for batch in changed.chunks(EMBED_BATCH) {
            let texts: Vec<String> = batch.iter().map(|(_, text)| text.clone()).collect();
            let vectors = self
                .embedder
                .embed(EmbedMode::Document, &texts)
                .await
                .map_err(|e| Stop::Internal(Box::new(e)))?;
            if vectors.len() != batch.len() {
                return Err(Stop::Internal(
                    "the embedder returned a vector count that differs from its input".into(),
                ));
            }
            for (&(i, _), embedding) in batch.iter().zip(vectors) {
                records[i].vector = Some(FarmVector {
                    embedding,
                    model: model.clone(),
                });
            }
        }
        Ok(())
    }

    /// A run that stopped on the vendor or the limit: the cursors so far, the code, and when to
    /// try again. A refused token also makes the connector `reauth_needed`.
    async fn stop(
        &self,
        row: &ConnectorRow,
        cursors: BTreeMap<String, SyncCursor>,
        stop: &Halt,
    ) -> RunEnd {
        let now = self.clock.now();
        let (code, next) = outcome(stop, now);
        if let Halt::Vendor(error) = stop {
            tracing::info!(error = %chain(error), "vendor stopped the sync");
        }
        let outcome = SyncOutcome {
            due_at: row.next_sync_at,
            cursors,
            synced_at: None,
            next_sync_at: next,
            last_error: Some(code),
        };
        let end = self
            .finish(row.connector.id, outcome, RunEnd::Failed(code))
            .await;
        if code == SyncErrorCode::AuthRefused
            && let Err(e) = self
                .store
                .set_status(row.connector.id, ConnectorStatus::ReauthNeeded, Some(code))
                .await
        {
            tracing::warn!(error = %chain(&e), "reauth_needed not stored");
            return RunEnd::Internal;
        }
        end
    }

    /// A run that the store or the embedder stopped: logged, the cursors so far kept, and tried
    /// again after [`SYNC_EVERY`], so a lasting failure doesn't spend vendor calls every minute.
    async fn internal(
        &self,
        row: &ConnectorRow,
        cursors: BTreeMap<String, SyncCursor>,
        error: BoxError,
    ) -> RunEnd {
        tracing::warn!(error = %chain(&*error), "farm sync failed");
        let outcome = SyncOutcome {
            due_at: row.next_sync_at,
            cursors,
            synced_at: None,
            next_sync_at: self.clock.now().plus(SYNC_EVERY),
            last_error: row.last_error,
        };
        self.finish(row.connector.id, outcome, RunEnd::Internal)
            .await;
        RunEnd::Internal
    }

    async fn finish(&self, id: ConnectorId, outcome: SyncOutcome, end: RunEnd) -> RunEnd {
        match self.store.finish_sync(id, outcome).await {
            Ok(()) => end,
            Err(e) => {
                tracing::warn!(error = %chain(&e), "sync state not stored; the connector stays due");
                RunEnd::Internal
            }
        }
    }
}
```

**One connector's run.** The sync picks the adapter by the connector's kind and endpoint (`FarmSources`: the real Slack and Google Drive for owners' connectors, Era's simulators for the seeded `era` ones), unseals the tokens (renewing Google's, below), and gives the sources their turns in the round (below), reading each page by page from their stored cursors until a pass ends, the run's budget (3 minutes, checked between pages) is spent, or shutdown comes. For each page it chunks every item (`chunk::chunks`: a message is one chunk with its replies joined, cut at 2,000 characters; a document is split on paragraphs; a file listed by name only is its title), puts each chunk under a line naming its author and title (`chunk::embedded_text`: "Riley in #field-reports", a blank line, the chunk; EON gap G12) and hashes that (`chunk::text_hash`, SHA-256 in hex, EON gap G5), so a new display name or a renamed channel embeds the record again, reads the stored hashes of the page's items (`record_hashes`), reads the garden's record count (each page, so connectors of one garden syncing side by side pass the 10,000 limit by at most a page each), embeds only the new or changed chunks in `Document` mode in batches of 32, upserts them all (an unchanged chunk keeps its vector; a page of a source the owner dropped meanwhile is skipped by the store), drops a shortened document's extra chunks (`delete_chunks_from`) and deletes what the page reports gone. One `finish_sync` at the end stores every source's cursor, `synced_at` when the round ended with every source caught up, `next_sync_at` and `last_error`. A connector with no source chosen reads nothing and is due again 15 minutes later (`RunEnd::NoSources`).

**Rounds and turns.** The connector's sync state (`state::Rotation`, stored in its cursors under `SYNC_STATE_KEY`, `"#sync"`, which the store keeps when it drops a source's cursor) holds the round in progress, the sources that have had their turn in it (`turned`) and the source the next run starts from. Source ids never start with `#` (Slack's are `[A-Z0-9]`, Drive's `[A-Za-z0-9_-]`), so the key can't clash with one. A turn is the run asking a source for at least one page, whether the source then finishes its pass, runs out of budget mid-pass or fails with a vendor error (a page that shutdown interrupts isn't one). A run first gives their turns to the sources without one in the round, in the connector's order from `next`; each reads until its pass ends, the budget is spent or shutdown comes, and a vendor error takes the turn and stops the run as before. Then, if every source has had its turn and budget is left, the rest goes to the sources with pages left (a long first pass, a re-read), in order. The next run starts after the last source this one asked, and one source further on still after the round's end, so the sources first in a run, which get its older-item checks, change from round to round. When every chosen source has had its turn, the round ends (`turned` empties and the round number moves on): the next sync is 15 minutes away, and `synced_at` is set only if every source is caught up, with no pages left and no failed last turn (`SourceState::failing`, set by a vendor error and cleared by the source's next stored page). While a source's last turn failed, a run that didn't ask it keeps the stored `last_error`. So a source with a long first pass or a re-read, or one that always fails (Slack's `missing_scope`, an unparseable message), holds up the others for one turn a round at most, a source left mid-pass resumes from its own cursor, and a source just added joins the round in progress. A cursor without a rotation, or with one of an earlier shape, starts a fresh round.

**Google's tokens.** An access token lives an hour. For a connector of the vendor's own endpoint whose grant has a refresh token, the run first asks `ConnectVendors::refresh` (`force: false`), which renews a token with under 5 minutes left; the run's 3-minute budget then fits inside it. When Google refuses a token anyway (revoked early, a clock skew), the page's `AuthRefused` makes the run renew it at once (`force: true`) and ask the page again, once a run; a second refusal stands. Each renewed grant goes through `connect::refresh_and_store`, the source picker's compare-and-set against the sealed value the run read or last stored, so a refresh that loses to another keeps its own token for the run. A refused refresh (`invalid_grant`) is `auth_refused`, and the connector becomes `reauth_needed`; Google down is `vendor_unavailable`, and a missing or refused OAuth client of ours is logged as a warning and treated the same. An `era` connector is never renewed: its `ya29.era-demo` has no refresh token, and no simulator token goes to Google's token endpoint.

**Drive.** `DriveSource` ([integrations 15](integrations.md#15-farm-sources-slack-and-google-drive)) has no `reread_window`, so the sync never deletes a Drive record by absence within a pass: it deletes the files the adapter reports in `FarmPage::removed` (moved out of the folder, trashed, deleted). After a page that listed the whole folder (`FarmPage::listed`: the first sync, or a resync after Google refused the change token), it deletes the source's records whose vendor id the listing didn't hold (`delete_unlisted`, over `source_vendor_ids_since` from the epoch). A pass reads 25 files a page and one page of `changes.list` at a time, so a `429` keeps the pages already stored; one that comes within the first page of a listing (the listing and its first 25 reads) lists again on the next run. A source past 500 files or folders is `FarmSourceError::TooLarge`, stored as `too_large` and skipped by the due query until the owner changes the sources. The budget is checked only between pages, and one Drive page can overrun it: a listing walks every folder, and each read can wait out a `Retry-After` of up to 60 s and the client's retries, two failed reads at most a page.

**Shutdown** interrupts the renewal before a run as it does a page in flight: nothing is read, and the connector stays due (`RunEnd::PagesLeft`).

**Older-item checks.** `FarmPageRequest::probe_limit` tells the adapter how many older items it may check with an extra call (Slack's threads older than the re-read window); `FarmPage::probes` says how many it made. A run offers `PROBES_PER_RUN` (2) over all the connector's sources, none on a source's first pass (which read everything in full). Under Tier 1 the page's own call and each check can wait a paced minute, so a page is offered only as many as the budget left covers at 70 s each after 70 s for the page itself (`probes_offered`): a check needs 140 s left, so with the 3-minute budget checks are offered only in a run's first 40 s or so. That bounds the checks, not the run: the budget is checked between pages, and one page makes a `conversations.replies` call per changed thread and a `users.info` call per new author, each of which can wait a paced minute, so a page can still overrun the budget by several paced calls. The cap is per connector: two connectors on one Slack workspace share the client's pacer, so each one's checks also wait for the other's calls.

**Cursors.** A source's stored cursor is `state::SourceState` as JSON: the adapter's own opaque cursor, whether the last page said more follow, when a pass covering the re-read window last finished (`reread_at`), and the re-read pass in progress with the vendor ids it has seen. A pass with pages left (a `429`, a spent budget) continues from it on a later run; records are keyed by `(connector, vendor_id, n)`, so nothing is stored twice.

**The daily re-read.** For an adapter with a `reread_window` (Slack's 7 days), once `REREAD_EVERY` (a day) has passed since the last finished pass, the next pass starts 7 days back (`FarmPageRequest::reread_from`), or, in Slack's adapter, at the newest message seen when that is older, so a sync after a week or more without one skips nothing. At its end, the source's records from a second after that instant (`source_vendor_ids_since`) that the pass neither sent nor reported as kept are deleted: gone at the vendor. The pass keeps only the ids of items at or after it, and the kept ids whose record is in the window (`source_vendor_ids_since`), in `Reread::seen`, so a re-read catching up from older doesn't store every id of the catch-up. An adapter without one (Drive) is never deleted by absence; it reports removals in `FarmPage::removed`.

| How the run ended | `last_error` | `next_sync_at` | Also |
|---|---|---|---|
| The round ended, every source caught up (`Synced`) | none | 15 min later (`SYNC_EVERY`) | `synced_at` now |
| The round ended, a source with pages left or a failed last turn (`RoundEnded`) | none, or the stored one while a source's last turn failed | 15 min later | |
| Budget spent or shutdown, with sources still to have their turn (`PagesLeft`) | as above | now: the next run continues | |
| No source chosen (`NoSources`) | none | 15 min later | nothing read |
| `429` past the client's retries | `rate_limited` | the vendor's `Retry-After` later (the next run at the earliest) | cursor kept |
| Down, `5xx`, an answer that didn't parse | `vendor_unavailable` | 15 min later | |
| Token refused, or one that doesn't open under the key, or no account | `auth_refused` | 15 min later | `reauth_needed`; records stay searchable; nothing runs until the owner reconnects |
| A chosen channel or folder gone | `source_missing` | 15 min later | the due query skips it until the owner changes the sources |
| A page would take the garden past 10,000 records (its count read before each page) | `too_large` | 15 min later | nothing of that page is written; skipped until the sources change |
| The store or the embedder failed | unchanged | 15 min later | logged with its chain |
| No adapter for its kind and endpoint here | unchanged | 15 min later | |

Nothing a vendor sent reaches a log line: lines carry the connector id, the source id, the vendor and the code (`FarmSourceError`'s `Display`). The store keeps the author's display name only.

**Start-up seeding** (`farm_sync::seed`, EON E4.6 and gap G4). After `deletions.resume()`, with connectors on and `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` and `ERA_DEMO_GARDEN_ID` set, `seed_era_connectors` creates the demo garden's Slack and Drive connectors with `endpoint: "era"`, `status: active`, fixed ids (`ERA_SLACK_CONNECTOR`, `ERA_DRIVE_CONNECTOR`) and the tokens `xoxb-era-demo` and `ya29.era-demo` sealed under `CONNECTOR_KEY`, through `seed_connector`, which creates a connector when its id is free and repairs one that exists in the demo garden (the token sealed again under the current key, `status: active`, `last_error` cleared); so every start may run it. A connector with that id in another garden is left alone and logged as a warning (`Seeded::Elsewhere`), and one being deleted is left to its deletion (`Seeded::ConnectorDeleting`; a start after it finishes creates it again). The creator is the garden's first owner. The sources are the Maple Row Farm fixture's (`#field-reports`, `#crew`, the `Farm records` folder); the Slack account's workspace URL is the simulator's base URL, so a link points at the simulator, and start-up makes no call to it. A garden with no owner yet (E9.0 hasn't created it) is logged and skipped until a later start (`Seeded::NoGarden`); a garden being deleted is logged as a warning to point `ERA_DEMO_GARDEN_ID` at another (`Seeded::GardenDeleting`).

```rust
/// The seeded Slack connector's fixed id.
pub const ERA_SLACK_CONNECTOR: &str = "01K7ERA5EED000000000000001";

/// The seeded Drive connector's fixed id.
pub const ERA_DRIVE_CONNECTOR: &str = "01K7ERA5EED000000000000002";

/// The Slack simulator's token in open mode: any `xoxb-…` works.
pub const ERA_SLACK_TOKEN: &str = "xoxb-era-demo";

/// The Drive simulator's token in open mode: any bearer works.
pub const ERA_DRIVE_TOKEN: &str = "ya29.era-demo";

/// What seeding did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seeded {
    /// Both connectors exist in the demo garden (created or repaired now).
    Done,
    /// A connector with a fixed id lives in another garden and was left as it was; this is
    /// that garden. The demo garden is missing it.
    Elsewhere(GardenId),
    /// A connector with a fixed id is being deleted in the demo garden and was left so; the
    /// first start after its deletion finishes creates it again.
    ConnectorDeleting,
    /// The garden has no owner (it doesn't exist yet): nothing was created.
    NoGarden,
    /// The garden is being deleted: nothing was created or repaired.
    GardenDeleting,
}

/// Creates the two `era` connectors if their ids are free, and repairs those that exist in the
/// demo garden (token sealed under `key`, `status` active, no `last_error`).
pub async fn seed_era_connectors(
    store: &dyn Store,
    key: &ConnectorKey,
    clock: &dyn Clock,
    seed: &EraSeed,
) -> Result<Seeded, StoreError> {
    let Some(owner) = first_owner(store, seed.garden).await? else {
        // A garden being deleted has no owner to the store either.
        let deleting = store
            .unfinished_deletions()
            .await?
            .contains(&DeletionTarget::Garden(seed.garden));
        return Ok(if deleting {
            Seeded::GardenDeleting
        } else {
            Seeded::NoGarden
        });
    };
    let now = clock.now();
    let slack: ConnectorId = ERA_SLACK_CONNECTOR.parse().expect("a fixed, valid ULID");
    let drive: ConnectorId = ERA_DRIVE_CONNECTOR.parse().expect("a fixed, valid ULID");
    let connectors = [
        (
            slack,
            ConnectorKind::Slack,
            ConnectorAccount::Slack {
                team_id: "T_MAPLE".to_owned(),
                team_name: "Maple Row Farm".to_owned(),
                url: format!("{}/", seed.slack_base),
            },
            vec![source("C_FIELD", "field-reports"), source("C_CREW", "crew")],
            ERA_SLACK_TOKEN,
        ),
        (
            drive,
            ConnectorKind::GoogleDrive,
            ConnectorAccount::GoogleDrive {
                email: "demo@maplerowfarm.example".to_owned(),
            },
            vec![source("F_FARM", "Farm records")],
            ERA_DRIVE_TOKEN,
        ),
    ];
    let mut elsewhere = None;
    let mut deleting = false;
    for (id, kind, account, sources, token) in connectors {
        let connector = NewConnector {
            id,
            kind,
            status: ConnectorStatus::Active,
            endpoint: ConnectorEndpoint::Era,
            created_by: owner,
            account: Some(account),
            secret: Some(key.seal(SealPurpose::Token, id, token.as_bytes())),
            oauth: None,
            sources,
            created_at: now,
        };
        let stored = store.seed_connector(seed.garden, connector).await?;
        if stored.garden_id != seed.garden {
            elsewhere = Some(stored.garden_id);
        } else if stored.status == ConnectorStatus::Deleting {
            deleting = true;
        }
    }
    Ok(match (elsewhere, deleting) {
        (Some(other), _) => Seeded::Elsewhere(other),
        (None, true) => Seeded::ConnectorDeleting,
        (None, false) => Seeded::Done,
    })
}
```

**Alert and plan-trigger dedupe.** A forecast alert's key is `<garden>:<trigger>:<date>` and a plan trigger's `plan:<plan>:<step>:<date>` ([contracts 7](../architecture/contracts.md#task)); the store's insert fails on a duplicate and `create_alert_task` answers `None`, so overlapping runs, or a run repeated after a crash, create each task once and push once. "Today" is the forecast's first day: Open-Meteo answers in the cell's time zone, so no time-zone database is needed.

**The trigger rules** are pure functions of a cell's weather. Their thresholds are `domain::rules::weather::WeatherThresholds` ([domain 7, weather thresholds](domain.md#weather-thresholds)), held in `Config::weather` and also given to Open-Meteo (each day's `frost_risk`) and the pipeline (fusion), so an alert and a diagnosis never disagree about frost or a humid stretch. The thresholds aren't in the contracts, so the defaults are a proposal (section 23). Only the 3-day horizon is the scheduler's own:

```rust
/// Days of forecast looked at, today included. A scheduling choice, not a weather threshold:
/// alerts are about the next 3 days.
pub const HORIZON_DAYS: usize = 3;

/// Every trigger that fires for a cell, with the local date it is about. The forecast's first day
/// is the garden's local today (Open-Meteo answers in the cell's time zone), so no time-zone
/// database is needed here. A humid stretch fires on the date it starts.
pub fn fired(weather: &CellWeather, t: &WeatherThresholds) -> Vec<(TriggerKind, LocalDate)> {
    let mut out = Vec::new();
    let days = &weather.forecast[..weather.forecast.len().min(HORIZON_DAYS)];
    for day in days {
        if t.is_frost_day(day) {
            out.push((TriggerKind::Frost, day.date));
        }
        if t.is_heat_day(day) {
            out.push((TriggerKind::Heat, day.date));
        }
        if t.is_heavy_rain_day(day) {
            out.push((TriggerKind::HeavyRain, day.date));
        }
    }
    for start in t.humid_stretch_starts(days) {
        out.push((TriggerKind::HumidStretch, days[start].date));
    }
    if let Some(today) = weather.forecast.first() {
        if t.is_after_rain(&weather.current) {
            out.push((TriggerKind::AfterRain, today.date));
        }
        if t.is_dry_soil(&weather.soil) {
            out.push((TriggerKind::DrySoil, today.date));
        }
    }
    out
}
```

---

## 17. Deletion

```rust
/// Why a background cascade stopped; it resumes on the next start.
#[derive(Debug, thiserror::Error)]
pub enum DeletionError {
    /// The database.
    #[error("store failed")]
    Store(#[from] StoreError),
    /// The membership read of an account cascade.
    #[error("access check failed")]
    Access(#[from] AccessError),
    /// Some objects are still there after every attempt: the records stay so a later run
    /// deletes the objects first.
    #[error("objects not deleted; the records stay for a resumed run")]
    Objects,
    /// Shutdown interrupted the Clerk retries; the tombstone resumes them.
    #[error("shut down before the Clerk account was deleted")]
    ShutDown,
}

/// Runs cascades on the tracker.
#[derive(Clone)]
pub struct DeletionRunner {
    store: Arc<dyn Store>,
    deletes: DeleteQueue,
    auth_admin: Arc<dyn AuthAdmin>,
    pusher: Pusher,
    clock: Arc<dyn Clock>,
    tasks: TaskTracker,
    shutdown: CancellationToken,
    /// The key that opens connector tokens and the vendors that revoke them; `None` with
    /// connectors off (no `CONNECTOR_KEY`), when no token can be opened to revoke.
    revoke: Option<Revoke>,
}
```

```rust
impl DeletionRunner {
    /// A runner over the ports and `AppState`'s delete queue.
    pub fn new(
        store: Arc<dyn Store>,
        deletes: DeleteQueue,
        auth_admin: Arc<dyn AuthAdmin>,
        pusher: Pusher,
        clock: Arc<dyn Clock>,
        tasks: TaskTracker,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            store,
            deletes,
            auth_admin,
            pusher,
            clock,
            tasks,
            shutdown,
            revoke: None,
        }
    }

    /// With farm connectors on: `key` opens the tokens the cascades revoke through `vendors`.
    #[must_use]
    pub fn with_revoke(mut self, key: ConnectorKey, vendors: Arc<dyn ConnectVendors>) -> Self {
        self.revoke = Some(Revoke { key, vendors });
        self
    }

    /// Runs a started cascade in the background. Not tied to the shutdown token: shutdown waits
    /// for it up to its drain deadline, and a cascade cut off there resumes on the next start.
    pub fn spawn(&self, started: DeletionStarted) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let target = started.target;
            if let Err(error) = runner.run(started).await {
                tracing::error!(?target, error = %error, "deletion failed; it resumes on restart");
            }
        });
    }

    /// A removed member's connectors, then objects, then records, then pushes. Records go only
    /// after every object is gone.
    pub async fn run(&self, started: DeletionStarted) -> Result<(), DeletionError> {
        if let DeletionTarget::Member { garden, user } = started.target {
            self.finish_member_connectors(garden, user).await;
        }
        if !self.delete_objects(started.prefixes, started.keys).await {
            return Err(DeletionError::Objects);
        }
        self.delete_records(started.target).await?;
        if let Some(name) = started.garden_name
            && !started.notify.is_empty()
        {
            let payload = PushPayload::GardenDeleted {
                v: V1,
                garden_name: name.clone(),
            };
            self.pusher
                .send(
                    &started.notify,
                    "A garden was deleted",
                    &format!("{name} was deleted."),
                    payload,
                )
                .await;
        }
        Ok(())
    }

    /// The object deletes, submitted to the delete queue and awaited. Dropping this future (the
    /// cascade's task) never stops a delete: the queue owns it. `true` when every object is gone.
    async fn delete_objects(&self, prefixes: Vec<ObjectPrefix>, keys: Vec<ObjectKey>) -> bool {
        let mut handles: Vec<_> = prefixes
            .into_iter()
            .map(|prefix| self.deletes.submit(DeleteJob::Prefix(prefix)))
            .collect();
        if !keys.is_empty() {
            handles.push(self.deletes.submit(DeleteJob::Keys(keys)));
        }
        let mut all_done = true;
        for handle in handles {
            all_done &= handle.outcome().await.is_some_and(|o| o.is_done());
        }
        all_done
    }

    /// Every cascade step of the target, each repeated until a batch deletes nothing. A
    /// garden's connectors are revoked and deleted just before its `FarmRecordsAndConnectors`
    /// step, which then deletes anything left; a removed member's go before their objects
    /// ([`run`](Self::run)).
    pub async fn delete_records(&self, target: DeletionTarget) -> Result<(), StoreError> {
        for &step in CascadeStep::steps_for(target) {
            if step == CascadeStep::FarmRecordsAndConnectors
                && let DeletionTarget::Garden(garden) = target
            {
                for row in self.store.deleting_connectors(Some(garden)).await? {
                    self.finish_connector(row).await?;
                }
            }
            while self.store.delete_batch(target, step).await? > 0 {}
        }
        Ok(())
    }

    /// After a demotion (`PATCH /gardens/{id}/members/{user_id}`) of an owner committed:
    /// revokes and deletes, in the background, the connectors the demoted owner created in the
    /// garden, which the store marked `deleting` in that transaction.
    pub fn spawn_connectors_of(&self, garden: GardenId, owner: UserId) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            runner.finish_member_connectors(garden, owner).await;
        });
    }

    /// After `DELETE /connectors/{id}`, or a `complete` from the wrong user or device,
    /// committed the `deleting` mark: [`finish_connector`](Self::finish_connector) in the
    /// background.
    pub fn spawn_connector(&self, row: ConnectorRow) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let connector = row.connector.id;
            if let Err(error) = runner.finish_connector(row).await {
                tracing::error!(%connector, error = %error, "connector deletion stopped; it resumes on restart");
            }
        });
    }

    /// The connectors `owner` created in `garden` and the store marked `deleting` when they
    /// stopped being its owner, each finished in turn. Never fails: a store error is logged
    /// and the rest of the cascade goes on, since `resume` finishes what is left. Without
    /// `CONNECTOR_KEY` they stay `deleting`, so their tokens can be revoked once it is set.
    async fn finish_member_connectors(&self, garden: GardenId, owner: UserId) {
        if self.revoke.is_none() {
            tracing::warn!(%garden, "connectors off: a former owner's connectors stay `deleting` until CONNECTOR_KEY is set");
            return;
        }
        let rows = match self.store.deleting_connectors(Some(garden)).await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::error!(%garden, error = %error, "listing a former owner's connectors failed; they resume on restart");
                return;
            }
        };
        for row in rows.into_iter().filter(|r| r.created_by == owner) {
            let connector = row.connector.id;
            if let Err(error) = self.finish_connector(row).await {
                tracing::error!(%connector, error = %error, "connector deletion stopped; it resumes on restart");
            }
        }
    }

    /// Before an account's user record goes, which `connector.created_by` refuses while a
    /// connector names it: every connector the user created that is still marked `deleting`
    /// (this cascade's transfers and removals, or a run a restart cut off), revoked and
    /// deleted. Without `CONNECTOR_KEY` they are deleted unrevoked, as a garden's are, since the
    /// account deletion can't wait for a key; `revoke_stored` logs each by its id and kind.
    async fn finish_account_connectors(&self, user: UserId) -> Result<(), StoreError> {
        // Not `deleting_connectors(None)`, which skips a garden being deleted: its connector
        // still names the user in `created_by`.
        for row in self.store.deleting_connectors_of_user(user).await? {
            self.finish_connector(row).await?;
        }
        Ok(())
    }

    /// The one revoke-then-delete path of a connector marked `deleting`: revokes its token
    /// (unless another live connector holds the account's), then deletes its records and the
    /// row, 500 records a batch. A failed revoke is logged and the deletion goes on.
    pub async fn finish_connector(&self, row: ConnectorRow) -> Result<(), StoreError> {
        self.revoke_stored(&row).await;
        let connector = row.connector.id;
        while self.store.delete_connector_batch(connector).await? > 0 {}
        tracing::info!(%connector, "connector deleted");
        Ok(())
    }

    /// Unseals a stored connector's tokens and revokes them. Never fails: every outcome is
    /// logged by the connector's id and kind only, never the token or the vendor's body.
    async fn revoke_stored(&self, row: &ConnectorRow) {
        let connector = row.connector.id;
        let kind = row.connector.kind;
        if row.connector.endpoint == ConnectorEndpoint::Era {
            tracing::debug!(%connector, ?kind, "era connector: its demo token needs no revoke");
            return;
        }
        let Some(sealed) = &row.secret else {
            tracing::debug!(%connector, ?kind, "no token to revoke");
            return;
        };
        let Some(revoke) = &self.revoke else {
            tracing::warn!(%connector, ?kind, "connectors off: the token can't be opened to revoke");
            return;
        };
        let tokens = match StoredTokens::unseal(&revoke.key, connector, sealed) {
            Ok(tokens) => tokens,
            Err(error) => {
                tracing::warn!(%connector, ?kind, error = %error, "token not revoked: it doesn't open");
                return;
            }
        };
        self.revoke_tokens(connector, kind, row.connector.account.as_ref(), tokens)
            .await;
    }

    /// Revokes `tokens` for `connector` unless another live connector holds the same account's
    /// token, in a task of its own: a panic in the vendor call is a failed revoke, never the
    /// end of the cascade that asked. Also the `complete` route's revoke of tokens it got but
    /// couldn't store.
    pub async fn revoke_tokens(
        &self,
        connector: ConnectorId,
        kind: ConnectorKind,
        account: Option<&ConnectorAccount>,
        tokens: StoredTokens,
    ) {
        let Some(revoke) = &self.revoke else {
            tracing::warn!(%connector, ?kind, "connectors off: no vendor to revoke with");
            return;
        };
        if self.account_held_elsewhere(connector, kind, account).await {
            return;
        }
        let vendors = Arc::clone(&revoke.vendors);
        let call = self
            .tasks
            .spawn(async move { vendors.revoke(kind, &tokens).await });
        match call.await {
            Ok(Ok(())) => tracing::info!(%connector, ?kind, "token revoked"),
            Ok(Err(error)) => tracing::warn!(
                %connector,
                ?kind,
                error = %error,
                "token revoke failed; the records are deleted anyway"
            ),
            Err(error) => tracing::error!(
                %connector,
                ?kind,
                error = %error,
                "token revoke task failed; the records are deleted anyway"
            ),
        }
    }

    /// `true` when the token must stay: another live `vendor` connector holds this Slack
    /// workspace's bot token or this Google account's grant, or the store couldn't say.
    async fn account_held_elsewhere(
        &self,
        connector: ConnectorId,
        kind: ConnectorKind,
        account: Option<&ConnectorAccount>,
    ) -> bool {
        let key = match account {
            Some(ConnectorAccount::Slack { team_id, .. }) => team_id.clone(),
            Some(ConnectorAccount::GoogleDrive { email }) => email.clone(),
            None => return false,
        };
        match self
            .store
            .shared_account_connectors(kind, key, connector)
            .await
        {
            Ok(0) => false,
            Ok(_) => {
                tracing::info!(%connector, ?kind, "another connector holds this account's token; not revoked");
                true
            }
            Err(error) => {
                tracing::warn!(%connector, ?kind, error = %error, "couldn't check the account's other connectors; not revoked");
                true
            }
        }
    }

    /// Resumes every cascade a restart cut off: targets still marked `deleting_at`, account
    /// deletions whose tombstone has no `clerk_deleted_at`, and connectors still marked
    /// `deleting` outside a garden being deleted (a demotion's or a removal's, cut off between
    /// the store's commit and the last batch).
    pub async fn resume(&self) -> Result<usize, StoreError> {
        let targets = self.store.unfinished_deletions().await?;
        let accounts = self.store.unfinished_account_deletions().await?;
        let mut connectors = self.store.deleting_connectors(None).await?;
        if self.revoke.is_none() && !connectors.is_empty() {
            // A former owner's connectors wait for the key, so their tokens can be revoked.
            tracing::warn!(
                count = connectors.len(),
                "connectors off: connectors marked `deleting` stay until CONNECTOR_KEY is set"
            );
            connectors.clear();
        }
        let count = targets.len() + accounts.len() + connectors.len();
        if !connectors.is_empty() {
            let runner = self.clone();
            self.tasks.spawn(async move {
                for row in connectors {
                    let connector = row.connector.id;
                    if let Err(error) = runner.finish_connector(row).await {
                        tracing::error!(%connector, error = %error, "connector deletion stopped; it resumes on restart");
                    }
                }
            });
        }
        for marker in accounts {
            self.spawn_account_run(marker);
        }
        for target in targets {
            let prefixes = match target {
                DeletionTarget::Garden(garden) => vec![ObjectPrefix::garden(garden)],
                _ => Vec::new(),
            };
            self.spawn(DeletionStarted {
                target,
                prefixes,
                keys: Vec::new(),
                notify: Vec::new(),
                garden_name: None,
            });
        }
        Ok(count)
    }

    /// The synchronous part of `DELETE /me` and `user.deleted`: what happens to each garden
    /// (architecture 5, deletion steps 1–3), for the `202` body.
    pub async fn plan_account(&self, user: UserId) -> Result<AccountDeletion, ApiError> {
        let plan = self.store.plan_account_deletion(user).await?;
        Ok(AccountDeletion {
            deleted_gardens: plan.delete,
            transferred_gardens: plan.transfer,
        })
    }

    /// `DELETE /me` and `user.deleted`: starts the account cascade unless one already runs for
    /// this account (the tombstone's compare-and-set). `false` when it was already running.
    pub async fn start_account(&self, user: UserId, auth_id: AuthId) -> Result<bool, StoreError> {
        let started_at = self.clock.now();
        if !self
            .store
            .start_account_deletion(user, &auth_id, started_at)
            .await?
        {
            tracing::info!(%user, "account deletion already running");
            return Ok(false);
        }
        self.spawn_account_run(AccountDeletionMarker {
            user,
            auth_id,
            started_at,
            user_deleted: false,
        });
        Ok(true)
    }

    fn spawn_account_run(&self, marker: AccountDeletionMarker) {
        let runner = self.clone();
        self.tasks.spawn(async move {
            let user = marker.user;
            if let Err(error) = runner.run_account(marker).await {
                tracing::error!(%user, error = %error, "account deletion stopped; it resumes on restart");
            }
        });
    }

    /// The account cascade, in the background: ownership transfers, the gardens the user alone
    /// used, the shared gardens they leave (their chats there), their scan feedback, the user
    /// record, and last the account at Clerk, retried until it succeeds or the process stops
    /// (the tombstone resumes it). Every step is idempotent, so a resumed run repeats them; a
    /// garden whose own cascade a restart cut off is resumed by `unfinished_deletions`.
    async fn run_account(&self, marker: AccountDeletionMarker) -> Result<(), DeletionError> {
        let user = marker.user;
        if !marker.user_deleted {
            let plan = self.store.plan_account_deletion(user).await?;
            for transfer in plan.transfer {
                self.store.transfer_ownership(transfer, user).await?;
            }
            let now = self.clock.now();
            for garden in plan.delete {
                let owner = authz::check_membership_of::<Owner>(&*self.store, user, garden).await?;
                let started = self.store.start_garden_deletion(owner, now).await?;
                self.run(started).await?;
            }
            for garden in plan.leave {
                let member =
                    authz::check_membership_of::<Viewer>(&*self.store, user, garden).await?;
                let removed = self.store.remove_member(member, user).await?;
                if self.revoke.is_some() {
                    self.finish_member_connectors(garden, user).await;
                }
                if !self.delete_objects(removed.chat_prefixes, Vec::new()).await {
                    return Err(DeletionError::Objects);
                }
                self.delete_records(DeletionTarget::Member { garden, user })
                    .await?;
            }
            self.store.delete_user_feedback(user).await?;
            self.finish_account_connectors(user).await?;
            self.store.delete_user(user).await?;
        }
        let mut wait = Duration::from_secs(1);
        while let Err(error) = self.auth_admin.delete_account(&marker.auth_id).await {
            tracing::warn!(%user, error = %error, ?wait, "Clerk account deletion failed; retrying");
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = self.shutdown.cancelled() => return Err(DeletionError::ShutDown),
            }
            wait = (wait * 2).min(ADMIN_BACKOFF_MAX);
        }
        self.store
            .finish_account_deletion(&marker.auth_id, self.clock.now())
            .await?;
        Ok(())
    }
}
```

| Target | Request | Background |
|---|---|---|
| A garden (`DELETE /gardens/{id}`, owner) | `start_garden_deletion`: `deleting_at`, jobs cancelled, step claims and question steps cleared; `202` | Its prefix in `RustFS`; every cascade step in order; `garden_deleted` pushes to the other members |
| A plant (`DELETE /plants/{id}`, owner) | `start_plant_deletion`; `202` | Its scans' prefixes; the steps up to memories, plants and the map |
| A pending scan (`DELETE /scans/{id}`, editor) | `start_scan_deletion` (`pending` only, `409` otherwise); `204` | Its prefix; its own short step list |
| A member leaving or removed | `remove_member` (the last owner can't), which marks the connectors they created there `deleting` | Their conversations' image prefixes; those connectors, revoked and deleted; their conversations and unshared chat memories in that garden |
| An owner demoted (`PATCH /gardens/{id}/members/{user_id}` to any other role) | `change_role`, which marks the connectors they created there `deleting`; `200` | Those connectors, revoked and deleted (`spawn_connectors_of`) |
| An account (`DELETE /me`, or `user.deleted`) | The plan, for the `202` body; then `start_account_deletion`, the tombstone's compare-and-set: a second request, or the webhook during a `DELETE /me` cascade, starts nothing | Transfers; each solely used garden as above; each shared garden left as a member; scan feedback; the user; the account at Clerk, retried with back-off up to an hour until it succeeds; then `finish_account_deletion` |

**Agent Memory.** A garden's or plant's cascade erases its scope in Agent Memory without a step of its own: `start_garden_deletion` and `start_plant_deletion` queue the subject in `belief_forget` in the transaction that sets `deleting_at` ([store](store.md)), and the `beliefs` task erases it within seconds while Agent Memory is up (section 16). The queued row is a plain link, not a reference, so it outlives the records, and a cascade resumed after a restart needs nothing. From `deleting_at` on, the sweep records nothing of that plant or garden. Account deletion needs nothing: shared memories stay with the garden (a garden deleted with the account is queued as above) and private ones were never sent.

The record steps are `CascadeStep::steps_for(target)` in [database 4](../database/README.md#deletion-) order, each a batch of 500 repeated until it deletes nothing; a missed step fails loudly on `REFERENCE ON DELETE REJECT` instead of orphaning.

**Objects first, never cancelled, one queue.** On `RustFS` 1.0.1 an interrupted `DeleteObjects` can leave objects that can never be listed or deleted ([storage 4](../storage/README.md#4-writes-and-deletes)). Every `RustFS` delete in `api` goes through `AppState::deletes`, one `blob::DeleteQueue` ([blob 6](blob.md#6-deletes-that-are-never-cancelled)): cascades, a removed shot (`DELETE /scans/{id}/images/{image_id}`), depth maps a failed upload had written, and the orphan sweep. The queue runs each job in a task it owns, so dropping the caller or its handle never stops a delete; each attempt is bounded only by the client's 120-second operation timeout; a job gets 5 attempts with 1–8 s back-off (deleting a missing key is a no-op); and shutdown drains it (section 4). A cascade awaits its handles and deletes records only when every object is gone; otherwise it stops with `DeletionError::Objects`, the target stays marked, and the next start retries it from the objects ("a failure partway is retried from the start"). A removed shot or an upload's depth maps are submitted without waiting: their records are gone or never existed, so the orphan sweep finds what a delete left.

**Resumable.** At start, `unfinished_deletions` lists every target still marked `deleting_at`, and each is run again from its first step. A garden's objects are one prefix, deleted again; a plant's or scan's objects, whose keys aren't known any more once their records are gone, are left to the orphan sweep, which deletes them once nothing points to them.

**The account tombstone** (`deleted_user:[auth_id]`, `AccountDeletionMarker`, [database 4](../database/README.md#deletion-)) is written first, by `start_account_deletion`, as a compare-and-set on its absence, so two cascades for one user can't run at once: a `DELETE /me` repeated, or a dashboard deletion's `user.deleted` arriving while a `DELETE /me` cascade runs, gets `false` and starts nothing. While it exists, `upsert_user` refuses the auth id, so a session token still valid after the deletion answers `401` and never creates the user again (section 9). It also makes the Clerk deletion durable: `resume` lists `unfinished_account_deletions` (no `clerk_deleted_at`), re-runs the record steps when the user record is still there (each step is idempotent; a garden whose own cascade was cut off is resumed from `deleting_at`), and retries Clerk until it answers; `finish_account_deletion` records it, and the `cleanup` task deletes the tombstone 24 h later (`TOMBSTONE_KEEP`, far longer than a Clerk session token lives). Clerk answers `404` for an account already gone (a dashboard deletion), which counts as success.

**Farm connectors** ([connectors 8](../connectors/README.md#8-privacy-and-security); roadmap E3.6). A connector goes with its garden, with the owner who created it when that owner stops being one, and when its owner disconnects it. The store marks it `deleting` in the transaction that changes the garden, the membership or the connector: `start_garden_deletion`, `change_role` from `owner` (which returns `RoleChange { demoted_owner }`, so the route starts the task only for a demotion), `remove_member`, account deletion's `transfer_ownership`, and `start_connector_deletion`. The vendor's revoke is an HTTP call, so the runner makes it after that commit and never inside a transaction, through one path, `finish_connector`: it skips the revoke for an `era` connector (the fixed demo string of start-up seeding) and one without a token, opens the rest with `StoredTokens::unseal`, and leaves the token alone while another live `vendor` connector holds the same account (`shared_account_connectors`: Slack gives one bot token per app and workspace, and revoking Google's refresh token ends the whole grant of that account to our client; a store error counts as held). Otherwise it calls `ConnectVendors::revoke` (the routes' own instance; Google's refresh token first) in a task of its own on the tracker, so a panic is a `JoinError` logged as one failed revoke. The outcome is logged by connector id and kind only, never the token or the vendor's body, and a failure never stops what follows: `delete_connector_batch` until it deletes nothing (records 500 at a time, then the row). A garden's connectors are finished just before its `FarmRecordsAndConnectors` step, whose own batches then delete anything left; a removed member's before their chat objects, and a store error listing them is logged without skipping the chat steps; a demotion's in a task of its own (`spawn_connectors_of`); a disconnect's with `spawn_connector`.

**Without `CONNECTOR_KEY`** nothing can be unsealed. A garden's cascade still deletes its connectors, unrevoked, because the garden goes. An account deletion can't wait either: before `DELETE $user`, which `connector.created_by` rejects while a connector names the user, `finish_account_connectors` deletes every connector the user created that is still `deleting` (listed by creator through `deleting_connectors_of_user`, in any garden, including one being deleted, which `resume`'s listing skips), with its records, unrevoked, each logged by its id and kind (with the key, it revokes them first, and finishes a run a restart cut off). A demoted, removed or leaving owner's connectors stay `deleting`, hidden from the list and the sync, and `resume` logs a warning at each start instead of finishing them, so their tokens are revoked once the key is configured.

**Why revoke after the mark and before the delete.** Revoking after the rows were gone would leave the only copy of the token in memory, and a crash in between would lose the revoke for good. With the mark committed first, the row keeps its sealed token until the revoke has been asked for, so `resume` can finish it: it lists `deleting_connectors(None)` (every connector still `deleting` outside a garden being deleted, whose resumed cascade finishes its own) and runs each through `finish_connector`. The worst case after a restart is a second revoke of a token already revoked, which both vendors answer as revoked (Slack's `token_revoked` and `invalid_auth`, Google's `invalid_token`). The tests record the revokes through a `ConnectVendors` fake: one revoke per connector, after the store's marking call and before the first batch; a failed or panicking revoke followed by every batch; a shared account's token kept; a restart between the commit and the revoke finished by `resume`; and, with connectors off, a former owner's connectors left `deleting` while a garden's are deleted.

**Account deletion from the webhook** has no `CurrentUser` (no session token), so it uses `authz::check_membership_of`, an additive domain function that reads the role for a `UserId` the verified webhook named (a domain change this crate made; the proof still says what the store says).

---

## 18. Media

```rust
/// `?exp=&sig=` of a signed URL.
#[derive(Debug, Clone, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SignedQuery {
    /// Expiry, Unix seconds.
    pub exp: i64,
    /// HMAC-SHA256, lowercase hex.
    pub sig: String,
}

/// Checks the signature for `path` at the current time.
pub fn verify(state: &AppState, path: &ImagePath, query: &SignedQuery) -> Result<(), ApiError> {
    verify_signed(
        &*state.ports.signer,
        path,
        query.exp,
        &query.sig,
        state.ports.clock.now(),
    )
    .map_err(|_| invalid_signature())
}

/// Streams an object. The body is the blob store's `'static` chunk stream handed straight to
/// hyper: if the client goes away, dropping it closes the connection to `RustFS`. It ends when
/// `shutdown` fires, because the route's `TimeoutLayer` bounds only the response head and a slow
/// download would otherwise hold `axum::serve`'s graceful shutdown open; the client sees a short
/// body and fetches the image again from the next process.
pub async fn stream_object(
    blob: &dyn BlobStore,
    object: ObjectRef,
    shutdown: &CancellationToken,
) -> Result<Response, ApiError> {
    let fetched = blob.get(&object.key).await?;
    let body = fetched.body.take_until(shutdown.clone().cancelled_owned());
    let mut response = (StatusCode::OK, Body::from_stream(body)).into_response();
    let headers = response.headers_mut();
    let content_type = HeaderValue::from_str(&fetched.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static(object.content_type.as_str()));
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=900"),
    );
    if let Some(etag) = fetched.etag.and_then(|e| HeaderValue::from_str(&e).ok()) {
        // The S3 ETag already carries its quotes; passed through verbatim.
        headers.insert(header::ETAG, etag);
    }
    if let Some(length) = fetched.length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    Ok(response)
}
```

```rust
/// `GET /scans/{id}/images/{image_id}?exp=&sig=`: the face-blurred photo, `image/webp`.
#[utoipa::path(get, path = "/scans/{id}/images/{image_id}", tag = "media", security(()),
    params(("id" = String, Path), ("image_id" = String, Path), SignedQuery),
    responses((status = 200, content_type = "image/webp"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn scan_image(
    State(state): State<SharedState>,
    ApiPath((scan, image)): ApiPath<(ScanId, ImageId)>,
    ApiQuery(query): ApiQuery<SignedQuery>,
) -> Result<Response, ApiError> {
    media::verify(&state, &ImagePath::ScanImage { scan, image }, &query)?;
    let object = state
        .ports
        .store
        .image_object(scan, image)
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}

/// `GET /species/{id}/images/{hash}?exp=&sig=`: a species reference photo from `knowledge/`.
#[utoipa::path(get, path = "/species/{id}/images/{hash}", tag = "media", security(()),
    params(("id" = String, Path), ("hash" = String, Path), SignedQuery),
    responses((status = 200, content_type = "image/webp"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn species_photo(
    State(state): State<SharedState>,
    ApiPath((species, hash)): ApiPath<(SpeciesId, String)>,
    ApiQuery(query): ApiQuery<SignedQuery>,
) -> Result<Response, ApiError> {
    let hash = PhotoHash::parse(&hash).ok_or_else(ApiError::not_found)?;
    media::verify(
        &state,
        &ImagePath::SpeciesPhoto {
            species: species.clone(),
            hash,
        },
        &query,
    )?;
    let object = state
        .ports
        .store
        .species_photo_object(&species, hash)
        .await?
        .ok_or_else(ApiError::not_found)?;
    media::stream_object(&*state.ports.blob, object, &state.shutdown).await
}
```

The signature is the authorisation ([storage 3](../storage/README.md#3-how-images-are-served)): no `Authorization` header, so plain `<img>` tags work in the web build. `verify_signed` ([domain 5](domain.md#5-signed-url-typing)) refuses an `exp` in the past or more than 15 minutes ahead and compares the HMAC in constant time (`Hmac::verify_slice`); all three are `403 invalid_signature`. A valid signature for a record that no longer exists is `404`. The body is the blob store's `'static` chunk stream handed to hyper with `Body::from_stream` (`BlobError` implements `std::error::Error + Send + Sync`, which `Body` needs); if the client goes away, dropping it closes the connection to `RustFS`, which is fine for reads. Headers: `Content-Type`, `ETag` passed through verbatim (S3 ETags carry their quotes), `Content-Length`, `Cache-Control: private, max-age=900`. Compression skips `image/*`. Depth maps have no route.

✓ The HMAC: `signed_urls_verify_until_they_expire` signs a URL with the real `HmacSigner`, verifies it, refuses it at 16 minutes, and refuses a tampered signature.

---

## 19. Rate limits and budgets

```rust
/// The keyed limiters, keyed by the authenticated user, so an unauthenticated caller can't
/// fill the key map with strings of its choosing (the limiter runs after the token check).
pub struct RateLimits {
    /// Every authenticated request: `RATE_PER_MINUTE` (default 120, a placeholder: the documents
    /// give no number).
    pub per_user: DefaultKeyedRateLimiter<UserId>,
    /// `POST /invites/{code}/accept`: at most 10 in any minute per user ([contracts 9]), as a
    /// burst of 5 then one every 12 s. `Quota::per_minute(10)` would be a burst of 10 refilling one
    /// every 6 s, which lets about 20 through in a fresh minute; a burst of `n / 2` refilling
    /// `n / 2` a minute never lets more than `n` through in any 60 s.
    pub invite_accept: DefaultKeyedRateLimiter<UserId>,
}

/// Checks one key: `429 rate_limited` with `retry_after_s` rounded up, never 0.
pub fn check(limiter: &DefaultKeyedRateLimiter<UserId>, user: UserId) -> Result<(), ApiError> {
    limiter.check_key(&user).map_err(|not_until| {
        let wait = not_until.wait_time_from(limiter.clock().now());
        let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
        ApiError::rate_limited(u32::try_from(seconds).unwrap_or(u32::MAX))
    })
}
```

| Limit | Where | Answer |
|---|---|---|
| General: `RATE_PER_MINUTE` per user (120, a placeholder) | `AuthUser`, after the token | `429 rate_limited`, `retry_after_s` rounded up |
| Invite codes: at most 10 per user in any minute ([contracts 9](../architecture/contracts.md#members-and-invites)), held strictly as a burst of 5 then one every 12 s (`Quota::per_minute(5)`), because `per_minute(10)` would let about 20 through in a fresh minute (a burst of 10, then one every 6 s) | `accept_invite` | `429 rate_limited` (✓ `the_sixth_quick_invite_attempt_waits_12_seconds`) |
| Daily scans: 30 per user | `ScanRepo::create_scan`, counted in the scan's transaction ([database 4](../database/README.md#limits-and-counters-)) | `429 budget_exceeded`, `budget: "scans"`, `resets_at` the next UTC midnight, `retryable: false` |
| Daily chat tokens | The agent's `start_turn` | `429 budget_exceeded`, `budget: "chat_tokens"` |

governor's `Quota::per_minute(n)` allows a burst of `n` and replenishes one cell every `60/n` seconds. The limiters are keyed by `UserId` and live in memory, which suits the single instance; a keyed limiter keeps every key it has seen, so the `idle` task prunes it.

---

## 20. Concurrency, lifetimes and generics

### What runs where

| Work | Where | Shared state |
|---|---|---|
| Handlers, extractors, streams | tokio's multi-threaded runtime | `Arc<AppState>` |
| Upload image pipelines | `spawn_blocking`, behind `UPLOAD_CPU_PERMITS` | `Arc<dyn ImageProcessing>` |
| One writer actor per active scan | a task on the tracker | its `mpsc::Receiver`; the hub map (`std::sync::Mutex`, never held across `.await`) |
| SSE streams | hyper's connection tasks | a `broadcast::Receiver` each |
| Follow-ups (steps, question steps) | tasks on the tracker | the pipeline |
| Scheduler loops | tasks on the tracker | |
| Object deletes | tasks owned by the `DeleteQueue`'s own `TaskTracker`, never cancelled, drained at shutdown | `Arc<dyn BlobStore>` inside the queue |
| Idempotency release on drop | `tokio::spawn` from `Drop` | |

The hub, writer and plan-lock maps use `std::sync::Mutex`: their critical sections are one insert, lookup or `retain`, never across `.await`, which tokio's own guidance prefers to an async mutex. A poisoned lock is still used (`PoisonError::into_inner`): every operation leaves the map consistent. The plan lock itself is a `tokio::sync::Mutex` because it *is* held across `.await` (the plan writer's call); `lock_owned` gives an owned guard that can be moved into a task.

### Lifetimes and `'static`

| Item | Lifetime | Why |
|---|---|---|
| `scan_stream(..) -> impl Stream + Send + 'static` | `'static` | axum moves the body into hyper's connection task, so the stream owns everything it uses: the `Arc<AppState>`, the proof (`Copy`), the scan id, the receiver. Under edition 2024 an `impl Trait` return captures every in-scope lifetime; the function takes only owned arguments, so nothing is captured and `'static` holds |
| `events(..) -> Sse<impl Stream + Send + use<>>` | `use<>` | The handler takes extractors by value and `State` by value; the empty `use<>` bound says the opaque type captures no lifetime and no type parameter |
| `TurnParts.events: BoxStream<'static, ChatEvent>` | `'static` | The agent's stream is moved into the response body. Inside the agent, `ChatModel::stream<'a>` borrows its request for `'a`; the agent's task owns the request and the model `Arc`, so what crosses to `api` is owned ([domain 10](domain.md#10-concurrency-and-lifetimes)) |
| `Signer<'a>` | `'a` borrows the `UrlSigner` and `SpeciesPhotos` ports | `AppState::signer(&self) -> Signer<'_>`: one signer per response or per event send, `Copy`, passed by reference to each `sign`, so no `Arc` is cloned per URL. Signing happens before serialisation: `sign` turns `X<Stored>` into an owned `X<Signed>`, and `Wire<T>` serialises that, so no borrow outlives the handler. In the stream, a signer is made for each event and dropped before the next `.await` |
| `UploadInput<'a>`, `DepthInput<'a>` | `'a` borrows the closure's owned `Bytes` | `spawn_blocking`'s closure must be `'static`; it owns the buffers and lends them for one call |
| `WebhookHeaders<'a>` | `'a` borrows the request's headers | Inputs of one synchronous verify call |
| `CancellationToken::cancelled_owned()` | `'static` | An owned future, for `take_until` and `with_graceful_shutdown`, which can't hold a borrow of the token |

### Generics

| Generic | Where | Why |
|---|---|---|
| `S` with `Arc<AppState>: FromRef<S>`, `S: Send + Sync` | every extractor | Section 9: works on any state that yields an `Arc<AppState>`; `Sync` because the `Send` future holds `&S` |
| `R: MinRole` | `Access<R, T>`, `ConvAccess<R, T>`, `AppState::member::<R>` | The minimum role as a type; the proof carries it to the repository call |
| `T: Scoped` / `T: ConvScoped` | the same | The table of the `{id}` capture, which says how to find its garden or conversation |
| `T: DeserializeOwned + Send` | `IdemJson<T>` | Any request body; `Send` because the extractor's future holds it |
| `T: Serialize + ToSchema` | `Wire<T>`, `Begun::finish<T>` | Only signed records are `ToSchema`, so an unsigned one can't be returned |
| `F: Fn(Arc<AppState>) -> Fut`, `Fut: Future + Send + 'static` | `scheduler::every` | A task body; owned by a spawned loop |
| `Handled<T>` | the seams | A response value with an optional follow-up |

**No `#[async_trait]` in `api`'s own extractors**: axum 0.8's traits use return-position `impl Trait`, so native `async fn` works, and static dispatch is all they need. The seams (`ScanWriter`, `PipelineApi`, `ChatTurns`) are trait objects, so they use `#[async_trait]` like the domain ports: an `async fn` in a trait is not dyn compatible on Rust 1.99 ([domain 10](domain.md#10-concurrency-and-lifetimes)). `ImageProcessing` is synchronous and needs neither.

---

## 21. Testing

| What | How |
|---|---|
| Handlers | The real router (`router::app`), so layers, extractors, the renderer and request ids are under test, driven with `tower::ServiceExt::oneshot`. The ports are mockall mocks from `domain`'s `mock` feature; the store is `tests/fake_store.rs`, a `Store` made of one mock per repository trait with every method delegated, **generated** from `domain::ports::store` by `backend/tools/apigen/gen_fake_store.py` (regenerate when a store trait changes). The generator scans each trait body for `async fn` signatures that end in `;` or open a default body, skipping the body by brace matching, so a method with a default body (`live_lease`, `soil_profile_failed`, `gardens_due_soil_profile`, the account-tombstone methods) is delegated like any other and a test sets its expectations. The seams are small fakes: `NoPipeline` and `NoChat` (`unreachable!()`), `FixedImages` with a fixed answer. `tests/pipeline.rs` builds the real `pipeline::Pipeline` over the same fake store (`state_with_pipeline`), with a recording `ScanWriter` and `unreachable!()` measurement and plan writer, to drive `Pipeline::job_completed` and `POST /plants/{id}/species` |
| Time | `FixedClock`; tokio's paused clock (`start_paused = true`) for the broker's long poll |
| Scheduler | `scheduler::fetch_soil_profile` called directly, with a mocked `SoilProvider`, and with the adapter `wiring.rs` builds (`SoilGrids` over `wiremock`, answering Toronto's live null pixel, and the embedded SLC lookup) |
| Farm sync | `farm_sync/tests.rs`: `FarmSync` on the embedded `SurrealDB` 3.3.4 engine (`surrealdb` with `kv-mem`, a dev-dependency) with a scripted `FarmSource`, a counting embedder and `FixedClock`: at most 4 due connectors a run and `next_sync_at` 15 minutes on (T-E4.1); unchanged text costs no embedding and a changed chunk is embedded (T-E4.7), and so is one whose author or channel was renamed (gap G12), and records stored with the text-only hash under a cursor without `text_version` are read again from the start and embedded again once, keeping their ids; a refused token makes the connector `reauth_needed` and keeps the records, the 10,001st record stops it with `too_large`, and the cleanup deletes old Slack messages but keeps an old document (T-E4.9); seeding twice gives one pair of sealed `era` connectors, a missing garden is skipped, and a garden or a connector being deleted is reported and left so (T-E4.10); a throttled pass resumes from its cursor; ten quiet paced sources each have their turn within four runs, once and in order, the round then ends synced, and the next round starts one further on; no run reports `Synced` while a source is mid-pass, a source added meanwhile included, and a source left mid-pass resumes from its cursor; a new source with a 1,000-page first pass leaves nine others read in every round, and the round it finishes in sets `synced_at`; a source that always fails leaves the others read in every round, the connector keeps its error and no `synced_at` until it answers; a connector without sources reads nothing and waits 15 minutes; a source added later joins the others' round; older-item checks are two a run over every source, none on a first pass, and a page is offered only as many as its budget covers; a page of a source dropped meanwhile doesn't count toward the limit, and the limit counts what another connector stored during the run; the daily re-read deletes what the vendor dropped and keeps what it reported as kept; a re-read catching up after a month keeps only its window's ids in the cursor, an unchanged thread's when its record is in the window, and deletes nothing older; an outage waits 15 minutes. Its `drive` module runs `DriveSource` against a `wiremock` Google with a mocked `ConnectVendors`: an expiring token renewed before the run and stored over the one read, a refused token renewed once and the page asked again, a refused refresh making the connector `reauth_needed`, an `era` connector never renewed, a renewal that loses the compare-and-set keeping its own token for the run while the other grant stays stored, a token refused again after its renewal making the connector `reauth_needed`, nested folders read and a file moved out losing its records, a resync deleting the records of files no longer listed, a document cut at 1 MB before chunking, and a `429` that keeps the first page and the cursor (T-E4.3, T-E4.5). `wiring/connect.rs` tests `VendorClients::refresh` against a mocked token endpoint: an expiring token renewed and a fresh one left, `force` renewing a fresh one, the old refresh token kept when Google sends none, and `expires_at` the clock plus `expires_in`. `farm_sync/tests_era.rs` runs the Slack sync against Era's Slack simulator on the Maple Row Farm fixture (`#[ignore]`d, `era_` names, the nextest `era` group), with `integrations`' `era-sim` harness as a dev-dependency: the first sync stores the chosen channel with authors, links and joined replies (T-E4.2); a quiet channel costs one `conversations.history` call and no embedding, a new message arrives, and a new reply arrives through the re-read, with the `conversations.replies` calls counted exactly through a forwarding proxy that records each `limit` (one check, the 3-minute budget's, and no full read on a quiet pass, one full read for the new reply) (T-E4.4, T-E4.7); a check, one a run in turn over the two threads due one, finds a new reply to a thread older than 7 days and reads it in full; the re-read deletes a message gone at the vendor (T-E4.6); after `day2` and 91 days, the first run, a re-read, reads from each channel's newest message, so a message from the quarter's middle arrives with the last week's and a new thread with its reply, each once, with one `conversations.history` call per channel and nothing deleted; the 91 days away don't count toward a thread's quiet days, so the next sync's check finds a new reply to a thread active at the `day1` sync, and one to a thread quiet for 44 days then is missed; the 180-day cleanup then deletes the 47 fixture messages older than 180 days, and the next day's re-read brings none back (T-E4.5); a throttled first sync resumes without duplicates, `fail_next` is waited out, and an outage is `vendor_unavailable` for 15 minutes (T-E4.8); no log line or record holds a fixture email or a message's text (T-E4.11). Its `drive` module runs the Drive sync on an `era` connector against Era's Drive simulator: every file under `F_FARM`, a nested folder's included, becomes records and nothing outside it does, the Sheet's text is its first tab and the PDF is kept by name (T-E4.3); after `day2` and 91 days, the changed files arrive through `changes.list`, read past Northwind's thousands of changes, with no listing again and no record stored twice (T-E4.5); a change token Drive refuses (`400`) gets a new start token and the folder listed again, the records of a file gone from the folder are deleted and the others kept with their ids and vectors, and the next sync reads `changes.list` (E4.3's resync); a throttled Drive sync finishes on a later run |
| Pure parts | Config parsing, error mapping and statuses, the fallback renderer, canonical fingerprints, multipart fingerprints, the HMAC signer, the invite limiter, trigger rules, hour truncation, worker tokens |

```rust
#[tokio::test]
async fn an_upload_that_fails_the_quality_check_is_422_and_never_stored() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    let mut blob = MockBlobStore::new();
    blob.expect_put().never();
    store.scan_repo.expect_add_image().never();
    let images = FixedImages(|| {
        Err(UploadError::Quality {
            problem: QualityProblem::TooBlurry,
            blur_score: 41.2,
            exposure: 0.52,
        })
    });
    let app = crate::router::app(state(store, auth, blob, images));

    let boundary = "XyZ";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\nleaf_closeup\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\nnot really a jpeg\r\n--{boundary}--\r\n"
    );
    let request = Request::post(format!("/scans/{SCAN}/images"))
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "image_quality");
    assert_eq!(body["error"]["retryable"], true);
    assert_eq!(body["error"]["details"]["problem"], "too_blurry");
    assert_eq!(body["error"]["details"]["hint"], "tap_to_focus");
    assert!(
        body["error"]["request_id"]
            .as_str()
            .unwrap()
            .starts_with("req_")
    );
}
```

```rust
#[tokio::test]
async fn a_repeated_key_replays_the_stored_response_without_running_the_handler() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Editor));
    let stored = serde_json::json!({ "id": SCAN, "status": "pending" });
    let replay = stored.clone();
    store
        .idempotency_repo
        .expect_begin()
        .times(1)
        .returning(move |scope, fingerprint, _| {
            assert_eq!(scope.route, "POST /scans");
            assert_eq!(fingerprint.len(), 64);
            Ok(IdempotencyBegin::Replay(StoredResponse {
                status: 201,
                body: replay.clone(),
            }))
        });
    store.scan_repo.expect_create_scan().never();
    store.idempotency_repo.expect_finish().never();
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));

    let request = Request::post("/scans")
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "7d444840-9dc0-11d1-b245-5ffdce74fad2")
        .body(Body::from(format!(
            r#"{{"garden_id":"{GARDEN}","new_plant":{{"name":"Tomato"}}}}"#
        )))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(json(response).await, stored);
}
```

```rust
#[tokio::test]
async fn a_non_member_gets_the_same_404_as_an_unknown_scan() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, None);
    store.scan_repo.expect_scan_detail().never();
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));

    let response = app.oneshot(get(&format!("/scans/{SCAN}"))).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let mut body = json(response).await;
    body["error"]["request_id"] = serde_json::Value::Null;
    assert_eq!(
        body,
        serde_json::json!({ "error": {
            "code": "not_found", "message": "Not found.", "retryable": false, "request_id": null
        }})
    );
}
```

```rust
#[tokio::test]
async fn sse_resume_replays_after_last_event_id_then_goes_live_without_duplicates() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    store.event_repo.expect_stream_state().returning(|_| {
        Ok(StreamState {
            status: ScanStatus::Analysing,
            question_unresolved: false,
            guided_check_analysing: false,
            step_claimed: false,
        })
    });
    store
        .event_repo
        .expect_events_after()
        .times(1)
        .returning(|_, _, after| {
            assert_eq!(after, EventSeq(1));
            Ok(vec![event(2), event(3)])
        });
    let state = state(store, auth, MockBlobStore::new(), no_images());
    let app = crate::router::app(Arc::clone(&state));

    let request = Request::get(format!("/scans/{SCAN}/events"))
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header("last-event-id", "1")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut body = response.into_body();
    // The replay: 2 then 3. The subscription was made before it.
    let mut seen = String::new();
    while ids(&seen).len() < 2 {
        let frame = body.frame().await.unwrap().unwrap();
        seen.push_str(std::str::from_utf8(frame.data_ref().unwrap()).unwrap());
    }
    assert_eq!(ids(&seen), ["2", "3"]);
    assert!(seen.contains("event: stage_started\n"));
    // Live: a repeat of 3, then 4, then 5, which brings the scan to rest.
    let scan: ScanId = SCAN.parse().unwrap();
    state.hub.publish(
        scan,
        Published {
            event: event(3),
            rests: false,
        },
    );
    state.hub.publish(
        scan,
        Published {
            event: event(4),
            rests: false,
        },
    );
    state.hub.publish(
        scan,
        Published {
            event: event(5),
            rests: true,
        },
    );
    // `collect` returns only because the stream ends after 5.
    let rest = tokio::time::timeout(Duration::from_secs(5), body.collect())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ids(std::str::from_utf8(&rest.to_bytes()).unwrap()),
        ["4", "5"]
    );
}
```

The paths added in v0.3 each have a test. A worker's events after its job completed answer `204` and never reach the pipeline (`NoPipeline::worker_events` would panic); a tombstoned account's token is `401 unauthenticated` and `me` is never read; a second `DELETE /me` gets `false` from the tombstone's compare-and-set, answers `202` and spawns no cascade; and `scheduler::fetch_soil_profile` stores the next attempt 7 days after `NotFound` and 15 minutes after an outage:

```rust
#[tokio::test]
async fn late_worker_events_after_complete_are_204_and_dropped() {
    let mut store = FakeStore::default();
    // The job was completed with this token: `live_lease` answers `None`. `NoPipeline`'s
    // `worker_events` panics, so forwarding the events would fail the test.
    store
        .job_repo
        .expect_live_lease()
        .times(1)
        .returning(|lease, _| {
            assert_eq!(lease.job.to_string(), JOB);
            assert_eq!(lease.token.to_string(), LEASE);
            Ok(None)
        });
    store.job_repo.expect_check_lease().never();
    let app = crate::router::app(state(
        store,
        MockAuthVerifier::new(),
        MockBlobStore::new(),
        no_images(),
    ));
    let request = Request::post(format!("/internal/jobs/{JOB}/events"))
        .header(header::AUTHORIZATION, format!("Bearer {}", "x".repeat(40)))
        .header("x-lease-token", LEASE)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"v":1,"events":[{"type":"stage_started","stage":"plant_check"}]}"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
```

```rust
#[tokio::test]
async fn a_tombstoned_accounts_token_is_401_and_never_creates_a_user() {
    let mut store = FakeStore::default();
    let mut auth = MockAuthVerifier::new();
    auth.expect_verify().returning(|_, _| {
        Ok(VerifiedToken {
            auth_id: "user_2abc".parse().unwrap(),
            display_name: None,
            expires_at: "2026-10-18T15:00:00Z".parse().unwrap(),
        })
    });
    // The store refuses a tombstoned auth id with `NotFound` (`IdentityRepo::upsert_user`).
    store
        .identity_repo
        .expect_upsert_user()
        .times(1)
        .returning(|_, _| Err(domain::ports::StoreError::NotFound));
    store.identity_repo.expect_me().never();
    let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
    let response = app.oneshot(get("/me")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json(response).await["error"]["code"], "unauthenticated");
}
```

```rust
#[tokio::test]
async fn a_second_delete_me_while_a_cascade_runs_starts_nothing() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Owner));
    store
        .deletion_repo
        .expect_plan_account_deletion()
        .returning(|_| Ok(domain::ports::AccountDeletionPlan::default()));
    // The tombstone already exists: the compare-and-set answers `false`.
    store
        .deletion_repo
        .expect_start_account_deletion()
        .times(1)
        .returning(|user, auth_id, _| {
            assert_eq!(user.to_string(), USER);
            assert_eq!(auth_id.as_str(), "user_2abc");
            Ok(false)
        });
    store.deletion_repo.expect_transfer_ownership().never();
    store.deletion_repo.expect_delete_user().never();
    let app_state = state(store, auth, MockBlobStore::new(), no_images());
    let app = crate::router::app(Arc::clone(&app_state));
    let request = Request::delete("/me")
        .header(header::AUTHORIZATION, "Bearer session-token")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(app_state.tasks.len(), 0, "no second cascade was spawned");
}
```

Agent Memory (`tests/beliefs.rs`, over `MockBeliefStore` and a hand-written `SlowBeliefs`): `GET /plants/{id}/beliefs` answers `available: false` without Agent Memory, on an error and after 3 s of a slow store (paused time); returns the current beliefs and only the superseded history; is `404` to a non-member. That a cascade queues its erase is a store test (`remove_fail_feedback_and_plant_deletion` and the account flow, [store](store.md)), and the sweep is the agent's (`beliefs`, [agent](agent.md)). The config tests cover the three variables: none set, all set (the key never in the formatted config), and a partial set refused naming the missing one.

```rust
#[tokio::test]
async fn a_member_reads_current_beliefs_and_the_superseded_history() {
    let mut beliefs = MockBeliefStore::new();
    beliefs.expect_enabled().return_const(true);
    beliefs
        .expect_current()
        .withf(|g, p| *g == garden() && *p == Some(plant()))
        .times(1)
        .returning(|_, _| Ok(vec![belief("watering", "Watered every 2 days.", None)]));
    beliefs
        .expect_history()
        .withf(|s| s.garden == garden() && s.plant == Some(plant()))
        .times(1)
        .returning(|_| {
            Ok(vec![
                belief("watering", "Watered every 2 days.", None),
                belief(
                    "watering",
                    "Watered every 3 days.",
                    Some("2026-10-17T15:00:00Z"),
                ),
            ])
        });
    let (status, body) = get_beliefs(Some(Role::Viewer), Arc::new(beliefs)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!({
            "available": true,
            "current": [{
                "key": "watering", "value": "Watered every 2 days.", "about_plant": true,
                "learned_at": "2026-10-14T15:00:00Z"
            }],
            "history": [{
                "key": "watering", "value": "Watered every 3 days.", "about_plant": true,
                "learned_at": "2026-10-14T15:00:00Z", "superseded_at": "2026-10-17T15:00:00Z"
            }]
        })
    );
}
```

```rust
#[tokio::test]
async fn soil_failures_schedule_the_next_attempt_by_their_kind() {
    let garden: GardenId = GARDEN.parse().unwrap();
    let at = domain::records::weather::Coordinates::new(43.65, -79.38).unwrap();
    let now: Timestamp = NOW.parse().unwrap();
    for (error, wait) in [
        (
            (|| domain::ports::ProviderError::NotFound {
                provider: "soilgrids",
            }) as fn() -> _,
            Duration::from_hours(168),
        ),
        (
            || domain::ports::ProviderError::Unavailable {
                provider: "soilgrids",
                source: "timeout".into(),
            },
            Duration::from_mins(15),
        ),
    ] {
        let mut store = FakeStore::default();
        store
            .garden_repo
            .expect_soil_profile_failed()
            .times(1)
            .returning(move |g, retry_at| {
                assert_eq!(g, garden);
                assert_eq!(retry_at, now.plus(wait));
                Ok(())
            });
        store.garden_repo.expect_set_soil_profile().never();
        let app_state = soil_state(store, error);
        crate::scheduler::fetch_soil_profile(&app_state, garden, at)
            .await
            .unwrap();
    }
}
```

The 34 tests: `an_upload_that_fails_the_quality_check_is_422_and_never_stored`, `a_repeated_key_replays_the_stored_response_without_running_the_handler`, `a_key_still_running_is_409_request_in_progress`, `a_non_member_gets_the_same_404_as_an_unknown_scan`, `a_viewer_is_403_with_the_required_role`, `unknown_paths_get_an_error_body_and_a_request_id`, `sse_resume_replays_after_last_event_id_then_goes_live_without_duplicates`, `a_resting_event_stored_during_the_replay_still_ends_the_stream`, `a_resting_point_reached_without_an_event_closes_the_stream`, `deleting_the_scan_closes_its_stream`, `a_scan_at_rest_replays_and_closes`, `an_empty_queue_answers_none_at_the_deadline_after_polling`, `signed_urls_verify_until_they_expire`, `an_image_body_ends_on_the_shutdown_token`, `the_sixth_quick_invite_attempt_waits_12_seconds`, `key_order_doesnt_change_the_fingerprint`, `the_fingerprint_covers_bytes_not_boundaries`, `every_code_keeps_its_status_and_retryable`, `a_timeout_503_carries_retry_after_like_any_other`, `store_conflicts_map_to_their_codes`, `unknown_statuses_fall_back_to_a_contract_code`, `defaults_and_derived_values`, `short_signing_key_is_refused`, `frost_and_a_humid_stretch_fire_on_their_dates`, `hours_truncate`, `identifies_only_exact_tokens`, `ids_used_by_the_tests_parse`, `late_worker_events_after_complete_are_204_and_dropped`, `a_claim_body_over_the_json_limit_is_refused_before_the_handler`, `a_tombstoned_accounts_token_is_401_and_never_creates_a_user`, `a_second_delete_me_while_a_cascade_runs_starts_nothing`, `soil_failures_schedule_the_next_attempt_by_their_kind`, `a_toronto_garden_gets_its_soil_profile_from_slc`, `a_signal_handler_that_fails_to_install_never_fires`.

**Still to add once the crates are wired:** a graceful-shutdown test with an open stream (the research probe's), the writer actor's retire race, the chat stream's interrupt and replay, and an end-to-end run against `SurrealDB` and `RustFS` in containers (the project's `rust-testing` skill has the patterns).

---

## 22. Decisions on the contracts' open questions

The questions of [contracts 12](../architecture/contracts.md#12-open-questions) that `api` settles, as the code does it:

| Question | Decision | Where |
|---|---|---|
| 12, timestamp precision | Whole seconds on the wire (`domain::wire::Timestamp`), full precision in the database | domain 4; every response here |
| 13, unknown fields in app requests | Rejected: `400 invalid_request` naming the field (`deny_unknown_fields` on the domain's request types, the path from `serde_path_to_error`). So the API is deployed before an app that sends a new field. Worker requests keep ignoring unknown fields | Sections 8, 10 |
| 14, a malformed id | In a **path**: `404 not_found`, exactly like an unknown id: the app only builds paths from ids it received, so a garbled one is an unknown URL, and `404` reveals nothing. In a **body or query**: `400 invalid_request` naming the field, because the decoder fails on it like any wrong-typed field. | `extract.rs`, `From<PathRejection>`; section 8 |
| 15, oversized JSON | `400 invalid_request`, `field: "body"`, the limit (1 MiB) in the message; `413 upload_too_large` stays for images | Sections 7, 8 |
| 16, `405`, `408`, `504` | A method a route doesn't have is `404 not_found` without `Allow` (indistinguishable from an unknown path); `408` never occurs (timeouts answer as in 17); `504` isn't produced | `from_status` |
| 17, timeouts and `retryable` | A request past its timeout is `503 upstream_unavailable` with `upstream: "api"`, `retryable: true`, `retry_after_s: 5`: such a request is almost always waiting on a slow dependency, every create call is idempotent, and the timeout drops the handler's future, which releases its idempotency key | `router::timeout`, `from_status` |
| 22, the per-scan writer's lifecycle | The actor starts on the first `emit` and retires after 60 s without a command (or at shutdown), draining its queue; the broadcast channel is created by the first subscriber and removed only with no receivers; the plan lock is removed only when nobody holds or waits for it. Correctness doesn't depend on any of these lifetimes, because `seq` comes from the event's transaction. Settled with the pipeline author | Section 13 |
| 23, a missing `X-Lease-Token` | `400 invalid_request`, `field: "X-Lease-Token"`; a wrong or stale token is `409 lease_lost` from the store's write | `JobLease` |
| 24, auth webhook failures | A bad signature, or a timestamp more than 5 minutes away in either direction: `401 unauthenticated`. An event type it doesn't handle, or a signed body the adapter can't read: `204` (Svix retries anything else, and the body will never parse). `user.created` changes nothing: the user record is made by the first authenticated request, because Clerk doesn't guarantee event order and a late `user.created` must never recreate a deleted user. This narrows architecture 4 ("created by the webhook, or on the first request as a fallback") to the first request. A deleted account can't come back through a still-valid session token either: its tombstone makes `AuthUser` answer `401` (section 17) | `ops::auth_webhook`, `authz::authenticate` |

Also settled here, outside the numbered questions:

- **A location that can't be geocoded** is `400 invalid_request` with `details.field` naming the input (`location.postal_code`, `location.city`, `location.lat`). For GPS that means only a point with no time zone: a failed reverse lookup gives the label "<zone city> time zone" and the garden is created ([integrations 9](integrations.md#9-geocoding)).
- **`retry_after_s`** is `2` for `request_in_progress`, `2` for `processing_unavailable`, `5` for `upstream_unavailable` unless the provider said otherwise, and the limiter's wait, rounded up, for `rate_limited`.
- **A depth map that fails validation** doesn't raise the stored photo's long side to 2048 px; contracts 3 says "a photo uploaded with depth", read here (with the vision author) as one whose depth was kept, since nothing else can be measured.

---

## 23. Open questions

1. **Trigger thresholds** (section 16), now `domain::rules::weather::WeatherThresholds`, shared with Open-Meteo's `frost_risk` and the pipeline's fusion ([domain 12](domain.md#12-open-questions), item 17): frost at a 2 m minimum ≤ 0.5 °C (or the day's `frost_risk`), hard frost ≤ −2 °C, heat ≥ 30 °C, a humid stretch of two days in a row each with ≥ 10 hours over 90% RH, heavy rain ≥ 20 mm a day, after rain ≥ 5 mm in 24 h, dry soil below 0.15 m³/m³ with ET₀ ≥ 3 mm. The contracts fix the vocabulary, not the numbers; they belong beside the fusion thresholds in [architecture 15](../architecture/README.md#15-open-decisions).
2. **The general rate limit** (120 requests a minute per user) and the JSON body limit (1 MiB) are placeholders the documents don't give.
3. **One user upsert per request.** `authz::authenticate` calls `IdentityRepo::upsert_user` on every authenticated request; the store reads first and checks the account's tombstone in the same call. A short in-process cache of `auth_id → UserId` would remove the read, but it would also skip the tombstone check, so a deleted account's token would work until the entry expired; not done.
4. **Resumed plant and scan deletions** leave their objects to the daily orphan sweep, up to a day later, because `unfinished_deletions` returns targets without their prefixes. If that delay matters, the store can return each target's prefixes from the records still present.
5. **Two OpenAPI component names** are shared by every instantiation of a generic domain type (`Id`, `Slug`), and `ReferenceImage` appears twice (`ReferenceImage`, `ReferenceImage_Signed`); the generated TypeScript types should be checked once ([domain 12](domain.md#12-open-questions)).

---

## 24. Sources

The crate facts were checked against each crate's source at the pinned version (`~/.cargo/registry/src`) and in research probes run while writing this design (an HTTP probe and an auth probe, not kept in the repository); a ✓ means it compiled or ran in this crate or a probe. The links point at the same versions' documentation.

**Rust**
- [The Book, 18.2: trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [16.4: `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [17: async and await](https://doc.rust-lang.org/book/ch17-00-async-await.html)
- [Reference: dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility) · [trait upcasting coercion](https://doc.rust-lang.org/reference/type-coercions.html#r-coerce.unsize.trait-upcast) · [`impl Trait` and `use<..>` bounds](https://doc.rust-lang.org/reference/types/impl-trait.html) · [lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html)
- [Edition Guide: RPIT lifetime capture rules (2024)](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · [disjoint closure captures (2021)](https://doc.rust-lang.org/edition-guide/rust-2021/disjoint-capture-in-closures.html)
- [`std::future::ready`](https://doc.rust-lang.org/std/future/fn.ready.html) · [`std::sync::PoisonError::into_inner`](https://doc.rust-lang.org/std/sync/struct.PoisonError.html#method.into_inner)
- [Async Book](https://rust-lang.github.io/async-book/) · [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

**tokio**
- [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) · [`Semaphore::acquire_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned) · [`Notify`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Notify.html) (`notified`, `enable`, `notify_waiters`) · [`broadcast`](https://docs.rs/tokio/1.53.3/tokio/sync/broadcast/index.html) · [`mpsc`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/index.html) · [`Mutex::lock_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html#method.lock_owned) · [`interval_at`, `MissedTickBehavior`](https://docs.rs/tokio/1.53.3/tokio/time/fn.interval_at.html) · [Tokio tutorial: shared state](https://tokio.rs/tokio/tutorial/shared-state)
- [`CancellationToken`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html) · [`TaskTracker`](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html)

**HTTP**
- axum 0.8.9: [`FromRequestParts`](https://docs.rs/axum-core/0.5.6/axum_core/extract/trait.FromRequestParts.html) · [`FromRef`](https://docs.rs/axum/0.8.9/axum/extract/trait.FromRef.html) · [the order of extractors](https://docs.rs/axum/0.8.9/axum/extract/index.html#the-order-of-extractors) · [customizing extractor responses](https://docs.rs/axum/0.8.9/axum/extract/index.html#customizing-extractor-responses) · [`RawPathParams`](https://docs.rs/axum/0.8.9/axum/extract/struct.RawPathParams.html) · [`DefaultBodyLimit`](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html) · [`Multipart`](https://docs.rs/axum/0.8.9/axum/extract/struct.Multipart.html) · [SSE](https://docs.rs/axum/0.8.9/axum/response/sse/index.html) · [middleware ordering](https://docs.rs/axum/0.8.9/axum/middleware/index.html#ordering) · [`Serve::with_graceful_shutdown`](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown) · [CHANGELOG 0.8.0](https://docs.rs/crate/axum/0.8.9/source/CHANGELOG.md)
- tower-http 0.7.1: [`TimeoutLayer`](https://docs.rs/tower-http/0.7.1/tower_http/timeout/struct.TimeoutLayer.html) · [`CorsLayer`](https://docs.rs/tower-http/0.7.1/tower_http/cors/struct.CorsLayer.html) · [compression predicates](https://docs.rs/tower-http/0.7.1/tower_http/compression/predicate/index.html) · [request id](https://docs.rs/tower-http/0.7.1/tower_http/request_id/index.html) · [`CatchPanicLayer`](https://docs.rs/tower-http/0.7.1/tower_http/catch_panic/index.html) · [`RequestBodyLimitLayer`](https://docs.rs/tower-http/0.7.1/tower_http/limit/struct.RequestBodyLimitLayer.html)
- [utoipa 6.0.0](https://docs.rs/utoipa/6.0.0/utoipa/) · [utoipa-axum 0.3.0](https://docs.rs/utoipa-axum/0.3.0/utoipa_axum/) · [governor 0.10.4](https://docs.rs/governor/0.10.4/governor/) · [`serde_path_to_error`](https://docs.rs/serde_path_to_error/0.1.20/serde_path_to_error/)
- [hmac 0.13.0](https://docs.rs/hmac/0.13.0/hmac/) · [aes-gcm 0.11.1](https://docs.rs/aes-gcm/0.11.1/aes_gcm/) · [rand 0.10.3](https://docs.rs/rand/0.10.3/rand/) · [secrecy 0.10.3](https://docs.rs/secrecy/0.10.3/secrecy/) · [aws-lc-rs `constant_time`](https://docs.rs/aws-lc-rs/1.18.2/aws_lc_rs/constant_time/index.html)
- [MDN: CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS) · [MDN: preflight](https://developer.mozilla.org/en-US/docs/Glossary/Preflight_request) · [WHATWG: server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html) · [Svix: verifying payloads](https://docs.svix.com/receiving/verifying-payloads/how) · [Expo: sending notifications](https://docs.expo.dev/push-notifications/sending-notifications/)

**Project**
- [contracts](../architecture/contracts.md) v0.9 · [architecture](../architecture/README.md) · [database](../database/README.md) · [storage](../storage/README.md) · [frontend](../frontend/README.md) · [domain](domain.md) · [domain-types](domain-types.md) · [integrations](integrations.md)
