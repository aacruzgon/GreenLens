# PDM Backend: `domain` crate

> **Status:** Draft v0.3.13, 2026-10-20. v0.3.13: `ClassifierTask::Route` carries `farm_records`, whether the garden has farm records, so the router can count a question about them as on topic (gap G10); no wire type changed. v0.3.12: `FarmPage::listed` carries every vendor id when an adapter listed the whole source (Drive's listing and resync), so the sync deletes the source's other records, and `FarmSourceError::TooLarge` stops a source past what its adapter follows with `too_large`. v0.3.11: `rules::limits::IDEMPOTENCY_RUNNING_STALE` (10 minutes) lets a retry take over a `running` idempotency key, and `FarmRecordStore::deleting_connectors_of_user` lists a user's `deleting` connectors in any garden for the account deletion; `FarmRecordStore::delete_expired_pending` takes a second cutoff, and `rules::limits::CONNECT_CLAIM_HOLD` (10 minutes) keeps a pending connector whose exchange may still be running. v0.3.10: `FarmRecordStore::deleting_connectors` lists the connectors marked `deleting` with their sealed tokens, for a garden (a cascade or a demotion) or for every garden not being deleted (resume after a restart), so `api` revokes the tokens after the marking transaction and then deletes the records (roadmap E3.6); `connectors_created_by` says which transactions mark them. v0.3.9: `SYNC_STATE_KEY` also holds the sources that had their turn in the round, and `SyncOutcome` sets `synced_at` only at a round's end with every source caught up and keeps `last_error` while a source's last turn failed. The `FarmSource` port gains what the sync needs (E4.2): `reread_window()` (Slack's 7 days, none for Drive), `FarmPageRequest::reread_from` to start the daily re-read, and `FarmPage::kept` for items still at the vendor that weren't sent again. v0.3.8: `FarmRecordStore` is implemented by the store (roadmap E1.6) and changes for the sync: `request_sync` returns `SyncRequest` (accepted, or too soon with the instant the next is allowed, for `retry_after_s`), and `delete_chunks_from` and `source_vendor_ids_since` cover a re-chunked document and the daily 7-day re-read; `NewMessage` carries `farm_citations`; `rules::limits` gains the connector limits (`MAX_CONNECTORS_PER_GARDEN`, `MAX_FARM_RECORDS_PER_GARDEN`, `SYNC_NOW_INTERVAL`, `FARM_RECORD_KEEP`, `PENDING_CONNECT_TTL`). v0.3.7: the farm connectors' ports (roadmap E1.2): `FarmSource` for the Slack and Drive adapters with `FarmSourceError`, and `FarmRecordStore` for the `connector` and `farm_record` tables, part of `Store`; `GardenResource::Connector`; the garden cascade's `FarmRecordsAndConnectors` step before `Garden`; a new `connectors` module of wire types. v0.3.6: `AnalysisRepo::finish_guided_check` takes `now`, like `complete_job`, so the lease check and the `score` claim follow the `Clock` port instead of the database clock. v0.3.5: the `BeliefStore` port records shared memories in Agent Memory, and `MemoryStore` gains the belief sweep's four methods (`unsynced_shared`, `mark_synced`, `belief_forgets`, `finish_belief_forget`). v0.3.4: `ToolSpec` gains `strict`, set by `agent` for the tools that propose changes. v0.3.3: section 1 says these types are the contracts' source of truth now that the code exists. v0.3.2: the schema link points to `backend/migrations/schema.surql`. v0.3.1: the section 1 note says the Rust skills follow this design and keeps only the timestamp difference from the brief; section 10 no longer says `rust-async` prefers a hand-written `BoxFuture`. v0.3: the cross-crate fixes add `PastWeather.longest_humid_run_days` and the past-week humid-stretch rules, store methods with default bodies (`JobRepo::live_lease`, `GardenRepo::soil_profile_failed` and `gardens_due_soil_profile`, the account tombstone's `DeletionRepo` methods), `AuthError::AccountDeleted` for a tombstoned account, `soil_retry_after` and `TOMBSTONE_KEEP`, and make `QuestionState::after_failed_photo(false)` supersede the question as contracts 5 says. v0.2: one definition of the weather thresholds (`rules::weather::WeatherThresholds`: frost at a 2 m minimum ≤ 0.5 °C, hard frost ≤ −2 °C, a humid stretch of two days with ≥ 10 h over 90%), `TextEmbedder::embed` takes an `EmbedMode`, `PhotoAnswerStart::Segment` boxes its job, corrected `BoxFormat`, `JobRepo::check_lease` and `DeletionRepo::delete_batch` comments, every addition the other crates made since v0.1 (section 12), and the open questions that contracts v0.9 and database v1.3.3–v1.4 resolved marked as such. v0.1: the shared crate every other crate builds on: typed ids and wire primitives, the error codes, the pure rules, the authorization proofs, the signed-URL typing and every boundary trait, all compiled on Rust 1.99.1.

