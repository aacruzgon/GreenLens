# PDM Backend: `api` binary

> **Status:** Draft v0.7.5, 2026-10-19. v0.7.5: the connect and deletion branches share one path: `DeletionRunner::finish_connector` is the only revoke-then-delete (the owner's `DELETE /connectors/{id}` spawns it; `revoke.rs` and `connect::disconnect` are gone), `sealing::StoredTokens` the only reader of a sealed token, and `ConnectVendors` the only revoke seam; a token another live `vendor` connector of the same Slack team or Google account holds isn't revoked; each revoke runs in a task of its own; a demotion starts the task only when an owner was demoted; `complete` claims the state before the exchange and revokes tokens it can't store; a refreshed Google token is stored with a compare-and-set; the callback passes on only a short vendor error code (sections 11, 17). v0.7.4: connector deletion (roadmap E3.6): a garden's cascade, an owner's demotion, removal or leaving, and account deletion delete the connectors that owner created; the store marks them `deleting` in its transaction, and the deletion runner unseals each token, revokes it through the `TokenRevoker` seam (`revoke.rs`, Slack's `auth.revoke` and Google's `revoke`; never for an `era` connector) after that commit, then deletes the records and the row, resumed at start while the mark remains; the deletion runner opens tokens with `unseal` too (section 17). v0.7.3: farm connectors connect (roadmap E3.3–E3.5): `connect.rs` signs the OAuth state (`OAuthState`, an HMAC-SHA256 under `ConnectorKey::state_key` over the connector id, the nonce and the expiry), seals the tokens as `StoredTokens` (Slack's bot token as its plain string, Google's grant as JSON `{access_token, refresh_token, expires_at}`), and holds the `ConnectVendors` seam that `wiring/connect.rs` implements over the Slack and Drive clients on the vendors' own hosts; the seven connector routes are built (section 11), refuse an `era` connector with `409 invalid_state`, and the `cleanup` task deletes pending connectors after 10 minutes (section 16). v0.7.2: farm connectors are configured (roadmap E3.0–E3.2): `CONNECTOR_KEY` (64 hex characters) is the one switch, read through `AppState::connector_key`; without it the list answers `available: false` and the owner routes `409 invalid_state`, and with it the list comes from the store. The Slack and Google OAuth clients (each pair together or not at all), `CONNECTOR_RETURN_URL` and the Era bases are read for the routes and seeding to come. `sealing.rs` seals vendor tokens with AES-256-GCM (`aes-gcm` 0.11.1), a random nonce per seal and the connector id as associated data. v0.7.1: the farm connector routes are declared (roadmap E1.4): `GET /gardens/{id}/connectors` answers `available: false`, and the owner routes and the public OAuth callback answer `409 invalid_state` until connectors are configured; `Access<R, table::Connector>` resolves a connector's garden. v0.7: shared memories are recorded in `SurrealDB` Agent Memory: the optional `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and `AGENT_MEMORY_API_KEY` (all three or none), `Ports.beliefs`, the new `GET /plants/{id}/beliefs` (never a 5xx), the `beliefs` scheduler task every 10 s (the only writer to Agent Memory: queued erases, then unsynced shared memories), deletions that queue their erase in their first transaction, and `POST /memories` taking a `topic`; 42 tests. v0.6.5: `RUSTFS_BUCKET` defaults to `pdm-images`, the bucket storage 2 creates, not `pdm`. v0.6.4: section 4 proposes that start-up step 3 applies the numbered migrations and the generated `knowledge.surql` after the schema, and that step 6 stops on an embedder that differs from the knowledge set's. v0.6.3: section 7 says `backend/openapi.json` is committed and a test fails when it falls behind the types. v0.6.2: section 21 names `gen_fake_store.py` at its place in the repository, `backend/tools/apigen/`. v0.6.1: worker tokens are kept as SHA-256 digests and compared digest to digest in constant time, so a token's length no longer shows in the comparison time. v0.6: a live scan stream re-reads the state after 15 s without an event, so a resting point reached without one and a deleted scan close it; image bodies end on the shutdown token; the claim group has the 1 MiB JSON limit (it had axum's 2 MiB default); a signal handler that fails to install no longer shuts the API down; invite accepts are held to at most 10 in any minute (a burst of 5, then one every 12 s); the replay-completeness claim is corrected; the section 1 skills table is gone, since the skills follow this design; the fake-store generator moves to `backend/tools/apigen/`. v0.5: the scan stream ends on a duplicate flagged `rests` (a resting event stored during the replay used to leave it open), the timeout's bare `503` gets `Retry-After` from `render_errors`, the global body cap is 20 MiB + 128 KiB (not 24 MiB), the `api` container's `stop_grace_period` is 190 s, the scheduler's `TASKS` listing is complete, and comments cite the settled contract sections. v0.4: the generated fake store now delegates every store method, default-bodied ones included, and four handler tests cover the v0.3 paths: late worker events dropped with `204`, a tombstoned account's token answered `401`, a second `DELETE /me` starting nothing, and the `SoilGrids` retry time by failure. v0.3: the cross-crate fixes: every `RustFS` delete goes through one `blob::DeleteQueue`, drained at shutdown for at least one full attempt; the store's `Notify` is the claim long poll's only wake-up; `complete` fails a job for good once; late `events` after `complete` are dropped with `204`; `Pipeline::sweep` owns the question continuation; `SoilGrids` retries 15 minutes after an outage and 7 days after no data; account deletion starts with a tombstone's compare-and-set that refuses the deleted account's tokens and resumes the Clerk deletion after a restart. v0.2: the trigger rules read `domain::rules::weather::WeatherThresholds` from `Config::weather`, the one instance also given to Open-Meteo and the pipeline, and one text embedder serves every consumer now that `TextEmbedder::embed` takes an `EmbedMode`. v0.1: the `api` binary designed down to every handler, extractor, layer and background task: start-up and shutdown, configuration, the layer stack, auth and membership proofs, idempotency, the error mapping, every endpoint of contracts v0.9, uploads, the resumable scan event stream, the chat stream, the job broker, the scheduler, the deletion runner and the signed media routes, compiled on Rust 1.99.1 against axum 0.8.9, tower-http 0.7.1 and utoipa 6, and wired to the real `store`, `blob`, `knowledge`, `vision`, `integrations`, `agent` and `pipeline` crates.

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

@@file ../Cargo.toml@@

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
├─ connect.rs        the consent URLs, ConnectVendors (the vendor seam: exchange, refresh, sources, revoke)
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

@@items main.rs main init_tracing signal until_signal@@

