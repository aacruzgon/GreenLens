# PDM Backend Architecture

> **Status:** Draft v0.2.15, 2026-10-23. v0.2.15: section 2's diagram puts the GPU rig on a host per card (D11, #13). v0.2.14: the workspace layout lists `integrations`' `slc` module with its embedded `data/slc_v3r2.bin`, and `tools/slc/build_slc.py`, which builds it (section 6). v0.2.13: the crate graph lists `era-eval`, the evaluation tool, which uses `api`'s library, and the rule reads "nothing in the deployed graph depends on a binary" (section 5). v0.2.12: the workspace layout lists `tools/era-eval`, the farm connectors' agent evaluation (a workspace member that runs `api`'s farm sync through `api`'s new library target), and `api`'s `lib.rs` (section 6). v0.2.11: `aes-gcm` 0.11.1 is pinned for sealing the farm connectors' tokens (section 4), and section 9 lists the connector variables: `CONNECTOR_KEY`, the Slack and Google OAuth clients, `CONNECTOR_RETURN_URL` and the Era bases. v0.2.10: the workspace layout shows `tests/fixtures/era/`, the Era simulators' farm fixture (section 6). v0.2.9: the roadmap now has ten slices, with S9 for the farm connectors (section 3). v0.2.8: Agent Memory adds the `beliefs` scheduler task, the only writer to it (section 8), and the three optional `AGENT_MEMORY_*` variables (section 9). v0.2.7: section 11 says CI's `docs` job regenerates the documents and gates on `checklinks.py` and `check_fences.py`. v0.2.6: section 12 links [docs/infra](../infra/README.md), keeps model files out of every image (they come from the `/models` volume) and takes the base images from infra 3. v0.2.5: adds [roadmap.md](roadmap.md), the work as slices with exit checks, to section 3 and section 13. v0.2.4: the schema now lives at `backend/migrations/schema.surql` and nowhere else, so section 6 no longer keeps two copies. v0.2.3: the workspace and the generator tools are now in `backend/`; section 6 lists `tools/`, and `migrations/schema.surql` is a copy of `docs/database/schema.surql` until the move. v0.2.2: section 7 says the Rust skills follow this design instead of listing departures; the generator tools move to `backend/tools/`; moving the scratch workspace into `backend/` is decided. v0.2.1: the release profile keeps symbols and adds line tables (stripping symbols empties panic backtraces, probed); the `api` container's `stop_grace_period` is 190 s; the scheduler tree lists the eight real tasks, with question continuation inside `sweep`; the database row names all four protected parameters. v0.2: the module trees, crate graph and configuration now match the compiled crates; adds the decisions on contracts open questions 12–24 and the changes to contracts v0.10, architecture v0.7 and database v1.4.
> v0.1: the Rust backend designed crate by crate, down to every struct, enum, trait, generic parameter and lifetime, with each definition compiled on Rust 1.99.1 against the pinned crates; it follows [contracts](../architecture/contracts.md) v0.9 and [architecture](../architecture/README.md) v0.6.

Companion to the [system architecture](../architecture/README.md), the [data contracts](../architecture/contracts.md), the [database](../database/README.md) and [storage](../storage/README.md) notes, the [frontend](../frontend/README.md) and the [computer-vision design](../vision/README.md). Those documents decide **what** the system does; this directory decides **how the Rust code is built**: crates, modules, types, traits, tasks and errors.

## Contents