Companion to the [backend architecture](README.md). The contract shapes themselves, section by section, are in [domain-types](domain-types.md). Every listing here is copied from the compiled source of `crates/domain`: `cargo check -p domain`, `cargo test -p domain` (112 unit tests, 5 compile-fail doc tests) and `cargo clippy -p domain --all-targets` pass, with and without the `mock` and `test-util` features.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [IDs and primitives](#4-ids-and-primitives)
5. [Signed-URL typing](#5-signed-url-typing)
6. [Errors](#6-errors)
7. [Rules](#7-rules)
8. [Authorization proofs](#8-authorization-proofs)
9. [Boundary traits](#9-boundary-traits)
10. [Concurrency and lifetimes](#10-concurrency-and-lifetimes)
11. [Testing](#11-testing)
12. [Open questions](#12-open-questions)
13. [Sources](#13-sources)

---

## 1. Purpose and boundaries

**What it owns.** Everything that more than one crate must agree on ([architecture 3, Rust crates](../architecture/README.md#rust-crates)):

- every shape that crosses a boundary, field for field with [contracts.md](../architecture/contracts.md) ([domain-types](domain-types.md)); these types are the contracts' source of truth, and `utoipa` and `schemars` generate the OpenAPI document and the seed and plan-writer JSON Schemas from them ([contracts 1, source of truth](../architecture/contracts.md#source-of-truth));
- typed ids (`Id<T>`, `Slug<K>`) and wire primitives (section 4);
- the type-level split between stored records (image ids) and wire records (signed URLs) (section 5);
- the error codes of [contracts 2](../architecture/contracts.md#2-errors) with their HTTP status as data, the rule failures every crate raises, and one error enum per kind of port (section 6);
- the pure rules: the scan state machine, upload rules, the role matrix, leases and runtimes, step claims, stage lists, stream closing, photo routing, limits, the weather thresholds (section 7);
- the authorization proofs `Membership<R>` and `ConversationOwner<R>` (section 8);
- every boundary trait: the store's repositories, the object store, the models, the providers, the auth provider, time, ids and URL signing (section 9).

**What it never does.** No I/O, no async runtime, no clock reads (time is passed in or read through `Clock`), no SurrealDB, AWS or HTTP types. It depends on no crate of the workspace ([backend crate graph](README.md)); `store` maps `Id<T>` to `surrealdb::RecordId` and `blob` maps `ByteStream` to the AWS one.

**Who calls it.** Every crate: `api` and `worker` wire implementations of the ports; `store`, `blob`, `integrations`, `vision` and `knowledge` implement them; `pipeline` and `agent` orchestrate through them.

**The project skills and the brief.** The Rust skills in `.claude/skills/` follow this document: the ports use `#[async_trait]` (`rust-async`, `traits.md`; section 10 says why), IDs are one generic `Id<T: Table>` with an alias per table and timestamps a `Timestamp` newtype (`rust-contract-types`, `primitives.md`), and the workspace has the `integrations` crate (`rust-cargo-lints`). Where a skill and this document disagree, this document wins. One deliberate difference from the shared backend brief remains: the brief gives timestamps as `chrono::DateTime<Utc>`, and here a `Timestamp` newtype writes whole seconds, because chrono writes a fraction for every clock reading and contracts 1's example has none.

---

## 2. Dependencies

@@file ../Cargo.toml@@

| Crate | Version | Why |
|---|---|---|
| `serde`, `serde_json` | 1.0.229, 1.0.154 | Every wire type; `serde_json::Value` for job results decoded by kind, tool calls and stored responses |
| `chrono` | 0.4.45 (`std`, `clock`, `serde`) | `Timestamp` and `LocalDate` wrap `DateTime<Utc>` and `NaiveDate` |
| `ulid` | 3.0.0 (`serde`) | Record keys. ulid 3 spells the constructor `Ulid::generate()` |
| `uuid` | 1.28.0 (`v4`, `serde`) | `IdempotencyKey` |
| `utoipa` | 6.0.0 (`axum_extras`, `chrono`) | `ToSchema` on every wire type, `IntoParams` on query structs |
| `schemars` | 1.2.3 (`chrono04`) | JSON Schema for the seed types and the plan writer's output |
| `thiserror` | 2.0.21 | `DomainError`, the port error enums, parse errors |
| `async-trait` | 0.1.92 | Async methods on dyn-compatible ports (section 10) |
| `futures` | 0.3.34 | `BoxStream` in `ChatModel` and `BlobStore` |
| `bytes` | 1.12.1 | Image and object bodies |
| `mockall` | 0.15.0, optional (`mock`) | `Mock<Trait>` for every port but `ChatModel`, for other crates' tests |

Versions and features follow the project's `rust/versions.md`.

---

## 3. Module layout

```
crates/domain/src/
├─ lib.rs              crate docs, module list
├─ prelude.rs          the items most crates use
├─ id.rs               Id<T>, table markers, Slug<K>, compound and opaque ids        (contracts 1)
├─ wire.rs             Timestamp, LocalDate, Confidence, Rated, Certainty, BBox,
│                      Region<U>, Page, Run, EventSeq, V1, BoolLit, Role             (contracts 1)
├─ url.rs              UrlState, Stored, Signed, SignedUrl, ImagePath, Signer, Sign  (contracts 1, 4)
├─ error.rs            ErrorCode, ErrorBody, ErrorKind, details, DomainError         (contracts 2)
├─ scan.rs             scans, images, plants, questions, guided checks, uploads       (contracts 3)
├─ events.rs           Stage, ProblemKind, ScanEvent<U>, ScanEventBody<U>            (contracts 4)
├─ jobs.rs             heartbeat, claim, Job, payloads, worker events, results      (contracts 5)
├─ vlm.rs              raw VLM output, box formats, VlmFindings, GuidedTexture      (contracts 6)
├─ records.rs          pipeline records                                               (contracts 7)
│  └─ records/         observation, measurement, weather, diagnosis, plan, task
├─ chat.rs             conversations, messages, chat events, pending actions,
│                      memory, tool return shapes                                    (contracts 8)
├─ connectors.rs       farm connectors, the connect requests, FarmCitation          (connectors 4–7)
├─ garden.rs           gardens, plants, tasks and account                            (contracts 9)
│  └─ garden/          gardens, members, plants, species, map, tasks, account,
│                      push, webhook, ops
├─ knowledge.rs        the seed format and SoilProfile                               (contracts 10)
├─ rules.rs            pure rules
│  └─ rules/           scan_state, uploads, questions, roles, jobs, steps, stages,
│                      stream, limits, routing, weather
├─ authz.rs            MinRole markers, Membership<R>, ConversationOwner<R>, checks
├─ ports.rs            boundary traits
│  ├─ ports/           errors, system, auth, blob, models, providers, knowledge,
│  │                   beliefs, farm
│  └─ ports/store/     access, identity, gardens, plants, scans, events, analysis,
│                      steps, plans, chat, knowledge, jobs, idempotency, weather,
│                      deletion, map, farm
├─ testing.rs          FixedClock, SequentialIds, FakeSigner, FixedPhotos,
│                      ScriptedChat (feature test-util)
└─ schema_util.rs      hand-written string schemas for utoipa and schemars
```

---

## 4. IDs and primitives

### `Id<T>`

A record key is the ULID of a SurrealDB record, sent bare ([contracts 1, wire format](../architecture/contracts.md#wire-format)). The table is a type parameter, so a `ScanId` can't be passed where a `PlantId` is expected, and `T::NAME` gives `store` the table for `RecordId::new(T::NAME, id.to_string())` ([database 2](../database/README.md#2-conventions)).

@@items id.rs Table Id@@

The trait impls are written by hand (`Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Ord`, `Debug` as `scan:01JAX…`, `Display` as the bare ULID, `FromStr`, serde and both schema traits): `#[derive]` would add `T: Clone` and similar bounds that uninhabited markers can't meet. The markers come from one macro:

@@items id.rs mod:table@@

### `Slug<K>`, compound and opaque ids

`Slug<K>` holds an `Arc<str>` so cloning a slug in candidate lists, scores and prompts is a reference-count bump. Its format is `[a-z][a-z0-9_]*`, at most 64 characters, and `"healthy"` is reserved for `ProblemOrHealthy` ([contracts 10](../architecture/contracts.md#10-knowledge-seed-format)). `IndicatorId` (`<problem_id>.<slug>`), `RegionId` (`<image_id>.<n>`) and `ChunkId` (`<source_id>.<n>`) parse into their parts. `WorkerId`, `LeaseToken` (a ULID, generated by the claim's `rand::ulid()`), `AuthId`, `InviteCode` (26 base32 characters), `PushToken`, `Cursor` and `StepId` are checked opaque strings; `IdempotencyKey` wraps a `uuid::Uuid`. Every one serialises as a bare string and has a string schema. Listings: [domain-types 1](domain-types.md#1-conventions).

@@items id.rs IdParseError@@

### Wire primitives

| Type | Wire | Rule |
|---|---|---|
| `Timestamp` | `"2026-10-18T14:03:22Z"` | Whole seconds on the wire, full precision in memory; no `now()`, only `Clock::now()` |
| `LocalDate` | `"2026-10-20"` | A date in the garden's time zone |
| `Confidence` | `0.71` | Finite, 0–1, checked when decoded |
| `ConfidenceBand` | `"medium"` | high ≥ 0.8, medium ≥ 0.5 ([contracts 1](../architecture/contracts.md#wire-format)) |
| `Rated` | `"confidence": 0.71, "confidence_band": "medium"` | Flattened; the band is written from the number and recomputed when read |
| `Certainty` | `"high"` | low 0.4, medium 0.65, high 0.85 ([contracts 6](../architecture/contracts.md#after-decoding)) |
| `BBox` | `[x, y, w, h]` | Each 0–1, `x + w ≤ 1`, `y + h ≤ 1` |
| `Run`, `EventSeq` | numbers | Newtypes, so they can't be mixed up |
| `V1` | `1` | Any other `v` fails to decode with `unsupported_version` |
| `BoolLit<B>` | `true` / `false` | A literal: decodes only from its own value |
| `Role` | `"editor"` | `Ord` in rank order |

The band rule and the certainty mapping are code, not prose:

@@impl wire.rs impl ConfidenceBand@@

@@impl wire.rs impl Certainty@@

---

## 5. Signed-URL typing

Stored payloads keep image and mask ids and never URLs, and every URL the app receives is signed as it is sent, live or replayed, so a replay hours later carries fresh 15-minute URLs ([contracts 4, transport](../architecture/contracts.md#transport); [database 4, scan events](../database/README.md#scan-events-a-gap-free-seq-); [storage 3](../storage/README.md#3-how-images-are-served)). The type system enforces both halves.

### The design: one URL-state parameter

Every type that can carry an image or mask URL takes `U: UrlState`, defaulting to `Signed`. `UrlState` is sealed and has two implementations; each names what a URL slot and a mask slot hold.

@@items url.rs UrlState WireState Stored Signed UrlSlot MaskSlot NoUrl url_absent StoredMask SignedMask@@

| | `X<Stored>` | `X<Signed>` (written `X`) |
|---|---|---|
| URL slots (`url`, `thumbnail_url`, `reference_image.url`, `attachments[].url`) | `NoUrl`: never serialised; decoding a URL into it fails | `SignedUrl` |
| Region mask | `mask_id` | `mask_url` |
| `utoipa::ToSchema` | not implemented | implemented |
| Who uses it | `store` (records, event payloads), the agent's tools ("`ScanDetail` without image URLs", [contracts 8](../architecture/contracts.md#tool-return-shapes-and-scope)) | `api` responses and SSE |

**Why one parameter rather than paired types.** The URL-bearing leaves (`Region`, `ScanImage`, `SpeciesCandidate`, `LatestScan`, `Attachment`) sit inside a deep tree: `Observation` → `GuidedCheck` → `Scan` → `ScanDetail`, and `Diagnosis.soil`, `Plant`, `Message`, `ConversationDetail`, `TimelineEntry`, five event variants. Paired types (`StoredScan` and `Scan`, …) would duplicate about twenty structs and the 17-variant event enum, and every change to a contract shape would have to be made twice. With a parameter each shape is written once; only the leaf slots differ, through `U::Url` and `U::Mask`. The cost is a `#[schema(value_type = …)]` override on each generic field and `#[serde(bound = "")]` on each generic type, both mechanical. utoipa 6 rejects `bound` on enums, so `ScanEventBody` relies on the default `U: ToSchema`, which only `Signed` satisfies (an empty schema that is never referenced).

**How it is enforced.**

1. **An unsigned record can't be sent.** Only `X<Signed>` implements `utoipa::ToSchema` (`#[schema(bound = "U: WireState")]`). `api`'s JSON response wrapper and every `#[utoipa::path]` response need `ToSchema`, so returning `Scan<Stored>` doesn't compile (proved by a compile-fail doc test, E0277).
2. **A signed record can't be stored.** The store's repository methods take `X<Stored>`, and nothing converts a signed value back (E0308 in a compile-fail doc test). Decoding JSON that contains a URL into a stored type fails on `NoUrl`.
3. **Signing is the only way across.** `Sign::sign` builds the signed form; `SignedUrl` has no public constructor and is built by `Signer::url`.

### Signing

@@items url.rs PhotoHash ImagePath SigningInput SIGNED_URL_TTL Signer@@

@@impl url.rs impl<'a> Signer<'a>@@

@@items url.rs SignatureError verify_signed Sign@@

`Scope` carries what a value doesn't know about itself but its URLs need: a region's mask route needs its scan, a message's attachments need their conversation. A typical implementation:

@@impl records/observation.rs Sign for Observation<Stored>@@

`Vec<T>`, `Option<T>` and `Page<T>` sign element-wise, so `ScanDetail` signs in one call: `detail.sign((), &Signer::new(&*state.signer, &*state.photos, clock.now()))`.

**The HMAC.** `sig` is HMAC-SHA256 under the server secret over the canonical `"GET\n{path}\n{exp}"` ([storage 3](../storage/README.md#3-how-images-are-served) names the three parts; the byte layout is fixed here). `verify_signed` refuses an `exp` in the past (`Expired`) or more than 15 minutes ahead (`TooFarAhead`) before asking the port to compare in constant time; any refusal is `403 invalid_signature`.

---

## 6. Errors

### Codes as data

Each code of [contracts 2](../architecture/contracts.md#2-errors) knows its HTTP status, its default `retryable`, and whether a response with it is stored under an idempotency key ([contracts 1, idempotency](../architecture/contracts.md#idempotency)). The status is a `u16`; only `api` turns it into an axum response.

@@impl error.rs impl ErrorCode@@

| Code | Status | `retryable` | Stored for idempotency |
|---|---|---|---|
| `invalid_request` | 400 | false | yes |
| `unauthenticated` | 401 | false | no |
| `forbidden`, `invalid_signature` | 403 | false | yes |
| `not_found` | 404 | false | yes |
| `invalid_state`, `lease_lost` | 409 | false | no |
| `request_in_progress` | 409 | true | no |
| `upload_too_large` | 413 | false | yes |
| `unsupported_media_type` | 415 | false | yes |
| `image_quality` | 422 | true (as in contracts 2's example: a retake succeeds) | yes |
| `missing_required_shot`, `too_many_images`, `idempotency_conflict` | 422 | false | yes |
| `rate_limited` | 429 | true | no |
| `budget_exceeded` | 429 | false: it lasts until `resets_at` | no |
| `internal` | 500 | false | no |
| `processing_unavailable`, `upstream_unavailable` | 503 | true | no |

The response body, with `details` typed per code, is in [domain-types 2](domain-types.md#2-errors).

### `DomainError`

A rule refused the call. Every crate raises it, and `kind()` maps it to the contract code and details in one place.

@@items error.rs DomainError@@

@@impl error.rs impl DomainError@@

### Port errors

One enum per kind of port instead of an associated `Error` per trait, because `Arc<dyn Trait>` would otherwise have to name the error type and every caller of a port of one kind handles the same cases. Implementations keep the library error as the `source`, so the log line carries the chain; as the project's `rust-errors` skill asks, `Display` is the variant's own message, never its source's.

@@file ports/errors.rs@@

| Error | Becomes |
|---|---|
| `StoreError::NotFound` | `404 not_found` (also what a non-member gets) |
| `StoreError::Rejected(e)` | `e.kind()` |
| `StoreError::Conflict(StalePlan { current })` | `409 invalid_state` with `current_plan_id` (a chat revision); an automatic re-plan retries instead |
| `StoreError::Conflict(StaleMap { current_version })` | `409 invalid_state` with `current_map_version` |
| `StoreError::Conflict(LeaseLost { job_id })` | `409 lease_lost` |
| `StoreError::Conflict(ClaimLost \| QuestionStepLost)` | never an HTTP error: the step stops ([contracts 3](../architecture/contracts.md#scan-lifecycle)) |
| `StoreError::Conflict(AlreadyExists)` | handled by the caller (idempotency, membership, alert dedupe) |
| `StoreError::Unavailable`, `Conflict(Retries)` | `503 upstream_unavailable` with `upstream: "database"` |
| `StoreError::Query` | `500 internal` |
| `BlobError::Unavailable` | `503 upstream_unavailable` with `upstream: "storage"` |
| `ProviderError::Unavailable` / `RateLimited` | `503 upstream_unavailable` with `upstream` = `provider()` |
| `ProviderError::NotFound` (geocoder) | `400 invalid_request` on `location` |
| `ModelError` in chat | `503 upstream_unavailable`, or the `error` event once the stream started |
| `ModelError` in a worker | `CompleteJob.error` with `retryable()` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)) |
| `AuthError::InvalidToken` / `InvalidWebhook` | `401 unauthenticated` |
| `AuthError::Unavailable` | `503 upstream_unavailable` with `upstream: "auth"` |

---

## 7. Rules

Pure functions over the contracts' rules, fully implemented and unit-tested. Time is an argument; nothing reads a clock.

### Scan state machine

[Contracts 3, scan lifecycle](../architecture/contracts.md#scan-lifecycle) and [architecture 8.1](../architecture/README.md#81-scan--diagnosis--plan).

@@items rules/scan_state.rs ScanTrigger TransitionError@@

@@impl rules/scan_state.rs impl ScanTrigger@@

@@impl rules/scan_state.rs impl ScanStatus@@

| Trigger | From | To | Caller |
|---|---|---|---|
| `Submit` | `pending` | `queued` | `POST /scans/{id}/submit` |
| `Resubmit { failure }` | `failed` (retryable failure only) | `queued` | the same, after a `processing_error` or `upstream_unavailable` |
| `Claimed` | `queued` | `analysing` | the claim of the run's first job |
| `QuestionAsked` | `analysing`, `awaiting_answer` | `awaiting_answer` | the `score` step |
| `Diagnosed` | `analysing`, `awaiting_answer` | `diagnosed` | the `score` step, or the scheduler continuing without an answer |
| `Planned` | `diagnosed` | `planned` | the `plan` step |
| `SpeciesChanged` | `analysing`, `awaiting_answer`, `diagnosed`, `planned` | `analysing` (new run) | `POST /plants/{id}/species` |
| `Fail` | `queued`, `analysing`, `awaiting_answer`, `diagnosed` | `failed` | job attempts exhausted, `no_plant_found`, `photos_unusable`, an upstream down |
| `LateWork` | `diagnosed`, `planned` | unchanged | a late answer or photo, a guided check |

A refused trigger is `409 invalid_state` with `status` and `allowed`, and `hint: "new_scan"` for a resubmit of a non-retryable failure. `planned` can't fail: nothing of a run is in flight once it is planned, and late work never fails a scan (see [section 12](#12-open-questions) for the wording of architecture 8.1).

@@items rules/scan_state.rs SubmitDecision decide_submit SPECIES_CHANGE_WINDOW SpeciesFollow species_follow check_deletable check_feedback_allowed@@

### Uploads

[Contracts 3, `POST /scans/{id}/images`](../architecture/contracts.md#post-scansidimages).

| Upload | Allowed when | Route | Kind limit |
|---|---|---|---|
| Plant or soil kind, no `question_id` | `pending` | `RunShot` | `whole_plant` 1, `leaf_closeup` 2, `leaf_underside` 1, `soil_surface` 1, `flower_fruit` 1 |
| `squeeze_test`, `jar_test` | any status but `failed` | `GuidedCheck` | 1 each before submit, none after |
| Plant or soil kind with `question_id` | the question is `open` | `QuestionPhoto` | doesn't count |

@@items rules/uploads.rs IMAGE_MAX_BYTES DEPTH_MAX_BYTES ACCEPTED_MEDIA_TYPES REQUIRED_SHOTS UploadRoute route_upload check_kind_limit check_required_shots@@

@@impl rules/uploads.rs impl ImageKind@@

@@items rules/uploads.rs check_depth check_depth_coverage stored_long_side@@

`check_depth` drops depth with `not_for_this_kind` for guided kinds, `too_large` over 4 MB, `invalid_meta` for a mirrored EXIF orientation (2, 4, 5, 7), and `aspect_mismatch` beyond 1%; `check_depth_coverage` gives `too_sparse` under 20% ([contracts 3, depth](../architecture/contracts.md#post-scansidimages)).

### Questions

@@items rules/questions.rs MAX_QUESTIONS_PER_RUN CONTINUE_AFTER QuestionState QuestionStep may_ask@@

### Roles

[Contracts 1, roles and access](../architecture/contracts.md#roles-and-access).

@@items rules/roles.rs Action@@

@@impl rules/roles.rs impl Action@@

@@impl rules/roles.rs impl Role@@

| Action | Lowest role |
|---|---|
| Read gardens, plants, scans, plans, tasks, timeline, measurements, the map; chat with the read tools and `remember` | viewer |
| Scan feedback | reviewer |
| Scan, upload, delete a pending scan, answer, confirm species, guided checks; map the garden; write tools and cards; manage tasks; create and edit plants; share memories and add notes | editor |
| Rename the garden, invites, members and roles, delete plants or the garden, delete shared memories | owner |

`leaves_no_owner(owners, member_role, new_role)` refuses demoting or removing the last owner.

### Jobs

[Contracts 5, leases, runs and attempts](../architecture/contracts.md#leases-runs-and-attempts) and [architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback).

@@items rules/jobs.rs LEASE HEARTBEAT_INTERVAL WORKER_STALE_AFTER FALLBACK_DELAY MAX_ATTEMPTS FREE_REQUEUES@@

@@impl rules/jobs.rs impl JobKind@@

@@impl rules/jobs.rs impl Capability@@

@@items rules/jobs.rs claimable_kinds ClaimPolicy claim_policy extended_lease attempts_after_requeue exhausted FinalFailure final_failure@@

| Kind | `max_runtime` | On the fallback | Claimed by |
|---|---|---|---|
| `perception` | 2 min | 4 min | `perception`, `fallback` |
| `vlm` | 5 min | 10 min | `vlm`, `fallback` |
| `verify`, `guided` | 3 min | 6 min | `vlm`, `fallback` |

| A job that fails for good | Effect |
|---|---|
| A run's job | The scan fails with `processing_error`; its question is superseded |
| A late `verify` job | `question_resolved` with `dropped`; the scan stays |
| A `guided` job | `guided_check_done` with a failed check; the scan stays |
| A question photo's segmentation job | The photo goes on to `verify` unmeasured |

### Steps between jobs

[Contracts 3, steps between jobs, and recovery](../architecture/contracts.md#scan-lifecycle); [database 4, steps between jobs](../database/README.md#steps-between-jobs-).

@@items rules/steps.rs PipelineStep Handover handover ScoreOutcome@@

| Step | Stale after | Hands over to |
|---|---|---|
| `retrieve` | 2 min | released when the `vlm` job is queued |
| `score` | 2 min | released (a question, or nothing to do), or `plan` |
| `plan` | 5 min | released |
| any, with `rescore_pending` set | | `score` |

### Stages, stream and routing

@@items rules/stages.rs EarlierRun run_stages perception_stages@@

@@items rules/stream.rs StreamState@@

@@impl rules/stream.rs impl StreamState@@

Photo routing to the VLM ([contracts 5, payloads](../architecture/contracts.md#payloads)) works on borrowed slices and is generic over what it routes:

@@items rules/routing.rs RoutableImage FINDINGS_ORDER findings_images indicator_images@@

### Limits

@@items rules/limits.rs DAILY_SCAN_LIMIT PENDING_ACTION_TTL INVITE_TTL IDEMPOTENCY_TTL IDEMPOTENCY_RUNNING_STALE INVITE_ATTEMPTS_PER_MINUTE MESSAGE_MAX_CHARS MESSAGE_MAX_IMAGES WEATHER_CACHE_KEEP FINISHED_JOB_KEEP DELETE_BATCH WEATHER_CELLS_PER_REQUEST MAX_PLAN_NEIGHBOURS NEIGHBOUR_RADIUS_M CROWDING_FACTOR MAX_CANDIDATES MAX_VLM_IMAGES SSE_KEEPALIVE TOMBSTONE_KEEP SOIL_RETRY_OUTAGE SOIL_RETRY_NO_DATA MAX_CONNECTORS_PER_GARDEN MAX_FARM_RECORDS_PER_GARDEN SYNC_NOW_INTERVAL FARM_RECORD_KEEP PENDING_CONNECT_TTL soil_retry_after check_daily_scans next_utc_midnight check_message@@

Other rules live beside their types: `GardenMapInput::validate` and the map geometry (`polygon_area`, `is_simple`, `neighbours`, `crowded`) in `garden::map`, `Repeat::next_due` and `UpdateTaskRequest::check` in `garden::tasks`, `BoxFormat::to_bbox` in `vlm`, `MeasureQuantity::measurable_from` in `records::measurement`, `AlertKey`'s format in `records::task`.

### Weather thresholds

**Defaults:** frost at a 2 m minimum ≤ 0.5 °C, hard frost ≤ −2 °C, heat ≥ 30 °C, a humid hour over 90% RH, a humid day ≥ 10 humid hours, a humid stretch 2 humid days in a row (forecast or in the past 7 days), heavy rain ≥ 20 mm a day, after rain ≥ 5 mm in 24 h, dry soil below 0.15 m³/m³ at 0–9 cm with ET₀ ≥ 3 mm in 24 h. This section is the one owner of these numbers ([contracts open question 27](../architecture/contracts.md#12-open-questions), [architecture open decision 20](../architecture/README.md#15-open-decisions)).

Frost, heat, a humid day, a humid stretch, heavy rain, after rain and dry soil have one definition, read by four crates: `integrations` sets each forecast day's `frost_risk` and counts `hours_rh_over_90` with it, `api`'s scheduler fires forecast alerts and plan triggers ([architecture 8.3](../architecture/README.md#83-forecast-alerts)), `pipeline`'s fusion rates `frost_damage` and the humid stretch ([architecture 6.1](../architecture/README.md#61-stages)), and `knowledge`'s weather fit relies on the humid hour. Before v0.2 there were three definitions: `frost_risk` at a minimum ≤ 0 °C in `integrations`, frost at ≤ 0.5 °C and a humid day of ≥ 10 hours over 90% in the scheduler, and a humid day of 90% mean humidity or 12 hours (or 24 hours in the past week) in fusion. They now agree:

| Rule | Default | Why |
|---|---|---|
| Frost (`frost_risk`) | 2 m minimum ≤ 0.5 °C | Air at 2 m runs warmer than the ground on a clear night; ground frost forms with the air at 1–2 °C |
| Hard frost (`frost_damage` `high`) | 2 m minimum ≤ −2 °C | Fusion's existing level, now configurable beside frost |
| Heat | maximum ≥ 30 °C | The scheduler's value |
| Humid hour | RH > 90% | Fixed by the contracts' field name `hours_rh_over_90`, so a constant, not configuration |
| Humid day | ≥ 10 humid hours | The scheduler's value |
| Humid stretch | 2 humid days in a row, forecast or in the past 7 days | One rule for the trigger (forecast only) and fusion's crowding (forecast or past week, contracts 9). The past week is read from `past_7_days.longest_humid_run_days`, the longest run of humid days among the 7 before today, which Open-Meteo's adapter computes from the hourly history with these thresholds (`longest_humid_run`) |
| Heavy rain | ≥ 20 mm in a day | The scheduler's value |
| After rain | ≥ 5 mm in the last 24 h | The scheduler's value |
| Dry soil | 0–9 cm < 0.15 m³/m³ with ET₀ ≥ 3 mm in 24 h | The scheduler's value |

`api` holds the one instance (`Config::weather`) and passes it to `OpenMeteoConfig`, `PipelineConfig` and the scheduler, so the `frost_risk` cached with a cell and the triggers read from it can't disagree. Horizons stay with their readers: the scheduler looks 3 days ahead, fusion 7 for the humid stretch and 3 for frost, weather fit 3. The contracts fix the vocabulary, not the numbers (open question 17).

@@file rules/weather.rs@@

The defaults are a decision, so a test pins every number; changing one is a design change that shows up in review:

@@test rules/weather.rs defaults_are_pinned@@

---

## 8. Authorization proofs

[Contracts 1, roles and access](../architecture/contracts.md#roles-and-access); [architecture 4, authorization model](../architecture/README.md#authorization-model).

@@file authz.rs@@

**How the guarantee holds across crates.** `Membership::new` is private to `domain`. The only functions that call it are `check_membership`, `check_membership_of`, `check_access` and `check_conversation`, which read the stored role through the `AccessRepo` port and refuse with `404` (not a member) or `403` (role too low). A handler in `api` therefore gets a proof only by running the check, and the signature of the repository method it calls decides the minimum role at compile time. `at_least` is the free downgrade (an owner's proof works where an editor's is asked for), and `require` the run-time upgrade for calls whose role depends on the request, such as removing a member (an owner, or the member themselves). `check_membership_of` takes a bare `UserId` instead of a `CurrentUser`, for the account-deletion cascade started by a verified `user.deleted` webhook, which has no session; it reads the role from the store all the same.

**Why not a constructor the store calls.** `store` depends on `domain`, so `domain` can't name `store` to give it a private constructor, and a "sealed" `pub` constructor is still callable by every crate. A witness type the store produces would have to be defined in `domain` with a public constructor too. Putting the check itself in `domain`, behind a port, means every proof comes from one function. What remains forgeable is an `AccessRepo` implementation that lies: a visible trait implementation that only `store` and test fakes have, not an accident in a handler.

**Trade-offs.** The check is one read (`member_of:[user, garden]`, [database 4, membership](../database/README.md#membership-)) before the repository call, not inside its transaction, so a member removed between the two can complete one in-flight request. Repository methods still scope every query to `membership.garden()`, so a proof for one garden never reaches another garden's records.

---

## 9. Boundary traits

[Architecture 2, design principles](../architecture/README.md#design-principles): a trait at every external boundary. Every trait is `Send + Sync` and dyn compatible, so `api` and `worker` hold them as `Arc<dyn Trait>`; async methods use `#[async_trait]` (section 10). With the `mock` feature each has a `mockall` mock (`MockStore` doesn't exist: mock the area traits).

@@file ports.rs@@

### Time, ids, URL signing and species photos

@@file ports/system.rs@@

### Auth provider

@@file ports/auth.rs@@

### Object storage

[Storage 2](../storage/README.md#2-bucket-keys-and-access), [storage 4](../storage/README.md#4-writes-and-deletes), [storage 5](../storage/README.md#5-rust-client).

@@file ports/blob.rs@@

### Models

`VisionModel` has one method per job kind rather than one generic `run(&JobSpec)`: each kind has its own payload and result type, so the compiler checks that a `vlm` payload produces a `VlmResult`, and the worker matches `JobSpec` once. An adapter returns `ModelError::Unsupported` for the kinds its capability doesn't claim; the fallback adapter answers all four.

`TextEmbedder::embed` takes an `EmbedMode` on every call. arctic-embed-xs is an asymmetric retrieval model: a query gets an instruction prefix and a document none ([contracts 10, checks and generated files](../architecture/contracts.md#checks-and-generated-files); [architecture 10](../architecture/README.md#10-models-and-gpu-placement)). Before v0.2 the port had no mode, so `vision` offered two `TextEmbedder` instances over one set of sessions (`ArcticEmbedder::queries()` and `.documents()`) and `api` had to wire the right one into each consumer (the agent held two). A wrong wiring still returns vectors, only worse matches, so nothing would have caught it. With the mode in the call, one `Arc<dyn TextEmbedder>` serves `api` (garden notes: `Document`), the pipeline (memory summaries: `Document`) and the agent (the user's message and search queries: `Query`; memories it saves: `Document`), and the choice sits at the line that knows what the text is. A model without a prefix ignores the mode. `EmbedMode` is `Copy`, so the adapter moves it into its `spawn_blocking` closure with the owned texts.

@@file ports/models.rs@@

### Providers

@@file ports/providers.rs@@

### Knowledge base and memory

@@file ports/knowledge.rs@@

### Beliefs (Agent Memory)

Shared memories are recorded in `SurrealDB` Agent Memory as triples, so a newer memory on the same key supersedes the older one and the history is kept ([agent-memory](../database/agent-memory.md); [architecture 7, memory](../architecture/README.md#memory)). The `memory` table stays the system of record. Requests only write rows: a shared memory is created unsynced, and a delete or a cascade queues its subject in `belief_forget` in the same transaction (the `MemoryStore` methods under "the belief sweep" above). `api`'s `beliefs` task, `agent::beliefs::sync_pending`, is the only caller of this port's writes: it erases the queued subjects first, then records what is unsynced. `integrations::agent_memory::AgentMemory` implements the port; `NoBeliefs` stands in when Agent Memory isn't configured, so nothing else changes.

@@file ports/beliefs.rs@@

### Farm connectors' vendors

The Slack and Google Drive adapters behind the farm connectors ([connectors 5–6](../connectors/README.md#6-sync)). `integrations` implements `FarmSource` once per vendor, each client built with a base URL so tests point it at Era's simulators; `api` picks one by the connector's `kind` and `endpoint` and hands it a token it unsealed. The adapter's cursor is opaque to the sync, which keeps it per source inside its own; the sync starts the daily re-read of a vendor with a `reread_window` and deletes what that pass didn't see (`FarmPage::kept` counts as seen). After a page that listed the whole source (`FarmPage::listed`, Drive's listing), it deletes the source's records whose vendor id isn't listed. An adapter that can't follow a source past its own limit (Drive's 500 files or folders) returns `FarmSourceError::TooLarge`, stored as `too_large`. Chunking, embedding and the record limit happen in the sync, outside the adapter. The OAuth exchanges are not part of the port: they run once per connect in `api`'s routes.

@@file ports/farm.rs@@

### The store

The store's area traits, `Store` as the supertrait of all of them, and the claim and lease tokens the write patterns of [database 4](../database/README.md#4-write-patterns) need.

@@file ports/store.rs@@

| Write pattern ([database 4](../database/README.md#4-write-patterns)) | Method |
|---|---|
| Gap-free event `seq` | `EventRepo::append_event` |
| Plan compare-and-set | `PlanRepo::write_plan`, `PlanRepo::write_revision` (`Conflict::StalePlan`) |
| Job claim with `slots` | `JobRepo::claim_job` with `rules::jobs::ClaimPolicy` |
| Lease extension, requeue | `JobRepo::heartbeat`, `JobRepo::requeue_stale` |
| Idempotency keys | `IdempotencyRepo::begin`, `finish`, `release` |
| Limits and counters | `ScanRepo::create_scan` (scans), `UsageRepo::add_chat_usage` (tokens) |
| Membership insert | `GardenRepo::accept_invite`, `GardenRepo::create_garden` |
| Alert dedupe | `PlanRepo::create_alert_task` (`Ok(None)` on a duplicate key) |
| Step claim, commit, sweep | `StepRepo::claim_step`, `commit_step`, `stale_steps`, `reclaim_step`; question steps in `AnalysisRepo` |
| Garden map versions | `MapRepo::put_map` (`Conflict::StaleMap`, `Rejected(UnknownPlants)`) |
| Deletion | `DeletionRepo::start_*_deletion`, `delete_batch` per `CascadeStep`, `unfinished_deletions`; accounts: `start_account_deletion` (compare-and-set on the tombstone), `unfinished_account_deletions`, `finish_account_deletion` |
| Cleanup (no TTL) | `IdempotencyRepo::delete_expired`, `WeatherCacheRepo::delete_before`, `ChatRepo::expire_pending_actions`, `JobRepo::delete_finished_jobs`, `DeletionRepo::delete_finished_tombstones` |

#### Access lookups

@@file ports/store/access.rs@@

#### Identity

@@file ports/store/identity.rs@@

#### Gardens, members and invites

@@file ports/store/gardens.rs@@

#### Plants

@@file ports/store/plants.rs@@

#### Scans, images and masks

@@file ports/store/scans.rs@@

#### Events

@@file ports/store/events.rs@@

#### Observations, questions and guided checks

@@file ports/store/analysis.rs@@

#### Steps between jobs

@@file ports/store/steps.rs@@

#### Plans, tasks and trigger rules

@@file ports/store/plans.rs@@

#### Chat

@@file ports/store/chat.rs@@

#### Knowledge reads

@@file ports/store/knowledge.rs@@

#### Jobs and workers

@@file ports/store/jobs.rs@@

#### Idempotency and usage

@@file ports/store/idempotency.rs@@

#### Weather cache

@@file ports/store/weather.rs@@

#### Garden map

@@file ports/store/map.rs@@

#### Farm connectors and records

The `connector` and `farm_record` tables ([connectors 4](../connectors/README.md#4-data)), implemented by the store in roadmap task E1.6 ([store 16](store.md#16-chat-memory-and-knowledge)). No route reaches them before E3.

@@file ports/store/farm.rs@@

#### Deletion cascades

@@file ports/store/deletion.rs@@

---

## 10. Concurrency and lifetimes

`domain` runs nothing itself; this section explains why each generic parameter, marker, lifetime and bound is there, so the crates that do run things can rely on them.

### Dyn dispatch and async methods

- **Native `async fn` in a trait is not dyn compatible on Rust 1.99.1.** A probe crate holding `Arc<dyn Clock>` for `trait Clock { async fn now(&self) -> u64; }` fails with `E0038` ("method `now` is `async`"), and the `-> impl Future + Send` form fails the same way ("references an `impl Trait` type in its return type"). The Rust Reference lists both: a dispatchable method must "not have an opaque return type; that is, not be an `async fn` … not have a return position `impl Trait` type" ([Reference, dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)). [The Book, chapter 18.2](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) explains trait objects and why dispatch needs a vtable.
- **So async methods use `#[async_trait]`**, which rewrites each into `fn m<'life0, …, 'async_trait>(&'life0 self, …) -> Pin<Box<dyn Future<Output = T> + Send + 'async_trait>>`: a nameable return type, so the trait stays dyn compatible, and a `Send` future, so `tokio::spawn` accepts it ([async-trait 0.1.92](https://docs.rs/async-trait/0.1.92/async_trait/)). It costs one allocation per call, which is nothing next to a database, network or model call. A hand-written `BoxFuture` has the same runtime shape, but `mockall` 0.15 can't mock it when the future borrows a named lifetime (`E0521`, probed), and every borrowed argument would need that lifetime spelt out.
- **Synchronous ports stay plain**: `Clock`, `IdGen`, `UrlSigner`, `SpeciesPhotos`, `WebhookVerifier`, `SpotSegmenter` do CPU work measured in microseconds (an HMAC, a lookup) or run on a blocking thread.
- **No associated constants and no generic methods on dyn ports**, because either makes a trait not dyn compatible ([Reference, dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#r-items.traits.dyn-compatible.associated-consts)). `TextEmbedder::model()` returns the pinned model's name, version and dimension as data; `IdGen::ulid()` is the dispatchable method and `IdGenExt::id::<T>()` the generic convenience, blanket-implemented for `G: IdGen + ?Sized` so it works on `dyn IdGen` too.
- **`Store` is a supertrait of the area traits with a blanket impl.** `api` holds one `Arc<dyn Store>`; a component that needs one area receives `Arc<dyn PlanRepo>` by trait upcasting, stable since Rust 1.86 ([Reference, trait upcasting](https://doc.rust-lang.org/reference/type-coercions.html#r-coerce.unsize.trait-upcast); [release notes 1.86.0](https://doc.rust-lang.org/releases.html#version-1860-2025-04-03)).

### `Send`, `Sync` and `'static`

- Every port is `Send + Sync`: implementations are shared through `Arc` across tokio's worker threads ([The Book, 16.4](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html)).
- `Arc<dyn Trait>` means `Arc<dyn Trait + 'static>` by the default object lifetime ([Reference, default trait object lifetimes](https://doc.rust-lang.org/reference/lifetime-elision.html#default-trait-object-lifetimes)), so an implementation owns its state (a client, a pool) and can move into spawned tasks. No trait adds `'static` as a supertrait; the containers ask for it.
- Every value that crosses a task, a channel, the database or the wire is owned: requests and records are owned structs (`NewScan`, `PlanWrite`, `ScanEvent<Stored>`), ids are `Copy`, slugs are `Arc<str>`. `ByteStream` is `BoxStream<'static, …>` for the same reason: `api` moves it into the response body.
- The proofs `Membership<R>` and `ConversationOwner<R>` are `Copy + Send + Sync + 'static` (a unit test checks), so a handler can pass one by value to a repository call or into the per-scan writer's task.

### Markers and `PhantomData`

| Where | Marker | Why |
|---|---|---|
| `Id<T>` | `PhantomData<fn() -> T>` | The id never owns or drops a `T`. `fn() -> T` is covariant in `T` and always `Send + Sync` with no drop-check obligation ([Nomicon, table of `PhantomData` patterns](https://doc.rust-lang.org/nomicon/phantom-data.html#table-of-phantomdata-patterns); [Reference, variance](https://doc.rust-lang.org/reference/subtyping.html#variance)). `PhantomData<T>` would tie the id's auto traits to the marker type; the markers are uninhabited enums, so it would change nothing today, but the id should not depend on what a marker is |
| `Slug<K>` | `PhantomData<fn() -> K>` | The same |
| `Membership<R>` | `PhantomData<fn() -> R>` | The proof holds no `R`; the minimum role is purely a type |
| `MinRole::ROLE` | an associated `const` | Fine here: the markers are only used as type parameters, never as trait objects |
| `AtLeast<R>` | a marker trait implemented for every ordered pair | The compile-time "or above". Only the four markers can implement it (its supertrait `MinRole` is sealed), and the orphan rule stops another crate adding a pair, since both the trait and the markers are `domain`'s |
| `UrlState` | sealed trait with associated types `Url` and `Mask` | Two states only; the slot types differ per state ([section 5](#5-signed-url-typing)) |
| `BoolLit<const B: bool>` | a const generic | One literal type for `true` and `false` |

### Named lifetimes

| Item | Lifetime | Why |
|---|---|---|
| `Signer<'a>` | `'a` borrows the `UrlSigner` and `SpeciesPhotos` ports | One signer per response or event send; `Copy`, so it is passed by reference to each `sign` call without cloning `Arc`s |
| `ChatModel::stream<'a>(&'a self, request: &'a ChatRequest) -> BoxStream<'a, …>` | `'a` ties the stream to both the model and the request | The adapter can keep references into the request (messages, tool schemas, image bytes) while it streams. The SSE response body must be `'static`, so `agent` moves the `Arc<dyn ChatModel>` and the owned `ChatRequest` into an `async_stream` generator and borrows them inside it: the generator owns both, and a borrow held across `.await` points inside an async block is allowed ([Async Book](https://rust-lang.github.io/async-book/)) |
| `RgbView<'a>`, `MaskView<'a>` | `'a` borrows decoded pixel buffers | `SpotSegmenter` runs inside `spawn_blocking`, whose closure must be `'static`: the measurement code moves the owned buffers into the closure and lends them to the segmenter for the call. The output `SpotMask` is owned |
| `GeocodeQuery<'a>`, `WebhookHeaders<'a>` | `'a` borrows strings from the request | Inputs of one call; the geocoder's future and the verifier's result own everything they return |
| `SpotSegmenter::segment<'a>`, `WebhookVerifier::verify<'a>` | the input views' lifetime | Named because two borrowed inputs share it |
| `WeatherThresholds::humid_stretch_starts<'d>(&self, days: &'d [ForecastDay]) -> impl Iterator + use<'d>` | `'d` borrows the forecast | Edition 2024 would also capture `&self`'s lifetime; the thresholds are `Copy` and copied into the closure, so `use<'d>` leaves the receiver out and a caller can drop them while it holds the iterator ([Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html); [Reference, use bounds](https://doc.rust-lang.org/reference/types/impl-trait.html#r-type.impl-trait.generic-capture.precise.use)) |
| `ProblemSeed::discriminating(&self) -> impl Iterator + '_` | the receiver | Under edition 2024 an `impl Trait` return captures every in-scope lifetime ([Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)); the only one here is `self`'s, so no `use<..>` bound is needed |

Pure, synchronous work borrows: `routing` takes `&[RoutableImage<T>]` and `&[&Indicator]`, `GardenMapInput::validate` consumes its input, the map geometry borrows `&[Pin]`. Under `#[async_trait]` a method's borrowed arguments (`&Lease`, `&[CellRequest]`, `&AuthId`) get their own `'lifeN` lifetimes, all outliving the boxed future, so no signature spells them.

### The generic URL-state parameter

`U: UrlState = Signed` appears on every type with a URL slot. The default means `api` code writes `Scan` and `store` code writes `Scan<Stored>`. `#[serde(bound = "")]` drops serde's inferred `U: Serialize` bound: what has to be serialisable is `U::Url` and `U::Mask`, which `UrlSlot` and `MaskSlot` already require. The derives of `Clone`, `Debug` and `PartialEq` add `U: Clone` and so on, which both markers satisfy.

---

## 11. Testing

- **Unit tests (90)** in each module: the band rule and the certainty mapping; `BBox`, `V1`, `BoolLit` and `Timestamp` on the wire; id and slug parsing; the error body example of contracts 2 (exact JSON); every error code's status; the scan state machine (every trigger, the submit decision, the species window, deletion and feedback); upload routes, kind limits and depth checks; the question cap and steps; the role matrix; runtimes, claimable kinds, lease extension, attempts and final failures; step stale limits and hand-overs; run stages for submit and species changes; the stream's resting points; photo routing; map validation, polygons, neighbours and crowding; repeats; alert keys; object keys; the cascade order; the weather thresholds (the pinned defaults, frost at its threshold, humid hours strictly over 90%, a humid stretch only from days in a row, the past week's longest humid run); the `SoilGrids` retry delays; `after_failed_photo` against `final_failure`.
- **Exact JSON against contracts.md**: the SSE `finding` envelope (key order included) signed with `FakeSigner`, a `Job` of each payload kind, `CompleteJob` tagged by status with results decoded by kind, worker events with an unknown type, the `ProblemSeed` example as the JSON its YAML parses to, the strict seed and plan-writer schemas.
- **Compile-fail doc tests (5)** prove the type-level guarantees with their error codes: a `Membership<Viewer>` where `Membership<Editor>` is required (E0308), an upgrade through `at_least` (E0277), building a proof outside `domain` (E0624), returning `Scan<Stored>` through a `ToSchema` bound (E0277), and passing a signed event to a method that takes a stored one (E0308) ([rustdoc, documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html)).
- **For other crates**: the `mock` feature generates `Mock<Trait>` for every port except `ChatModel` (`mockall` can't mock a stream that borrows its request); the `test-util` feature adds `FixedClock`, `SequentialIds`, `FakeSigner`, `FixedPhotos` and `ScriptedChat`:

```toml
[dev-dependencies]
domain = { workspace = true, features = ["mock", "test-util"] }
```

---

## 12. Open questions

Ambiguities found in the documents while writing the types, with the choice made here. Open ones belong in [contracts 12](../architecture/contracts.md#12-open-questions) or [database 9](../database/README.md#9-open-questions). The numbering of v0.1 is kept, so other documents' references stay valid.

1. **Resolved by contracts v0.9** ([contracts 1, wire format](../architecture/contracts.md#wire-format)): worker results and VLM shapes carry a bare confidence; every record the API returns carries the band. The types follow each shape as written; only `Observation` uses `Rated`.
2. **Resolved by contracts v0.9** ([contracts 5, complete](../architecture/contracts.md#post-internaljobsidcomplete)): the result is read by the job's recorded kind, as `JobOutcome::Done` and `JobResult::decode` do.
3. **Resolved by contracts v0.9** ([contracts 5, payloads](../architecture/contracts.md#payloads)): `PerceptionPayload.regions` omits `mask_url`, as `PriorRegion` does.
4. **Resolved by contracts v0.9** ([contracts 6, box formats](../architecture/contracts.md#box-formats-per-model-adapter)): every format is corners `[x1, y1, x2, y2]`, which `BoxFormat::to_bbox` reads. `norm_1000` is the chosen Qwen3-VL-class models' format; `abs_px_prepared` is the Qwen2.5-VL baseline's and the fallback's until a live call confirms it (contracts open question 26).
5. **Resolved by contracts v0.9** ([contracts 7, WeatherSummary](../architecture/contracts.md#weathersummary)): the cache holds a cell's weather without `location_precision` (`CellWeather`), and the API adds the garden's precision (`for_garden`).
6. **Can a `planned` scan fail?** Architecture 8.1 draws "any state after pending → failed"; contracts 3 says late work never fails a scan, and nothing of a run is in flight once it is planned. The state machine refuses `planned → failed`.
7. **Resolved by database v1.4** ([database 4, indicator answers](../database/README.md#indicator-answers-)): the `indicator_answer` table, keyed `[scan, run, indicator_id]` and ranked user > measurement > VLM, holds every answer of a run between the steps; `StepOutput::Retrieve.evidence` writes it and `ScanSnapshot.evidence` reads it. `scan.candidates` and `question.indicator` were added in the same version.
8. **Resolved by contracts v0.9** ([contracts 9, members and invites](../architecture/contracts.md#members-and-invites)): `created_by?: Id`, absent once that user is deleted. `Invite.created_by` is `Option<UserId>`, omitted when `None`.
9. **The plan writer's output shape isn't in contracts 7.** `PlanWriterOutput` is `ActionPlan`'s model-written fields; its JSON Schema is strict, its serde form tolerant.
10. **`soil_conditions.yaml`'s shape isn't in contracts 10.** `SoilConditionSeed` follows the `soil_condition` table.
11. **The signed URL's HMAC input.** Storage 3 names the method, the path and `exp`; the byte layout `"GET\n{path}\n{exp}"` is fixed here.
12. **Resolved by contracts v0.9** ([contracts 9, gardens](../architecture/contracts.md#gardens)): `precision` defaults to `exact` when `lat` and `lon` are sent without it.
13. **12 MB** is read as 12 MiB, as the `rust-axum-api` skill does.
14. **Resolved by database v1.3.3** ([database 1](../database/README.md#1-version-and-setup)): ULIDs come from `Ulid::generate()`, the ulid 3.0.0 name.
15. **`Diagnosis.organisms[].preys_on` is never stored**, but `Diagnosis<Stored>` has the field: the store drops it on write and `api` fills it when read. A third URL-like state for computed fields would enforce it in types; not worth the complexity now.
16. **Membership is checked once per request**, before the repository call, rather than inside each repository query as architecture 4 words it ("every garden-scoped repository function takes the user and checks membership"). Repository queries still scope to the proof's garden (section 8).
17. **The weather thresholds aren't in the contracts.** Contracts 7 names `frost_risk` and `hours_rh_over_90`, and `TriggerKind` names the rules, but no numbers. `WeatherThresholds::default()` (section 7) is this design's proposal; it belongs beside the fusion thresholds of [architecture 15](../architecture/README.md#15-open-decisions) and contracts open question 10, to be set from the evaluation set ([contracts open question 27](../architecture/contracts.md#12-open-questions) points here as the one owner). The past-week wording of contracts 9 (crowding) and architecture 6.8 now holds: `WeatherSummary.past_7_days.longest_humid_run_days` (contracts v0.10, section 7) carries the longest run of humid days, so a humid stretch in the past week is detectable.

### Additions since v0.1

Each crate's author added what its code needed, small and additive; the listings above are regenerated from the source.

| Asked for by | Addition | Where |
|---|---|---|
| `agent` | `StopReason::Refusal` (the provider's safety classifier declined) | `ports::models` |
| `agent` | `ContentPart::ToolResult.is_error` (a refused call the model may rewrite) | `ports::models` |
| `agent` | `ReasoningBlock` (thinking or redacted thinking, replayed within a tool loop), `ContentPart::Reasoning`, `ChatChunk::Reasoning` | `ports::models` |
| `agent` | `ChatPurpose::Classification` (the classifier's LLM fallback) | `ports::models` |
| `store` | `AnalysisRepo::similar_cases`, `ChatRepo::message_count` | `ports::store` |
| `store` | `QuestionAdvance` (a photo answer's next step, or `score` claimed in the same transaction) | `ports::store::analysis` |
| `store` | `CompletionOutcome::Stored.diff` (what the final result changed against the partial results) | `ports::store::jobs` |
| `store` | `GuidedOutcome` without `check` (the store finds the check from the leased job) | `ports::store::analysis` |
| `pipeline` | `StepOutput` restructured per step (`retrieve` carries evidence, candidates, weather and its job; `score` its diagnosis, status and hand-over) | `ports::store::steps` |
| `pipeline` | `ScanSnapshot.candidates`, `weather`, `open_question_claim`, `open_question_indicator`, `plan_diagnosis` | `ports::store::scans` |
| `pipeline` | `PhotoAnswerStart` and `NewScanImage.photo_answer_start` (a photo answer's first step, recorded with its upload) | `ports::store::scans` |
| `pipeline` | `JobRepo::live_workers` and `LiveWorker` (the label-version rule with no live worker) | `ports::store::jobs` |
| `pipeline` | `PlantRepo::garden_plants` | `ports::store::plants` |
| `pipeline` | `StreamState.step_claimed` (a held step claim keeps the stream open) | `rules::stream` |
| `pipeline` | `NewQuestion.indicator` (`question.indicator`, server-side) | `ports::store::steps` |
| `api` | `authz::check_membership_of` (section 8) | `authz` |
| `api` | `Notifier::check_receipts`, with a default body | `ports::providers` |
| `api` | `PlanTriggerRule.title`, `detail` (the task's text when the rule fires) | `ports::store::plans` |
| `vision` | `JobErrorCode` (`JobError.code`), `ModelVersionKey` | `jobs` ([domain-types 5](domain-types.md#5-worker-api-and-jobs)) |
| `store` | `Invite.created_by: Option<UserId>` (item 8) | `garden::members` |
| this audit | `rules::weather::WeatherThresholds` (section 7) | `rules::weather` |
| this audit | `EmbedMode` and `TextEmbedder::embed(mode, texts)` (section 9) | `ports::models` |
| this audit | `PhotoAnswerStart::Segment(Box<NewJob>)` (`clippy::large_enum_variant`); `StepOutput` lost an `#[expect]` it no longer needs | `ports::store` |
| this audit | Comments: `BoxFormat` names each format's models (item 4); `JobRepo::check_lease` serves `events` and `complete` and accepts a job already completed with the same token; `DeletionRepo::delete_batch` returns `$before.id` | `vlm`, `ports::store` |
| cross-crate fixes | `PastWeather.longest_humid_run_days` (`#[serde(default)]` for weather cached or snapshotted before it, `#[schema(required = true)]` on the wire); `WeatherThresholds::longest_humid_run`, `had_humid_stretch`, `humid_stretch_now_or_past` | `records::weather`, `rules::weather` |
| cross-crate fixes | `JobRepo::live_lease` (late `events` after `complete` are dropped); `requeue_stale` and `enqueue_job` docs: the store's `Notify` is the claim long poll's only wake-up | `ports::store::jobs` |
| cross-crate fixes | `GardenRepo::soil_profile_failed`, `gardens_due_soil_profile` (`garden.soil_retry_at`); `rules::limits::soil_retry_after`, `SOIL_RETRY_OUTAGE`, `SOIL_RETRY_NO_DATA` | `ports::store::gardens`, `rules::limits` |
| cross-crate fixes | `AccountDeletionMarker`, `DeletionRepo::start_account_deletion` (the tombstone's compare-and-set), `unfinished_account_deletions`, `finish_account_deletion`, `delete_finished_tombstones`; `IdentityRepo::upsert_user` refuses a tombstoned auth id with `NotFound`, which `authz::authenticate` turns into `AuthError::AccountDeleted` (`401`); `rules::limits::TOMBSTONE_KEEP` | `ports::store::deletion`, `ports::store::identity`, `ports::errors`, `authz` |
| cross-crate fixes | `QuestionState::after_failed_photo(false)` is `Superseded`, not `Open`: an in-run photo answer's `verify` job out of attempts fails the scan (contracts 5, `final_failure`), which supersedes the question | `rules::questions` |
| store (EON E1.6) | `FarmRecordStore::request_sync` returns `SyncRequest` instead of `bool`, so the route has the instant for `retry_after_s` without a second read; `delete_chunks_from` (a document re-chunked into fewer chunks keeps the vectors of the chunks that stay, which deleting the whole item would lose); `source_vendor_ids_since` (the daily 7-day re-read needs the stored items to find what is gone); `NewMessage.farm_citations`; the connector limits in `rules::limits` | `ports::store::farm`, `ports::store::chat`, `rules::limits` |

The new store methods were added with default bodies, so the workspace kept compiling while the store author implemented them; each default keeps the behaviour from before (no marker, no retry time, every accepted lease live), and `store` overrides all of them.

---

## 13. Sources

**Rust language**
- The Book: [18.2 trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [17 async and await](https://doc.rust-lang.org/book/ch17-00-async-await.html) · [20.2 advanced traits](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html)
- The Reference: [dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility) · [subtyping and variance](https://doc.rust-lang.org/reference/subtyping.html#variance) · [trait upcasting](https://doc.rust-lang.org/reference/type-coercions.html#r-coerce.unsize.trait-upcast) · [default trait object lifetimes](https://doc.rust-lang.org/reference/lifetime-elision.html#default-trait-object-lifetimes) · [associated items](https://doc.rust-lang.org/reference/items/associated-items.html)
- [The Nomicon, `PhantomData`](https://doc.rust-lang.org/nomicon/phantom-data.html) · [std `PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html) · [std `Send`](https://doc.rust-lang.org/std/marker/trait.Send.html)
- [Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · [release notes](https://doc.rust-lang.org/releases.html) (1.86 trait upcasting) · [Async Book](https://rust-lang.github.io/async-book/)
- [Rust API Guidelines: naming](https://rust-lang.github.io/api-guidelines/naming.html) · [future proofing (sealed traits)](https://rust-lang.github.io/api-guidelines/future-proofing.html) · [rustdoc documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html)

**Crates at the pinned versions**
- [async-trait 0.1.92](https://docs.rs/async-trait/0.1.92/async_trait/) · [mockall 0.15.0](https://docs.rs/mockall/0.15.0/mockall/) · [futures 0.3.34 `BoxStream`](https://docs.rs/futures/0.3.34/futures/stream/type.BoxStream.html) · [thiserror 2.0.21](https://docs.rs/thiserror/2.0.21/thiserror/)
- [utoipa 6 `ToSchema`](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html) · [schemars 1.2.3](https://docs.rs/schemars/1.2.3/schemars/) · [serde enum representations](https://serde.rs/enum-representations.html) · [serde `flatten`](https://serde.rs/attr-flatten.html)
- [ulid 3.0.0](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html) · [chrono 0.4.45 `DateTime`](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html)

**Project documents and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [database](../database/README.md) and [schema](../../backend/migrations/schema.surql) · [storage](../storage/README.md) · [frontend](../frontend/README.md) · [domain-types](domain-types.md)
- Skills in the main checkout's `.claude/skills/`: `rust` (with `versions.md`), `rust-contract-types`, `rust-errors`, `rust-async`, `rust-testing`

Every external link was opened on 2026-10-15.