**The order** ([architecture 12](../architecture/README.md#12-deployment)):

| Step | What | Fails how |
|---|---|---|
| 1 | Tracing: JSON logs, `RUST_LOG` (default `info`) | — |
| 2 | Configuration from the environment (section 5) | Exits naming the variable |
| 3 | `SurrealDB`: apply `schema.surql` (idempotent, [database 1](../database/README.md#1-version-and-setup)) over a short-lived root connection, then connect and sign in as the `api` database user; load the knowledge index (`knowledge::SurrealKnowledgeStore::load`). Proposed: the same root connection then applies the migrations, `store::SurrealStore::migrate` ([store 4](store.md#4-connecting-and-the-schema)): new `NNNN_<slug>.surql` files in order, then `knowledge.surql` when its hash changed | Exits: nothing works without them. Proposed: also exits, naming the file, on a failed file, an edited or missing recorded file, or a file merged out of order |
| 4 | `RustFS` client ([storage 5](../storage/README.md#5-rust-client)) | Exits |
| 5 | Outbound adapters: one `reqwest` client shared by Clerk, Svix, Open-Meteo, `SoilGrids`, the geocoder (its time-zone boundaries parsed on a blocking thread) and Expo Push | Exits on a bad secret |
| 6 | Models on CPU from `{MODELS_DIR}/manifest.toml`, every file's hash checked, on a blocking thread: the text embedder (one instance; each call names its `EmbedMode`), the upload pipeline's face detector, and the measurer with its colour spot segmenter. Proposed: then compare the embedder's `<name>@<version>` and dimension with `knowledge_meta:current` | Exits: face blurring fails closed, so uploads can't run without it. Proposed: also exits on an embedder that differs from the knowledge set's, since query vectors would not match the stored ones |
| 7 | The species photo index from `KnowledgeRepo::load_knowledge`; the product catalogue for the dose and plan checks | Exits |
| 8 | `api`'s own pieces: hub, broker, writers, pusher, deletion runner; then the pipeline and the agent, which need the writer | — |
| 9 | Resume cascades marked `deleting_at`, and account deletions whose tombstone isn't finished (section 17) | Exits on a store error |
| 10 | Start the scheduler's tasks on the `TaskTracker` (section 16) | — |
| 11 | Bind and serve | Exits |

`seed-upload` has already finished when `api` starts (`depends_on` with `service_completed_successfully`, [architecture 12](../architecture/README.md#12-deployment)), so the migration that references the species photos and the photo index see them.

**The migrations (proposed).** The design is in [database 1](../database/README.md#1-version-and-setup) ("Applying the migrations") and the crate's side in [store 4](store.md#4-connecting-and-the-schema). Production runs one `api`; each file's transaction checks the hash it expects, so a second process applying the same file at once rolls back and moves on. ✓ On the pinned image a 21 MB knowledge file of 4,000 chunks applies in 2.2 s on empty tables and 6–7 s over existing rows, so an unchanged file is skipped rather than re-applied. A failed migration exits the process; Compose restarts it into the same failure until a fixed build ships, and the files applied before it stay applied ([database 7](../database/README.md#7-operations)).

**Shutdown.** One `CancellationToken` for the process, cancelled on Ctrl-C or SIGTERM. A handler that fails to install is logged and waits forever (`until_signal`), so it can't shut the API down the moment it starts (✓ `a_signal_handler_that_fails_to_install_never_fires`). `axum::serve(..).with_graceful_shutdown(token.cancelled_owned())` stops accepting and then waits for every open connection; it returns only when the last one closes. So every long response ends on the token: the scan stream and the chat stream through `.take_until(token.cancelled_owned())`, the claim long poll through its `select!` (it answers `204` and the worker reconnects), and image bodies through `take_until` in `media::stream_object`, since `TimeoutLayer` bounds only the response head and a slow download would otherwise hold shutdown open (the client sees a short body and fetches the image again; ✓ `an_image_body_ends_on_the_shutdown_token`). Without that, one open SSE stream keeps the process up forever (✓ verified in the HTTP research probe: `completed = false` without `take_until`, true with it). `cancelled_owned()` returns an owned, `'static` future, which is what `with_graceful_shutdown` and `take_until` need.

Then `tracker.close()` and `tracker.wait()` with `SHUTDOWN_DRAIN_S` (30 s) as the deadline, and at the same time `DeleteQueue::drain` with `max(SHUTDOWN_DRAIN_S, 120 s)` as its deadline: pending `RustFS` deletes get at least one full attempt (the client's 120-second operation timeout, `blob::client::OPERATION_TIMEOUT`) even when the rest of the drain is shorter, because an interrupted delete can leak objects ([storage 4](../storage/README.md#4-writes-and-deletes)). The tracker holds the scheduler's loops (which end on the token), the writer actors (which drain and stop on the token), spawned follow-ups, and deletion cascades. Cascades don't watch the token, except an account cascade's Clerk retries, which stop on it (the tombstone resumes them); a cascade still running at the deadline is cut off with the process and resumes on the next start from `deleting_at` or the tombstone. Its object deletes belong to the delete queue, not to the cascade, so cutting the cascade off never interrupts one.

**The container's grace period.** Docker Compose sends SIGKILL `stop_grace_period` after SIGTERM, 10 s by default ([Compose file reference](https://docs.docker.com/reference/compose-file/services/#stop_grace_period)), so the `api` service sets `stop_grace_period: 190s`: the slowest in-flight request (60 s, `SLOW_TIMEOUT`), then the delete drain (`max(SHUTDOWN_DRAIN_S, 120 s)`), plus 10 s of margin. A `SHUTDOWN_DRAIN_S` above 120 s raises it by the difference.

---

## 5. Configuration

@@items config.rs Config DatabaseConfig StorageConfig ClerkConfig AgentMemorySettings AGENT_MEMORY_VARS ProviderConfig OAuthClient SLACK_VARS GOOGLE_VARS ConnectorSettings@@

@@items config.rs agent_memory connectors oauth_client@@

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
| `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` | unset | Era's Slack and Drive simulators, `http(s)://` origins, for start-up seeding of the demo garden's `era` connectors (roadmap E4.6). No route reads them (decision 8) |

Every secret is a `SecretString`: `Debug` prints `[REDACTED]` (the test checks the signing key never appears in the formatted config). `Config::from_lookup` takes any variable source, so tests build a config from a map.

**Sealing.** `CONNECTOR_KEY` parses into a `ConnectorKey`, which holds the expanded AES-256-GCM key (wiped on drop through `aes-gcm`'s `zeroize` feature) and prints as `ConnectorKey([REDACTED])`. `seal` draws a fresh 96-bit nonce from the OS for every call, so sealing one token twice gives two values, and writes `nonce (12 bytes) || ciphertext || tag (16 bytes)` into `domain::ports::Sealed`, whose `Debug` prints only the length. The associated data is the connector's id as it is written on the wire, so a sealed value opens only on its own connector ([connectors 4](../connectors/README.md#4-data)). `unseal` returns the plaintext as a `SecretSlice<u8>`, wiped on drop; a wrong key, another connector's id or any changed byte is the same opaque `UnsealError::Rejected`. It lives in `api` because every caller does: the connect routes (E3.3, E3.4), the `connectors` scheduler task (E4.1) and start-up seeding (E4.6). The `FarmSource` adapters in `integrations` get an unsealed `VendorToken` and never see the key, and `domain` holds only the opaque bytes, so neither depends on a cipher. The connect routes seal Google's PKCE verifier (`SealPurpose::Pkce`) and, after the exchange, the vendor's tokens (`SealPurpose::Token`) as `sealing::StoredTokens`: Slack's bot token as its plain string, Google's grant as JSON `{access_token, refresh_token?, expires_at?}`, read back as JSON exactly when the plaintext starts with `{`; the routes, the deletion runner and the sync all read it through `StoredTokens::unseal`. The deletion runner opens a deleted connector's tokens to revoke them (E3.6, section 17).

@@items sealing.rs ConnectorKey KeyError UnsealError StoredTokens@@

@@impl sealing.rs impl ConnectorKey@@

**One switch.** `AppState::connector_key` (section 6) is the only place that decides whether connectors are on. The list route asks it and answers `available: false`; each of the seven write routes calls `require_connectors` first and answers `409 invalid_state` when it is off; the `connectors` task and seeding ask it too; `wiring.rs` logs the decision at start, with a warning when vendor or Era settings are set without the key.

---

## 6. Application state, wiring and the seams

@@items state.rs Ports AppState SharedState@@

@@impl state.rs impl AppState@@

**One state, no generics.** `AppState` holds every port as `Arc<dyn Trait>` (convention 3); handlers get `State<Arc<AppState>>`. `Store` is the supertrait of the 17 repositories, so `state.ports.store.create_scan(..)` and `state.ports.store.append_event(..)` go through one `Arc<dyn Store>`; where a component needs one area (the broker needs `JobRepo`), the `Arc<dyn Store>` is upcast to `Arc<dyn JobRepo>` (trait upcasting, stable since Rust 1.86).

**The seams.** Three library APIs aren't domain ports: the pipeline's entry points and its `ScanWriter` port, the agent's chat turn, and the vision crate's upload pipeline. `api` holds each as a trait object of its own (`PipelineApi`, `ChatTurns`, `ImageProcessing`), so a handler test swaps it for a fake exactly like a domain port; `wiring.rs` implements those traits for `pipeline::Pipeline`, `agent::ChatAgent` and `vision::upload::UploadProcessor` (a local trait on a foreign type, which the orphan rule allows). The types that cross are the crates' own, re-exported (`pub use pipeline::{FollowUp, Handled, PhotoInfo, PipelineError, …}`), and each seam method has the pipeline method's exact signature: `PipelineError` is the pipeline's enum (`LabelsNotLoaded { label_set_version, classifier_labels_version }`, `PlanWriter(agent::PlanWriterError)`), and `photo_answer_added` is synchronous and infallible as in the pipeline. The continuation of an unanswered question isn't a seam method: `sweep` owns it (section 16).

@@items seams.rs PipelineApi TurnParts ChatTurns ImageProcessing@@

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

@@impl wiring.rs impl ChatTurns for agent::ChatAgent@@

and builds everything in the order of section 4:

@@items wiring.rs SystemClock UlidGen Models load_models Databases connect_databases Outbound outbound agent_memory log_connectors build@@

**Agent Memory.** `outbound` builds `integrations::agent_memory::AgentMemory` over the shared `reqwest` client when the three variables are set, and `NoBeliefs` otherwise, logging which at start (the URL and context, never the key). `Ports.beliefs` holds it, and goes to the agent's and the pipeline's deps for their reads. No request writes to Agent Memory: the `beliefs` task records what the requests left unsynced or queued (section 16; [agent 14](agent.md#beliefs-in-agent-memory)).

**One embedder, two modes.** The text embedder (snowflake-arctic-embed-xs) embeds queries and documents differently, and every `TextEmbedder::embed` call names its `EmbedMode` ([domain 9](domain.md#9-boundary-traits)). So `api` loads one embedder and shares one `Arc<dyn TextEmbedder>` (`Ports.embedder`): garden notes are embedded as `Document`, the pipeline's memory summaries as `Document`, and the agent embeds the user's message and its searches as `Query` and the memories it writes as `Document`.

The two synchronous ports `api` implements itself:

@@items signing.rs HmacSigner PhotoIndex@@

@@impl signing.rs impl UrlSigner for HmacSigner@@

---

## 7. The router and the layer stack

Every route is registered through utoipa's `OpenApiRouter` and `routes!`, which reads each handler's `#[utoipa::path]` and hands axum the same path string, so the spec can't drift from the routes. Paths use axum 0.8's `{id}` captures (`/:id` panics at router build). `routes!(a, b)` takes several handlers only when they share one path.

@@items router.rs CLAIM_TIMEOUT SLOW_TIMEOUT WORKER_BODY_LIMIT JSON_BODY_LIMIT routes@@

**Groups.** Auth is in the extractors, so groups differ only in what a `.layer` on them sets. `OpenApiRouter::layer` wraps every route registered *so far* in that router, so each group is its own `OpenApiRouter`, layered, then merged (✓ the research probe found that a body limit on a shared router reached every route):

| Group | Routes | Timeout | Body limit |
|---|---|---|---|
| app | everything else, both SSE streams, media, ops, the webhook | `REQUEST_TIMEOUT_S` (10 s) | 1 MiB |
| uploads | `POST /scans/{id}/images`, `POST /conversations/{id}/images` | 60 s | 12 MiB + 2 × 4 MiB + 128 KiB |
| connect | `POST /connectors/{id}/complete` (two vendor calls, 20 s or 30 s deadlines each; section 11) | 60 s | 1 MiB |
| claim | `POST /internal/jobs/claim` | 35 s (the long poll waits up to 30 s) | 1 MiB |
| worker | heartbeat, events, complete, job images | 60 s | 16 MiB |

`TimeoutLayer` bounds only the time to the response **head** (✓ research probe: an SSE body kept flowing past a 1 s layer), so it is safe on the SSE routes. `TimeoutLayer::with_status_code` is used because `TimeoutLayer::new` is deprecated since tower-http 0.6.7.

@@items router.rs MakeReqId cors app@@

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

@@items openapi.rs ApiDoc@@

---

## 8. Errors

Every non-2xx response carries the `ErrorBody` of [contracts 2](../architecture/contracts.md#2-errors). `IntoResponse` can't see the request, so it can't know the `request_id`; the body is therefore written in one place.

@@items error.rs ApiError ErrorParts@@

@@impl error.rs impl IntoResponse for ApiError@@

@@items error.rs render_errors from_status panic_response@@

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

@@items extract/auth.rs AuthUser@@

@@impl extract/auth.rs impl<S> FromRequestParts<S> for AuthUser@@

`domain::authz::authenticate` verifies the token through the `AuthVerifier` port (Clerk's JWKS, `exp`, `nbf`, `iss`, `azp` against the web origins) and maps `sub` to a user with `IdentityRepo::upsert_user`, creating the user on first sight: this request is how a user record is made (section 22, question 24). A deleted account's session token stays valid at Clerk for up to its lifetime (60 s by default); while the account's tombstone exists (section 17), `upsert_user` refuses the auth id with `StoreError::NotFound`, checked on the read path and inside the create's transaction, and `authenticate` turns that into `AuthError::AccountDeleted`: `401 unauthenticated`, never a new, empty user. Then the general rate limit, keyed by the user, so an unauthenticated caller can't fill the limiter's key map. The `CurrentUser` is cached in the request's extensions, so `Access` and `IdemJson` on the same request don't verify twice.

### Membership proofs: `Access<R, T>`

@@items extract/access.rs Locate Scoped Access@@

@@impl extract/access.rs impl<S, R, T> FromRequestParts<S> for Access<R, T>@@

The role is a type parameter, so the role a route needs is written in its signature (`access: Access<Editor, table::Scan>`), and the repository call it makes only compiles with a proof of at least that role ([domain 8](domain.md#8-authorization-proofs)). `R: MinRole` is checked at run time against the stored role by `authz::check_membership` / `check_access` and carried in `Membership<R>`; `T: Scoped` is the table of the `{id}` capture, which says how to find its garden: the id itself for `table::Garden`, a `GardenResource` lookup for the others (which skips records being deleted). `Scoped` is a local trait implemented for the domain's table markers, which the orphan rule allows.

| Outcome | Status |
|---|---|
| The `{id}` isn't a ULID | `404` |
| No such record, being deleted, or the caller isn't a member | `404`, the same body ✓ (`a_non_member_gets_the_same_404_as_an_unknown_scan`) |
| A member below `R` | `403 forbidden` with `required_role` ✓ (`a_viewer_is_403_with_the_required_role`) |

A garden id in a body or query (`POST /scans`, `GET /plants?garden_id=`) is checked in the handler with `state.member::<R>(&user, garden)`, after the body is parsed. A route whose role depends on the request (removing a member: an owner, or the member themselves) takes `Access<Viewer, …>` and calls the proof's run-time `require::<Owner>()`. A handler that holds `Membership<Editor>` and calls a method that wants `Membership<Viewer>` downgrades with `at_least()` at compile time.

### Conversations: `ConvAccess<R, T>`

@@items extract/access.rs ConvScoped ConvAccess@@

@@impl extract/access.rs impl ConvScoped for table::Conversation@@

`ConvScoped::conversation` returns `impl Future + Send` instead of being an `async fn`: an `async fn` in a trait has no `Send` bound on its future, and the extractor that awaits it must be `Send`. Implementations may still be written as `async fn` (their futures are `Send`, and an impl may refine the declared return type). The conversation's own impl does no lookup, so it returns `std::future::ready(..)`, as clippy's `unused_async_trait_impl` suggests. The pending-action and message impls look the conversation up, then `authz::check_conversation::<R>` checks owner **and** membership ([contracts 8](../architecture/contracts.md#access)); anyone else gets `404`.

### Workers

@@items extract/auth.rs WorkerAuth JobLease@@

@@items workers.rs WorkerTokens sha256@@

One bearer token per worker process, mapped to its `WorkerId`. The configured tokens are kept as SHA-256 digests, made once at start; the bearer is hashed and its digest compared in constant time with every configured digest (✓ `identifies_only_exact_tokens`). Comparing raw tokens would leak their length, because `verify_slices_are_equal` returns at once when the lengths differ (aws-lc-rs 1.18.2, `constant_time.rs`), while two digests are always 32 bytes. A claim or heartbeat whose body names another worker is `401`. `JobLease` adds the `{id}` capture and `X-Lease-Token`; a missing or malformed header is `400 invalid_request` naming it (question 23). The lease isn't checked here: the store compares token, worker and status inside the write's transaction, so a stale lease is `409 lease_lost` from the write itself, never from a separate read that a requeue could race.

### The auth webhook

`POST /webhooks/auth` reads the raw body as `Bytes` and verifies it through the `WebhookVerifier` port (Svix's HMAC over `id.timestamp.body`, constant time, every `v1` entry, the timestamp within 5 minutes either way) before anything parses it (section 11, `auth_webhook`).

---

## 10. Idempotency

The create calls of [contracts 1](../architecture/contracts.md#idempotency) accept `Idempotency-Key`, stored as `idempotency_key:[user, route, key]` for 24 hours ([database 4](../database/README.md#idempotency-keys-)).

@@items extract/idempotency.rs Begin Begun@@

@@impl extract/idempotency.rs impl Begun@@

@@impl extract/idempotency.rs impl Drop for Begun@@

@@items extract/idempotency.rs begin canonical_json_hash@@

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

@@items extract/idempotency.rs IdemJson IdemRejection@@

@@impl extract/idempotency.rs impl<S, T> FromRequest<S> for IdemJson<T>@@

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

@@items reply.rs Wire replay@@

### Handlers of note

A thin handler is one extractor and one repository call; most are. These carry logic worth reading. Scans:

@@items routes/scans.rs create_scan submit answer events@@

The garden map: shape rules without the database (`GardenMapInput::validate`: counts, simple polygons, distances, one pin per plant), then the store checks `base_version`, the plants and deletions in one transaction ([database 4](../database/README.md#garden-map-versions-)); a stale version is `409` with `current_map_version`, unknown plants `400` with every one of them:

@@items routes/gardens.rs put_map remove_member new_invite_code@@

Locations are geocoded once, at the precision the user chose, with the time zone ([architecture 8.5](../architecture/README.md#85-garden-setup)):

@@items routes/gardens.rs resolve@@

The webhook:

@@items routes/ops.rs auth_webhook healthz@@

A plant's beliefs: Agent Memory's current beliefs of the plant and its garden, and the plant's superseded ones, read together under the adapter's entity-read deadline. Agent Memory off, failing or slow answers `200` with `available: false`, never a `5xx`, because the screen has the memories list to fall back on:

@@items routes/plants.rs BELIEFS_TIMEOUT plant_beliefs@@

**Farm connectors** ([connectors 5](../connectors/README.md#5-connecting-and-tokens); roadmap E3.1–E3.4 of [docs/EON](../EON/README.md)). Without `CONNECTOR_KEY` (`AppState::connector_key`, section 5) connectors are off: the list answers `available: false` without asking the store, and everything else `409 invalid_state` after its access checks, not `503`, which clients retry (decision 9). With the key:

- **Starting.** `POST /gardens/{id}/connectors` (owner) creates a `pending` connector whose `oauth` holds the nonce, the app's `device_hash` and, for Google, the PKCE verifier sealed with `SealPurpose::Pkce`; it answers `201` with the vendor's consent URL. A vendor without its OAuth client (`SLACK_CLIENT_ID`, `GOOGLE_CLIENT_ID`) is `409 invalid_state`, and so is a third connector in a garden (the store's limit).
- **The state** is `<connector id>.<nonce>.<expiry in unix seconds>.<mac>`: four URL-safe fields, the MAC an HMAC-SHA256 under `ConnectorKey::state_key` over the first three as written, checked in constant time before anything else is read. A state works only while the pending connector still holds its nonce and expiry and no `complete` has claimed it (`oauth.claimed_at`); the exchange clears them, so a tampered, expired, claimed or used state is `400 invalid_request` on `state`, at the callback and at `complete`.
- **The callback** is public: it checks the state, stores nothing, and answers `302` to `{CONNECTOR_RETURN_URL}/{id}/callback` with the code (or the vendor's `error`, when the owner declined, passed on only when it is 1 to 64 characters of `[a-z_]` and as `vendor_error` otherwise) and the state, with `Cache-Control: no-store` and `Referrer-Policy: no-referrer`.
- **`complete`** (owner) checks the state against the `{id}` in the path, then the device: anyone but the owner who started it, or a `device_verifier` whose SHA-256 isn't the stored hash, gets `404 not_found`, and the pending connector is deleted (`finish_connector` on the tracker). Then it claims the state (`claim_connect`, a compare-and-set on `pending`, the nonce, the expiry and no earlier claim), so of two concurrent `complete`s only one exchanges the code and the other gets the used-state `400`. Then it exchanges the code: a code the vendor refuses (the client's `AuthRefused`) is `400 invalid_request` on `code`; our own request rejected (the client's `Refused`, a misconfigured OAuth client) is `500`; an outage is `503 upstream_unavailable` with `upstream: "slack"` or `"google_drive"`. A failed exchange gave no tokens, so `release_connect` clears the claim and a retry can claim again. The tokens are sealed and the store's compare-and-set on the claimed nonce activates the connector; when it loses (the connector was deleted or cleaned up during the exchange) the answer is the used-state `400` or `404`, and the tokens nothing stores are revoked in the background (`DeletionRunner::revoke_tokens`, with the shared-account rule).

  **Not cancelled with its request.** The handler spawns the work on the tracker and only awaits its `JoinHandle`, so the route's timeout (the `connect` group's 60 s) or a client that goes away drops the handler, not the exchange: the task still stores the tokens, revokes them or releases the claim, and finishes the idempotency record, so a retry with the same `Idempotency-Key` gets `409 request_in_progress` until it is done and then its answer. The claimed part (exchange, then `complete_connect`) is a task of its own: a panic in it comes back as a `JoinError`, which releases the claim and answers `500`.
- **Sources.** `available-sources` first asks `ConnectVendors::refresh` for a Google access token with under 5 minutes left, and stores what comes back with `replace_secret`, a compare-and-set against the sealed value it read (of two refreshes at once one is stored, and the other call uses its own token); then it asks the vendor (channels where the bot is a member, or the Google account's folders); a refused token makes the connector `reauth_needed` and answers `409 invalid_state` with that status. `PUT sources` takes at most 10 channels or 1 folder, each id from the vendor's list (its name comes from there), stores them, then deletes each dropped source's records, 500 at a time, before answering with the fresh record count.
- **Sync now** is the store's compare-and-set (decision 12): `202`, or `429 rate_limited` with `retry_after_s` up to the next allowed instant.
- **Disconnect.** `DELETE` has the store mark the connector `deleting` (the list and the sync skip it) and answers `202`; a task on the tracker runs `DeletionRunner::finish_connector` (section 17), the one revoke-then-delete path, which every cascade uses too.
- **Demo connectors** (`endpoint: "era"`) refuse every owner route with `409 invalid_state` (decision 11). No route takes a URL: the consent URLs are constants, and `wiring/connect.rs` builds the Slack and Drive clients with their default configurations, the vendors' own hosts (decision 8).

@@items routes/connectors.rs connectors_off list_connectors create_connector@@

@@items connect/oauth.rs OAuthState StateError@@

@@items connect.rs SLACK_AUTHORIZE_URL GOOGLE_AUTHORIZE_URL VendorError ConnectVendors@@

---

## 12. Uploads

@@items uploads.rs DEPTH_META_MAX_BYTES UPLOAD_BODY_LIMIT FilePart DepthPart UploadForm read_capped read_form@@

@@impl uploads.rs impl UploadForm@@

**Streaming with caps.** Each part is read chunk by chunk and stops at its cap, so nothing past it is ever buffered: the photo at 12 MiB (`413 upload_too_large`, `max_bytes: 12582912`; contracts 2's "12 MB" read as MiB, domain open question 13), each depth map at 4 MiB (over it, the depth is dropped with `too_large` and the photo kept: a depth problem never fails the upload, [contracts 3](../architecture/contracts.md#post-scansidimages)), `depth_meta` at 64 KiB. The route's `DefaultBodyLimit` (all parts plus overhead) answers first for a body that is too big as a whole. The idempotency record begins only after the whole form is read, because the fingerprint covers every part.

@@items routes/scans.rs upload_image@@

@@items uploads.rs process store_scan_image store_depth@@

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

@@items uploads.rs store_chat_image@@

---

## 13. Scan events: writer, hub and stream

[Contracts 4](../architecture/contracts.md#4-scan-events-sse) asks for one serialised writer per scan that assigns `seq`, stores, then broadcasts; a stream that subscribes, replays after `Last-Event-ID`, and goes live; URLs signed on send; and closing at the resting points.

### The writer

@@items events/writer.rs WRITER_IDLE Command Handle Inner ScanWriters@@

@@items events/writer.rs run_actor append_all may_rest@@

@@impl events/writer.rs impl ScanWriter for ScanWriters@@

- **One actor per active scan**, fed by a bounded `mpsc` channel (64 commands). Every event of a scan goes through `ScanWriter::emit`, from the pipeline (worker results, answers, the sweep, species changes) and from `api` itself (the claim's `queued → analysing`). For each event the actor calls `EventRepo::append_event`, which increments `scan.last_event_seq` and creates `scan_event:[scan, seq]` in one transaction ([database 4](../database/README.md#scan-events-a-gap-free-seq-)), then publishes. Because `seq` comes from the database, ordering doesn't depend on the actor's lifetime: a writer recreated later continues the sequence.
- **Resting points.** After the last event of each `emit` call, if that event can end a resting point (`plan_ready`, `plan_superseded`, `diagnosis`, `question_resolved`, `guided_check_done`, `failed`), the actor reads `EventRepo::stream_state` and flags the event `rests` when the scan is at rest: `planned` or `failed`, no question open or `answering`, no guided check `analysing`, **and no step claim held** (`step_claimed`, the pipeline's addition, so a late answer's `question_resolved`, emitted while `score` runs, doesn't close the stream before the new diagnosis and plan). The pipeline emits each resting point's final events in one call (agreed by message).
- **Pushes.** The writer sees every event, so the push rule of [contracts 9](../architecture/contracts.md#push-notifications) lives here (`Pusher::scan_reason`): `plan_ready`, `plan_superseded` (not for a chat revision), `question` and `failed` go to the scan's creator while still a member, in a spawned task.
- **Plan writes** of a scan run one at a time under `plan_lock`: an owned guard of a per-scan `tokio::sync::Mutex<()>`, separate from the event channel so a 10-second plan-writer call never delays event delivery ([contracts 7](../architecture/contracts.md#actionplan), re-planning).

**Lifecycle (question 22).** Three pieces, each with its own rule, so none keeps another alive: the actor starts on a scan's first `emit` and retires after 60 s without a command or on shutdown, removing only its own entry (a generation number), then closing its channel and draining what was queued, so no accepted command is lost; an `emit` that races the retirement gets its command back from the failed send and starts a new actor. The broadcast channel lives in the hub (below). The plan lock is swept when nobody holds or waits for it (`Arc::strong_count == 1`). Settled with the pipeline author.

### The hub

@@items events/hub.rs Published EventHub@@

One `tokio::sync::broadcast` channel per scan with live subscribers, in process, which assumes one `api` instance ([architecture 12](../architecture/README.md#operations)). A subscriber creates the channel; a publish to a scan without one is a no-op (nobody to tell; the event is stored). The writer looks the sender up under the map's lock for every publish and never caches it, and the idle sweep removes a channel under the same lock only when `receiver_count() == 0`, so a sweep can never strand a subscriber on a channel the writer no longer uses. A full channel overwrites its oldest value; a slow receiver gets `Lagged` and replays from the database.

### The stream

@@items routes/scans.rs events@@

@@items events/stream.rs REST_CHECK scan_stream to_sse@@

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

@@items routes/chat.rs send_message@@

- **Before the stream** (validation, the token budget, routing, storing the user message) errors are ordinary `ErrorBody` responses, from the agent's `start_turn` ([contracts 8](../architecture/contracts.md#sending-a-message)).
- **The stream** is the agent's `'static` event stream mapped to frames without `id:` lines (`Event::default().event(name).json_data(&event)`): not resumable.
- **Disconnect.** When the client goes away, hyper drops the response body, which drops the agent's receiver; the turn task sees its next send fail (or `tx.closed()` resolve while it waits on the model), stops the LLM call and stores the message with `interrupted: true`. A guard held in a closure would not do: with edition 2021+ disjoint captures, a `move` closure that never mentions the guard doesn't capture it (research probe), which is why the agent detects the drop through the channel.
- **Shutdown** ends the stream through `take_until`, which interrupts the turn the same way.
- **Idempotency.** The turn task resolves a oneshot with its outcome once the message is stored; a spawned task finishes the record: `Completed` stores the signed `200 { message }` replay, anything else releases the key, so the app's "resend", with a new key, runs again. The app tells a replay from a stream by its `Content-Type` ([frontend 10](../frontend/README.md#chat-messages)).

**Confirmation cards** ([contracts 8](../architecture/contracts.md#pending-actions-confirmation-cards)):

@@items routes/chat.rs confirm_pending_action confirm_revision@@

The confirm on a plan card applies the preview to its base plan exactly as the card showed it (`agent::tools::revise::apply_preview`, pure), resolves the citations, and hands the write to `Pipeline::store_revision`, which takes the scan's plan lock and compares and sets `scan.plan`: a base that lost the race to an automatic re-plan is `409 invalid_state` with `current_plan_id`, never retried. A step naming a neighbour deleted since is `409` with `hint: "neighbour_removed"`; a task card whose plant was deleted is `409` with `hint: "plant_removed"`. A confirmed card confirmed again returns the first result.

---

## 15. The job broker and the worker API

@@items broker.rs CLAIM_POLL JobBroker@@

@@impl broker.rs impl JobBroker@@

**The long poll** ([database 4](../database/README.md#job-claim-with-slots-), [architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback)): claim at once if a job is eligible (the store's transaction: the worker's `slots`, its kinds, `queued_at` 15 s back for the fallback, the photo-answer hold); otherwise wait for a wake-up, the 1-second fallback poll, the deadline (`204`) or shutdown (`204`, and the worker reconnects to the next process). `Notify::notify_waiters` wakes **every** waiting claim, each re-runs its query and one wins (the transaction's re-check makes the losers conflict and retry onto the next job), and it stores no permit, so a claim that starts later doesn't wake for nothing. The `Notified` future is created and `enable`d *before* each query, so a job queued between the query and the wait still wakes it. ✓ `an_empty_queue_answers_none_at_the_deadline_after_polling` (paused time: 3 queries in a 3-second wait).

**One wake-up path.** The `Notify` is the store's own (`SurrealStore::jobs_queued()`), and only the store signals it, after the commit of every write that queues or requeues a job: `enqueue_job`, step commits and run starts, `add_image` (a guided or segmentation job), a retryable failure in `complete_job`, and `requeue_stale` when any job went back to `queued` ([store 14](store.md#14-jobs-and-workers)). Neither `api` nor the pipeline signals it by hand (there is no `ScanWriter::job_queued` or `JobBroker::job_queued`), so a new write path can't forget to, and the stale-lease requeue wakes waiting claims like every other.

@@items routes/internal.rs heartbeat claim job_events complete job_image@@

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

@@items scheduler.rs Task TASKS start every@@

@@items scheduler.rs BELIEF_SYNC_BATCH beliefs@@

Each task is a loop on the process `TaskTracker`: `interval_at` with a first delay (so start-up isn't one burst of queries), `MissedTickBehavior::Delay` (a run that overran doesn't cause catch-up runs), and a `select!` on the shutdown token. A failed run is logged and the next tick tries again. `every` is generic over the task body: `F: Fn(Arc<AppState>) -> Fut` and `Fut: Future<Output = Result<(), ApiError>>`, both `Send + 'static` because the loop is spawned and owns them; plain `async fn`s are passed directly.

| Task | Interval | What | Database pattern |
|---|---|---|---|
| `requeue` | 5 s | Expired leases and jobs of workers silent for 30 s back to `queued` with `queued_at` reset; a job out of attempts fails for good through the pipeline; the store wakes the claims after its commit | `UPDATE job SET status = 'queued' … WHERE status = 'leased' AND lease_until < time::now()` on the `job_lease` index ([database 4](../database/README.md#job-claim-with-slots-)) |
| `sweep` | 30 s | `Pipeline::sweep`, the one owner of recovery and continuation: stale steps (2 min; 5 for `plan`) and question photos stuck in `measuring` for 2 min, re-claimed and re-run; questions unanswered for 10 minutes, continued with the best candidate marked uncertain (skipped while a photo for it is analysing). Follow-ups spawned | Compare-and-set on the old stamp and run ([database 4](../database/README.md#steps-between-jobs-)); the `score` claim |
| `idle` | 60 s | Limiter keys (`retain_recent`, `shrink_to_fit`), idle broadcast channels, unheld plan locks | In memory |
| `beliefs` | 10 s | `agent::beliefs::sync_pending(…, 50)`, the only writer to Agent Memory: erases the subjects queued in `belief_forget` (then re-marks their remaining shared memories unsynced), then records the unsynced shared memories, oldest first. A failure is logged and retried next run without holding up the rest; a failed erase holds back the records under its subject. The counts are logged when any is above 0. With Agent Memory off it only empties the queue | `belief_forget` (`belief_forgets`, `finish_belief_forget`); `memory.belief_synced_at` (`unsynced_shared`, `mark_synced`) |
| `cleanup` | 10 min | Expired idempotency keys, weather older than 2 hours, expired pending actions, finished jobs older than 7 days, account tombstones 24 h after Clerk confirmed the deletion (`TOMBSTONE_KEEP`), farm connectors still `pending` 10 minutes after their connect started (`PENDING_CONNECT_TTL`, `delete_expired_connects`) | Batches of 500; `weather_cache` by key range ([database 4](../database/README.md#cleanup-no-ttl)) |
| `weather` | 1 h | Group gardens by cell, fetch each missing cell once (batches of 50, a failed batch retried once, then cell by cell, `integrations::open_meteo::fetch_cells`), cache them; fire forecast alerts (task + push to every member) and plan triggers (task on the rule's plant) | `weather_cache:[hour, cell]`; tasks deduplicated by the unique `alert_key` ([database 4](../database/README.md#alert-dedupe-)) |
| `soil` | 15 min | `SoilGrids` for due gardens, at most 4 a run (its limit is 5 calls a minute): `pending`, or `unavailable` past `soil_retry_at`. A failure stores `unavailable` with the next attempt `soil_retry_after` it ([contracts 9](../architecture/contracts.md#gardens)): 15 minutes after an outage (timeout, connection error, `5xx`, `429`), 7 days after `NotFound` (null pixels at the point and the 8 nearby probes). So a null-pixel garden costs its 9 paced calls once a week, not every 15 minutes; an outage stops at the first call. `POST /gardens` makes the first attempt the same way (`fetch_soil_profile`) | `garden.soil_profile`, `garden.soil_retry_at`; `gardens_due_soil_profile`, `soil_profile_failed` |
| `receipts` | 15 min | Expo push receipts: forget `DeviceNotRegistered` tokens (`Notifier::check_receipts`) | `push_token` deletes |
| `orphans` | 24 h | Objects older than an hour under each garden's prefix that no record points to, deleted through the delete queue ([storage 4](../storage/README.md#4-writes-and-deletes)) | `DeletionRepo::unreferenced_keys` over every key field |

@@items scheduler.rs soil fetch_soil_profile@@

@@items scheduler.rs weather_and_triggers@@

**Alert and plan-trigger dedupe.** A forecast alert's key is `<garden>:<trigger>:<date>` and a plan trigger's `plan:<plan>:<step>:<date>` ([contracts 7](../architecture/contracts.md#task)); the store's insert fails on a duplicate and `create_alert_task` answers `None`, so overlapping runs, or a run repeated after a crash, create each task once and push once. "Today" is the forecast's first day: Open-Meteo answers in the cell's time zone, so no time-zone database is needed.

**The trigger rules** are pure functions of a cell's weather. Their thresholds are `domain::rules::weather::WeatherThresholds` ([domain 7, weather thresholds](domain.md#weather-thresholds)), held in `Config::weather` and also given to Open-Meteo (each day's `frost_risk`) and the pipeline (fusion), so an alert and a diagnosis never disagree about frost or a humid stretch. The thresholds aren't in the contracts, so the defaults are a proposal (section 23). Only the 3-day horizon is the scheduler's own:

@@items scheduler/triggers.rs HORIZON_DAYS fired@@

---

## 17. Deletion

@@items deletion.rs DeletionError DeletionRunner@@

@@impl deletion.rs impl DeletionRunner@@

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

**Without `CONNECTOR_KEY`** nothing can be unsealed. A garden's cascade still deletes its connectors, unrevoked, because the garden goes. A demoted, removed or leaving owner's connectors stay `deleting`, hidden from the list and the sync, and `resume` logs a warning at each start instead of finishing them, so their tokens are revoked once the key is configured (an account deletion of such an owner waits for them too, since `connector.created_by` rejects the user's deletion).

**Why revoke after the mark and before the delete.** Revoking after the rows were gone would leave the only copy of the token in memory, and a crash in between would lose the revoke for good. With the mark committed first, the row keeps its sealed token until the revoke has been asked for, so `resume` can finish it: it lists `deleting_connectors(None)` (every connector still `deleting` outside a garden being deleted, whose resumed cascade finishes its own) and runs each through `finish_connector`. The worst case after a restart is a second revoke of a token already revoked, which both vendors answer as revoked (Slack's `token_revoked` and `invalid_auth`, Google's `invalid_token`). The tests record the revokes through a `ConnectVendors` fake: one revoke per connector, after the store's marking call and before the first batch; a failed or panicking revoke followed by every batch; a shared account's token kept; a restart between the commit and the revoke finished by `resume`; and, with connectors off, a former owner's connectors left `deleting` while a garden's are deleted.

**Account deletion from the webhook** has no `CurrentUser` (no session token), so it uses `authz::check_membership_of`, an additive domain function that reads the role for a `UserId` the verified webhook named (a domain change this crate made; the proof still says what the store says).

---

## 18. Media

@@items media.rs SignedQuery verify stream_object@@

@@items routes/media.rs scan_image species_photo@@

The signature is the authorisation ([storage 3](../storage/README.md#3-how-images-are-served)): no `Authorization` header, so plain `<img>` tags work in the web build. `verify_signed` ([domain 5](domain.md#5-signed-url-typing)) refuses an `exp` in the past or more than 15 minutes ahead and compares the HMAC in constant time (`Hmac::verify_slice`); all three are `403 invalid_signature`. A valid signature for a record that no longer exists is `404`. The body is the blob store's `'static` chunk stream handed to hyper with `Body::from_stream` (`BlobError` implements `std::error::Error + Send + Sync`, which `Body` needs); if the client goes away, dropping it closes the connection to `RustFS`, which is fine for reads. Headers: `Content-Type`, `ETag` passed through verbatim (S3 ETags carry their quotes), `Content-Length`, `Cache-Control: private, max-age=900`. Compression skips `image/*`. Depth maps have no route.

✓ The HMAC: `signed_urls_verify_until_they_expire` signs a URL with the real `HmacSigner`, verifies it, refuses it at 16 minutes, and refuses a tampered signature.

---

## 19. Rate limits and budgets

@@items limits.rs RateLimits check@@

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
| Handlers | The real router (`router::app`), so layers, extractors, the renderer and request ids are under test, driven with `tower::ServiceExt::oneshot`. The ports are mockall mocks from `domain`'s `mock` feature; the store is `tests/fake_store.rs`, a `Store` made of one mock per repository trait with every method delegated, **generated** from `domain::ports::store` by `backend/tools/apigen/gen_fake_store.py` (regenerate when a store trait changes). The generator scans each trait body for `async fn` signatures that end in `;` or open a default body, skipping the body by brace matching, so a method with a default body (`live_lease`, `soil_profile_failed`, `gardens_due_soil_profile`, the account-tombstone methods) is delegated like any other and a test sets its expectations. The seams are small fakes: `NoPipeline` and `NoChat` (`unreachable!()`), `FixedImages` with a fixed answer |
| Time | `FixedClock`; tokio's paused clock (`start_paused = true`) for the broker's long poll |
| Scheduler | `scheduler::fetch_soil_profile` called directly, with a mocked `SoilProvider` |
| Pure parts | Config parsing, error mapping and statuses, the fallback renderer, canonical fingerprints, multipart fingerprints, the HMAC signer, the invite limiter, trigger rules, hour truncation, worker tokens |

@@test tests.rs an_upload_that_fails_the_quality_check_is_422_and_never_stored@@

@@test tests.rs a_repeated_key_replays_the_stored_response_without_running_the_handler@@

@@test tests.rs a_non_member_gets_the_same_404_as_an_unknown_scan@@

@@test tests.rs sse_resume_replays_after_last_event_id_then_goes_live_without_duplicates@@

The paths added in v0.3 each have a test. A worker's events after its job completed answer `204` and never reach the pipeline (`NoPipeline::worker_events` would panic); a tombstoned account's token is `401 unauthenticated` and `me` is never read; a second `DELETE /me` gets `false` from the tombstone's compare-and-set, answers `202` and spawns no cascade; and `scheduler::fetch_soil_profile` stores the next attempt 7 days after `NotFound` and 15 minutes after an outage:

@@test tests.rs late_worker_events_after_complete_are_204_and_dropped@@

@@test tests.rs a_tombstoned_accounts_token_is_401_and_never_creates_a_user@@

@@test tests.rs a_second_delete_me_while_a_cascade_runs_starts_nothing@@

Agent Memory (`tests/beliefs.rs`, over `MockBeliefStore` and a hand-written `SlowBeliefs`): `GET /plants/{id}/beliefs` answers `available: false` without Agent Memory, on an error and after 3 s of a slow store (paused time); returns the current beliefs and only the superseded history; is `404` to a non-member. That a cascade queues its erase is a store test (`remove_fail_feedback_and_plant_deletion` and the account flow, [store](store.md)), and the sweep is the agent's (`beliefs`, [agent](agent.md)). The config tests cover the three variables: none set, all set (the key never in the formatted config), and a partial set refused naming the missing one.

@@test tests/beliefs.rs a_member_reads_current_beliefs_and_the_superseded_history@@

@@test tests.rs soil_failures_schedule_the_next_attempt_by_their_kind@@

The 33 tests: `an_upload_that_fails_the_quality_check_is_422_and_never_stored`, `a_repeated_key_replays_the_stored_response_without_running_the_handler`, `a_key_still_running_is_409_request_in_progress`, `a_non_member_gets_the_same_404_as_an_unknown_scan`, `a_viewer_is_403_with_the_required_role`, `unknown_paths_get_an_error_body_and_a_request_id`, `sse_resume_replays_after_last_event_id_then_goes_live_without_duplicates`, `a_resting_event_stored_during_the_replay_still_ends_the_stream`, `a_resting_point_reached_without_an_event_closes_the_stream`, `deleting_the_scan_closes_its_stream`, `a_scan_at_rest_replays_and_closes`, `an_empty_queue_answers_none_at_the_deadline_after_polling`, `signed_urls_verify_until_they_expire`, `an_image_body_ends_on_the_shutdown_token`, `the_sixth_quick_invite_attempt_waits_12_seconds`, `key_order_doesnt_change_the_fingerprint`, `the_fingerprint_covers_bytes_not_boundaries`, `every_code_keeps_its_status_and_retryable`, `a_timeout_503_carries_retry_after_like_any_other`, `store_conflicts_map_to_their_codes`, `unknown_statuses_fall_back_to_a_contract_code`, `defaults_and_derived_values`, `short_signing_key_is_refused`, `frost_and_a_humid_stretch_fire_on_their_dates`, `hours_truncate`, `identifies_only_exact_tokens`, `ids_used_by_the_tests_parse`, `late_worker_events_after_complete_are_204_and_dropped`, `a_claim_body_over_the_json_limit_is_refused_before_the_handler`, `a_tombstoned_accounts_token_is_401_and_never_creates_a_user`, `a_second_delete_me_while_a_cascade_runs_starts_nothing`, `soil_failures_schedule_the_next_attempt_by_their_kind`, `a_signal_handler_that_fails_to_install_never_fires`.

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

- **A location that can't be geocoded** is `400 invalid_request` with `details.field` naming the input (`location.postal_code`, `location.city`, `location.lat`).
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