1. [Goals](#1-goals)
2. [System context](#2-system-context)
3. [Documents in this directory](#3-documents-in-this-directory)
4. [Stack and version pins](#4-stack-and-version-pins)
5. [Crate graph and dependency rules](#5-crate-graph-and-dependency-rules)
6. [Workspace layout](#6-workspace-layout)
7. [Rust conventions](#7-rust-conventions)
8. [Runtime composition](#8-runtime-composition)
9. [Configuration](#9-configuration)
10. [Observability](#10-observability)
11. [Testing and tooling](#11-testing-and-tooling)
12. [Builds and deployment](#12-builds-and-deployment)
13. [Build plan](#13-build-plan)
14. [Changes this design made to other documents](#14-changes-this-design-made-to-other-documents)
15. [Open decisions](#15-open-decisions)
16. [Sources](#16-sources)

---

## 1. Goals

| Goal | How the design meets it |
|---|---|
| **The contracts hold by construction** | Every wire shape is a type in `domain` ([domain-types](domain-types.md)), so `utoipa` generates the OpenAPI document the app's types come from, and `schemars` the seed and plan-writer schemas ([contracts 1, source of truth](../architecture/contracts.md#source-of-truth)) |
| **Rules the compiler checks** | Garden access is a proof value, `Membership<R>`, that only the membership check can create, and "editor or above" is a type bound ([domain 8](domain.md#8-authorization-proofs)). A stored event with image ids can't be sent, and a signed URL can't be stored ([domain 5](domain.md#5-signed-url-typing)) |
| **Every database guarantee in one place** | The write patterns verified on SurrealDB 3.3.4 ([database 4](../database/README.md#4-write-patterns)) are named statements in `store`, each behind one repository method |
| **A trait at every external boundary** | The boundary traits of [architecture 2](../architecture/README.md#design-principles) live in `domain::ports`; implementations sit in leaf crates and are wired in the binaries, so tests swap in mocks and a vendor change is one adapter |
| **Nothing blocks the runtime** | CPU work (image decoding, face blurring, measurement, embeddings, ONNX) runs on blocking threads behind semaphores; every long-lived task has a cancellation token and is tracked for shutdown |
| **Recoverable after any crash** | Every step between jobs holds a claim the sweep can recover ([contracts 3](../architecture/contracts.md#scan-lifecycle)); deletions resume from `deleting_at`; workers hold leases, never state |

---

## 2. System context

The backend is two binaries built from one Cargo workspace, run as five kinds of process:

```
                         ┌──────────── control plane ─────────────────────────────────────────┐
  app (Expo) ──HTTPS──▶  │ Caddy ──▶ api ─────────┬──▶ SurrealDB 3.3.4   (store)              │
  Clerk ──webhooks────▶  │           │            ├──▶ RustFS 1.0.1      (blob)               │
                         │           │            └──▶ LLM API, Open-Meteo, SoilGrids,        │
                         │           │                 geocoders, Expo Push, Clerk (integrations)
                         │           ▲                                                         │
                         │  worker --caps fallback  (LLM API vision)                            │
                         │  seed-upload (one-shot, knowledge photos, not Rust)                  │
                         └───────────┬────────────────────────────────────────────────────────┘
                                     │ outbound HTTPS only: claim · heartbeat · events · complete
                         ┌───────────┴──────────── GPU rig (a host per card) ─────────────────┐
                         │ worker --caps perception   RTX 3070     ort / CUDA                  │
                         │ worker --caps vlm          RTX 5070 Ti  ──▶ vLLM (CUDA) sidecar     │
                         │ worker --caps vlm          RX 9070 XT   ──▶ vLLM (ROCm) sidecar     │
                         └─────────────────────────────────────────────────────────────────────┘
```

| Process | Binary | Crates it links | Designed in |
|---|---|---|---|
| `api` | `api` | all library crates | [api](api.md) |
| `perception-worker` | `worker --caps perception` | `vision`, `integrations`, `domain` | [worker](worker.md), [vision](vision.md) |
| `vlm-worker` (one per VLM card) | `worker --caps vlm` | the same | [worker](worker.md) |
| `fallback-worker` | `worker --caps fallback` | the same | [worker](worker.md) |
| `seed-upload` | Python loader | none | [architecture 5, knowledge base](../architecture/README.md#knowledge-base) |

The workers never touch the database or object storage: they only call the worker API ([contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)). `api` is the only process with credentials for SurrealDB and RustFS, and the only one that talks to Clerk, the weather and soil services and Expo Push.

---

## 3. Documents in this directory

| Document | What it designs | Read it when |
|---|---|---|
| [domain.md](domain.md) | The shared crate: ids, wire primitives, the signed-URL typing, error codes, the pure rules (state machines, role matrix, routing), authorization proofs, every boundary trait | You touch anything; it is the vocabulary of every other document |
| [domain-types.md](domain-types.md) | Every contract shape as Rust, numbered like contracts.md (contracts 7 ↔ domain-types 7), with a TypeScript → Rust map | You write or read a request, response, event, job, VLM shape or seed record |
| [store.md](store.md) | `store`: SurrealDB connection, schema migration, row types, every repository method and its SurrealQL, transactions and conflict retries | You write a query or a repository method |
| [blob.md](blob.md) | `blob`: the RustFS object store, keys, streaming, batched deletes that are never cancelled | You touch images, masks or depth maps in storage |
| [integrations.md](integrations.md) | `integrations`: the LLM API and OpenAI-compatible clients, Open-Meteo, SoilGrids, geocoding, Clerk, Svix, Expo Push, Jev | You call an outside service |
| [vision.md](vision.md) | `vision`: ONNX sessions and execution providers, perception stages, the CPU upload pipeline (decode, faces, WebP, depth), LiDAR measurement, the text embedder, mask RLE | You touch a model call, an upload or a measurement |
| [knowledge.md](knowledge.md) | `knowledge`: the knowledge store, candidate ranking, deterministic scoring and diagnosis status | You touch retrieval or verification |
| [agent.md](agent.md) | `agent`: chat turns, tools and confirmation cards, the dose check and plan validation, the plan writer, context, memory | You touch chat, plans or safety |
| [pipeline.md](pipeline.md) | `pipeline`: the steps between jobs, claims and hand-overs, measurable indicators, fusion, re-planning, late work | You touch what happens to a scan between jobs |
| [api.md](api.md) | The `api` binary: start-up, layers, auth and access, idempotency, every endpoint, uploads, SSE, the job broker, the scheduler, deletion, media | You add or change an endpoint or a background task |
| [worker.md](worker.md) | The `worker` binary: leases, heartbeats, cancellation, the VLM adapters and per-job schemas, the fallback, multi-vendor GPUs and the rig's compose services | You touch a worker or the rig |
| [flows.md](flows.md) | Server-side sequences across crates: a request, an upload, a scan from submit to plan, step claims and the sweep, SSE resume, a job's lease, a chat turn, deletion | You implement or debug a flow end to end |
| [roadmap.md](roadmap.md) | The build-week work as ten slices (S0–S9): what each delivers, what it needs from the other tracks, its exit checks and the decisions it closes, plus the gaps still to design | You plan or pick up backend work, or need to know what "done" means for a capability |

**How the listings stay true.** Each listing of a type or trait is copied from the compiled source by a small generator (section 11), so a document can be regenerated when the code changes.

---

## 4. Stack and version pins

| Item | Pin | Notes |
|---|---|---|
| Rust | 1.99.1, edition 2024, `rust-version = "1.99"` | `rust-toolchain.toml`; resolver 3 |
| Async runtime | `tokio` 1.53.3, `tokio-util` 0.7.20, `tokio-stream` 0.1.19, `futures` 0.3.34 | `CancellationToken`, `TaskTracker`, `BroadcastStream` |
| HTTP server | `axum` 0.8.9, `tower` 0.5.3, `tower-http` 0.7.1, `governor` 0.10.4 | `/{id}` paths, native-async extractors |
| OpenAPI and schemas | `utoipa` 6.0.0, `utoipa-axum` 0.3.0, `schemars` 1.2.3 | OpenAPI 3.1 |
| Wire types | `serde` 1.0.229, `serde_json` 1.0.154, `chrono` 0.4.45, `ulid` 3.0.0, `uuid` 1.28.0 | `Ulid::generate()` in ulid 3 |
| Errors and logs | `thiserror` 2.0.21, `anyhow` 1.0.104 (binaries only), `tracing` 0.1.44, `tracing-subscriber` 0.3.23 | JSON logs |
| Async traits | `async-trait` 0.1.92 | [domain 10](domain.md#10-concurrency-and-lifetimes) |
| Database | `surrealdb` =3.3.4 | Moves with the server ([database 1](../database/README.md#1-version-and-setup)) |
| Object storage | `aws-sdk-s3` 1.152.0, default features off, `default-https-client`, `rt-tokio`, `http-1x` | Builds the client of [storage 5](../storage/README.md#5-rust-client) |
| Outbound HTTP | `reqwest` 0.13.5 (`json`, `stream`, `query`), `jsonwebtoken` 11.1.0 (`aws_lc_rs`), `moka` 0.12.16, `secrecy` 0.10.3, `tzf-rs` 2.1.3 | rustls by default in reqwest 0.13 |
| Sealing | `aes-gcm` 0.11.1 (`zeroize`) | AES-256-GCM over the farm connectors' vendor tokens, in `api` ([api 5](api.md#5-configuration)) |
| Models and images | `ort` =2.0.0-rc.14 (`load-dynamic`, API level 29; ONNX Runtime 1.31 on CUDA 13), `ndarray` 0.17.2, `tokenizers` 0.23.2, `image` 0.25.10, `imageproc` 0.27.0, `fast_image_resize` 6.1.0, `webp` 0.3.1, `png` 0.18.1, `libheif-rs` 3.0.0, `nalgebra` 0.35.0, `chacha20` 0.10 | `ort` is a release candidate: pinned exactly |
| Tests | `mockall` 0.15.0, `insta` 1.49.0, `proptest` 1.12.0, `wiremock` 0.6.5, cargo-nextest 0.9.148 | SurrealDB's embedded `kv-mem` engine in dev-dependencies only |

The pins match the project's Rust skills (`.claude/skills/rust/versions.md`), which hold what was checked for each, such as the duplicate-crate notes and why `aws-sdk-s3` drops its default features. Several are recent majors (`tower-http` 0.7, `utoipa` 6, `reqwest` 0.13, `ulid` 3, `rand` 0.10, `jsonwebtoken` 11, `surrealdb` 3); the crate documents note every API that differs from older releases.

**Not used, and why.**
- **`fastembed`** pins `ort` rc.13, which can't link beside rc.14 (`links = "onnxruntime"`); the embedder is written on `ort` and `tokenizers` directly ([vision](vision.md)).
- **The `svix` crate** is a full API client; webhook verification is the documented HMAC scheme in 30 lines, with `svix` kept as a test oracle ([integrations](integrations.md)).
- **`aws-config`**: credentials are passed explicitly.
- **SurrealDB live queries**: `api` makes every write, so it broadcasts events itself.

---

## 5. Crate graph and dependency rules

Each line lists what a crate depends on. Nothing in the deployed graph depends on a binary, and there are no cycles. The evaluation tool `era-eval` is a dev-only binary that never ships; it uses `api`'s library target, which exists for it, and nothing depends on `era-eval`.

```
api (bin)      ──▶ pipeline, agent, knowledge, vision, integrations, store, blob, domain
worker (bin)   ──▶ vision, integrations, domain
era-eval (tool bin) ──▶ api (lib), agent, store, blob, knowledge, vision, integrations, domain
pipeline       ──▶ agent, knowledge, vision, domain
agent          ──▶ integrations, domain
knowledge      ──▶ domain            (reads through the KnowledgeRepo port, which store implements)
vision         ──▶ domain
integrations   ──▶ domain
store          ──▶ domain
blob           ──▶ domain
domain         ──▶ no workspace crate
```

| Crate | Depends on | Owns | Never |
|---|---|---|---|
| `domain` | none in the workspace | Contract types, rules, proofs, ports | I/O, a runtime, a clock read |
| `store` | `domain` | SurrealDB | Business rules beyond the database patterns |
| `blob` | `domain` | RustFS | Knowing what an object is for |
| `integrations` | `domain` | Outbound HTTP | Storage, business rules |
| `vision` | `domain` | Models, images, geometry | HTTP, storage |
| `knowledge` | `domain` | The knowledge store and the scoring rules, reading through `KnowledgeRepo` | Writing anything but knowledge reads |
| `agent` | `integrations`, `domain` | Chat, tools, safety, the plan writer | Storing a plan (it returns one) |
| `pipeline` | `agent`, `knowledge`, `vision`, `domain` | The steps between jobs | HTTP, SSE, concrete adapters |
| `api` | all | Composition, HTTP, SSE, broker, scheduler | Business rules that belong in a crate |
| `worker` | `vision`, `integrations`, `domain` | The worker loop and VLM adapters | Any database or storage access |

**The rule behind the graph:** a crate depends on another for its **code**, never for its **traits**, which all live in `domain`. So `pipeline` drives storage through `Arc<dyn Store>` without depending on `store`, and tests run it against mocks.

`integrations` is an addition to [architecture 3](../architecture/README.md#rust-crates): one crate for every outbound HTTP adapter, so the worker's fallback and `agent` share one LLM client.

---

## 6. Workspace layout

```
backend/
├─ Cargo.toml              [workspace]: members, [workspace.package], [workspace.dependencies], [workspace.lints]
├─ Cargo.lock              committed: the binaries are deployed
├─ rust-toolchain.toml     1.99.1 with rustfmt and clippy
├─ clippy.toml             allow-unwrap-in-tests
├─ crates/
│  ├─ domain/              src/{id, wire, url, error, events, authz, scan, jobs, vlm, chat, knowledge, schema_util, prelude, testing}.rs
│  │                       + garden/, records/, rules/, ports/
│  ├─ store/               src/{client, tx, error, ids, rows, fragments, read}.rs + repo/ (one module per area)
│  ├─ blob/                src/{client, s3, delete, memory, sweep, error}.rs
│  ├─ integrations/        src/{http, sse, openai_compat, soilgrids, slc, clerk, svix, expo_push, jev}.rs + anthropic/, open_meteo/, geocode/
│  │                       + data/slc_v3r2.bin (Soil Landscapes of Canada, embedded)
│  ├─ vision/              src/{depth, embed, imaging, manifest, mask, rle, error}.rs + runtime/, perception/, upload/, measure/
│  ├─ knowledge/           src/{store, index, candidates, weather_fit, scoring, status, error}.rs
│  ├─ agent/               src/{config, deps, error, prompt, context, memory}.rs + safety/, model/, plan_writer/, tools/, turn/
│  └─ pipeline/            src/{entry, writer, runs, payloads, results, findings, questions, fusion, replan, plan_author, error}.rs
│                          + steps/, measure/
├─ tools/
│  ├─ docgen/              docgen.py, regen_all.sh, check_fences.py, checklinks.py and one <doc>.md.tpl per crate document
│  ├─ apigen/              gen_fake_store.py: api's test FakeStore, from the store traits
│  ├─ slc/                 build_slc.py: integrations' data/slc_v3r2.bin, from AAFC's published SLC v3.2 files
│  └─ era-eval/            a workspace binary: the farm connectors' agent evaluation on Era's simulators (connectors 9.3,
│                          docs/EON E8); src/{main, bench, garden, answers, score, journal, meter, report}.rs
├─ bins/
│  ├─ api/                 src/{lib, main, config, wiring, state, seams, router, openapi, error, reply, limits, signing, uploads,
│  │                       broker, workers, push, deletion, media}.rs + extract/, events/, routes/, scheduler/, tests/
│  └─ worker/              src/{main, config, api, job, fallback}.rs + lifecycle/, vlm/
├─ migrations/
│  └─ schema.surql         the database schema, embedded by store and applied by api on start (database 1)
└─ tests/
   └─ fixtures/era/        the Era simulators' farm fixtures (Maple Row Farm), read by the era tests at run time;
                           left out of the image build context (docs/EON E0.4)
```

Each crate document has its own module tree. Modules follow the `foo.rs` + `foo/` layout, without `mod.rs`.

---

## 7. Rust conventions

The decisions every crate follows. Each links to where it is explained in full.

| Topic | Rule | Where |
|---|---|---|
| Edition and lints | Edition 2024; `unsafe_code = "deny"`; clippy pedantic with a few allows; `unwrap_used` warns outside tests | [section 11](#11-testing-and-tooling) |
| IDs | `Id<T: Table>` over a ULID with a `PhantomData<fn() -> T>` marker; `Slug<K>` for knowledge ids; serialised as bare strings; `store` maps them to `RecordId` | [domain 4](domain.md#4-ids-and-primitives) |
| Time | A `Timestamp` newtype (whole seconds on the wire, full precision in memory) and `LocalDate`; every "now" comes from the `Clock` port | [domain 4](domain.md#4-ids-and-primitives) |
| Wire types | Field for field with contracts.md; optional fields omitted, never `null`, unless the contract says `| null`; tagged unions as the contract tags them; `deny_unknown_fields` on request bodies only | [domain-types](domain-types.md) |
| Signed URLs | URL-bearing types carry `U: UrlState = Signed`; `Stored` holds ids, `Signed` holds URLs; only signed types can reach a response, and only `Sign` crosses | [domain 5](domain.md#5-signed-url-typing) |
| Authorization | `Membership<R>` and `ConversationOwner<R>` proofs with private constructors; `at_least::<R2>()` downgrades at compile time | [domain 8](domain.md#8-authorization-proofs) |
| Dispatch | Adapters behind `Arc<dyn Port>` with `#[async_trait]`, since native `async fn` in traits isn't dyn compatible on 1.99; pure algorithms and typestate use generics; no generic `AppState` | [domain 10](domain.md#10-concurrency-and-lifetimes) |
| Lifetimes | Owned data across tasks, channels, the database and the wire; borrowed views (`&'a T`, `Cow<'a, str>`) for hot synchronous work such as scoring, the dose check, schema building and geometry. Each document explains every named lifetime | every crate document's "Concurrency and lifetimes" |
| Errors | `thiserror` enums per crate with `#[from]` upwards; port errors shared in `domain`; `ErrorCode` is data, mapped to HTTP only in `api`; `anyhow` only in `main.rs` | [domain 6](domain.md#6-errors), [api](api.md) |
| Async | tokio; `spawn_blocking` for CPU work behind a `Semaphore`; `mpsc` actors for per-scan writers; `broadcast` for the SSE hub; `Notify` for job inserts; `CancellationToken` and `TaskTracker` for shutdown | [api](api.md), [worker](worker.md) |
| Names | Contract nouns for types; verbs on records for repository methods; no `get_` prefixes ([API Guidelines C-GETTER](https://rust-lang.github.io/api-guidelines/naming.html#getter-names-follow-rust-convention-c-getter)) | |

**The project's Rust skills.** The skills in `.claude/skills/rust-*` follow this design, including the choices that once differed from them: `#[async_trait]` on the ports, one generic `Id<T>`, the `integrations` crate, store tests on SurrealDB's embedded `kv-mem` engine, `503 upstream_unavailable` for a request timeout (contracts 2, timeouts; question 17), and access checks as extractors that produce proofs. Where a skill and these documents disagree, the documents win.

---

## 8. Runtime composition

### `api`

```
main
 ├─ tracing (JSON) · config (env, secrets as SecretString)
 ├─ SurrealDB: connect once, sign in as `api`, apply schema.surql (store)
 ├─ RustFS client (blob) · outbound clients (integrations)
 ├─ CPU models: face detector, text embedder, measurement config (vision)
 ├─ AppState { Arc<dyn Store>, Arc<dyn BlobStore>, Arc<dyn …> ports, ScanWriters, EventHub, JobNotify, … }
 ├─ TaskTracker
 │   ├─ scheduler tasks: requeue (leases, silent workers) · sweep (stale steps, stalled measuring, question
 │   │                   continuation) · idle · cleanup · weather (alerts, plan triggers) · soil · receipts · orphans ·
 │   │                   beliefs (every 10 s, the only writer to Agent Memory: queued erases, then unsynced shared memories)
 │   ├─ deletion runner (resumes unfinished deletions on start)
 │   └─ per-scan writer actors (spawned on demand, stopped when idle)
 └─ axum::serve(...).with_graceful_shutdown(token)   SSE streams end with take_until(token)
```

### `worker`

```
main --caps <perception|vlm|fallback>
 ├─ config · model manifest check (SHA-256 of every file) · adapters (ort sessions or the VLM server client)
 ├─ heartbeat task, every 10 s: current jobs + lease tokens + versions → cancel_job_ids
 ├─ claim loop: long-poll while held jobs < slots
 │   └─ per-job task: CancellationToken (cancel, max_runtime, shutdown) · fetch images · run · events · complete (retried)
 └─ SIGTERM: stop claiming, let held jobs finish within their deadline, then exit
```

The details are in [api](api.md) and [worker](worker.md); the sequences across them are in [flows](flows.md).

---

## 9. Configuration

Each binary reads typed configuration from the environment at start and refuses to start on a missing or malformed value. Secrets are `SecretString`, never logged. The full tables are in [api 5](api.md#5-configuration) and [worker 4](worker.md#4-configuration); the variables that tie the system together are:

| Variable | Used by | Meaning |
|---|---|---|
| `SURREAL_URL`, `SURREAL_USER`, `SURREAL_PASS` | api | `ws://surrealdb:8000`, the `api` database user |
| `SURREAL_ROOT_USER`, `SURREAL_ROOT_PASS` | api | Root credentials used only at start to apply `schema.surql` ([store](store.md)) |
| `RUSTFS_ENDPOINT`, `RUSTFS_ACCESS_KEY`, `RUSTFS_SECRET_KEY` | api | `http://rustfs:9000`, the `PDMAPI01` identity ([storage 2](../storage/README.md#2-bucket-keys-and-access)) |
| `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID`, `AGENT_MEMORY_API_KEY` | api | Optional, all three or none. SurrealDB Agent Memory's REST base URL, the context (`pdm-dev`, `pdm-prod`) and a key limited to `gardens/*` ([database: Agent Memory](../database/agent-memory.md#2-setup)). Unset: shared memories aren't mirrored |
| `CONNECTOR_KEY`, `SLACK_CLIENT_ID`, `SLACK_CLIENT_SECRET`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `CONNECTOR_RETURN_URL`, `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` | api | Optional. The farm connectors: the 32-byte sealing key as 64 hex characters (unset: connectors off), each vendor's OAuth client (both of a pair or neither), where the OAuth callback sends the app, and Era's simulators for the demo garden's seeding ([api 5](api.md#5-configuration)) |
| `API_URL_SIGNING_KEY` | api | HMAC key of signed image URLs, at least 32 bytes |
| `CLERK_ISSUER`, `CLERK_SECRET_KEY`, `CLERK_WEBHOOK_SECRET` | api | Session-token verification, account deletion, Svix verification |
| `LLM_API_KEY`, `LLM_CHAT_MODEL`, `LLM_CHEAP_MODEL` | api | `claude-opus-5-5` and `claude-haiku-5-5` by default |
| `LLM_API_KEY`, `FALLBACK_VISION_MODEL` | fallback worker | The hosted vision model, `claude-opus-5-5` by default |
| `WORKER_TOKENS` / `WORKER_TOKEN` | api / worker | Per-worker bearer tokens |
| `--caps`, `API_BASE_URL`, `WORKER_ID` | worker | The role (a flag), where to claim, and the worker's name |
| `VLM_ADAPTER`, `VLM_BASE_URL`, `VLM_MODEL`, `VLM_QUANT`, `VLM_SERVER`, `VLM_BOX_FORMAT`, `VLM_CELL_PX`, `VLM_MAX_PIXELS`, `VLM_SLOTS` | vlm worker | Per card ([contracts 6](../architecture/contracts.md#image-preparation)); the values become `model_versions` keys |
| `ORT_EP`, `ORT_DYLIB_PATH`, `PERCEPTION_MAX_BATCH` and the per-model caps | perception worker | `cuda` on the 3070; ONNX Runtime is loaded at run time; the job-wide batch cap |

---

## 10. Observability

- **Logs:** `tracing` with a JSON subscriber and `RUST_LOG` filtering. Every request has a `req_<ULID>` id on its span and in error bodies ([contracts 2](../architecture/contracts.md#2-errors)); every job has a span with `job_id`, `kind`, `scan_id` and `run`.
- **Never logged:** tokens, keys, signed URLs, request bodies, locations, chat text, image bytes.
- **Health:** `GET /healthz` (SurrealDB and RustFS reachable) and `GET /status` (worker heartbeats, queue depth, loaded model versions) ([contracts 9, ops](../architecture/contracts.md#ops)).
- **Audit trail:** the `job` table and `scan_event` already record what happened and when; `model_versions` on every record says what produced it.

---

## 11. Testing and tooling

| Check | What | Where |
|---|---|---|
| Format and lint | `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` | CI |
| Unit tests | Pure rules in `domain`, scoring and status in `knowledge`, the dose check and plan validation in `agent`, step decisions in `pipeline`, geometry and RLE in `vision`, schema building in `worker` | `cargo nextest run` |
| Contract tests | Exact JSON against contracts.md examples; snapshot tests (`insta`) of the OpenAPI document, VLM schemas and SSE frames | CI |
| Property tests | `proptest` for the dose check, scoring, the state machine and RLE round-trips | CI |
| Type-level tests | Compile-fail doc tests for the proofs and the URL states | `cargo test --doc` |
| Database tests | `store` against SurrealDB's embedded engine (`kv-mem`), multi-threaded for conflict retries; the full patterns against the pinned server image in a container | CI, `#[ignore]` for the container |
| Storage tests | `blob` against the pinned RustFS image in a container | `#[ignore]` locally |
| HTTP tests | axum handlers with `tower::ServiceExt::oneshot` and `domain`'s `mock` ports; upstream clients against `wiremock` and recorded fixtures | CI |
| Models | `ort` loads ONNX Runtime at run time (`load-dynamic`), so every build links; tests never build a session, and the execution providers (`cuda`, `migraphx`, `webgpu`) are cargo features chosen at run time by `ORT_EP` ([vision](vision.md)) | rig |

**`domain`'s test helpers** are features: `mock` gives a `mockall` mock of every port except `ChatModel`, and `test-util` gives `FixedClock`, `SequentialIds`, `FakeSigner`, `FixedPhotos` and `ScriptedChat` ([domain 11](domain.md#11-testing)).

**Regenerating the documents.** The listings in these documents are produced from the source by `docgen.py`. Templates (`<doc>.md.tpl`) hold the prose and directives (`@@items`, `@@file`, `@@impl`, `@@test`) that pull definitions by name, and `checklinks.py` checks every anchor. They live in `backend/tools/docgen/` with `regen_all.sh`, which rewrites every crate document in `docs/backend` (README.md and flows.md are written by hand), and `check_fences.py` (every listing balanced). After a change to the source, run `backend/tools/docgen/regen_all.sh` and commit the documents with the code; CI's `docs` job ([infra 2](../infra/README.md#2-ci)) runs the generator and fails when a document differs, so a listing that no longer matches its code fails the build; it also fails on a broken link (`checklinks.py`) or an unbalanced listing (`check_fences.py`). `backend/tools/apigen/gen_fake_store.py` writes `api`'s test `FakeStore` from the store traits ([api 21](api.md#21-testing)).

---

## 12. Builds and deployment

The images, their bases and registry, the compose files, CI and deploys are designed in [docs/infra](../infra/README.md); this section keeps what the Rust build decides.

| Image | Contents | Runs |
|---|---|---|
| `api` | Release build of `api`; ONNX Runtime CPU; libheif ≥ 1.19. No model files: the face detector and the text embedder are read from the `/models` volume ([vision 6](../vision/README.md#delivering-model-files-to-the-backend)) | Control plane ([architecture 12](../architecture/README.md#12-deployment)) |
| `worker-cuda` | `worker` with the `cuda` feature; the ONNX Runtime 1.31 CUDA 13 library beside it (`ORT_DYLIB_PATH`); the perception models come from the `/models` volume, not the image | The 3070's perception worker; also usable for a CUDA VLM worker |
| `worker` (no GPU) | `worker` without ONNX models | The VLM workers (the model runs in the vLLM sidecar) and the fallback worker |
| `vllm` sidecars | `vllm/vllm-openai` (CUDA) and `vllm/vllm-openai-rocm` v0.31.0, pinned by digest | One beside each VLM worker |

- **Release profile:** `lto = "thin"`, `codegen-units = 1`, `panic = "unwind"` (so `CatchPanicLayer` and task joins can contain a panic), `debug = "line-tables-only"`, `overflow-checks = true`, and no `strip`. `strip = "symbols"` would remove the symbol table, and a panic's backtrace then has no frames at all (`RUST_BACKTRACE=full` shows only `<unknown>` addresses). Without line tables, frames have function names but no file or line, because Cargo already strips debug info from a release build that requests none; line tables add them, at the cost of a larger binary (✓ rustc 1.99.1 on Linux in `rust:1.99.1-slim` and on macOS: a test binary went from 0.46 MB to 2.25 MB). The panic message's own `file:line:col` is kept either way ([Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)).
- **Docker builds** use `cargo-chef`-style dependency caching and a non-root user. `api` runs on `debian:trixie-slim` for libheif 1.19; the worker is built on bookworm so one binary runs on the trixie base and on the CUDA image's Ubuntu 24.04 ([infra 3](../infra/README.md#3-images)).
- **The rig's compose services** pin NVIDIA cards by GPU UUID and the RX 9070 XT by its render node ([worker](worker.md)).
- **The `api` service sets `stop_grace_period: 190s`** in `infra/control-plane.compose.yml`. Compose sends SIGKILL 10 s after SIGTERM unless a service sets it ([Compose file reference](https://docs.docker.com/reference/compose-file/services/#stop_grace_period)), which would cut the delete drain short and can leak objects on RustFS 1.0.1. 190 s covers the slowest in-flight request (60 s, the upload routes' timeout), then one full delete attempt (120 s), plus 10 s of margin. Raising `SHUTDOWN_DRAIN_S` above 120 s raises it by the difference ([api 4](api.md#4-start-up-and-shutdown)). The workers' services set theirs from `DRAIN_SECONDS` ([worker](worker.md)).

---

## 13. Build plan

The backend track of [architecture 14](../architecture/README.md#14-build-plan). [roadmap.md](roadmap.md) breaks it into slices with their dependencies and exit checks.

| Dates | Backend goal |
|---|---|
| Oct 16–17 | Workspace, lints and CI. `domain` from this design. `store`: connect, migrate, event append, job claim, idempotency. `api`: auth (Clerk), membership proofs, scans and uploads with the CPU blur check, submit, SSE with resume, the job broker with heartbeats and requeue. `worker` with a stub `VisionModel`. Text embedder in `api`. Control plane deployed. LiDAR day-1 check |
| Oct 18–19 | `vision` perception on the 3070 (BioCLIP 2, detector, SAM, disease head); face blurring and WebP; depth upload path. `integrations`: Open-Meteo, SoilGrids, geocoding. VLM worker on the 5070 Ti; AMD day-1 check on the 9070 XT. Species confirmation. Knowledge loading |
| Oct 20–21 | `knowledge` ranking and scoring; `pipeline` retrieve/score/plan steps with claims and the sweep; the VLM adapters and schemas; questions and provisional plans; fusion; the plan writer with validation. Measurement and measurable indicators |
| Oct 22 | `agent`: chat turns, tools, confirmation cards, memory, the dose check on every sentence. Garden map endpoints, neighbours and spacing. Second VLM worker live |
| Oct 23 | Scheduler: forecast alerts with dedupe, plan triggers, push. Deletion runner and the account cascade. Kill the rig mid-scan: the fallback takes over within about 45 seconds |
| Oct 24 | Feature freeze, security review against [architecture 11](../architecture/README.md#11-security-privacy-and-safety), evaluation run |
| Oct 25 | Final deploy |

---

## 14. Changes this design made to other documents

Writing the types showed points the other documents left open. They were settled in the same pull request:

| Document | Change |
|---|---|
| [contracts](../architecture/contracts.md) v0.9 | The model adapters' cell size and box format; `bbox_2d` corner order; vLLM's `response_format`; the fallback's stripped schema keywords and box format; a second VLM worker with its own `VLM_SLOTS`; the same checkpoint on every VLM worker; `model_versions` keys; per-model perception caps; one SAM prompt per box; the plant check; `JobErrorCode` with `unknown_label_version`; `complete` results read by the job's kind; no mask URLs in worker payloads; where bare confidences are allowed; `Invite.created_by` optional; the default location precision; the soil-moisture derivation; `bioclip_label` and the classifier-label flow |
| [contracts](../architecture/contracts.md) v0.10 | Answers to open questions 12–24 (timestamps, unknown fields, malformed ids, body limits, timeouts, job error codes, lease hand-back, unknown kinds, the per-scan writer's lifecycle, the lease-token header, webhook failures); the geocoding error; `retry_after_s` values; stored photo sizes; the stream staying open while a step holds the claim; chat tool signatures; the fallback's image and token budgets; new open questions from 27 |
| [architecture](../architecture/README.md) v0.6, v0.7 | The RX 9070 XT; ONNX Runtime 1.31 on CUDA 13; the chosen models; the `integrations` crate; the layout with `docs/backend` and `docs/vision`; then the crate graph, user creation on the first request, and the chat tools |
| [database](../database/README.md) v1.4, v1.4.1, v1.4.2 and [schema](../../backend/migrations/schema.surql) | The `indicator_answer` table and its ranking; `scan.candidates`, `question.indicator`, `mask.rle`, `garden.soil_retry_at` and the account tombstone `deleted_user`; the embedding dimensions (384 text, 768 image); every `time::now()` bound as `$now`; the entry point's two step-claim statements made conditional on each other; parameter names SurrealDB protects (`auth`, `token`, `access`, `session`) and how binds named `before`, `after`, `this` and `parent` behave inside statements; point literals with parameters ([store](store.md)) |
| [storage](../storage/README.md) v1.2.3 | What the 120-second delete timeout is, and draining pending deletes at shutdown. `BehaviorVersion` is pinned to `v2026_01_12` and every delete runs on one `DeleteQueue` ([blob](blob.md)) |

The remaining questions each crate document found are in its "Open questions" section.

---

## 15. Open decisions

1. **The RX 9070 XT's VLM server:** vLLM on ROCm (proposed) or llama.cpp with Vulkan, after the AMD day-1 check ([architecture open decision 19](../architecture/README.md#15-open-decisions)).
2. **Where the control plane runs** ([architecture open decision 2](../architecture/README.md#15-open-decisions)): the binaries are the same either way.
3. **Moving the scratch workspace into `backend/`:** decided. The compiled skeleton behind these documents becomes the real workspace on day 1 (Oct 16), so the code starts from the reviewed design rather than being retyped.
4. **Weather and trigger thresholds:** one definition in [domain 7](domain.md#weather-thresholds), with defaults to confirm from the evaluation set ([architecture open decision 20](../architecture/README.md#15-open-decisions), contracts open question 27).
5. **Running more than one `api` instance** would need SurrealDB live queries or a message bus for the SSE hub and the per-scan writers ([architecture 12, operations](../architecture/README.md#operations)). One instance is the design for build week.

---

## 16. Sources

**Rust language**
- The Book: [10 generics, traits and lifetimes](https://doc.rust-lang.org/book/ch10-00-generics.html) · [16 concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html) · [17 async and await](https://doc.rust-lang.org/book/ch17-00-async-await.html) · [18.2 trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [20 advanced features](https://doc.rust-lang.org/book/ch20-00-advanced-features.html)
- [The Rust Reference](https://doc.rust-lang.org/reference/) · [Edition Guide, Rust 2024](https://doc.rust-lang.org/edition-guide/rust-2024/index.html) · [Async Book](https://rust-lang.github.io/async-book/) · [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) · [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) · [Cargo lints table](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)

**Crates and tools**
- [tokio](https://docs.rs/tokio/1.53.3/tokio/) and its [tutorial](https://tokio.rs/tokio/tutorial) · [tokio-util](https://docs.rs/tokio-util/0.7.20/tokio_util/) · [axum 0.8.9](https://docs.rs/axum/0.8.9/axum/) · [tower-http 0.7.1](https://docs.rs/tower-http/0.7.1/tower_http/) · [utoipa 6.0.0](https://docs.rs/utoipa/6.0.0/utoipa/) · [surrealdb 3.3.4](https://docs.rs/surrealdb/3.3.4/surrealdb/) · [aws-sdk-s3 1.152.0](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/) · [ort 2.0.0-rc.14](https://docs.rs/ort/2.0.0-rc.14/ort/) · [cargo-nextest](https://nexte.st/)

**Project documents**
- [System architecture](../architecture/README.md) · [data contracts](../architecture/contracts.md) · [database](../database/README.md) and [schema](../../backend/migrations/schema.surql) · [storage](../storage/README.md) · [frontend](../frontend/README.md) · [computer vision](../vision/README.md)
