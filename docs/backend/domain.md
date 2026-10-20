# PDM Backend: `domain` crate

> **Status:** Draft v0.3.9, 2026-10-19. v0.3.9: `FarmRecordStore::deleting_connectors` lists the connectors marked `deleting` with their sealed tokens, for a garden (a cascade or a demotion) or for every garden not being deleted (resume after a restart), so `api` revokes the tokens after the marking transaction and then deletes the records (roadmap E3.6); `connectors_created_by` says which transactions mark them. v0.3.8: `FarmRecordStore` is implemented by the store (roadmap E1.6) and changes for the sync: `request_sync` returns `SyncRequest` (accepted, or too soon with the instant the next is allowed, for `retry_after_s`), and `delete_chunks_from` and `source_vendor_ids_since` cover a re-chunked document and the daily 7-day re-read; `NewMessage` carries `farm_citations`; `rules::limits` gains the connector limits (`MAX_CONNECTORS_PER_GARDEN`, `MAX_FARM_RECORDS_PER_GARDEN`, `SYNC_NOW_INTERVAL`, `FARM_RECORD_KEEP`, `PENDING_CONNECT_TTL`). v0.3.7: the farm connectors' ports (roadmap E1.2): `FarmSource` for the Slack and Drive adapters with `FarmSourceError`, and `FarmRecordStore` for the `connector` and `farm_record` tables, part of `Store`; `GardenResource::Connector`; the garden cascade's `FarmRecordsAndConnectors` step before `Garden`; a new `connectors` module of wire types. v0.3.6: `AnalysisRepo::finish_guided_check` takes `now`, like `complete_job`, so the lease check and the `score` claim follow the `Clock` port instead of the database clock. v0.3.5: the `BeliefStore` port records shared memories in Agent Memory, and `MemoryStore` gains the belief sweep's four methods (`unsynced_shared`, `mark_synced`, `belief_forgets`, `finish_belief_forget`). v0.3.4: `ToolSpec` gains `strict`, set by `agent` for the tools that propose changes. v0.3.3: section 1 says these types are the contracts' source of truth now that the code exists. v0.3.2: the schema link points to `backend/migrations/schema.surql`. v0.3.1: the section 1 note says the Rust skills follow this design and keeps only the timestamp difference from the brief; section 10 no longer says `rust-async` prefers a hand-written `BoxFuture`. v0.3: the cross-crate fixes add `PastWeather.longest_humid_run_days` and the past-week humid-stretch rules, store methods with default bodies (`JobRepo::live_lease`, `GardenRepo::soil_profile_failed` and `gardens_due_soil_profile`, the account tombstone's `DeletionRepo` methods), `AuthError::AccountDeleted` for a tombstoned account, `soil_retry_after` and `TOMBSTONE_KEEP`, and make `QuestionState::after_failed_photo(false)` supersede the question as contracts 5 says. v0.2: one definition of the weather thresholds (`rules::weather::WeatherThresholds`: frost at a 2 m minimum ≤ 0.5 °C, hard frost ≤ −2 °C, a humid stretch of two days with ≥ 10 h over 90%), `TextEmbedder::embed` takes an `EmbedMode`, `PhotoAnswerStart::Segment` boxes its job, corrected `BoxFormat`, `JobRepo::check_lease` and `DeletionRepo::delete_batch` comments, every addition the other crates made since v0.1 (section 12), and the open questions that contracts v0.9 and database v1.3.3–v1.4 resolved marked as such. v0.1: the shared crate every other crate builds on: typed ids and wire primitives, the error codes, the pure rules, the authorization proofs, the signed-URL typing and every boundary trait, all compiled on Rust 1.99.1.

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

```toml
[package]
name = "domain"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[features]
# mockall mocks of every port trait, for other crates' tests:
# domain = { workspace = true, features = ["mock"] } under [dev-dependencies].
mock = ["dep:mockall"]
# FixedClock, SequentialIds, FakeSigner, FixedPhotos and ScriptedChat, for other crates' tests.
test-util = []

[dependencies]
async-trait.workspace = true
bytes.workspace = true
chrono.workspace = true
futures.workspace = true
mockall = { workspace = true, optional = true }
schemars.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
ulid.workspace = true
utoipa.workspace = true
uuid.workspace = true
```

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

```rust
/// A `SurrealDB` table whose records have a ULID key. Implemented by the uninhabited markers in
/// [`table`]; `NAME` is the table name in `schema.surql`.
pub trait Table: 'static {
    /// The table name, as in `DEFINE TABLE` ([database 3]).
    const NAME: &'static str;
}

/// The ULID key of a record in table `T`.
///
/// `PhantomData<fn() -> T>` rather than `PhantomData<T>`: an `Id<T>` never owns or drops a `T`,
/// so the marker should say "produces a `T`", which is covariant in `T` and is always `Send`,
/// `Sync`, `Unpin` and `'static`-friendly whatever `T` is. The markers are uninhabited enums,
/// so this changes nothing today, but it keeps the auto traits independent of the marker type
/// (Rust Reference, `PhantomData` and variance; std `PhantomData` docs, "ownership and the drop
/// check"). The trait impls are written by hand because `#[derive]` would add a `T: Clone`
/// (etc.) bound that the markers don't and needn't satisfy.
pub struct Id<T: Table> {
    ulid: Ulid,
    table: PhantomData<fn() -> T>,
}
```

The trait impls are written by hand (`Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Ord`, `Debug` as `scan:01JAX…`, `Display` as the bare ULID, `FromStr`, serde and both schema traits): `#[derive]` would add `T: Clone` and similar bounds that uninhabited markers can't meet. The markers come from one macro:

```rust
/// One uninhabited marker per table with a ULID key in `schema.surql`. Tables with natural
/// array keys (`member_of`, `scan_event`, `push_token`, `worker`, `usage_daily`,
/// `idempotency_key`, `weather_cache`, `message_feedback`, `classifier_label`,
/// `soil_condition`) and knowledge tables (slugs) have none.
pub mod table {
    use super::Table;

    macro_rules! tables {
        ($($(#[$doc:meta])* $marker:ident => $name:literal),* $(,)?) => {
            $(
                $(#[$doc])*
                #[derive(Debug)]
                pub enum $marker {}
                impl Table for $marker {
                    const NAME: &'static str = $name;
                }
            )*
        };
    }

    tables! {
        /// `user`
        User => "user",
        /// `garden`
        Garden => "garden",
        /// `invite`
        Invite => "invite",
        /// `plant`
        Plant => "plant",
        /// `garden_map`
        GardenMap => "garden_map",
        /// `scan`
        Scan => "scan",
        /// `scan_image`
        ScanImage => "scan_image",
        /// `mask`
        Mask => "mask",
        /// `observation`
        Observation => "observation",
        /// `question`
        Question => "question",
        /// `guided_check`
        GuidedCheck => "guided_check",
        /// `diagnosis`
        Diagnosis => "diagnosis",
        /// `scan_feedback`
        ScanFeedback => "scan_feedback",
        /// `action_plan`
        ActionPlan => "action_plan",
        /// `plan_trigger`
        PlanTrigger => "plan_trigger",
        /// `task`
        Task => "task",
        /// `conversation`
        Conversation => "conversation",
        /// `chat_image`
        ChatImage => "chat_image",
        /// `message`
        Message => "message",
        /// `pending_action`
        PendingAction => "pending_action",
        /// `memory`
        Memory => "memory",
        /// `job`
        Job => "job",
        /// `connector`
        Connector => "connector",
    }
}
```

### `Slug<K>`, compound and opaque ids

`Slug<K>` holds an `Arc<str>` so cloning a slug in candidate lists, scores and prompts is a reference-count bump. Its format is `[a-z][a-z0-9_]*`, at most 64 characters, and `"healthy"` is reserved for `ProblemOrHealthy` ([contracts 10](../architecture/contracts.md#10-knowledge-seed-format)). `IndicatorId` (`<problem_id>.<slug>`), `RegionId` (`<image_id>.<n>`) and `ChunkId` (`<source_id>.<n>`) parse into their parts. `WorkerId`, `LeaseToken` (a ULID, generated by the claim's `rand::ulid()`), `AuthId`, `InviteCode` (26 base32 characters), `PushToken`, `Cursor` and `StepId` are checked opaque strings; `IdempotencyKey` wraps a `uuid::Uuid`. Every one serialises as a bare string and has a string schema. Listings: [domain-types 1](domain-types.md#1-conventions).

```rust
/// Why a string isn't a valid id of the expected kind.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind}: {problem}")]
pub struct IdParseError {
    /// What was being parsed, such as `"scan id"` or `"indicator id"`.
    pub kind: &'static str,
    /// What is wrong with it.
    pub problem: &'static str,
}
```

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

```rust
impl ConfidenceBand {
    /// The band rule of contracts 1.
    pub fn of(confidence: Confidence) -> Self {
        if confidence >= Confidence::HIGH {
            Self::High
        } else if confidence >= Confidence::MEDIUM {
            Self::Medium
        } else {
            Self::Low
        }
    }
}
```

```rust
impl Certainty {
    /// The fixed mapping: low 0.4, medium 0.65, high 0.85.
    pub fn confidence(self) -> Confidence {
        Confidence(match self {
            Self::Low => 0.4,
            Self::Medium => 0.65,
            Self::High => 0.85,
        })
    }
}
```

---

## 5. Signed-URL typing

Stored payloads keep image and mask ids and never URLs, and every URL the app receives is signed as it is sent, live or replayed, so a replay hours later carries fresh 15-minute URLs ([contracts 4, transport](../architecture/contracts.md#transport); [database 4, scan events](../database/README.md#scan-events-a-gap-free-seq-); [storage 3](../storage/README.md#3-how-images-are-served)). The type system enforces both halves.

### The design: one URL-state parameter

Every type that can carry an image or mask URL takes `U: UrlState`, defaulting to `Signed`. `UrlState` is sealed and has two implementations; each names what a URL slot and a mask slot hold.

```rust
/// The state of every URL slot in a value: [`Stored`] or [`Signed`]. Sealed: there are exactly
/// two states.
pub trait UrlState:
    sealed::Sealed + Clone + Copy + fmt::Debug + PartialEq + Default + Send + Sync + 'static
{
    /// What an image URL slot holds.
    type Url: UrlSlot;
    /// What a region's mask slot holds, flattened into the region.
    type Mask: MaskSlot;
}

/// Implemented only by [`Signed`]: the bound on every `utoipa::ToSchema` impl of a type with
/// URL slots, so only signed values can appear in the API's schema and responses.
pub trait WireState: UrlState {}

/// IDs only: what the database stores and what the agent's tools return to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stored;

/// Fresh, signed, short-lived URLs: what the app receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Signed;

/// A URL slot's contents.
pub trait UrlSlot:
    Clone + fmt::Debug + PartialEq + Serialize + de::DeserializeOwned + Default + Send + Sync + 'static
{
    /// True when the slot holds nothing to serialise (always, for [`NoUrl`]).
    fn is_absent(&self) -> bool;
}

/// A region's mask slot.
pub trait MaskSlot:
    Clone + fmt::Debug + PartialEq + Serialize + de::DeserializeOwned + Default + Send + Sync + 'static
{
}

/// The empty URL slot of a stored value. It is skipped when serialising, and deserialising
/// anything into it fails, so a stored record can't hold a URL even through JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NoUrl;

/// `skip_serializing_if` for optional URL slots.
pub fn url_absent<T: UrlSlot>(slot: &Option<T>) -> bool {
    slot.as_ref().is_none_or(UrlSlot::is_absent)
}

/// The mask slot of a stored region: the mask record's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StoredMask {
    /// The `mask` record, when segmentation produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_id: Option<MaskId>,
}

/// The mask slot of a signed region: `mask_url?` of contracts 1.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SignedMask {
    /// Signed PNG mask, when segmentation produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub mask_url: Option<SignedUrl>,
}
```

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

```rust
/// The first 16 hex digits of a species photo's WebP SHA-256, in its storage key and URL
/// ([contracts 10], [storage 2]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhotoHash([u8; 16]);

/// Every route that serves an image through a signed URL ([storage 3]). Depth maps have none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImagePath {
    /// `GET /scans/{scan}/images/{image}`
    ScanImage {
        /// The scan.
        scan: ScanId,
        /// The image.
        image: crate::id::ImageId,
    },
    /// `GET /scans/{scan}/masks/{mask}`
    Mask {
        /// The scan.
        scan: ScanId,
        /// The mask.
        mask: MaskId,
    },
    /// `GET /conversations/{conversation}/images/{image}`
    ChatImage {
        /// The conversation.
        conversation: ConversationId,
        /// The image.
        image: ChatImageId,
    },
    /// `GET /species/{species}/images/{hash}`
    SpeciesPhoto {
        /// The species.
        species: SpeciesId,
        /// The photo's content hash.
        hash: PhotoHash,
    },
}

/// What the HMAC covers: the method, the path and `exp` ([storage 3]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningInput {
    /// Always `"GET"`.
    pub method: &'static str,
    /// The route path, without query string.
    pub path: String,
    /// Expiry, Unix seconds.
    pub exp: i64,
}

/// How long a signed URL lives ([contracts 1]; open question 5 of contracts 12).
pub const SIGNED_URL_TTL: Duration = Duration::from_mins(15);

/// Signs URLs for one response or one event send. Borrows the signer and the species photo
/// index for `'a`, the time it takes to build the response, so nothing is cloned per URL.
#[derive(Clone, Copy)]
pub struct Signer<'a> {
    port: &'a dyn UrlSigner,
    photos: &'a dyn SpeciesPhotos,
    exp: i64,
}
```

```rust
impl<'a> Signer<'a> {
    /// A signer whose URLs expire [`SIGNED_URL_TTL`] after `now`.
    pub fn new(signer: &'a dyn UrlSigner, photos: &'a dyn SpeciesPhotos, now: Timestamp) -> Self {
        let ttl = i64::try_from(SIGNED_URL_TTL.as_secs()).unwrap_or(i64::MAX);
        Self {
            port: signer,
            photos,
            exp: now.unix_seconds().saturating_add(ttl),
        }
    }

    /// The signed URL of an image route.
    pub fn url(&self, path: &ImagePath) -> SignedUrl {
        let input = SigningInput {
            method: "GET",
            path: path.to_string(),
            exp: self.exp,
        };
        let sig = self.port.signature(&input);
        SignedUrl(format!(
            "{}{}?exp={}&sig={sig}",
            self.port.public_base(),
            input.path,
            self.exp
        ))
    }

    /// The species photo index, for `SpeciesCandidate.reference_image`.
    pub fn photos(&self) -> &'a dyn SpeciesPhotos {
        self.photos
    }
}
```

```rust
/// Why a signed URL was refused: `403 invalid_signature` ([contracts 2]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignatureError {
    /// `exp` is in the past.
    #[error("signed URL expired")]
    Expired,
    /// `exp` is further ahead than any URL the API issues.
    #[error("signed URL expiry too far ahead")]
    TooFarAhead,
    /// The signature doesn't match.
    #[error("signature mismatch")]
    Mismatch,
}

/// Checks an incoming signed request ([storage 3]): `exp` not past, not beyond the TTL, and the
/// signature matching (compared by the port in constant time).
pub fn verify_signed(
    signer: &dyn UrlSigner,
    path: &ImagePath,
    exp: i64,
    sig: &str,
    now: Timestamp,
) -> Result<(), SignatureError> {
    let now = now.unix_seconds();
    let ttl = i64::try_from(SIGNED_URL_TTL.as_secs()).unwrap_or(i64::MAX);
    if exp < now {
        return Err(SignatureError::Expired);
    }
    if exp > now.saturating_add(ttl) {
        return Err(SignatureError::TooFarAhead);
    }
    let input = SigningInput {
        method: "GET",
        path: path.to_string(),
        exp,
    };
    if signer.verify(&input, sig) {
        Ok(())
    } else {
        Err(SignatureError::Mismatch)
    }
}

/// Turns a stored value into its signed wire form. `Scope` is what the value doesn't know about
/// itself but its URLs need (the scan of a region's mask, the conversation of a message's
/// attachment); `()` when the value carries it.
pub trait Sign {
    /// The signed form.
    type Signed;
    /// The parent id the URLs need.
    type Scope: Copy;
    /// Signs every URL slot.
    fn sign(self, scope: Self::Scope, signer: &Signer<'_>) -> Self::Signed;
}
```

`Scope` carries what a value doesn't know about itself but its URLs need: a region's mask route needs its scan, a message's attachments need their conversation. A typical implementation:

```rust
impl Sign for Observation<Stored> {
    type Signed = Observation<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> Observation<Signed> {
        Observation {
            region: self.region.sign(self.scan_id, signer),
            id: self.id,
            scan_id: self.scan_id,
            run: self.run,
            stage: self.stage,
            pipeline_stage: self.pipeline_stage,
            label: self.label,
            knowledge_ref: self.knowledge_ref,
            role: self.role,
            detail: self.detail,
            confidence: self.confidence,
            source: self.source,
            model_version: self.model_version,
            created_at: self.created_at,
        }
    }
}
```

`Vec<T>`, `Option<T>` and `Page<T>` sign element-wise, so `ScanDetail` signs in one call: `detail.sign((), &Signer::new(&*state.signer, &*state.photos, clock.now()))`.

**The HMAC.** `sig` is HMAC-SHA256 under the server secret over the canonical `"GET\n{path}\n{exp}"` ([storage 3](../storage/README.md#3-how-images-are-served) names the three parts; the byte layout is fixed here). `verify_signed` refuses an `exp` in the past (`Expired`) or more than 15 minutes ahead (`TooFarAhead`) before asking the port to compare in constant time; any refusal is `403 invalid_signature`.

---

## 6. Errors

### Codes as data

Each code of [contracts 2](../architecture/contracts.md#2-errors) knows its HTTP status, its default `retryable`, and whether a response with it is stored under an idempotency key ([contracts 1, idempotency](../architecture/contracts.md#idempotency)). The status is a `u16`; only `api` turns it into an axum response.

```rust
impl ErrorCode {
    /// Every code, in the order of contracts 2.
    pub const ALL: [Self; 19] = [
        Self::InvalidRequest,
        Self::Unauthenticated,
        Self::Forbidden,
        Self::InvalidSignature,
        Self::NotFound,
        Self::InvalidState,
        Self::RequestInProgress,
        Self::LeaseLost,
        Self::UploadTooLarge,
        Self::UnsupportedMediaType,
        Self::ImageQuality,
        Self::MissingRequiredShot,
        Self::TooManyImages,
        Self::IdempotencyConflict,
        Self::RateLimited,
        Self::BudgetExceeded,
        Self::Internal,
        Self::ProcessingUnavailable,
        Self::UpstreamUnavailable,
    ];

    /// The HTTP status of contracts 2.
    pub const fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::Unauthenticated => 401,
            Self::Forbidden | Self::InvalidSignature => 403,
            Self::NotFound => 404,
            Self::InvalidState | Self::RequestInProgress | Self::LeaseLost => 409,
            Self::UploadTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::ImageQuality
            | Self::MissingRequiredShot
            | Self::TooManyImages
            | Self::IdempotencyConflict => 422,
            Self::RateLimited | Self::BudgetExceeded => 429,
            Self::Internal => 500,
            Self::ProcessingUnavailable | Self::UpstreamUnavailable => 503,
        }
    }

    /// The default `retryable`: whether the same request may succeed later. `true` for
    /// `request_in_progress`, `rate_limited` and both `503`s (contracts 2), and for
    /// `image_quality` as in contracts 2's example (a retake succeeds). `budget_exceeded` is
    /// `false`: it lasts until `resets_at`.
    pub const fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::RequestInProgress
                | Self::RateLimited
                | Self::ImageQuality
                | Self::ProcessingUnavailable
                | Self::UpstreamUnavailable
        )
    }

    /// Whether a response with this code is stored under its idempotency key ([contracts 1]:
    /// `2xx` and the deterministic `400`, `403`, `404`, `413`, `415`, `422`).
    pub const fn is_stored_for_idempotency(self) -> bool {
        matches!(self.http_status(), 400 | 403 | 404 | 413 | 415 | 422)
    }

    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::InvalidSignature => "invalid_signature",
            Self::NotFound => "not_found",
            Self::InvalidState => "invalid_state",
            Self::RequestInProgress => "request_in_progress",
            Self::LeaseLost => "lease_lost",
            Self::UploadTooLarge => "upload_too_large",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::ImageQuality => "image_quality",
            Self::MissingRequiredShot => "missing_required_shot",
            Self::TooManyImages => "too_many_images",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::RateLimited => "rate_limited",
            Self::BudgetExceeded => "budget_exceeded",
            Self::Internal => "internal",
            Self::ProcessingUnavailable => "processing_unavailable",
            Self::UpstreamUnavailable => "upstream_unavailable",
        }
    }
}
```

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

```rust
/// A rule refused the call. Raised by `domain::rules`, `authz` and every crate's own logic, and
/// mapped to a contract code in one place ([`DomainError::kind`]). Infrastructure failures are
/// not `DomainError`s: they live in each port's error enum (`StoreError`, `BlobError`, …),
/// which wraps this one with `#[from]`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DomainError {
    /// The record doesn't exist, is being deleted, or the caller may not see it. Always 404,
    /// never 403, for non-members ([contracts 1], roles and access).
    #[error("not found")]
    NotFound,
    /// A member whose role is below `required`.
    #[error("requires the {required} role")]
    Forbidden {
        /// The lowest role that may make the call.
        required: Role,
    },
    /// A request field breaks a rule that serde can't express (exactly one of two fields, a
    /// length, a range).
    #[error("{field}: {problem}")]
    InvalidRequest {
        /// The field, as a dotted path.
        field: String,
        /// What is wrong.
        problem: String,
    },
    /// Map pins name plants that aren't plants of this garden ([contracts 9]).
    #[error("unknown plants in pins")]
    UnknownPlants {
        /// Every such plant.
        plant_ids: Vec<PlantId>,
    },
    /// The record isn't in a state that allows the call.
    #[error("invalid state")]
    InvalidState(InvalidStateDetails),
    /// A scan status transition the state machine refuses.
    #[error(transparent)]
    Transition(#[from] TransitionError),
    /// A kind limit before submit ([contracts 3]).
    #[error("at most {max} {kind:?} images")]
    TooManyImages {
        /// The kind.
        kind: ImageKind,
        /// Its limit.
        max: u32,
    },
    /// Submit without the required shots.
    #[error("missing required shots")]
    MissingRequiredShot {
        /// The missing kinds.
        missing: Vec<ImageKind>,
    },
    /// A daily budget is used up.
    #[error("{budget:?} budget used up until {resets_at}")]
    BudgetExceeded {
        /// Which budget.
        budget: Budget,
        /// When it resets.
        resets_at: Timestamp,
    },
    /// The same idempotency key is still running.
    #[error("request in progress")]
    RequestInProgress {
        /// When to retry.
        retry_after_s: u32,
    },
    /// The same idempotency key was used with another body.
    #[error("idempotency key reused with a different body")]
    IdempotencyConflict,
    /// A worker's lease is gone ([contracts 5]).
    #[error("lease lost for job {job_id}")]
    LeaseLost {
        /// The job.
        job_id: JobId,
    },
}
```

```rust
impl DomainError {
    /// A shorthand for [`DomainError::InvalidRequest`].
    pub fn invalid(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self::InvalidRequest {
            field: field.into(),
            problem: problem.into(),
        }
    }

    /// A shorthand for `InvalidState` with a hint.
    pub fn invalid_state_hint(hint: InvalidStateHint) -> Self {
        Self::InvalidState(InvalidStateDetails {
            hint: Some(hint),
            ..InvalidStateDetails::default()
        })
    }

    /// The contract code and details.
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFound => ErrorKind::NotFound,
            Self::Forbidden { required } => ErrorKind::Forbidden {
                required_role: *required,
            },
            Self::InvalidRequest { field, problem } => {
                ErrorKind::InvalidRequest(InvalidRequestDetails {
                    field: field.clone(),
                    problem: problem.clone(),
                    plant_ids: None,
                })
            }
            Self::UnknownPlants { plant_ids } => ErrorKind::InvalidRequest(InvalidRequestDetails {
                field: "pins".to_owned(),
                problem: "unknown_plant".to_owned(),
                plant_ids: Some(plant_ids.clone()),
            }),
            Self::InvalidState(details) => ErrorKind::InvalidState(details.clone()),
            Self::Transition(e) => ErrorKind::InvalidState(e.details()),
            Self::TooManyImages { kind, max } => ErrorKind::TooManyImages {
                kind: *kind,
                max: *max,
            },
            Self::MissingRequiredShot { missing } => ErrorKind::MissingRequiredShot {
                missing: missing.clone(),
            },
            Self::BudgetExceeded { budget, resets_at } => ErrorKind::BudgetExceeded {
                budget: *budget,
                resets_at: *resets_at,
            },
            Self::RequestInProgress { .. } => ErrorKind::RequestInProgress,
            Self::IdempotencyConflict => ErrorKind::IdempotencyConflict,
            Self::LeaseLost { job_id } => ErrorKind::LeaseLost { job_id: *job_id },
        }
    }

    /// `retry_after_s`, when the code carries one.
    pub fn retry_after_s(&self) -> Option<u32> {
        match self {
            Self::RequestInProgress { retry_after_s } => Some(*retry_after_s),
            _ => None,
        }
    }
}
```

### Port errors

One enum per kind of port instead of an associated `Error` per trait, because `Arc<dyn Trait>` would otherwise have to name the error type and every caller of a port of one kind handles the same cases. Implementations keep the library error as the `source`, so the log line carries the chain; as the project's `rust-errors` skill asks, `Display` is the variant's own message, never its source's.

```rust
//! The shared error enums of the ports. One enum per kind of boundary rather than an
//! associated `Error` type per trait: `Arc<dyn Trait>` would otherwise have to name the error
//! (`dyn Trait<Error = …>`), and every caller of a port of the same kind handles the same cases.
//! Each implementation maps its library's errors into these, keeping the library error as the
//! `source` for the log.

use crate::{
    connectors::{ConnectorKind, SyncErrorCode},
    error::DomainError,
    id::{JobId, PlanId},
};

/// A boxed error from a library, kept as the source of the log chain.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Which compare-and-set or uniqueness check lost ([database 4]). The handler maps it to the
/// right contract code before `?`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// A plan write lost: `stale_plan`; carries the scan's current plan.
    StalePlan {
        /// The plan that won, if any.
        current: Option<PlanId>,
    },
    /// A step's claim, stamp or run no longer holds: `claim_lost`. The step stops.
    ClaimLost,
    /// A question step's claim no longer holds.
    QuestionStepLost,
    /// The lease token, worker or status no longer matches: `409 lease_lost`.
    LeaseLost {
        /// The job.
        job_id: JobId,
    },
    /// A map upload's `base_version` is stale: `409` with `current_map_version`.
    StaleMap {
        /// The garden's current version.
        current_version: u32,
    },
    /// A record with that key exists (idempotency key, membership, alert key).
    AlreadyExists,
    /// Transaction conflicts still failing after the store's 5 retries.
    Retries,
}

/// The store's error ([database 4], [database 6]).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No such record, or one being deleted, or one outside the proof's garden.
    #[error("record not found")]
    NotFound,
    /// A compare-and-set or uniqueness check lost.
    #[error("conflict: {0:?}")]
    Conflict(Conflict),
    /// A rule checked inside the transaction refused the write (the last owner, a dead plant,
    /// the daily scan limit, unknown map plants).
    #[error("rejected by a rule")]
    Rejected(#[source] DomainError),
    /// `SurrealDB` can't be reached.
    #[error("database unavailable")]
    Unavailable(#[source] BoxError),
    /// A query failed for another reason: a bug or a schema mismatch.
    #[error("query failed: {what}")]
    Query {
        /// The operation, such as `"append event"`.
        what: &'static str,
        /// The SDK error.
        #[source]
        source: BoxError,
    },
}

impl From<DomainError> for StoreError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::NotFound => Self::NotFound,
            other => Self::Rejected(other),
        }
    }
}

/// The object store's error ([storage 4]).
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// No object under that key.
    #[error("object not found")]
    NotFound,
    /// `If-None-Match: *` refused: the key exists (`412`). Keys are never overwritten.
    #[error("object already exists")]
    AlreadyExists,
    /// `RustFS` can't be reached or timed out.
    #[error("storage unavailable")]
    Unavailable(#[source] BoxError),
    /// `RustFS` refused the call (a policy, a bad key).
    #[error("storage refused {operation}")]
    Rejected {
        /// The S3 operation.
        operation: &'static str,
        /// The SDK error.
        #[source]
        source: BoxError,
    },
}

/// An external HTTP provider's error: weather, soil, geocoding, push, the auth provider's
/// Backend API.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The provider knows nothing for this input (an unknown postal code).
    #[error("{provider}: not found")]
    NotFound {
        /// The provider, such as `"geocoder"`.
        provider: &'static str,
    },
    /// Rate limited; retry after the given seconds.
    #[error("{provider}: rate limited")]
    RateLimited {
        /// The provider.
        provider: &'static str,
        /// When to retry.
        retry_after_s: Option<u32>,
    },
    /// Down, timed out or answering 5xx: `503 upstream_unavailable` with `upstream`.
    #[error("{provider}: unavailable")]
    Unavailable {
        /// The provider.
        provider: &'static str,
        /// The client error.
        #[source]
        source: BoxError,
    },
    /// The response didn't parse.
    #[error("{provider}: bad response")]
    BadResponse {
        /// The provider.
        provider: &'static str,
        /// The parse error.
        #[source]
        source: BoxError,
    },
}

impl ProviderError {
    /// The `upstream` of `503 upstream_unavailable`.
    pub const fn provider(&self) -> &'static str {
        match self {
            Self::NotFound { provider }
            | Self::RateLimited { provider, .. }
            | Self::Unavailable { provider, .. }
            | Self::BadResponse { provider, .. } => provider,
        }
    }
}

/// A farm connector vendor's error ([connectors 6]): Slack or Google Drive. Each maps to the
/// fixed code stored in `connector.last_error`; the vendor's response text is never kept,
/// because it may echo record text.
///
/// [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
#[derive(Debug, thiserror::Error)]
pub enum FarmSourceError {
    /// The vendor refused the token (Slack's `invalid_auth` or `token_revoked`, Google's
    /// `invalid_grant`): the connector becomes `reauth_needed`.
    #[error("{vendor}: token refused")]
    AuthRefused {
        /// The vendor.
        vendor: ConnectorKind,
    },
    /// A chosen channel or folder is gone or no longer visible to the token (Slack's
    /// `channel_not_found`, Drive's `404`): nothing runs until the owner changes the sources. A
    /// scope refusal (Slack's `missing_scope`, Drive's `insufficientPermissions`) is
    /// [`Self::AuthRefused`], not this.
    #[error("{vendor}: source {source_id} is missing")]
    SourceMissing {
        /// The vendor.
        vendor: ConnectorKind,
        /// The channel or folder id from the owner's choice.
        source_id: String,
    },
    /// `429` past the pacer: the cursor is saved and the next run continues.
    #[error("{vendor}: rate limited")]
    RateLimited {
        /// The vendor.
        vendor: ConnectorKind,
        /// The vendor's `Retry-After`.
        retry_after_s: Option<u32>,
    },
    /// Down, timed out or answering 5xx: the next attempt is 15 minutes later.
    #[error("{vendor}: unavailable")]
    Unavailable {
        /// The vendor.
        vendor: ConnectorKind,
        /// The client error, without the response body.
        #[source]
        source: BoxError,
    },
    /// The response didn't parse.
    #[error("{vendor}: bad response")]
    BadResponse {
        /// The vendor.
        vendor: ConnectorKind,
        /// The parse error, without the response body.
        #[source]
        source: BoxError,
    },
}

impl FarmSourceError {
    /// The `connector.last_error` code. A response that didn't parse counts as the vendor being
    /// unavailable: the sync tries again later either way.
    pub const fn code(&self) -> SyncErrorCode {
        match self {
            Self::AuthRefused { .. } => SyncErrorCode::AuthRefused,
            Self::SourceMissing { .. } => SyncErrorCode::SourceMissing,
            Self::RateLimited { .. } => SyncErrorCode::RateLimited,
            Self::Unavailable { .. } | Self::BadResponse { .. } => SyncErrorCode::VendorUnavailable,
        }
    }
}

/// A model's error: the LLM API, the VLM sidecar, `ort` sessions, the text embedder.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The adapter doesn't do this (a perception adapter asked for a `vlm` job).
    #[error("{model} does not support {what}")]
    Unsupported {
        /// The model.
        model: &'static str,
        /// The operation.
        what: &'static str,
    },
    /// The model's output failed validation after its one retry ([contracts 6], after
    /// decoding): the job fails with `retryable: true`.
    #[error("{model} returned invalid output")]
    InvalidOutput {
        /// The model.
        model: &'static str,
        /// Why.
        #[source]
        source: BoxError,
    },
    /// Out of memory, a crashed session, a 5xx from the sidecar: worth another attempt.
    #[error("{model} failed")]
    Failed {
        /// The model.
        model: &'static str,
        /// The cause.
        #[source]
        source: BoxError,
    },
    /// The hosted API is down or rate limited: `503 upstream_unavailable` for chat.
    #[error("{model} unavailable")]
    Unavailable {
        /// The model.
        model: &'static str,
        /// The cause.
        #[source]
        source: BoxError,
    },
}

impl ModelError {
    /// Whether a job that failed with this error may be retried ([contracts 5], `retryable`).
    pub const fn retryable(&self) -> bool {
        !matches!(self, Self::Unsupported { .. })
    }
}

/// The auth provider's error.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Missing, malformed, expired or wrongly signed token: `401 unauthenticated`.
    #[error("invalid token")]
    InvalidToken(#[source] Option<BoxError>),
    /// A webhook whose Svix signature or timestamp failed: `401`.
    #[error("invalid webhook signature")]
    InvalidWebhook,
    /// The JWKS couldn't be fetched: `503 upstream_unavailable`.
    #[error("auth provider unavailable")]
    Unavailable(#[source] BoxError),
    /// A valid token for an account that was deleted (its tombstone exists): `401
    /// unauthenticated`, never a new user.
    #[error("account deleted")]
    AccountDeleted,
}

#[cfg(test)]
mod farm_source_tests {
    use super::*;

    #[test]
    fn each_farm_source_error_has_its_stored_code() {
        let missing = FarmSourceError::SourceMissing {
            vendor: ConnectorKind::Slack,
            source_id: "C024BE91L".into(),
        };
        assert_eq!(missing.code(), SyncErrorCode::SourceMissing);
        assert_eq!(missing.code().as_str(), "source_missing");
        let limited = FarmSourceError::RateLimited {
            vendor: ConnectorKind::GoogleDrive,
            retry_after_s: Some(30),
        };
        assert_eq!(limited.code(), SyncErrorCode::RateLimited);
        assert!(matches!(
            limited,
            FarmSourceError::RateLimited {
                retry_after_s: Some(30),
                ..
            }
        ));
        let refused = FarmSourceError::AuthRefused {
            vendor: ConnectorKind::Slack,
        };
        assert_eq!(refused.code(), SyncErrorCode::AuthRefused);
    }
}
```

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

```rust
/// What moves a scan from one status to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanTrigger {
    /// `POST /scans/{id}/submit` on a pending scan: run 1.
    Submit,
    /// `POST /scans/{id}/submit` on a failed scan; allowed only for a retryable failure.
    Resubmit {
        /// The scan's failure.
        failure: ScanFailureCode,
    },
    /// A worker leased the run's first job.
    Claimed,
    /// The `score` step asked a question (at most 2 per run, [`crate::rules::questions`]).
    QuestionAsked,
    /// The `score` step stored the run's diagnosis, or the scheduler continued without an
    /// answer.
    Diagnosed,
    /// The `plan` step stored the run's plan.
    Planned,
    /// The user picked a different species while the run was past `queued`.
    SpeciesChanged,
    /// A run's job ran out of attempts, no plant was found, the photos were unusable, or an
    /// upstream was down.
    Fail,
    /// A late answer, a late photo or a guided check: allowed while `diagnosed` or `planned`,
    /// and changes nothing.
    LateWork,
}

/// The state machine refused a transition: `409 invalid_state` with the current `status` and
/// the `allowed` ones, and `hint: "new_scan"` for a resubmit of a non-retryable failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("scan in status {from:?} can't take {trigger:?}")]
pub struct TransitionError {
    /// The current status.
    pub from: ScanStatus,
    /// What was attempted.
    pub trigger: ScanTrigger,
}
```

```rust
impl ScanTrigger {
    /// The statuses this trigger is allowed from.
    pub const fn allowed_from(self) -> &'static [ScanStatus] {
        use ScanStatus as S;
        match self {
            Self::Submit => &[S::Pending],
            Self::Resubmit { .. } => &[S::Failed],
            Self::Claimed => &[S::Queued],
            Self::QuestionAsked | Self::Diagnosed => &[S::Analysing, S::AwaitingAnswer],
            Self::Planned => &[S::Diagnosed],
            Self::SpeciesChanged => &[S::Analysing, S::AwaitingAnswer, S::Diagnosed, S::Planned],
            Self::Fail => &[S::Queued, S::Analysing, S::AwaitingAnswer, S::Diagnosed],
            Self::LateWork => &[S::Diagnosed, S::Planned],
        }
    }
}
```

```rust
impl ScanStatus {
    /// Applies a trigger: the new status, or why not.
    pub fn apply(self, trigger: ScanTrigger) -> Result<Self, TransitionError> {
        let refuse = Err(TransitionError {
            from: self,
            trigger,
        });
        if !trigger.allowed_from().contains(&self) {
            return refuse;
        }
        Ok(match trigger {
            ScanTrigger::Submit => Self::Queued,
            ScanTrigger::Resubmit { failure } => {
                if !failure.retryable() {
                    return refuse;
                }
                Self::Queued
            }
            ScanTrigger::Claimed | ScanTrigger::SpeciesChanged => Self::Analysing,
            ScanTrigger::QuestionAsked => Self::AwaitingAnswer,
            ScanTrigger::Diagnosed => Self::Diagnosed,
            ScanTrigger::Planned => Self::Planned,
            ScanTrigger::Fail => Self::Failed,
            ScanTrigger::LateWork => self,
        })
    }

    /// A scan at rest has no run work in flight: `planned` or `failed`.
    pub const fn at_rest(self) -> bool {
        matches!(self, Self::Planned | Self::Failed)
    }

    /// The sweep re-claims steps on any scan that isn't `pending` or `failed` ([contracts 3]).
    pub const fn sweepable(self) -> bool {
        !matches!(self, Self::Pending | Self::Failed)
    }
}
```

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

```rust
/// What `POST /scans/{id}/submit` does ([contracts 3], submit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitDecision {
    /// A run starts: `202 Scan`.
    StartRun(RunReason),
    /// Nothing; submit is idempotent: `200 Scan`.
    AlreadySubmitted,
}

/// Decides a submit. `failure` is the scan's failure when it is `failed`.
pub fn decide_submit(
    status: ScanStatus,
    failure: Option<ScanFailureCode>,
) -> Result<SubmitDecision, TransitionError> {
    match (status, failure) {
        (ScanStatus::Pending, _) => Ok(SubmitDecision::StartRun(RunReason::Submit)),
        (ScanStatus::Failed, Some(code)) => status
            .apply(ScanTrigger::Resubmit { failure: code })
            .map(|_| SubmitDecision::StartRun(RunReason::Resubmit)),
        (ScanStatus::Failed, None) => Err(TransitionError {
            from: status,
            trigger: ScanTrigger::Resubmit {
                failure: ScanFailureCode::ProcessingError,
            },
        }),
        _ => Ok(SubmitDecision::AlreadySubmitted),
    }
}

/// How long after its creation a scan still follows a species change ([contracts 3]; open
/// question 6 of contracts 12).
pub const SPECIES_CHANGE_WINDOW: Duration = Duration::from_hours(24);

/// What a species confirmation does to the plant's most recent scan ([contracts 3],
/// `POST /plants/{id}/species`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesFollow {
    /// Too old, failed, or the same species: nothing.
    Nothing,
    /// `pending` or `queued`: the run uses the new species when it starts.
    AtRunStart,
    /// A new run starts from `analysing`.
    NewRun,
}

/// Decides whether the most recent scan follows the species change.
pub fn species_follow(
    status: ScanStatus,
    scan_created_at: Timestamp,
    now: Timestamp,
    differs_from_working_species: bool,
) -> SpeciesFollow {
    if now.since(scan_created_at) > SPECIES_CHANGE_WINDOW || status == ScanStatus::Failed {
        return SpeciesFollow::Nothing;
    }
    match status {
        ScanStatus::Pending | ScanStatus::Queued => SpeciesFollow::AtRunStart,
        _ if differs_from_working_species => SpeciesFollow::NewRun,
        _ => SpeciesFollow::Nothing,
    }
}

/// Only a `pending` scan can be deleted ([contracts 3], `DELETE /scans/{id}`).
pub fn check_deletable(status: ScanStatus) -> Result<(), InvalidStateDetails> {
    if status == ScanStatus::Pending {
        Ok(())
    } else {
        Err(InvalidStateDetails {
            status: Some(status.as_str().to_owned()),
            allowed: Some(vec![ScanStatus::Pending.as_str().to_owned()]),
            ..InvalidStateDetails::default()
        })
    }
}

/// Feedback needs a submitted scan ([contracts 3], feedback).
pub fn check_feedback_allowed(status: ScanStatus) -> Result<(), InvalidStateDetails> {
    if status == ScanStatus::Pending {
        Err(InvalidStateDetails {
            status: Some(status.as_str().to_owned()),
            ..InvalidStateDetails::default()
        })
    } else {
        Ok(())
    }
}
```

### Uploads

[Contracts 3, `POST /scans/{id}/images`](../architecture/contracts.md#post-scansidimages).

| Upload | Allowed when | Route | Kind limit |
|---|---|---|---|
| Plant or soil kind, no `question_id` | `pending` | `RunShot` | `whole_plant` 1, `leaf_closeup` 2, `leaf_underside` 1, `soil_surface` 1, `flower_fruit` 1 |
| `squeeze_test`, `jar_test` | any status but `failed` | `GuidedCheck` | 1 each before submit, none after |
| Plant or soil kind with `question_id` | the question is `open` | `QuestionPhoto` | doesn't count |

```rust
/// 12 MB, read as MiB like the API's body limit ([contracts 2], `upload_too_large`).
pub const IMAGE_MAX_BYTES: u64 = 12 * 1024 * 1024;

/// 4 MB per depth map ([contracts 3], depth).
pub const DEPTH_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// JPEG, PNG, WebP or HEIC ([contracts 1], images); `details.accepted` of `415`.
pub const ACCEPTED_MEDIA_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/heic"];

/// Submit needs these ([contracts 2], `missing_required_shot`).
pub const REQUIRED_SHOTS: [ImageKind; 1] = [ImageKind::WholePlant];

/// Where an accepted upload goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadRoute {
    /// A plant or soil shot of the coming run (the scan is `pending`).
    RunShot,
    /// A new guided check with its own `guided` job.
    GuidedCheck,
    /// A photo for an open `photo_request` question: it becomes `answering`.
    QuestionPhoto,
}

/// Decides where an upload goes, or refuses it.
///
/// - With `question_id`: the question must be `open`, whatever the scan's status; the photo is
///   a plant or soil kind and doesn't count toward kind limits.
/// - A `squeeze_test` or `jar_test`: any status except `failed`.
/// - Any other kind: only while `pending`.
pub fn route_upload(
    kind: ImageKind,
    scan: ScanStatus,
    question: Option<QuestionState>,
) -> Result<UploadRoute, DomainError> {
    if let Some(state) = question {
        if !kind.is_plant_or_soil() {
            return Err(DomainError::invalid(
                "kind",
                "a question photo is a plant or soil shot",
            ));
        }
        return if state == QuestionState::Open {
            Ok(UploadRoute::QuestionPhoto)
        } else {
            Err(DomainError::InvalidState(InvalidStateDetails {
                status: Some(state.as_str().to_owned()),
                allowed: Some(vec![QuestionState::Open.as_str().to_owned()]),
                ..InvalidStateDetails::default()
            }))
        };
    }
    let allowed = if kind.guided().is_some() {
        scan != ScanStatus::Failed
    } else {
        scan == ScanStatus::Pending
    };
    if !allowed {
        return Err(DomainError::InvalidState(InvalidStateDetails {
            status: Some(scan.as_str().to_owned()),
            ..InvalidStateDetails::default()
        }));
    }
    Ok(if kind.guided().is_some() {
        UploadRoute::GuidedCheck
    } else {
        UploadRoute::RunShot
    })
}

/// Checks the kind limit for an upload that counts toward it. `existing` counts the scan's
/// images of that kind that aren't question photos. After submit, guided kinds have no limit.
pub fn check_kind_limit(
    kind: ImageKind,
    scan: ScanStatus,
    existing: u32,
) -> Result<(), DomainError> {
    let limited = scan == ScanStatus::Pending || kind.is_plant_or_soil();
    let max = kind.max_before_submit();
    if limited && existing >= max {
        Err(DomainError::TooManyImages { kind, max })
    } else {
        Ok(())
    }
}

/// Checks that the run's shots include every required kind.
pub fn check_required_shots(present: &[ImageKind]) -> Result<(), DomainError> {
    let missing: Vec<ImageKind> = REQUIRED_SHOTS
        .iter()
        .copied()
        .filter(|k| !present.contains(k))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(DomainError::MissingRequiredShot { missing })
    }
}
```

```rust
impl ImageKind {
    /// The most of this kind a scan holds before submit (contracts 3, `ImageKind`).
    pub const fn max_before_submit(self) -> u32 {
        match self {
            Self::LeafCloseup => 2,
            Self::WholePlant
            | Self::LeafUnderside
            | Self::SoilSurface
            | Self::FlowerFruit
            | Self::SqueezeTest
            | Self::JarTest => 1,
        }
    }

    /// Depth is kept only for plant and soil kinds; sent with a guided check it is dropped
    /// with `not_for_this_kind`.
    pub const fn takes_depth(self) -> bool {
        self.is_plant_or_soil()
    }
}
```

```rust
/// The depth checks that need only sizes and counts. `orientation` is the photo's EXIF
/// orientation (1–8); the mirrored ones (2, 4, 5, 7) can't come from the rear camera.
pub fn check_depth(
    kind: ImageKind,
    photo: (u32, u32),
    depth: (u32, u32),
    depth_bytes: u64,
    orientation: u8,
) -> Result<(), DepthProblem> {
    if !kind.takes_depth() {
        return Err(DepthProblem::NotForThisKind);
    }
    if depth_bytes > DEPTH_MAX_BYTES {
        return Err(DepthProblem::TooLarge);
    }
    if matches!(orientation, 2 | 4 | 5 | 7) || !(1..=8).contains(&orientation) {
        return Err(DepthProblem::InvalidMeta);
    }
    if photo.0 == 0 || photo.1 == 0 || depth.0 == 0 || depth.1 == 0 {
        return Err(DepthProblem::InvalidMeta);
    }
    let photo_aspect = f64::from(photo.0) / f64::from(photo.1);
    let depth_aspect = f64::from(depth.0) / f64::from(depth.1);
    if (photo_aspect / depth_aspect - 1.0).abs() > DEPTH_ASPECT_TOLERANCE {
        return Err(DepthProblem::AspectMismatch);
    }
    Ok(())
}

/// `too_sparse` when fewer than 20% of the depth pixels are non-zero.
pub fn check_depth_coverage(non_zero: u64, total: u64) -> Result<(), DepthProblem> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel counts are far below 2^52"
    )]
    let share = if total == 0 {
        0.0
    } else {
        non_zero as f64 / total as f64
    };
    if share < DEPTH_MIN_COVERAGE {
        Err(DepthProblem::TooSparse)
    } else {
        Ok(())
    }
}

/// The long side to store a photo at: at least 2048 pixels with depth (or its own size when
/// smaller), and `default_long_side` without.
pub fn stored_long_side(original_long_side: u32, has_depth: bool, default_long_side: u32) -> u32 {
    let target = if has_depth {
        DEPTH_PHOTO_MIN_LONG_SIDE.max(default_long_side)
    } else {
        default_long_side
    };
    original_long_side.min(target)
}
```

`check_depth` drops depth with `not_for_this_kind` for guided kinds, `too_large` over 4 MB, `invalid_meta` for a mirrored EXIF orientation (2, 4, 5, 7), and `aspect_mismatch` beyond 1%; `check_depth_coverage` gives `too_sparse` under 20% ([contracts 3, depth](../architecture/contracts.md#post-scansidimages)).

### Questions

```rust
/// At most 2 questions per run, all before the scan is `planned`.
pub const MAX_QUESTIONS_PER_RUN: u32 = 2;

/// The scheduler continues a scan without an answer after 10 minutes.
pub const CONTINUE_AFTER: Duration = Duration::from_mins(10);

/// The stored question states: `open` and `answering` are what the app sees, the rest record
/// how it resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestionState {
    /// Waiting for an answer.
    Open,
    /// A photo for it is being analysed.
    Answering,
    /// Answered by the user or a photo.
    Answered,
    /// A new run, or the scan failing, superseded it.
    Superseded,
    /// The photo sent for it couldn't be analysed.
    Dropped,
}

/// A photo answer's step ([contracts 3]): `segmenting` (the perception job), `measuring`
/// (`api`'s own work), `verifying` (the `verify` job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestionStep {
    /// Its segmentation-only `perception` job is queued or running.
    Segmenting,
    /// `api` measures the photo; recovered by the sweep after 2 minutes.
    Measuring,
    /// Its `verify` job is queued or running.
    Verifying,
}

/// Whether the run may ask another question: fewer than 2 asked, still `analysing` or
/// `awaiting_answer`, and the result ambiguous. Late answers and guided checks never ask.
pub const fn may_ask(asked_this_run: u32, still_in_run: bool) -> bool {
    still_in_run && asked_this_run < MAX_QUESTIONS_PER_RUN
}
```

### Roles

[Contracts 1, roles and access](../architecture/contracts.md#roles-and-access).

```rust
/// Every action of the role matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Read gardens, plants, scans, plans, tasks, timeline, measurements, the garden map.
    Read,
    /// Chat in their own conversations, with the read tools and `remember`.
    Chat,
    /// Scan feedback.
    GiveFeedback,
    /// Create scans, upload, delete a scan before it is submitted, answer questions, confirm
    /// species, add guided soil checks.
    Scan,
    /// Map the garden (from a `LiDAR` device).
    MapGarden,
    /// Chat write tools (create and complete tasks, revise plans) and their cards.
    UseWriteTools,
    /// Create, complete and dismiss tasks.
    ManageTasks,
    /// Create and edit plants.
    EditPlants,
    /// Share their own chat memories with the garden, add garden notes.
    ShareMemories,
    /// Rename the garden, manage invites and members, change roles, delete plants or the garden,
    /// delete shared memories.
    ManageGarden,
}
```

```rust
impl Action {
    /// Every action.
    pub const ALL: [Self; 10] = [
        Self::Read,
        Self::Chat,
        Self::GiveFeedback,
        Self::Scan,
        Self::MapGarden,
        Self::UseWriteTools,
        Self::ManageTasks,
        Self::EditPlants,
        Self::ShareMemories,
        Self::ManageGarden,
    ];

    /// The lowest role allowed: the matrix of contracts 1.
    pub const fn min_role(self) -> Role {
        match self {
            Self::Read | Self::Chat => Role::Viewer,
            Self::GiveFeedback => Role::Reviewer,
            Self::Scan
            | Self::MapGarden
            | Self::UseWriteTools
            | Self::ManageTasks
            | Self::EditPlants
            | Self::ShareMemories => Role::Editor,
            Self::ManageGarden => Role::Owner,
        }
    }
}
```

```rust
impl Role {
    /// Whether this role may do `action`: ranks are ordered, so "editor or above".
    pub fn allows(self, action: Action) -> bool {
        self >= action.min_role()
    }
}
```

| Action | Lowest role |
|---|---|
| Read gardens, plants, scans, plans, tasks, timeline, measurements, the map; chat with the read tools and `remember` | viewer |
| Scan feedback | reviewer |
| Scan, upload, delete a pending scan, answer, confirm species, guided checks; map the garden; write tools and cards; manage tasks; create and edit plants; share memories and add notes | editor |
| Rename the garden, invites, members and roles, delete plants or the garden, delete shared memories | owner |

`leaves_no_owner(owners, member_role, new_role)` refuses demoting or removing the last owner.

### Jobs

[Contracts 5, leases, runs and attempts](../architecture/contracts.md#leases-runs-and-attempts) and [architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback).

```rust
/// A lease lasts 2 minutes; a heartbeat listing the job extends it to 2 minutes from then.
pub const LEASE: Duration = Duration::from_mins(2);

/// Workers heartbeat every 10 seconds.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// A worker whose heartbeat is older than this has its jobs requeued.
pub const WORKER_STALE_AFTER: Duration = Duration::from_secs(30);

/// A job unclaimed this long after `queued_at` becomes claimable by the fallback worker.
pub const FALLBACK_DELAY: Duration = Duration::from_secs(15);

/// After the 3rd attempt the job fails for good.
pub const MAX_ATTEMPTS: u32 = 3;

/// Requeues up to this many are free; every later one counts as an attempt.
pub const FREE_REQUEUES: u32 = 2;
```

```rust
impl JobKind {
    /// `max_runtime` ([contracts 5]): perception 2 minutes, vlm 5, verify and guided 3, twice
    /// that on the fallback worker.
    pub const fn max_runtime(self, on_fallback: bool) -> Duration {
        let secs = match self {
            Self::Perception => 2 * 60,
            Self::Vlm => 5 * 60,
            Self::Verify | Self::Guided => 3 * 60,
        };
        Duration::from_secs(if on_fallback { secs * 2 } else { secs })
    }

    /// Whether the job belongs to a run (a new run cancels it); `guided` jobs belong to the
    /// scan.
    pub const fn belongs_to_run(self) -> bool {
        !matches!(self, Self::Guided)
    }
}
```

```rust
impl Capability {
    /// The kinds this capability claims ([contracts 5]).
    pub const fn kinds(self) -> &'static [JobKind] {
        match self {
            Self::Perception => &[JobKind::Perception],
            Self::Vlm => &[JobKind::Vlm, JobKind::Verify, JobKind::Guided],
            Self::Fallback => &JobKind::ALL,
        }
    }
}
```

```rust
/// The kinds a worker with these capabilities may claim, without repeats, in kind order.
pub fn claimable_kinds(caps: &[Capability]) -> Vec<JobKind> {
    let mut kinds: Vec<JobKind> = caps
        .iter()
        .flat_map(|c| c.kinds().iter().copied())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// What the claim query needs to know about the claimer ([database 4], job claim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimPolicy {
    /// The kinds it may take.
    pub kinds: Vec<JobKind>,
    /// Jobs queued at or before this are eligible: now for GPU workers, now − 15 s for the
    /// fallback.
    pub eligible_before: Timestamp,
    /// The fallback worker takes a photo answer's segmentation job only when no perception
    /// worker has heartbeated for 30 seconds.
    pub hold_question_segmentation: bool,
    /// Whether runtimes are doubled.
    pub on_fallback: bool,
}

/// The claim policy for a worker's capabilities at `now`.
pub fn claim_policy(caps: &[Capability], now: Timestamp) -> ClaimPolicy {
    let on_fallback = caps.contains(&Capability::Fallback);
    ClaimPolicy {
        kinds: claimable_kinds(caps),
        eligible_before: if on_fallback {
            now.minus(FALLBACK_DELAY)
        } else {
            now
        },
        hold_question_segmentation: on_fallback,
        on_fallback,
    }
}

/// The new lease end after a heartbeat: 2 minutes from now, never beyond
/// `claimed_at + max_runtime`.
pub fn extended_lease(now: Timestamp, claimed_at: Timestamp, max_runtime: Duration) -> Timestamp {
    now.plus(LEASE).min(claimed_at.plus(max_runtime))
}

/// `attempts` after a requeue: every requeue after the 2nd counts.
pub const fn attempts_after_requeue(attempts: u32, requeues_after: u32) -> u32 {
    if requeues_after > FREE_REQUEUES {
        attempts + 1
    } else {
        attempts
    }
}

/// Whether a job with `attempts` counted is out of attempts.
pub const fn exhausted(attempts: u32) -> bool {
    attempts >= MAX_ATTEMPTS
}

/// What happens when a job fails for good ([contracts 5]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalFailure {
    /// A run's job: the scan goes to `failed` with `processing_error`, superseding its question.
    FailScan,
    /// A late `verify` job: `question_resolved` with `dropped`; the scan stays.
    DropQuestion,
    /// A `guided` job: `guided_check_done` with a failed check; the scan stays.
    FailGuidedCheck,
    /// A question photo's segmentation job: the photo goes on to `verify` unmeasured.
    ContinueUnmeasured,
}

/// Decides what a final failure does. `late` is true while the scan is `diagnosed` or
/// `planned`; `question_segmentation` marks a perception job with `question_id`.
pub const fn final_failure(kind: JobKind, late: bool, question_segmentation: bool) -> FinalFailure {
    match kind {
        JobKind::Guided => FinalFailure::FailGuidedCheck,
        JobKind::Perception if question_segmentation => FinalFailure::ContinueUnmeasured,
        JobKind::Verify if late => FinalFailure::DropQuestion,
        JobKind::Perception | JobKind::Vlm | JobKind::Verify => FinalFailure::FailScan,
    }
}
```

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

```rust
/// `scan.pipeline_step`: `retrieve`, `score` or `plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineStep {
    /// After the perception result: measurement, context, retrieval, queueing the `vlm` job.
    Retrieve,
    /// Scoring, fusion and the diagnosis, then a question or the plan.
    Score,
    /// The plan writer.
    Plan,
}

/// What a step's commit sets next ([contracts 3], handing over).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handover {
    /// Release the claim: a job is in flight (`retrieve`), a question was asked or nothing is
    /// left (`score`), or the plan is stored (`plan`).
    Release,
    /// Set the claim to this step with a new stamp.
    Next(PipelineStep),
}

/// The next step after a commit: `score` whenever input arrived while the step ran
/// (`rescore_pending`), else the step's own hand-over.
pub const fn handover(rescore_pending: bool, own: Handover) -> Handover {
    if rescore_pending {
        Handover::Next(PipelineStep::Score)
    } else {
        own
    }
}

/// What a `score` step decided, for its hand-over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreOutcome {
    /// A question was asked: the scan goes to `awaiting_answer`.
    AskedQuestion,
    /// A plan must be written (a run's first plan or a re-plan).
    NeedsPlan,
    /// Nothing more (a guided check with an unchanged texture).
    Done,
}
```

| Step | Stale after | Hands over to |
|---|---|---|
| `retrieve` | 2 min | released when the `vlm` job is queued |
| `score` | 2 min | released (a question, or nothing to do), or `plan` |
| `plan` | 5 min | released |
| any, with `rescore_pending` set | | `score` |

### Stages, stream and routing

```rust
/// What the earlier run finished, for a species-change run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EarlierRun {
    /// Plant check and segmentation both finished.
    pub perception_base_done: bool,
    /// Measurement finished (or wasn't needed).
    pub measured: bool,
}

/// The stages a run lists in `run_started.rerun_stages`, in pipeline order.
///
/// - Submit and resubmit: every stage; `measurement` only when a photo has depth.
/// - Species change, when the earlier run finished plant check and segmentation: species,
///   disease, the VLM stages and scoring (with fusion and plan), keeping plant check,
///   segmentation, image embeddings and measurements; measuring again only if the earlier run
///   hadn't finished it. Otherwise every perception stage and measurement again.
/// - With "something else" (`custom_species`), species identification is skipped.
pub fn run_stages(
    species_change: Option<EarlierRun>,
    has_depth: bool,
    custom_species: bool,
) -> Vec<Stage> {
    let keep_base = species_change.is_some_and(|e| e.perception_base_done);
    let measure =
        has_depth && species_change.is_none_or(|e| !e.perception_base_done || !e.measured);
    Stage::ALL
        .into_iter()
        .filter(|stage| match stage {
            Stage::PlantCheck | Stage::Segmentation | Stage::ImageEmbedding => !keep_base,
            Stage::Species => !custom_species,
            Stage::Measurement => measure,
            Stage::Guided => false,
            _ => true,
        })
        .filter(|stage| !matches!(stage, Stage::Verify))
        .collect()
}

/// The perception stages the run's `perception` job asks for.
pub fn perception_stages(stages: &[Stage]) -> Vec<PerceptionStage> {
    PerceptionStage::ALL
        .into_iter()
        .filter(|p| stages.contains(&p.stage()))
        .collect()
}
```

```rust
/// What the stream needs to know about the scan after an event is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamState {
    /// The scan's status.
    pub status: ScanStatus,
    /// A question is `open` or `answering`.
    pub question_unresolved: bool,
    /// At least one guided check is `analysing`.
    pub guided_check_analysing: bool,
    /// A pipeline step holds the scan's claim (`pipeline_step` is set): more events are
    /// coming from it, so the scan isn't at rest even if its status is. This lets a late
    /// answer's `question_resolved` or a guided check's `guided_check_done` be sent before the
    /// `score` step's `diagnosis` without the stream closing in between.
    pub step_claimed: bool,
}
```

```rust
impl StreamState {
    /// A resting point: `planned` or `failed`, no question open or `answering`, and no guided
    /// check `analysing`. The server closes the stream after the event that reaches it, and a
    /// stream opened at rest replays and closes.
    pub const fn at_rest(self) -> bool {
        self.status.at_rest()
            && !self.question_unresolved
            && !self.guided_check_analysing
            && !self.step_claimed
    }
}
```

Photo routing to the VLM ([contracts 5, payloads](../architecture/contracts.md#payloads)) works on borrowed slices and is generic over what it routes:

```rust
/// One candidate image for routing: its kind and its position among the scan's images of
/// that kind (0 for the first `leaf_closeup`, 1 for the second).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutableImage<T> {
    /// Its kind.
    pub kind: ImageKind,
    /// Its rank within its kind, in upload order.
    pub nth_of_kind: u32,
    /// Whatever the caller routes (an id, a `JobImage`).
    pub item: T,
}

/// The findings order: `whole_plant`, first `leaf_closeup`, `leaf_underside`, `soil_surface`,
/// second `leaf_closeup`, `flower_fruit`.
pub const FINDINGS_ORDER: [(ImageKind, u32); 6] = [
    (ImageKind::WholePlant, 0),
    (ImageKind::LeafCloseup, 0),
    (ImageKind::LeafUnderside, 0),
    (ImageKind::SoilSurface, 0),
    (ImageKind::LeafCloseup, 1),
    (ImageKind::FlowerFruit, 0),
];

/// The findings call: the first 4 present, in the findings order.
pub fn findings_images<T: Clone>(images: &[RoutableImage<T>]) -> Vec<T> {
    let mut ranked: Vec<(usize, &RoutableImage<T>)> = images
        .iter()
        .filter(|i| i.kind.is_plant_or_soil())
        .map(|i| (findings_rank(i.kind, i.nth_of_kind), i))
        .filter(|(rank, _)| *rank < FINDINGS_ORDER.len())
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .take(MAX_VLM_IMAGES)
        .map(|(_, i)| i.item.clone())
        .collect()
}

/// The indicators call: each image scores the number of indicators that list its kind; the 4
/// highest are sent, ties broken by the findings order, and `whole_plant` is included whenever
/// any indicator lists it. `first` is a photo that must come first (a `verify` job's new
/// photo); it takes one of the 4 places.
pub fn indicator_images<T: Clone>(
    images: &[RoutableImage<T>],
    indicators: &[&Indicator],
    first: Option<&RoutableImage<T>>,
) -> Vec<T> {
    let score = |kind: ImageKind| {
        indicators
            .iter()
            .filter(|ind| ind.image_kinds.contains(&kind))
            .count()
    };
    let mut ranked: Vec<(usize, usize, &RoutableImage<T>)> = images
        .iter()
        .filter(|i| i.kind.is_plant_or_soil())
        .map(|i| (score(i.kind), findings_rank(i.kind, i.nth_of_kind), i))
        .filter(|(s, _, _)| *s > 0)
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let room = MAX_VLM_IMAGES - usize::from(first.is_some());
    let mut chosen: Vec<&RoutableImage<T>> = ranked.iter().take(room).map(|(_, _, i)| *i).collect();
    let wants_whole = score(ImageKind::WholePlant) > 0;
    let has_whole = chosen.iter().any(|i| i.kind == ImageKind::WholePlant);
    if wants_whole
        && !has_whole
        && let Some((_, _, whole)) = ranked
            .iter()
            .find(|(_, _, i)| i.kind == ImageKind::WholePlant)
    {
        if chosen.len() == room {
            chosen.pop();
        }
        chosen.push(whole);
    }
    first
        .into_iter()
        .chain(chosen)
        .map(|i| i.item.clone())
        .collect()
}
```

### Limits

```rust
/// Daily scan limit per user, counted on `POST /scans` ([contracts 3]).
pub const DAILY_SCAN_LIMIT: u32 = 30;

/// Pending actions expire after 24 hours ([contracts 8]).
pub const PENDING_ACTION_TTL: Duration = Duration::from_hours(24);

/// Invites expire after 7 days ([contracts 9]).
pub const INVITE_TTL: Duration = Duration::from_hours(168);

/// Idempotency keys are kept for 24 hours ([contracts 1]).
pub const IDEMPOTENCY_TTL: Duration = Duration::from_hours(24);

/// Invite-code attempts per user per minute ([contracts 9]).
pub const INVITE_ATTEMPTS_PER_MINUTE: u32 = 10;

/// A chat message's text, in characters ([contracts 8]).
pub const MESSAGE_MAX_CHARS: usize = 2000;

/// Images per chat message ([contracts 8]).
pub const MESSAGE_MAX_IMAGES: usize = 3;

/// Weather cache rows older than this are deleted ([database 4], cleanup).
pub const WEATHER_CACHE_KEEP: Duration = Duration::from_hours(2);

/// Finished jobs older than this are deleted.
pub const FINISHED_JOB_KEEP: Duration = Duration::from_hours(168);

/// Cleanup and deletion batches ([database 4]).
pub const DELETE_BATCH: u32 = 500;

/// Open-Meteo multi-location requests carry at most 50 cells ([contracts 7]).
pub const WEATHER_CELLS_PER_REQUEST: usize = 50;

/// Up to 5 neighbours are given to the plan writer ([contracts 7]).
pub const MAX_PLAN_NEIGHBOURS: usize = 5;

/// Neighbours are pinned within 1.5 m, centre to centre ([contracts 9]).
pub const NEIGHBOUR_RADIUS_M: f64 = 1.5;

/// A plant is crowded when its nearest pin is closer than 0.75 × its spacing.
pub const CROWDING_FACTOR: f64 = 0.75;

/// At most 9 candidates in the pre-VLM set and in a diagnosis.
pub const MAX_CANDIDATES: usize = 9;

/// Every VLM call gets at most 4 images.
pub const MAX_VLM_IMAGES: usize = 4;

/// SSE keepalive comment interval ([contracts 4]).
pub const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

/// An account's tombstone is kept this long after the auth provider confirmed its deletion:
/// far longer than a Clerk session token lives (60 seconds by default), so no token issued
/// before the deletion can outlive it.
pub const TOMBSTONE_KEEP: Duration = Duration::from_hours(24);

/// `SoilGrids` retry after a provider outage ([contracts 9], the soil profile).
pub const SOIL_RETRY_OUTAGE: Duration = Duration::from_mins(15);

/// `SoilGrids` retry after no data at the point and every nearby probe: a null pixel doesn't
/// change on a quick retry.
pub const SOIL_RETRY_NO_DATA: Duration = Duration::from_hours(168);

/// Farm connectors per garden, `deleting` ones left out ([connectors 8], abuse and cost).
pub const MAX_CONNECTORS_PER_GARDEN: u32 = 2;

/// Farm records (chunks) per garden: a sync that would exceed it stops with `too_large`
/// ([connectors 8]).
pub const MAX_FARM_RECORDS_PER_GARDEN: u32 = 10_000;

/// **Sync now** is accepted once per connector in this window (EON decision 12; contracts 2,
/// `rate_limited`).
pub const SYNC_NOW_INTERVAL: Duration = Duration::from_mins(5);

/// Slack messages older than this are deleted by the cleanup; Drive files stay while they are
/// under the folder ([connectors 6]).
pub const FARM_RECORD_KEEP: Duration = Duration::from_hours(180 * 24);

/// A `pending` connector the owner didn't finish is deleted by the cleanup after this
/// ([connectors 5]).
pub const PENDING_CONNECT_TTL: Duration = Duration::from_mins(10);

/// How long to wait before the next `SoilGrids` attempt after this failure: 7 days after
/// `NotFound` (null pixels), 15 minutes after anything else (timeout, connection error, `5xx`,
/// `429`, an unreadable answer). The status is `unavailable` either way.
pub const fn soil_retry_after(error: &ProviderError) -> Duration {
    match error {
        ProviderError::NotFound { .. } => SOIL_RETRY_NO_DATA,
        _ => SOIL_RETRY_OUTAGE,
    }
}

/// Checks a scan count after it was incremented in the scan's transaction ([database 4],
/// limits): over 30 is `429 budget_exceeded` until the next UTC day.
pub fn check_daily_scans(count_after: u32, now: Timestamp) -> Result<(), DomainError> {
    if count_after > DAILY_SCAN_LIMIT {
        Err(DomainError::BudgetExceeded {
            budget: crate::error::Budget::Scans,
            resets_at: next_utc_midnight(now),
        })
    } else {
        Ok(())
    }
}

/// The start of the next UTC day, when daily budgets reset.
pub fn next_utc_midnight(now: Timestamp) -> Timestamp {
    now.utc_day()
        .succ_opt()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map_or(now, |t| Timestamp::from(t.and_utc()))
}

/// Checks a chat message's limits.
pub fn check_message(text: &str, images: usize) -> Result<(), DomainError> {
    let chars = text.chars().count();
    if chars == 0 || chars > MESSAGE_MAX_CHARS {
        return Err(DomainError::invalid("text", "1-2000 characters"));
    }
    if images > MESSAGE_MAX_IMAGES {
        return Err(DomainError::invalid("image_ids", "at most 3 images"));
    }
    Ok(())
}
```

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

```rust
//! The weather thresholds: one definition of frost, heat, a humid day, a humid stretch, heavy
//! rain, after rain and dry soil, read by every crate that judges the weather.
//!
//! | Reader | What it reads |
//! |---|---|
//! | `integrations` (Open-Meteo summary) | `frost_risk` of each forecast day; hours over 90% |
//! | `api` scheduler (forecast alerts and plan triggers, [architecture 8.3]) | every trigger rule |
//! | `pipeline` fusion ([architecture 6.1]) | the humid stretch, frost and hard frost |
//! | `knowledge` weather fit ([architecture 6.3]) | the humid hour (its other numbers are the profiles') |
//!
//! The contracts fix the vocabulary (`frost_risk`, `hours_rh_over_90`, `TriggerKind`) but not
//! the numbers, so the defaults are the backend's proposal (domain.md, open questions). `api`
//! holds the one instance in its configuration and hands it to the others, so the weather
//! cache's `frost_risk` and the triggers read from it can't disagree.

use crate::records::weather::{CurrentWeather, ForecastDay, PastWeather, SoilNow};

/// The thresholds of every weather rule. `Copy`: it is a handful of numbers, passed by value
/// into tasks and borrowed in the pure rules below.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherThresholds {
    /// Frost: a day whose 2 m minimum is at or below this, °C. Above 0 °C because air at 2 m
    /// runs warmer than the ground on a clear night: ground frost forms with the air at 1–2 °C.
    /// Also `frost_risk` of each forecast day, set by `integrations` when it fetches.
    pub frost_min_c: f64,
    /// Hard frost: a frost day at or below this, °C, is `high` `frost_damage` in fusion
    /// (`medium` otherwise).
    pub frost_hard_c: f64,
    /// Heat: a day's maximum at or above this, °C.
    pub heat_max_c: f64,
    /// A humid day: at least this many hours with RH over
    /// [`HUMID_HOUR_RH_PCT`](Self::HUMID_HOUR_RH_PCT) (`hours_rh_over_90`).
    pub humid_day_hours: u32,
    /// A humid stretch: this many humid days in a row.
    pub humid_stretch_days: usize,
    /// Heavy rain: a day's precipitation at or above this, mm.
    pub heavy_rain_mm: f64,
    /// After rain: the last 24 hours' precipitation at or above this, mm.
    pub after_rain_mm: f64,
    /// Dry soil: 0–9 cm moisture below this, m³/m³, with the last day's ET₀ at or above
    /// [`dry_soil_et0_mm`](Self::dry_soil_et0_mm).
    pub dry_soil_m3m3: f64,
    /// See [`dry_soil_m3m3`](Self::dry_soil_m3m3), mm over 24 hours.
    pub dry_soil_et0_mm: f64,
}

impl Default for WeatherThresholds {
    fn default() -> Self {
        Self {
            frost_min_c: 0.5,
            frost_hard_c: -2.0,
            heat_max_c: 30.0,
            humid_day_hours: 10,
            humid_stretch_days: 2,
            heavy_rain_mm: 20.0,
            after_rain_mm: 5.0,
            dry_soil_m3m3: 0.15,
            dry_soil_et0_mm: 3.0,
        }
    }
}

impl WeatherThresholds {
    /// An hour is humid when its RH is over this, %. Fixed, not configurable: the contracts'
    /// field `hours_rh_over_90` names it.
    pub const HUMID_HOUR_RH_PCT: f64 = 90.0;

    /// Whether an hour's relative humidity counts toward `hours_rh_over_90` (strictly over 90%).
    pub fn is_humid_hour(rh_pct: f64) -> bool {
        rh_pct > Self::HUMID_HOUR_RH_PCT
    }

    /// `hours_rh_over_90` of a run of hourly readings.
    pub fn humid_hours(rh_pct: impl IntoIterator<Item = f64>) -> u32 {
        let n = rh_pct
            .into_iter()
            .filter(|&v| Self::is_humid_hour(v))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Whether a 2 m minimum is frost: `frost_risk`.
    pub fn is_frost(&self, t_min_c: f64) -> bool {
        t_min_c <= self.frost_min_c
    }

    /// Whether a 2 m minimum is hard frost.
    pub fn is_hard_frost(&self, t_min_c: f64) -> bool {
        t_min_c <= self.frost_hard_c
    }

    /// A frost day: its stored `frost_risk` (set by this rule when fetched), or its minimum
    /// under this rule, so a cached cell fetched under other settings still counts.
    pub fn is_frost_day(&self, day: &ForecastDay) -> bool {
        day.frost_risk || self.is_frost(day.t_min_c)
    }

    /// A heat day.
    pub fn is_heat_day(&self, day: &ForecastDay) -> bool {
        day.t_max_c >= self.heat_max_c
    }

    /// A heavy-rain day.
    pub fn is_heavy_rain_day(&self, day: &ForecastDay) -> bool {
        day.precip_mm >= self.heavy_rain_mm
    }

    /// A humid day: at least `humid_day_hours` hours over 90%.
    pub fn is_humid_day(&self, day: &ForecastDay) -> bool {
        day.hours_rh_over_90 >= self.humid_day_hours
    }

    /// The index of each day that starts a humid stretch: `humid_stretch_days` humid days in a
    /// row from it. Overlapping stretches each count (three humid days give two starts).
    ///
    /// `'d` is the forecast's borrow. `use<'d>` keeps the iterator from also capturing `&self`'s
    /// lifetime, which edition 2024 would do by default (Edition Guide, RPIT lifetime capture
    /// rules): the thresholds are copied into the closure, so a caller can drop them while it
    /// still holds the iterator.
    pub fn humid_stretch_starts<'d>(
        &self,
        days: &'d [ForecastDay],
    ) -> impl Iterator<Item = usize> + use<'d> {
        let t = *self;
        days.windows(t.humid_stretch_days.max(1))
            .enumerate()
            .filter(move |(_, run)| run.iter().all(|d| t.is_humid_day(d)))
            .map(|(i, _)| i)
    }

    /// Whether the forecast holds a humid stretch.
    pub fn has_humid_stretch(&self, days: &[ForecastDay]) -> bool {
        self.humid_stretch_starts(days).next().is_some()
    }

    /// `past_7_days.longest_humid_run_days`: the longest run of consecutive humid days, from
    /// each past local day's count of hours over 90%, oldest first. Saturates at 255.
    pub fn longest_humid_run(&self, daily_humid_hours: impl IntoIterator<Item = u32>) -> u8 {
        let (mut best, mut run) = (0_u8, 0_u8);
        for hours in daily_humid_hours {
            run = if hours >= self.humid_day_hours {
                run.saturating_add(1)
            } else {
                0
            };
            best = best.max(run);
        }
        best
    }

    /// Whether the past week held a humid stretch.
    pub fn had_humid_stretch(&self, past: &PastWeather) -> bool {
        usize::from(past.longest_humid_run_days) >= self.humid_stretch_days.max(1)
    }

    /// A humid stretch forecast or in the past 7 days: the fungal-spread rule of crowding and
    /// fusion ([contracts 9], crowding; architecture 6.8).
    pub fn humid_stretch_now_or_past(&self, past: &PastWeather, forecast: &[ForecastDay]) -> bool {
        self.had_humid_stretch(past) || self.has_humid_stretch(forecast)
    }

    /// After rain: the last 24 hours' precipitation.
    pub fn is_after_rain(&self, current: &CurrentWeather) -> bool {
        current.precip_mm_last_24h >= self.after_rain_mm
    }

    /// Dry soil: low 0–9 cm moisture while the air is drawing water out.
    pub fn is_dry_soil(&self, soil: &SoilNow) -> bool {
        soil.moisture_0_9cm_m3m3 < self.dry_soil_m3m3
            && soil.et0_mm_last_24h >= self.dry_soil_et0_mm
    }
}
```

The defaults are a decision, so a test pins every number; changing one is a design change that shows up in review:

```rust
/// The defaults are a decision recorded in domain.md; changing one is a design change, not
/// a refactor, so this test pins every number.
#[test]
fn defaults_are_pinned() {
    let t = WeatherThresholds::default();
    assert_eq!(
        t,
        WeatherThresholds {
            frost_min_c: 0.5,
            frost_hard_c: -2.0,
            heat_max_c: 30.0,
            humid_day_hours: 10,
            humid_stretch_days: 2,
            heavy_rain_mm: 20.0,
            after_rain_mm: 5.0,
            dry_soil_m3m3: 0.15,
            dry_soil_et0_mm: 3.0,
        }
    );
    assert!((WeatherThresholds::HUMID_HOUR_RH_PCT - 90.0).abs() < f64::EPSILON);
}
```

---

## 8. Authorization proofs

[Contracts 1, roles and access](../architecture/contracts.md#roles-and-access); [architecture 4, authorization model](../architecture/README.md#authorization-model).

```rust
//! Authorization as types ([contracts 1], roles and access; architecture 4).
//!
//! A garden-scoped repository call takes a [`Membership<R>`]: a proof that a user is a member of
//! a garden with at least role `R`. The proof can only be produced by the checks in this module
//! ([`check_membership`], [`check_access`], [`check_conversation`]), which ask the store's
//! [`AccessRepo`] for the stored role. Its constructor is private to `domain`, it has no
//! `Default` and no `Deserialize`, and its fields are private, so a handler in another crate
//! cannot build one by hand. "Editor or above" is therefore checked where the handler is
//! written: a handler that holds a `Membership<Viewer>` and calls a method that takes
//! `Membership<Editor>` does not compile.
//!
//! ```compile_fail,E0308
//! use domain::authz::{Editor, Membership, Viewer};
//! fn create_plant(_: Membership<Editor>) {}
//! fn handler(proof: Membership<Viewer>) {
//!     create_plant(proof); // expected `Membership<Editor>`, found `Membership<Viewer>`
//! }
//! ```
//!
//! A downgrade is free and an upgrade doesn't exist at compile time:
//!
//! ```compile_fail,E0277
//! use domain::authz::{Editor, Membership, Owner};
//! fn handler(proof: Membership<Editor>) -> Membership<Owner> {
//!     proof.at_least::<Owner>() // `Editor: AtLeast<Owner>` is not satisfied
//! }
//! ```
//!
//! Nor can a proof be built outside `domain`:
//!
//! ```compile_fail,E0624
//! use domain::authz::{Membership, Owner};
//! use domain::wire::Role;
//! fn forge(user: domain::id::UserId, garden: domain::id::GardenId) -> Membership<Owner> {
//!     Membership::new(user, garden, Role::Owner) // associated function `new` is private
//! }
//! ```
//!
//! The non-member case is always `404 not_found`; a member whose role is too low gets
//! `403 forbidden` with `required_role`.
//!
//! **Why the check lives here and not in `store`.** `store` depends on `domain`, never the
//! other way, so `domain` cannot name `store` to grant it a constructor. A `pub` constructor
//! hidden behind a "sealed" trait would still be callable by any crate. Keeping the constructor
//! private and the check in `domain` makes every proof come from one function that reads the
//! role through a port. The remaining way to forge one is to write an `AccessRepo` that lies,
//! which is a visible trait implementation, not an accident in a handler; only `store` and test
//! fakes implement it.

use std::marker::PhantomData;

use crate::{
    error::DomainError,
    id::{AuthId, ConversationId, GardenId, UserId},
    ports::{AccessRepo, AuthError, AuthVerifier, GardenResource, IdentityRepo, StoreError},
    wire::{Role, Timestamp},
};

mod sealed {
    pub trait Sealed {}
}

/// A minimum role as a type. Sealed: exactly the four markers below.
pub trait MinRole: sealed::Sealed + Copy + std::fmt::Debug + Eq + Send + Sync + 'static {
    /// The role.
    const ROLE: Role;
}

/// Read and chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Viewer;
/// Viewer plus scan feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reviewer;
/// Scan, answer, manage tasks and plants, map, write tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Editor;
/// Manage the garden and its members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Owner;

impl sealed::Sealed for Viewer {}
impl sealed::Sealed for Reviewer {}
impl sealed::Sealed for Editor {}
impl sealed::Sealed for Owner {}
impl MinRole for Viewer {
    const ROLE: Role = Role::Viewer;
}
impl MinRole for Reviewer {
    const ROLE: Role = Role::Reviewer;
}
impl MinRole for Editor {
    const ROLE: Role = Role::Editor;
}
impl MinRole for Owner {
    const ROLE: Role = Role::Owner;
}

/// `Self` is at least `R`: the compile-time "or above". Implemented for every pair where
/// `Self::ROLE >= R::ROLE`, and only those, so `Membership<Owner>` downgrades to
/// `Membership<Editor>` and never the other way.
pub trait AtLeast<R: MinRole>: MinRole {}

macro_rules! at_least {
    ($($hi:ty => [$($lo:ty),*]);* $(;)?) => {
        $($(impl AtLeast<$lo> for $hi {})*)*
    };
}
at_least! {
    Viewer => [Viewer];
    Reviewer => [Viewer, Reviewer];
    Editor => [Viewer, Reviewer, Editor];
    Owner => [Viewer, Reviewer, Editor, Owner];
}

/// A signed-in user: produced only by [`authenticate`], from a verified token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUser {
    id: UserId,
    auth_id: AuthId,
    token_expires_at: Timestamp,
}

impl CurrentUser {
    /// The user.
    pub fn id(&self) -> UserId {
        self.id
    }

    /// The provider's id.
    pub fn auth_id(&self) -> &AuthId {
        &self.auth_id
    }

    /// When the session token expires.
    pub fn token_expires_at(&self) -> Timestamp {
        self.token_expires_at
    }
}

/// Proof that `user` is a member of `garden` with a role of at least `R`. `Copy`: it is two
/// ULIDs and a role, passed by value to repository calls. The `PhantomData<fn() -> R>` marker
/// holds no `R`, so the proof is `Send + Sync + 'static` whatever `R` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Membership<R: MinRole> {
    user: UserId,
    garden: GardenId,
    role: Role,
    min: PhantomData<fn() -> R>,
}

impl<R: MinRole> Membership<R> {
    /// Private to `domain`: only the checks below build proofs.
    const fn new(user: UserId, garden: GardenId, role: Role) -> Self {
        Self {
            user,
            garden,
            role,
            min: PhantomData,
        }
    }

    /// The member.
    pub const fn user(&self) -> UserId {
        self.user
    }

    /// The garden.
    pub const fn garden(&self) -> GardenId {
        self.garden
    }

    /// The member's actual role, which may be above `R`.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// The compile-time downgrade: an owner's proof works wherever an editor's is asked for.
    pub const fn at_least<R2: MinRole>(self) -> Membership<R2>
    where
        R: AtLeast<R2>,
    {
        Membership::new(self.user, self.garden, self.role)
    }

    /// The run-time upgrade, for a call whose role depends on the request (removing a member is
    /// allowed for an owner or the member themselves): checks the actual role.
    pub fn require<R2: MinRole>(self) -> Result<Membership<R2>, AccessError> {
        if self.role >= R2::ROLE {
            Ok(Membership::new(self.user, self.garden, self.role))
        } else {
            Err(AccessError::Forbidden { required: R2::ROLE })
        }
    }
}

/// Proof that `membership.user()` owns `conversation`, which belongs to
/// `membership.garden()` ([contracts 8], access). Anyone else gets `404`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationOwner<R: MinRole> {
    membership: Membership<R>,
    conversation: ConversationId,
}

impl<R: MinRole> ConversationOwner<R> {
    /// The owner's membership.
    pub const fn membership(&self) -> Membership<R> {
        self.membership
    }

    /// The conversation.
    pub const fn conversation(&self) -> ConversationId {
        self.conversation
    }

    /// The compile-time downgrade.
    pub const fn at_least<R2: MinRole>(self) -> ConversationOwner<R2>
    where
        R: AtLeast<R2>,
    {
        ConversationOwner {
            membership: self.membership.at_least(),
            conversation: self.conversation,
        }
    }

    /// The run-time upgrade: chat write tools and cards need an editor.
    pub fn require<R2: MinRole>(self) -> Result<ConversationOwner<R2>, AccessError> {
        Ok(ConversationOwner {
            membership: self.membership.require()?,
            conversation: self.conversation,
        })
    }
}

/// Why access was refused, or the lookup failed.
#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    /// Not a member, not the conversation's owner, or no such record: always `404`.
    #[error("not found")]
    NotFound,
    /// A member below the required role: `403` with `required_role`.
    #[error("requires the {required} role")]
    Forbidden {
        /// The required role.
        required: Role,
    },
    /// The token was refused: `401`.
    #[error("unauthenticated")]
    Unauthenticated(#[source] AuthError),
    /// The lookup failed.
    #[error("access lookup failed")]
    Store(#[from] StoreError),
}

impl AccessError {
    /// The rule failure, when it is one.
    pub fn domain(&self) -> Option<DomainError> {
        match self {
            Self::NotFound => Some(DomainError::NotFound),
            Self::Forbidden { required } => Some(DomainError::Forbidden {
                required: *required,
            }),
            Self::Unauthenticated(_) | Self::Store(_) => None,
        }
    }
}

/// Verifies a bearer token and maps its subject to a user, creating the user on first sight
/// (architecture 4, identity: an upsert on `auth_id`, racing the webhook safely).
pub async fn authenticate(
    verifier: &dyn AuthVerifier,
    identity: &dyn IdentityRepo,
    bearer: &str,
    now: Timestamp,
) -> Result<CurrentUser, AccessError> {
    let token = verifier
        .verify(bearer, now)
        .await
        .map_err(AccessError::Unauthenticated)?;
    // A tombstoned account (deleted, its session token still valid) is refused by the store
    // with `NotFound`: `401`, never a new user.
    let id = match identity
        .upsert_user(&token.auth_id, token.display_name.clone())
        .await
    {
        Err(StoreError::NotFound) => {
            return Err(AccessError::Unauthenticated(AuthError::AccountDeleted));
        }
        other => other?,
    };
    Ok(CurrentUser {
        id,
        auth_id: token.auth_id,
        token_expires_at: token.expires_at,
    })
}

/// The membership check for a garden id from the request ([database 4], membership: a direct
/// read of `member_of:[user, garden]`).
pub async fn check_membership<R: MinRole>(
    access: &dyn AccessRepo,
    user: &CurrentUser,
    garden: GardenId,
) -> Result<Membership<R>, AccessError> {
    check_membership_of(access, user.id, garden).await
}

/// The same check for a user whose identity comes from a verified auth webhook rather than a
/// session token: the account-deletion cascade started by `user.deleted` (architecture 5,
/// deletion), which has no [`CurrentUser`]. The role is still read from the store, so the proof
/// still says what the store says.
pub async fn check_membership_of<R: MinRole>(
    access: &dyn AccessRepo,
    user: UserId,
    garden: GardenId,
) -> Result<Membership<R>, AccessError> {
    let role = access
        .role_in(user, garden)
        .await?
        .ok_or(AccessError::NotFound)?;
    Membership::<Viewer>::new(user, garden, role).require()
}

/// The membership check for a record id from the request (a plant, scan, task, memory, invite
/// or pending action): the record's garden is looked up first, skipping records being deleted,
/// so a record the caller may not see and a missing one both give `404`.
pub async fn check_access<R: MinRole>(
    access: &dyn AccessRepo,
    user: &CurrentUser,
    resource: GardenResource,
) -> Result<Membership<R>, AccessError> {
    let garden = access
        .garden_of(resource)
        .await?
        .ok_or(AccessError::NotFound)?;
    check_membership(access, user, garden).await
}

/// The conversation check ([contracts 8]): the caller owns the conversation **and** is a member
/// of its garden with at least `R`; anyone else gets `404`.
pub async fn check_conversation<R: MinRole>(
    access: &dyn AccessRepo,
    user: &CurrentUser,
    conversation: ConversationId,
) -> Result<ConversationOwner<R>, AccessError> {
    let owner = access
        .conversation_owner(conversation)
        .await?
        .ok_or(AccessError::NotFound)?;
    if owner.user != user.id {
        return Err(AccessError::NotFound);
    }
    let membership = check_membership(access, user, owner.garden).await?;
    Ok(ConversationOwner {
        membership,
        conversation,
    })
}
```

**How the guarantee holds across crates.** `Membership::new` is private to `domain`. The only functions that call it are `check_membership`, `check_membership_of`, `check_access` and `check_conversation`, which read the stored role through the `AccessRepo` port and refuse with `404` (not a member) or `403` (role too low). A handler in `api` therefore gets a proof only by running the check, and the signature of the repository method it calls decides the minimum role at compile time. `at_least` is the free downgrade (an owner's proof works where an editor's is asked for), and `require` the run-time upgrade for calls whose role depends on the request, such as removing a member (an owner, or the member themselves). `check_membership_of` takes a bare `UserId` instead of a `CurrentUser`, for the account-deletion cascade started by a verified `user.deleted` webhook, which has no session; it reads the role from the store all the same.

**Why not a constructor the store calls.** `store` depends on `domain`, so `domain` can't name `store` to give it a private constructor, and a "sealed" `pub` constructor is still callable by every crate. A witness type the store produces would have to be defined in `domain` with a public constructor too. Putting the check itself in `domain`, behind a port, means every proof comes from one function. What remains forgeable is an `AccessRepo` implementation that lies: a visible trait implementation that only `store` and test fakes have, not an accident in a handler.

**Trade-offs.** The check is one read (`member_of:[user, garden]`, [database 4, membership](../database/README.md#membership-)) before the repository call, not inside its transaction, so a member removed between the two can complete one in-flight request. Repository methods still scope every query to `membership.garden()`, so a proof for one garden never reaches another garden's records.

---

## 9. Boundary traits

[Architecture 2, design principles](../architecture/README.md#design-principles): a trait at every external boundary. Every trait is `Send + Sync` and dyn compatible, so `api` and `worker` hold them as `Arc<dyn Trait>`; async methods use `#[async_trait]` (section 10). With the `mock` feature each has a `mockall` mock (`MockStore` doesn't exist: mock the area traits).

```rust
//! Every boundary trait ([architecture 2], "a trait at every external boundary"): the leaf
//! crates implement them, the binaries wire them, and every other crate depends only on these.
//!
//! **Dispatch.** Runtime-swappable adapters are held as `Arc<dyn Trait>`, so every trait here
//! is dyn compatible and `Send + Sync`. A trait with a native `async fn` (or any
//! `-> impl Trait` method) is not dyn compatible on Rust 1.99.1 (`E0038`; Rust Reference,
//! "Dyn compatibility"), so async methods use `#[async_trait]`, which rewrites each into a
//! method returning `Pin<Box<dyn Future + Send + 'async_trait>>`. Synchronous ports (`Clock`,
//! `IdGen`, `UrlSigner`, `SpeciesPhotos`, `SpotSegmenter`, `WebhookVerifier`) stay plain. With
//! the `mock` feature, `mockall` generates a `Mock<Trait>` for every trait except `ChatModel`,
//! whose stream borrows its request.

mod auth;
mod beliefs;
mod blob;
mod errors;
mod farm;
mod knowledge;
mod models;
mod providers;
pub mod store;
mod system;

pub use auth::*;
pub use beliefs::*;
pub use blob::*;
pub use errors::*;
pub use farm::*;
pub use knowledge::*;
pub use models::*;
pub use providers::*;
pub use store::*;
pub use system::*;
```

### Time, ids, URL signing and species photos

```rust
//! Synchronous ports every crate shares: time, ids, URL signing and the species photo index.

use ulid::Ulid;

use crate::{
    garden::species::SpeciesPhoto,
    id::{Id, SpeciesId, Table},
    url::SigningInput,
    wire::Timestamp,
};

/// Wall-clock time. Every reading goes through this port so tests control time
/// (convention 10). Monotonic waits use tokio's clock, which tests pause.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait Clock: Send + Sync {
    /// Now, in UTC.
    fn now(&self) -> Timestamp;
}

/// Record keys ([database 2]: ULID string keys generated in Rust).
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait IdGen: Send + Sync {
    /// A new ULID. `ulid` 3 spells it `Ulid::generate()`.
    fn ulid(&self) -> Ulid;
}

/// Typed ids from any [`IdGen`], including `dyn IdGen`. A generic method can't be called
/// through `dyn` (it isn't dispatchable, Rust Reference, dyn compatibility), so it lives in a
/// blanket extension trait over `?Sized` generators instead of in `IdGen` itself.
pub trait IdGenExt: IdGen {
    /// A new id of table `T`.
    fn id<T: Table>(&self) -> Id<T> {
        Id::from_ulid(self.ulid())
    }
}
impl<G: IdGen + ?Sized> IdGenExt for G {}

/// The HMAC behind signed image URLs ([storage 3]). Implemented in `api` with the server
/// secret; `domain` assembles and checks the URL around it ([`crate::url::Signer`],
/// [`crate::url::verify_signed`]).
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UrlSigner: Send + Sync {
    /// HMAC-SHA256 of `input.canonical()`, lowercase hex.
    fn signature(&self, input: &SigningInput) -> String;
    /// Compares `sig` with the expected signature in constant time.
    fn verify(&self, input: &SigningInput, sig: &str) -> bool;
    /// The API's public origin, such as `"https://api.example.org"`, prefixed to every URL.
    fn public_base(&self) -> &str;
}

/// The species reference photos, in seed order ([contracts 9], species). Loaded once at start
/// from the knowledge base and held in memory, because signing is synchronous and runs for
/// every candidate in every response and replayed event.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SpeciesPhotos: Send + Sync {
    /// The species' photos, at most 3; empty when it has none.
    fn photos(&self, species: &SpeciesId) -> Vec<SpeciesPhoto>;
}
```

### Auth provider

```rust
//! The auth provider (Clerk, with Svix webhooks): verifying session tokens and webhooks, and
//! deleting the account at the provider.

use crate::{
    garden::webhook::{AuthWebhookEvent, WebhookHeaders},
    id::AuthId,
    ports::{AuthError, ProviderError},
    wire::Timestamp,
};

/// A verified session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    /// The `sub` claim.
    pub auth_id: AuthId,
    /// A display name, when the token carries one; the user record's default.
    pub display_name: Option<String>,
    /// The `exp` claim.
    pub expires_at: Timestamp,
}

/// Verifies `Authorization: Bearer <Clerk session token>` against the provider's JWKS
/// ([contracts 1], authentication headers; architecture 4, identity).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AuthVerifier: Send + Sync {
    /// Checks the signature, issuer, `azp`, `exp` and `nbf` at `now`. Fetches and caches the
    /// JWKS, refreshing on an unknown `kid`.
    async fn verify(&self, bearer: &str, now: Timestamp) -> Result<VerifiedToken, AuthError>;
}

/// Verifies a Svix-delivered webhook before the body is parsed ([contracts 9], auth webhook):
/// the signature over `id.timestamp.body`, and a timestamp within 5 minutes of `now`.
/// Synchronous: an HMAC over a few kilobytes.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait WebhookVerifier: Send + Sync {
    /// Verifies, then parses `user.created` and `user.deleted`; other event types are
    /// `Ok(None)`.
    fn verify<'a>(
        &self,
        headers: WebhookHeaders<'a>,
        body: &'a [u8],
        now: Timestamp,
    ) -> Result<Option<AuthWebhookEvent>, AuthError>;
}

/// The provider's Backend API, for the last step of account deletion (architecture 5,
/// deletion step 5): retried until it succeeds.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AuthAdmin: Send + Sync {
    /// Deletes the provider account. An account that is already gone is success, so retries are
    /// idempotent.
    async fn delete_account(&self, auth_id: &AuthId) -> Result<(), ProviderError>;
}
```

### Object storage

[Storage 2](../storage/README.md#2-bucket-keys-and-access), [storage 4](../storage/README.md#4-writes-and-deletes), [storage 5](../storage/README.md#5-rust-client).

```rust
//! Object storage ([storage 2], [storage 4], [storage 5]): the key layout and the
//! `BlobStore` port the `blob` crate implements over `RustFS`.

use std::fmt;

use bytes::Bytes;
use futures::stream::BoxStream;

use crate::{
    id::{ChatImageId, ConversationId, GardenId, ImageId, MaskId, ScanId, SpeciesId},
    ports::BlobError,
    url::PhotoHash,
    wire::Timestamp,
};

/// An object key, built only from typed ids, so every segment under `gardens/` is a
/// 26-character ULID and never user input ([storage 2]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey(String);

impl ObjectKey {
    /// `gardens/{garden}/scans/{scan}/{image}.webp`
    pub fn scan_image(garden: GardenId, scan: ScanId, image: ImageId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/{image}.webp"))
    }

    /// `gardens/{garden}/scans/{scan}/masks/{mask}.png`
    pub fn mask(garden: GardenId, scan: ScanId, mask: MaskId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/masks/{mask}.png"))
    }

    /// `gardens/{garden}/scans/{scan}/depth/{image}.png`: never served.
    pub fn depth(garden: GardenId, scan: ScanId, image: ImageId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/depth/{image}.png"))
    }

    /// `gardens/{garden}/scans/{scan}/depth_confidence/{image}.png`: never served.
    pub fn depth_confidence(garden: GardenId, scan: ScanId, image: ImageId) -> Self {
        Self(format!(
            "gardens/{garden}/scans/{scan}/depth_confidence/{image}.png"
        ))
    }

    /// `gardens/{garden}/chat/{conversation}/{image}.webp`
    pub fn chat_image(garden: GardenId, conversation: ConversationId, image: ChatImageId) -> Self {
        Self(format!("gardens/{garden}/chat/{conversation}/{image}.webp"))
    }

    /// `knowledge/species/{species}/{hash}.webp`: written by the knowledge loader only.
    pub fn species_photo(species: &SpeciesId, hash: PhotoHash) -> Self {
        Self(format!("knowledge/species/{species}/{hash}.webp"))
    }

    /// A key read back from a record (`scan_image.storage_key` and the like). Accepts only the
    /// two top-level prefixes and no empty, `.` or `..` segment.
    pub fn from_stored(key: &str) -> Option<Self> {
        let valid_root = key.starts_with("gardens/") || key.starts_with("knowledge/");
        let valid_segments = key
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..");
        (valid_root && valid_segments).then(|| Self(key.to_owned()))
    }

    /// The key.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the key is under `prefix`.
    pub fn is_under(&self, prefix: &ObjectPrefix) -> bool {
        self.0.starts_with(&prefix.0)
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A key prefix, ending in `/`, for listing and prefix deletes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectPrefix(String);

impl ObjectPrefix {
    /// Everything of a garden: deleting a garden is one prefix delete.
    pub fn garden(garden: GardenId) -> Self {
        Self(format!("gardens/{garden}/"))
    }

    /// Everything of a scan: a plant's objects are deleted scan by scan.
    pub fn scan(garden: GardenId, scan: ScanId) -> Self {
        Self(format!("gardens/{garden}/scans/{scan}/"))
    }

    /// A conversation's images.
    pub fn chat(garden: GardenId, conversation: ConversationId) -> Self {
        Self(format!("gardens/{garden}/chat/{conversation}/"))
    }

    /// The prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The content types the API stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// Photos.
    Webp,
    /// Masks and depth maps.
    Png,
}

impl ContentType {
    /// The MIME type.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Webp => "image/webp",
            Self::Png => "image/png",
        }
    }
}

/// An object body as a stream of chunks. `'static` and `Send` so `api` can hand it to the
/// response body and move it across tasks; independent of `aws_smithy_types::ByteStream` so no
/// crate but `blob` sees AWS types.
pub type ByteStream = BoxStream<'static, Result<Bytes, BlobError>>;

/// A fetched object: its metadata and its streaming body.
pub struct BlobObject {
    /// `Content-Type`.
    pub content_type: String,
    /// `Content-Length`, when known.
    pub length: Option<u64>,
    /// `ETag`, sent to the client.
    pub etag: Option<String>,
    /// The body, streamed, never buffered.
    pub body: ByteStream,
}

impl fmt::Debug for BlobObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlobObject")
            .field("content_type", &self.content_type)
            .field("length", &self.length)
            .field("etag", &self.etag)
            .finish_non_exhaustive()
    }
}

/// A listed object, for the orphan sweep ([storage 4]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Its key.
    pub key: ObjectKey,
    /// When it was written.
    pub last_modified: Timestamp,
    /// Its size.
    pub size: u64,
}

/// What a batch delete removed and what it couldn't ([storage 4], deletes: check the `Errors`
/// list of every `DeleteObjects` response).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeleteReport {
    /// Keys deleted (a missing key counts: deleting it is a no-op).
    pub deleted: u64,
    /// Keys `RustFS` reported as failed; the caller retries.
    pub failed: Vec<ObjectKey>,
}

/// The object store ([storage 5]: "a small `ObjectStore` trait (`put`, `get_stream`,
/// `delete_prefix`)"). Deletes must never be cancelled mid-call on `RustFS` 1.0.1 ([storage 4]):
/// callers run them in a detached task with a 120-second timeout, and implementations don't
/// race them against anything.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// `PutObject` with `If-None-Match: *`: an existing key is [`BlobError::AlreadyExists`].
    async fn put(
        &self,
        key: &ObjectKey,
        body: Bytes,
        content_type: ContentType,
    ) -> Result<(), BlobError>;

    /// `GetObject`, streamed.
    async fn get(&self, key: &ObjectKey) -> Result<BlobObject, BlobError>;

    /// `GetObject`, buffered: for the measurement stage's depth maps and the face blur, which
    /// need the whole image.
    async fn get_bytes(&self, key: &ObjectKey) -> Result<Bytes, BlobError>;

    /// `DeleteObjects` in batches of up to 1000.
    async fn delete_keys(&self, keys: &[ObjectKey]) -> Result<DeleteReport, BlobError>;

    /// `ListObjectsV2` (paginated), then `DeleteObjects` in batches of up to 1000.
    async fn delete_prefix(&self, prefix: &ObjectPrefix) -> Result<DeleteReport, BlobError>;

    /// `ListObjectsV2`, every page, for the daily orphan sweep.
    async fn list(&self, prefix: &ObjectPrefix) -> Result<Vec<ObjectInfo>, BlobError>;
}
```

### Models

`VisionModel` has one method per job kind rather than one generic `run(&JobSpec)`: each kind has its own payload and result type, so the compiler checks that a `vlm` payload produces a `VlmResult`, and the worker matches `JobSpec` once. An adapter returns `ModelError::Unsupported` for the kinds its capability doesn't claim; the fallback adapter answers all four.

`TextEmbedder::embed` takes an `EmbedMode` on every call. arctic-embed-xs is an asymmetric retrieval model: a query gets an instruction prefix and a document none ([contracts 10, checks and generated files](../architecture/contracts.md#checks-and-generated-files); [architecture 10](../architecture/README.md#10-models-and-gpu-placement)). Before v0.2 the port had no mode, so `vision` offered two `TextEmbedder` instances over one set of sessions (`ArcticEmbedder::queries()` and `.documents()`) and `api` had to wire the right one into each consumer (the agent held two). A wrong wiring still returns vectors, only worse matches, so nothing would have caught it. With the mode in the call, one `Arc<dyn TextEmbedder>` serves `api` (garden notes: `Document`), the pipeline (memory summaries: `Document`) and the agent (the user's message and search queries: `Query`; memories it saves: `Document`), and the choice sits at the line that knows what the text is. A model without a prefix ignores the mode. `EmbedMode` is `Copy`, so the adapter moves it into its `spawn_blocking` closure with the owned texts.

```rust
//! Model ports: vision on the workers, species identification, the text classifier, text
//! embeddings, the chat/LLM API and the spot segmenter used by the measurement stage.

use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use futures::stream::BoxStream;

use crate::{
    chat::Usage,
    id::{JobId, ScanId, SpeciesId},
    jobs::{
        GuidedPayload, GuidedResult, JobImage, PerceptionPayload, PerceptionResult, SpeciesResult,
        VerifyPayload, VerifyResult, VlmPayload, VlmResult, WorkerEvent,
    },
    ports::{BoxError, ModelError},
    wire::Run,
};

// ---------------------------------------------------------------------------------------------
// Vision (workers)
// ---------------------------------------------------------------------------------------------

/// The job a model call belongs to, for logs and progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobContext {
    /// The job.
    pub job_id: JobId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Its run; absent for guided jobs.
    pub run: Option<Run>,
    /// 1 on the first try.
    pub attempt: u32,
}

/// An image the worker downloaded from its `JobImage.url`, still encoded (WebP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedImage {
    /// The payload's description of it.
    pub image: JobImage,
    /// The encoded bytes.
    pub bytes: Bytes,
}

/// Why progress couldn't be sent.
#[derive(Debug, thiserror::Error)]
pub enum ProgressError {
    /// `409 lease_lost`: the job was requeued, expired or cancelled; stop now.
    #[error("lease lost")]
    LeaseLost,
    /// The API couldn't be reached; the worker keeps going and `complete` repeats every stage.
    #[error("progress not delivered")]
    Transport(#[source] BoxError),
}

/// Where a model sends stage events and partial results while a job runs
/// (`POST /internal/jobs/{id}/events`, [contracts 5]). Passed as `Arc<dyn ProgressSink>`: the
/// worker keeps one per job and the model may hand a clone to a blocking task.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait ProgressSink: Send + Sync {
    /// Sends events in order.
    async fn send(&self, events: Vec<WorkerEvent>) -> Result<(), ProgressError>;
}

/// The vision backend of a worker ([contracts 5], [contracts 6]): the perception models on
/// the RTX 3070, the VLM sidecar on the 5070 Ti, or the LLM API on the fallback worker.
///
/// **One method per job kind**, not one generic dispatch: each kind has its own payload and
/// result type, so the compiler checks that a `vlm` payload produces a `VlmResult`, and the
/// worker matches `JobSpec` once. An adapter answers the kinds its capability claims and
/// returns [`ModelError::Unsupported`] for the others; the fallback adapter answers all four.
///
/// Cancellation is by drop: when a heartbeat lists the job in `cancel_job_ids`, the worker
/// drops the future. Adapters keep blocking work (`ort` sessions, image decoding) on
/// `spawn_blocking` and must tolerate being dropped at any `.await`.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait VisionModel: Send + Sync {
    /// Model name → version, sent in `complete.model_versions`.
    fn model_versions(&self) -> BTreeMap<String, String>;

    /// A `perception` job: each model once over the batch, one partial result per finished
    /// stage.
    async fn perceive(
        &self,
        job: JobContext,
        payload: &PerceptionPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<PerceptionResult, ModelError>;

    /// A `vlm` job: the findings call, then the indicators call unless measurements answered
    /// every indicator.
    async fn analyse(
        &self,
        job: JobContext,
        payload: &VlmPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VlmResult, ModelError>;

    /// A `verify` job: the indicators call only.
    async fn verify(
        &self,
        job: JobContext,
        payload: &VerifyPayload,
        images: &[LoadedImage],
        progress: Arc<dyn ProgressSink>,
    ) -> Result<VerifyResult, ModelError>;

    /// A `guided` job: one squeeze or jar test image, alone.
    async fn read_guided_check(
        &self,
        job: JobContext,
        payload: &GuidedPayload,
        image: &LoadedImage,
    ) -> Result<GuidedResult, ModelError>;
}

/// The label list `BioCLIP` scores against, generated from the knowledge base ([contracts 10]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesLabelSet {
    /// `label_set_version`.
    pub version: String,
    /// Each species' `bioclip_label`.
    pub labels: Vec<(SpeciesId, String)>,
}

/// Species identification (architecture 6.2): `BioCLIP` inside the perception worker today, an
/// external identification API behind the same trait later.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait SpeciesIdentifier: Send + Sync {
    /// The top candidates for the plant in these images.
    async fn identify(
        &self,
        images: &[LoadedImage],
        labels: &SpeciesLabelSet,
    ) -> Result<SpeciesResult, ModelError>;
}

// ---------------------------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------------------------

/// What the text classifier is asked (architecture 7, router and safety gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassifierTask {
    /// The chat route: `diagnosis_question`, `care_question`, `task_change`, `safety_sensitive`,
    /// `off_topic`.
    Route,
    /// Whether a turn holds a fact worth remembering.
    MemoryWorthy,
    /// A knowledge chunk's topic, at ingestion.
    ChunkTopic,
}

/// One label's score. Scores are compared with each other, not read as probabilities.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelScore {
    /// The label.
    pub label: String,
    /// Its score.
    pub score: f64,
}

/// The fast text classifier: Jev, or the cheapest LLM with structured output.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait TextClassifier: Send + Sync {
    /// Scores every label of the task, highest first.
    async fn classify(
        &self,
        task: ClassifierTask,
        text: &str,
    ) -> Result<Vec<LabelScore>, ModelError>;
}

/// The pinned text-embedding model (architecture 10): its name, version and dimension, which
/// must equal the `chunk_vec` index's `DIMENSION` ([database 5]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModel {
    /// Stored as `embedding_model` beside every vector.
    pub name: String,
    /// The model revision.
    pub version: String,
    /// The vector length.
    pub dimension: usize,
}

/// A text or image embedding.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    /// The vector length.
    pub fn dimension(&self) -> usize {
        self.0.len()
    }

    /// Cosine similarity; 0 when either vector is zero or the lengths differ.
    pub fn cosine(&self, other: &Self) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }
        let dot: f32 = self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum();
        let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let denom = norm(&self.0) * norm(&other.0);
        if denom == 0.0 { 0.0 } else { dot / denom }
    }
}

/// Which side of a retrieval the texts are on. Asymmetric retrieval models (arctic-embed-xs,
/// architecture 10) embed the two sides differently: a query gets the model's instruction
/// prefix, a document gets none. Mixing them up still returns vectors, only worse matches, so
/// the caller names the side on every call instead of being handed a pre-set embedder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbedMode {
    /// What is searched with: a chat question, a `search_knowledge` query, a memory search.
    Query,
    /// What is searched: a knowledge chunk, a memory being saved.
    Document,
}

/// Text embeddings on the control plane's CPU (architecture 10), for memory and knowledge
/// queries and the knowledge loader. The model's identity is data returned by a method, not an
/// associated `const`: a trait with an associated const is not dyn compatible (Rust Reference,
/// dyn compatibility), and the model is chosen at start-up. One instance serves both
/// [`EmbedMode`]s; a model without a query prefix ignores the mode.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait TextEmbedder: Send + Sync {
    /// The pinned model.
    fn model(&self) -> EmbeddingModel;

    /// One embedding per text, in order, embedded as `mode`. Runs the model on a blocking
    /// thread.
    async fn embed(&self, mode: EmbedMode, texts: &[String]) -> Result<Vec<Embedding>, ModelError>;
}

// ---------------------------------------------------------------------------------------------
// Chat / LLM API
// ---------------------------------------------------------------------------------------------

/// Which caller is asking, so the adapter picks the model tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatPurpose {
    /// A chat turn: the strongest model.
    Chat,
    /// The plan writer, with structured output.
    PlanWriter,
    /// Memory extraction: a cheap model.
    MemoryExtraction,
    /// An off-topic redirect: the cheapest model.
    OffTopic,
    /// The fallback worker's vision calls.
    VisionFallback,
    /// The text classifier's LLM fallback (routing, the memory gate, chunk topics): the
    /// cheapest model with structured output.
    Classification,
}

/// A message's author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatRole {
    /// The user.
    User,
    /// The model.
    Assistant,
    /// A tool result.
    Tool,
}

/// One part of a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentPart {
    /// Text.
    Text(String),
    /// An image sent to the model (`view_image`, the fallback worker).
    Image {
        /// `image/webp`.
        media_type: &'static str,
        /// The bytes.
        data: Bytes,
    },
    /// A tool call the model made earlier in the conversation.
    ToolCall {
        /// The call.
        call_id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        arguments: serde_json::Value,
    },
    /// A tool's result.
    ToolResult {
        /// The call it answers.
        call_id: String,
        /// The result, the same records the app sees, or `{ "error": reason }`.
        content: serde_json::Value,
        /// The call was refused (a dose in its arguments, a scope or validation failure): the
        /// model reads the reason and may rewrite the call (`tool_result.is_error`).
        is_error: bool,
    },
    /// A reasoning block the model produced earlier in this turn's tool loop. Replayed
    /// unchanged on the next request of the loop; never stored, never sent to the app.
    Reasoning(ReasoningBlock),
}

/// A model's reasoning block, opaque to `agent`. The LLM API requires the blocks of the
/// current tool loop to be sent back unchanged; the signature binds each block to the
/// conversation prefix that produced it, so a rebuilt context drops every earlier one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningBlock {
    /// A `thinking` block: its (possibly empty) text and its signature.
    Thinking {
        /// The summarised or omitted reasoning text.
        text: String,
        /// The provider's signature, replayed byte for byte.
        signature: String,
    },
    /// A `redacted_thinking` block.
    Redacted {
        /// The encrypted payload.
        data: String,
    },
}

/// One message of the context.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    /// Its author.
    pub role: ChatRole,
    /// Its parts.
    pub content: Vec<ContentPart>,
}

/// A tool the model may call, filtered by role before each turn ([contracts 8]).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    /// The tool name.
    pub name: String,
    /// What it does, for the model.
    pub description: String,
    /// JSON Schema of its arguments.
    pub parameters: serde_json::Value,
    /// Ask the provider to guarantee the arguments match the schema. Each strict tool adds to a
    /// grammar the provider compiles per request, and nineteen are too many for Anthropic's
    /// (a 400), so only the tools that propose changes set it.
    pub strict: bool,
}

/// One LLM call. Owned, so it can be built once and borrowed by the stream for its whole life.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// Who is asking.
    pub purpose: ChatPurpose,
    /// The system prompt.
    pub system: String,
    /// The context, oldest first.
    pub messages: Vec<ChatMessage>,
    /// The tools offered.
    pub tools: Vec<ToolSpec>,
    /// Structured output: the JSON Schema the answer must match.
    pub response_schema: Option<serde_json::Value>,
    /// The output budget.
    pub max_tokens: u32,
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The answer is complete.
    EndTurn,
    /// It wants the tool results.
    ToolUse,
    /// `max_tokens` was reached.
    MaxTokens,
    /// The provider's safety classifier declined (HTTP 200 with `stop_reason: "refusal"`):
    /// any text so far is incomplete and any tool call must not run.
    Refusal,
}

/// One piece of a streamed answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatChunk {
    /// More text. `agent` buffers it into sentences for the dose check before streaming.
    TextDelta(String),
    /// A complete tool call.
    ToolCall {
        /// The call.
        call_id: String,
        /// The tool.
        name: String,
        /// Its arguments.
        arguments: serde_json::Value,
    },
    /// A complete reasoning block, to replay on the next request of the tool loop.
    Reasoning(ReasoningBlock),
    /// Token counts, once known.
    Usage(Usage),
    /// The end.
    Done(StopReason),
}

/// A whole answer, for the non-streaming callers (the plan writer, memory extraction).
#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletion {
    /// The text, or the structured output as JSON text.
    pub text: String,
    /// Tool calls.
    pub tool_calls: Vec<ChatChunk>,
    /// Token counts.
    pub usage: Usage,
    /// Why it stopped.
    pub stop: StopReason,
}

/// The LLM API (architecture 7): streaming for chat, whole answers for the plan writer.
///
/// `stream` is a plain method returning a boxed stream, so the trait stays dyn compatible
/// without `#[async_trait]`. The named lifetime `'a` ties the stream to both `&self` and the
/// borrowed request: the adapter can keep references into the request (messages, tool schemas,
/// image bytes) while it streams instead of cloning them, and the borrow checker guarantees the
/// request outlives the stream. A caller that needs a `'static` stream (the SSE response body)
/// moves the `Arc<dyn ChatModel>` and the owned request into an `async_stream` generator and
/// borrows them inside it.
#[async_trait::async_trait]
pub trait ChatModel: Send + Sync {
    /// The model name, stored on every message and plan.
    fn model_name(&self, purpose: ChatPurpose) -> String;

    /// Streams the answer.
    fn stream<'a>(
        &'a self,
        request: &'a ChatRequest,
    ) -> BoxStream<'a, Result<ChatChunk, ModelError>>;

    /// The whole answer.
    async fn complete(&self, request: &ChatRequest) -> Result<ChatCompletion, ModelError>;
}

// ---------------------------------------------------------------------------------------------
// Spot segmentation (measurement stage)
// ---------------------------------------------------------------------------------------------

/// A borrowed RGB8 image, row-major: the stored photo the measurement stage already decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbView<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

impl<'a> RgbView<'a> {
    /// Checks `pixels.len() == width × height × 3`.
    pub fn new(width: u32, height: u32, pixels: &'a [u8]) -> Option<Self> {
        let expected = usize::try_from(u64::from(width) * u64::from(height) * 3).ok()?;
        (pixels.len() == expected).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels.
    pub const fn pixels(&self) -> &'a [u8] {
        self.pixels
    }
}

/// A borrowed 8-bit mask of the same size as its image: non-zero inside the leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskView<'a> {
    width: u32,
    height: u32,
    data: &'a [u8],
}

impl<'a> MaskView<'a> {
    /// Checks `data.len() == width × height`.
    pub fn new(width: u32, height: u32, data: &'a [u8]) -> Option<Self> {
        let expected = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
        (data.len() == expected).then_some(Self {
            width,
            height,
            data,
        })
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The mask.
    pub const fn data(&self) -> &'a [u8] {
        self.data
    }
}

/// What the segmenter calls each pixel inside the leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SpotClass {
    /// Outside the leaf mask.
    Outside = 0,
    /// Healthy leaf.
    Leaf = 1,
    /// Brown, black or pale against the leaf's own green: part of a spot.
    Necrotic = 2,
    /// Yellowing around a spot: counts toward `affected_area_pct`, never toward a spot's size.
    Halo = 3,
}

/// The segmenter's output, owned: one [`SpotClass`] per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotMask {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Row-major classes.
    pub classes: Vec<SpotClass>,
}

/// Marks necrotic and yellowed pixels inside a leaf mask ([contracts 7], geometry: spots;
/// architecture 6.7). Colour thresholds today; a lesion-segmentation model can replace them.
/// Synchronous and CPU-bound: the measurement stage calls it on a blocking thread, over
/// buffers it borrows for the call.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SpotSegmenter: Send + Sync {
    /// The algorithm version, part of a measurement's `model_version`.
    fn version(&self) -> String;

    /// Classifies every pixel inside `leaf`.
    fn segment<'a>(&self, image: RgbView<'a>, leaf: MaskView<'a>) -> SpotMask;
}
```

### Providers

```rust
//! External data providers and push delivery: Open-Meteo, `SoilGrids`, the geocoder, Expo Push.

use crate::{
    garden::{gardens::TimeZone, push::PushMessage},
    id::PushToken,
    knowledge::SoilProfile,
    ports::ProviderError,
    records::weather::{CellWeather, Coordinates, WeatherCell},
};

/// One cell to fetch: its centre is sent to Open-Meteo, and the time zone dates the forecast's
/// `LocalDate`s. Gardens that share a cell share a time zone, since a cell is about 1 km.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRequest {
    /// The cache cell.
    pub cell: WeatherCell,
    /// Its IANA time zone.
    pub time_zone: TimeZone,
}

/// One cell's result in a batch: a failed multi-location request is retried once, then its
/// cells are fetched one by one, and a cell that still fails skips its gardens until the next
/// run ([contracts 7], fetching and caching).
#[derive(Debug)]
pub struct CellResult {
    /// The cell.
    pub cell: WeatherCell,
    /// Its weather, or why not.
    pub weather: Result<CellWeather, ProviderError>,
}

/// Weather (architecture 6.4 and 8.3): Open-Meteo, cached per ~1 km cell and hour.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait WeatherProvider: Send + Sync {
    /// One multi-location request of at most 50 cells (`latitude` and `longitude` as
    /// comma-separated lists). Fails as a whole when the request fails; the scheduler then
    /// applies the retry rule above.
    async fn fetch_batch(&self, cells: &[CellRequest]) -> Result<Vec<CellWeather>, ProviderError>;

    /// One cell, for a scan's cache miss and the per-cell fallback.
    async fn fetch_cell(&self, cell: &CellRequest) -> Result<CellWeather, ProviderError>;
}

/// `SoilGrids` (architecture 6.4): fetched once per garden, in the background, 5 calls a minute
/// at most.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait SoilProvider: Send + Sync {
    /// The profile at a point, every depth band `SoilGrids` returns.
    async fn soil_profile(&self, at: Coordinates) -> Result<SoilProfile, ProviderError>;
}

/// What the geocoder is asked: coordinates (for the label and time zone), a postal code or a
/// city ([contracts 9], `GardenLocationInput`). Borrowed: the request owns the strings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeocodeQuery<'a> {
    /// GPS with consent: reverse-geocode the label.
    Coordinates(Coordinates),
    /// A postal code, geocoded once.
    PostalCode(&'a str),
    /// A city, geocoded once.
    City(&'a str),
}

/// A geocoded location.
#[derive(Debug, Clone, PartialEq)]
pub struct Geocoded {
    /// The point stored as `garden.centroid`.
    pub coordinates: Coordinates,
    /// "Toronto, ON".
    pub label: String,
    /// IANA, sets every `LocalDate` of the garden.
    pub time_zone: TimeZone,
}

/// The geocoder (architecture 8.5): coordinates, a label and the IANA time zone.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait Geocoder: Send + Sync {
    /// Resolves the query; an unknown postal code or city is [`ProviderError::NotFound`].
    async fn geocode<'a>(&self, query: GeocodeQuery<'a>) -> Result<Geocoded, ProviderError>;
}

/// What a push batch did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushReport {
    /// Messages Expo accepted.
    pub accepted: u32,
    /// Tokens Expo reported as `DeviceNotRegistered`: the API removes them.
    pub unregistered: Vec<PushToken>,
}

/// Push delivery through the Expo Push API ([contracts 9], push notifications). A push is a
/// hint, never the only copy, so a failure is logged and dropped.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    /// Sends the messages (iOS and Android tokens only), in Expo's batches of 100.
    async fn push(&self, messages: Vec<PushMessage>) -> Result<PushReport, ProviderError>;

    /// Checks the receipts of tickets sent at least 15 minutes ago (Expo's advice) and returns
    /// the tokens Expo reported as `DeviceNotRegistered`, to forget. The default does nothing,
    /// for adapters without receipts. Called by `api`'s scheduler.
    async fn check_receipts(&self) -> Result<Vec<PushToken>, ProviderError> {
        Ok(Vec::new())
    }
}
```

### Knowledge base and memory

```rust
//! The knowledge base and memory ports (architecture 5, knowledge base; architecture 7,
//! memory).

use crate::{
    authz::{Editor, Membership, Viewer},
    chat::{KnowledgeHit, Lookalike, Memory, MemoryKind, MemoryQuery, MemorySource, MemoryTopic},
    garden::species::{SpeciesPhoto, SpeciesProfileKind},
    id::{
        GardenId, MemoryId, MessageId, OrganismId, PlantId, ProblemId, SourceId, SpeciesId, UserId,
    },
    jobs::{OrganismOption, SpeciesOption},
    knowledge::{
        ChunkTopic, ClassifierLabels, OrganismSeed, ProblemSeed, ProductsSeed, SoilConditionSeed,
        SpeciesSeed, TreatmentSeed,
    },
    ports::{BeliefSubject, Embedding, StoreError},
    records::diagnosis::{Citation, Prey},
    scan::SpeciesRef,
    vlm::{DamagePattern, SoilAspect},
    wire::{Page, PageRequest, Timestamp},
};

/// A species for the picker: the reference, its profile kind and its photos (signed by `api`
/// into `SpeciesSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesEntry {
    /// `id`, `scientific_name`, `common_name`.
    pub species: SpeciesRef,
    /// `full` or `label_only`.
    pub profile: SpeciesProfileKind,
    /// The disease classifier covers it.
    pub classifier_supported: bool,
    /// At most 3, in seed order.
    pub photos: Vec<SpeciesPhoto>,
}

/// The allowed values of a VLM payload, generated by the loader ([contracts 10], generated
/// files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VlmOptions {
    /// `label_set_version`.
    pub label_set_version: String,
    /// `species_options`.
    pub species: Vec<SpeciesOption>,
    /// `organism_options`.
    pub organisms: Vec<OrganismOption>,
}

/// Filters applied before the vector search ([database 5]: indexed filters pre-filter the HNSW
/// search).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChunkFilter {
    /// Topic.
    pub topic: Option<ChunkTopic>,
    /// Crop.
    pub crop: Option<SpeciesId>,
    /// Problem.
    pub problem: Option<ProblemId>,
    /// Organism.
    pub organism: Option<OrganismId>,
}

/// The knowledge base as the pipeline, the agent and `api` read it. Implemented by the
/// `knowledge` crate, which loads the curated records once (they change only with a migration)
/// and answers lookups from memory; chunk search goes to the HNSW index.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// A species profile: `get_species_profile`.
    async fn species(&self, id: &SpeciesId) -> Result<Option<SpeciesSeed>, StoreError>;

    /// `GET /species?q=`: by common name; `q` matches common and scientific names,
    /// case-insensitively, anywhere in the name.
    async fn species_list(
        &self,
        q: Option<String>,
        page: PageRequest,
    ) -> Result<Page<SpeciesEntry>, StoreError>;

    /// `GET /species/{id}`.
    async fn species_entry(&self, id: &SpeciesId) -> Result<Option<SpeciesEntry>, StoreError>;

    /// A problem profile: `get_disease_profile`.
    async fn problem(&self, id: &ProblemId) -> Result<Option<ProblemSeed>, StoreError>;

    /// Several problems, in the order asked; unknown ids are left out.
    async fn problems(&self, ids: &[ProblemId]) -> Result<Vec<ProblemSeed>, StoreError>;

    /// The problems a species is `susceptible_to`, pests and wildlife included (the retrieve
    /// stage, architecture 6.3).
    async fn problems_of_species(
        &self,
        species: &SpeciesId,
    ) -> Result<Vec<ProblemSeed>, StoreError>;

    /// The problems an organism or damage pattern points to (`caused_by`, `damage_patterns`):
    /// what a `verify` job adds when the VLM found something outside the candidate set.
    async fn problems_pointed_to(
        &self,
        organisms: &[OrganismId],
        patterns: &[DamagePattern],
    ) -> Result<Vec<ProblemSeed>, StoreError>;

    /// `compare_lookalikes`: both directions of `looks_like`.
    async fn lookalikes(&self, problem: &ProblemId) -> Result<Vec<Lookalike>, StoreError>;

    /// An organism profile: `get_organism_profile`.
    async fn organism(&self, id: &OrganismId) -> Result<Option<OrganismSeed>, StoreError>;

    /// The pests a beneficial organism eats, from the `preys_on` graph:
    /// `Diagnosis.organisms[].preys_on`, computed when read.
    async fn prey_of(&self, organism: &OrganismId) -> Result<Vec<Prey>, StoreError>;

    /// Treatments, in the order asked.
    async fn treatments(
        &self,
        ids: &[crate::id::TreatmentId],
    ) -> Result<Vec<TreatmentSeed>, StoreError>;

    /// Sources resolved into citations.
    async fn citations(&self, ids: &[SourceId]) -> Result<Vec<Citation>, StoreError>;

    /// `products.yaml`, for the plan validator.
    async fn products(&self) -> Result<ProductsSeed, StoreError>;

    /// The current `classifier_labels.yaml`.
    async fn classifier_labels(&self) -> Result<ClassifierLabels, StoreError>;

    /// A soil condition's description and advice.
    async fn soil_condition(
        &self,
        aspect: SoilAspect,
        value: &str,
    ) -> Result<Option<SoilConditionSeed>, StoreError>;

    /// `species_options`, `organism_options` and `label_set_version` for VLM payloads.
    async fn vlm_options(&self) -> Result<VlmOptions, StoreError>;

    /// `search_knowledge`: the `k` nearest chunks with their citations.
    async fn search(
        &self,
        query: &Embedding,
        filter: ChunkFilter,
        k: usize,
    ) -> Result<Vec<KnowledgeHit>, StoreError>;
}

/// A memory to store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMemory {
    /// Its id.
    pub id: MemoryId,
    /// The plant, or none for a garden-level fact.
    pub plant: Option<PlantId>,
    /// What kind.
    pub kind: MemoryKind,
    /// What it is about; with the kind, its belief key in Agent Memory.
    pub topic: MemoryTopic,
    /// The text, dose-checked when the model wrote it.
    pub text: String,
    /// The vector of `text`.
    pub embedding: Embedding,
    /// The embedding model's name.
    pub embedding_model: String,
    /// The chat message it came from.
    pub source_msg: Option<MessageId>,
    /// When.
    pub created_at: Timestamp,
}

/// A memory with its similarity to a query.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryHit {
    /// The memory.
    pub memory: Memory,
    /// Cosine similarity.
    pub similarity: f32,
}

/// Garden memory ([contracts 8], memory). A separate port from the store so it can be mapped
/// onto `SurrealDB` Agent Memory (architecture 7). Search is exact within one garden
/// ([database 5]): `shared = true OR author = $user`, so a member never sees another's private
/// memories.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync {
    /// The `remember` tool and automatic extraction: a private chat memory of its author.
    /// Every role may write its own.
    async fn remember(
        &self,
        member: Membership<Viewer>,
        memory: NewMemory,
    ) -> Result<Memory, StoreError>;

    /// `POST /memories`: a garden note, `source: "user"`, shared.
    async fn add_note(
        &self,
        member: Membership<Editor>,
        memory: NewMemory,
    ) -> Result<Memory, StoreError>;

    /// The pipeline's summary after a scan: `source: "pipeline"`, visible to every member.
    async fn add_pipeline_memory(
        &self,
        garden: GardenId,
        memory: NewMemory,
    ) -> Result<Memory, StoreError>;

    /// `GET /memories`: shared memories plus the caller's own chat memories, newest first.
    async fn memories(
        &self,
        member: Membership<Viewer>,
        query: MemoryQuery,
        page: PageRequest,
    ) -> Result<Page<Memory>, StoreError>;

    /// `search_memory`: the `k` closest of the shared memories and the caller's own, within the
    /// plant (and the garden-level facts) when one is given.
    async fn search(
        &self,
        member: Membership<Viewer>,
        query: &Embedding,
        plant: Option<PlantId>,
        k: usize,
    ) -> Result<Vec<MemoryHit>, StoreError>;

    /// What the plan writer reads: shared memories only (`pipeline` and `user`), the plant's and
    /// the garden-level ones.
    async fn shared_for_plan(
        &self,
        garden: GardenId,
        plant: PlantId,
    ) -> Result<Vec<Memory>, StoreError>;

    /// `POST /memories/{id}/share`: the author, if an editor or owner, turns a chat memory into a
    /// `user` memory.
    async fn share(
        &self,
        member: Membership<Editor>,
        memory: MemoryId,
    ) -> Result<Memory, StoreError>;

    /// `DELETE /memories/{id}`: the author of a chat memory, or an owner for shared memories.
    async fn delete(&self, member: Membership<Viewer>, memory: MemoryId) -> Result<(), StoreError>;

    /// The memory's source and author, for the delete and share rules.
    async fn memory_owner(
        &self,
        memory: MemoryId,
    ) -> Result<Option<(MemorySource, Option<UserId>)>, StoreError>;

    // ---- the belief sweep (architecture 7, memory): internal, no membership check ----
    //
    // Only `api`'s `beliefs` task talks to Agent Memory, so nothing records a memory while its
    // subject is being erased. Requests only write rows: a shared memory is created with
    // `belief_synced_at` unset, and deleting one (or starting a plant's or garden's cascade)
    // queues its subject in `belief_forget` in the same transaction.

    /// Shared memories not yet recorded in Agent Memory (`belief_synced_at` unset), oldest
    /// `created_at` first, leaving out those of a plant or garden being deleted.
    async fn unsynced_shared(&self, limit: u32) -> Result<Vec<Memory>, StoreError>;

    /// Sets `belief_synced_at`.
    async fn mark_synced(&self, memory: MemoryId, at: Timestamp) -> Result<(), StoreError>;

    /// The subjects queued for erasure in Agent Memory, oldest first.
    async fn belief_forgets(&self, limit: u32) -> Result<Vec<BeliefSubject>, StoreError>;

    /// After the subject's scope was erased: unsets `belief_synced_at` on its shared memories
    /// (a garden's take its plants'), so the sweep records the remaining ones again, and
    /// removes it from the queue. One transaction.
    async fn finish_belief_forget(&self, subject: BeliefSubject) -> Result<(), StoreError>;
}
```

### Beliefs (Agent Memory)

Shared memories are recorded in `SurrealDB` Agent Memory as triples, so a newer memory on the same key supersedes the older one and the history is kept ([agent-memory](../database/agent-memory.md); [architecture 7, memory](../architecture/README.md#memory)). The `memory` table stays the system of record. Requests only write rows: a shared memory is created unsynced, and a delete or a cascade queues its subject in `belief_forget` in the same transaction (the `MemoryStore` methods under "the belief sweep" above). `api`'s `beliefs` task, `agent::beliefs::sync_pending`, is the only caller of this port's writes: it erases the queued subjects first, then records what is unsynced. `integrations::agent_memory::AgentMemory` implements the port; `NoBeliefs` stands in when Agent Memory isn't configured, so nothing else changes.

```rust
//! The belief store: shared memories mirrored into `SurrealDB` Agent Memory (architecture 7,
//! memory; docs/database/agent-memory.md).
//!
//! The `memory` table stays the system of record. Each **shared** memory (`pipeline` and
//! `user`) is also recorded in Agent Memory as one triple: entity `plant_<id>` or `garden_<id>`,
//! a key from [`belief_key`], and the memory's text as the value. Agent Memory then supersedes the
//! older value of the same key and keeps the history. A private chat memory never leaves the
//! database. Only `api`'s belief sweep (`agent::beliefs::sync_pending`) calls `record` and
//! `forget`, so no request waits on the service and nothing is recorded during an erase.
//!
//! Four facts measured against the hosted service on 2026-10-17 shape the port:
//! - supersession follows `observed_at`, not arrival: a write older than its key's current
//!   value is dropped, so a retry or a rebuild never brings an older value back;
//! - entities are matched by **name across the whole context**, so the entity name is our own
//!   id, never a name a model extracted;
//! - a read at a scope path returns only facts tagged at that exact path, so a plant read asks
//!   for the plant's path **or** the garden's;
//! - `/query` takes 2 to 10 s, so only `search` uses it, behind a timeout; the context builder
//!   and the plan writer use `current`, a direct entity read of about 0.4 s.

use crate::{
    chat::{Belief, BeliefHit, Memory, MemoryKind, MemorySource, MemoryTopic},
    id::{GardenId, MemoryId, PlantId},
    ports::ProviderError,
    wire::Timestamp,
};

/// The provider name on every [`ProviderError`] of the belief store.
pub const BELIEFS_PROVIDER: &str = "agent_memory";

/// What a belief is about: one plant, or its garden as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeliefSubject {
    /// The garden.
    pub garden: GardenId,
    /// The plant; none for a garden-level fact.
    pub plant: Option<PlantId>,
}

impl BeliefSubject {
    /// The subject a memory belongs to.
    pub const fn of(memory: &Memory) -> Self {
        Self {
            garden: memory.garden_id,
            plant: memory.plant_id,
        }
    }

    /// `gardens/<garden>` or `gardens/<garden>/plants/<plant>`: the scope path its facts are
    /// tagged with, and the subtree a forget erases. Ids are lowercased: Agent Memory refuses an
    /// uppercase letter in a path segment (`400 illegal character in segment`, measured
    /// 2026-10-17), and a ULID is case-insensitive.
    pub fn scope_path(&self) -> String {
        let garden = self.garden.to_string().to_ascii_lowercase();
        match self.plant {
            Some(p) => format!(
                "gardens/{garden}/plants/{}",
                p.to_string().to_ascii_lowercase()
            ),
            None => format!("gardens/{garden}"),
        }
    }

    /// `plant_<id>` or `garden_<id>`, lowercased like the path: unique in the context, because
    /// Agent Memory matches entities by name across every scope.
    pub fn entity_name(&self) -> String {
        match self.plant {
            Some(p) => format!("plant_{p}"),
            None => format!("garden_{}", self.garden),
        }
        .to_ascii_lowercase()
    }
}

/// The belief key of a memory. A newer memory with the same key on the same subject supersedes
/// the older one; `other` facts get a key of their own, so they never supersede anything.
///
/// | kind | key |
/// |---|---|
/// | `summary` | `scan_summary` |
/// | `fact`, topic `other` | `note_<memory id>` |
/// | `fact` | the topic: `watering`, `container`, … |
/// | `event` | `last_<topic>`: `last_treatment`, `last_other`, … |
pub fn belief_key(kind: MemoryKind, topic: MemoryTopic, id: MemoryId) -> String {
    match (kind, topic) {
        (MemoryKind::Summary, _) => "scan_summary".to_owned(),
        (MemoryKind::Fact, MemoryTopic::Other) => format!("note_{id}"),
        (MemoryKind::Fact, t) => t.as_str().to_owned(),
        (MemoryKind::Event, t) => format!("last_{}", t.as_str()),
    }
}

/// One shared memory, ready to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeliefWrite {
    /// The plant or garden.
    pub subject: BeliefSubject,
    /// From [`belief_key`].
    pub key: String,
    /// The memory's text.
    pub value: String,
    /// The memory, sent as the label `memory=<id>` so a belief traces back to its row.
    pub memory: MemoryId,
    /// `pipeline` or `user`, sent as the label `source=<source>`.
    pub source: MemorySource,
    /// The memory's `created_at`, sent as `observed_at`, so a rebuild replays the history in
    /// its original order.
    pub observed_at: Timestamp,
}

impl BeliefWrite {
    /// The write for a memory; `None` for a private chat memory, which is never recorded.
    pub fn from_memory(memory: &Memory) -> Option<Self> {
        if !memory.shared {
            return None;
        }
        Some(Self {
            subject: BeliefSubject::of(memory),
            key: belief_key(memory.kind, memory.topic, memory.id),
            value: memory.text.clone(),
            memory: memory.id,
            source: memory.source,
            observed_at: memory.created_at,
        })
    }
}

/// Shared memories in `SurrealDB` Agent Memory. `integrations` implements it over the REST API;
/// [`NoBeliefs`] stands in when it isn't configured. Every method is scoped to one garden by
/// its scope paths, and the caller has already checked membership.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait BeliefStore: Send + Sync {
    /// `false` for [`NoBeliefs`]: callers skip the work, and `GET /plants/{id}/beliefs` answers
    /// `available: false`.
    fn enabled(&self) -> bool;

    /// Records one shared memory as a triple, registering its scope path first. Idempotent:
    /// re-sending the same write leaves one current value.
    async fn record(&self, write: &BeliefWrite) -> Result<(), ProviderError>;

    /// The current beliefs of a plant and of its garden, or of the garden alone: one per key.
    async fn current(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
    ) -> Result<Vec<Belief>, ProviderError>;

    /// Every belief of one subject, current and superseded, newest first.
    async fn history(&self, subject: BeliefSubject) -> Result<Vec<Belief>, ProviderError>;

    /// The `k` current beliefs of the plant and its garden (or the garden alone) closest to a
    /// query. Slow (2 to 10 s measured), so callers bound it with a timeout.
    async fn search(
        &self,
        garden: GardenId,
        plant: Option<PlantId>,
        query: &str,
        k: usize,
    ) -> Result<Vec<BeliefHit>, ProviderError>;

    /// Erases everything under the subject's scope path, permanently: a garden takes its
    /// plants with it. Idempotent.
    async fn forget(&self, subject: BeliefSubject) -> Result<(), ProviderError>;
}

/// The belief store when Agent Memory isn't configured: records nothing, knows nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBeliefs;

#[async_trait::async_trait]
impl BeliefStore for NoBeliefs {
    fn enabled(&self) -> bool {
        false
    }

    async fn record(&self, _write: &BeliefWrite) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn current(
        &self,
        _garden: GardenId,
        _plant: Option<PlantId>,
    ) -> Result<Vec<Belief>, ProviderError> {
        Ok(Vec::new())
    }

    async fn history(&self, _subject: BeliefSubject) -> Result<Vec<Belief>, ProviderError> {
        Ok(Vec::new())
    }

    async fn search(
        &self,
        _garden: GardenId,
        _plant: Option<PlantId>,
        _query: &str,
        _k: usize,
    ) -> Result<Vec<BeliefHit>, ProviderError> {
        Ok(Vec::new())
    }

    async fn forget(&self, _subject: BeliefSubject) -> Result<(), ProviderError> {
        Ok(())
    }
}
```

### Farm connectors' vendors

The Slack and Google Drive adapters behind the farm connectors ([connectors 5–6](../connectors/README.md#6-sync)). `integrations` implements `FarmSource` once per vendor, each client built with a base URL so tests point it at Era's simulators; `api` picks one by the connector's `kind` and hands it a token it unsealed. Chunking, embedding and the record limit happen in the sync, outside the adapter. The OAuth exchanges are not part of the port: they run once per connect in `api`'s routes.

```rust
//! The farm connectors' vendor port ([connectors 5], [connectors 6]): what the `connectors`
//! scheduler task and the owner routes ask Slack or Google Drive. `integrations` implements it
//! once per vendor (`SlackClient`, `DriveClient`), each built with a base URL so tests point it
//! at Era's simulators; `api` picks the implementation by the connector's `kind`.
//!
//! The adapter is handed a token already unsealed by `api` and never sees the database. It
//! reads raw items (a message with its thread, a document's text); chunking, embedding,
//! `text_hash` and the 10,000-record limit happen in the sync, outside it. The OAuth exchanges
//! are not part of this port: they run once per connect in `api`'s routes (E3).
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
//! [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync

use std::{fmt, sync::Arc};

use crate::{
    connectors::{ConnectorAccount, ConnectorKind, ConnectorSource, FarmRecordKind},
    ports::FarmSourceError,
    wire::Timestamp,
};

/// A vendor access token (Slack's bot token, Google's access token), unsealed for one call.
/// `Debug` never prints it, so a token can't reach a log through a derived `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct VendorToken(Arc<str>);

impl VendorToken {
    /// Wraps an unsealed token.
    pub fn new(token: &str) -> Self {
        Self(Arc::from(token))
    }

    /// The token, for the `Authorization` header only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VendorToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VendorToken(..)")
    }
}

/// Where a source's sync stands, opaque outside its adapter: Slack's newest `ts` and page
/// cursor, Drive's page token or `changes.getStartPageToken`. Stored per source in
/// `connector.cursor`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyncCursor(pub String);

/// One page to read from one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmPageRequest {
    /// The connected workspace or account (Slack permalinks are built from its URL).
    pub account: ConnectorAccount,
    /// The channel or folder.
    pub source: ConnectorSource,
    /// Where the last page stopped; `None` for a first sync.
    pub cursor: Option<SyncCursor>,
    /// The oldest item wanted: 180 days back on a first sync, 7 days back for the daily
    /// re-read of Slack's trailing window.
    pub oldest: Timestamp,
}

/// One item as the vendor holds it, before chunking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmItem {
    /// Slack `"<channel>/<ts>"` or a Drive file id.
    pub vendor_id: String,
    /// A message or a document.
    pub kind: FarmRecordKind,
    /// `"#field-reports"`, `"Spray log 2026"`.
    pub title: String,
    /// The author's display name, from the per-team `users.info` cache.
    pub author: Option<String>,
    /// The permalink or `webViewLink`.
    pub link: String,
    /// A message with its replies joined, or a document's text (cut at 1 MB). Empty for a file
    /// listed by name only.
    pub text: String,
    /// The message's time, or the document's `modifiedTime`.
    pub at: Timestamp,
}

/// What one page returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmPage {
    /// New or changed items.
    pub items: Vec<FarmItem>,
    /// Vendor ids that are gone (deleted, trashed, moved out of the folder), whose records the
    /// sync deletes.
    pub removed: Vec<String>,
    /// The cursor to save after this page.
    pub cursor: SyncCursor,
    /// `true` when another page follows; the sync reads it now or on its next run.
    pub more: bool,
}

/// A vendor behind the farm connectors (Slack or Google Drive).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait FarmSource: Send + Sync {
    /// Which vendor this adapter speaks to.
    fn kind(&self) -> ConnectorKind;

    /// `GET /connectors/{id}/available-sources`: the channels the bot is a member of
    /// (`is_member`), or the folders the account can open.
    async fn available_sources(
        &self,
        token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError>;

    /// One page of one source, from `request.cursor`. Honours `Retry-After`; a `429` that
    /// outlasts the pacer is [`FarmSourceError::RateLimited`], so the sync saves its cursor.
    async fn read_page(
        &self,
        token: &VendorToken,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError>;

    /// Revokes the token on disconnect (`auth.revoke`, Google's `revoke`). A failure is logged
    /// by the caller and the records are deleted anyway.
    async fn revoke(&self, token: &VendorToken) -> Result<(), FarmSourceError>;
}
```

### The store

The store's area traits, `Store` as the supertrait of all of them, and the claim and lease tokens the write patterns of [database 4](../database/README.md#4-write-patterns) need.

```rust
//! The store's repository traits, one per area ([database 3], [database 4]), and [`Store`],
//! the supertrait of all of them that `api` holds as `Arc<dyn Store>`.
//!
//! Rules every method follows:
//! - A garden-scoped call takes a [`Membership<R>`](crate::authz::Membership) proof with the
//!   role the role matrix asks for; the store scopes its query to `membership.garden()`, so a
//!   record of another garden is [`StoreError::NotFound`](crate::ports::StoreError).
//! - A call that continues `api`'s own step takes the [`StepClaim`] it got from
//!   [`StepRepo::claim_step`]; a worker call takes the [`Lease`] from [`JobRepo::claim_job`].
//!   The store re-checks either in the write's transaction ([database 4]).
//! - Reads skip records being deleted (`deleting_at`), and writes that need a live record check
//!   it in the same transaction.
//! - Transactions are sent as one query and retried on conflict up to 5 times inside the store
//!   ([database 4]); a caller sees [`Conflict::Retries`](crate::ports::Conflict) only when that
//!   fails.
//! - Records come back in their stored URL state (`X<Stored>`); `api` signs them.

mod access;
mod analysis;
mod chat;
mod deletion;
mod events;
mod farm;
mod gardens;
mod idempotency;
mod identity;
mod jobs;
mod knowledge;
mod map;
mod plans;
mod plants;
mod scans;
mod steps;
mod weather;

pub use access::*;
pub use analysis::*;
pub use chat::*;
pub use deletion::*;
pub use events::*;
pub use farm::*;
pub use gardens::*;
pub use idempotency::*;
pub use identity::*;
pub use jobs::*;
pub use knowledge::*;
pub use map::*;
pub use plans::*;
pub use plants::*;
pub use scans::*;
pub use steps::*;
pub use weather::*;

use crate::{
    id::{JobId, LeaseToken, QuestionId, ScanId, WorkerId},
    ports::{ContentType, ObjectKey},
    rules::{questions::QuestionStep, steps::PipelineStep},
    wire::{Run, Timestamp},
};

/// A worker's hold on a job: what every later call about the job carries
/// (`X-Lease-Token`, [contracts 5]). The store compares all three in the write's transaction;
/// a mismatch, an expired lease or a cancelled job is `Conflict::LeaseLost`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// The job.
    pub job: JobId,
    /// Its current token.
    pub token: LeaseToken,
    /// The worker holding it.
    pub worker: WorkerId,
}

/// A scan's step claim (`pipeline_step`, `pipeline_step_run`, `pipeline_step_at`,
/// [database 4], steps between jobs). Produced only by the store's claim and re-claim; a commit
/// goes through only if all four still hold, the scan's `run` is unchanged and neither the plant
/// nor the garden is being deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepClaim {
    /// The scan.
    pub scan: ScanId,
    /// The step.
    pub step: PipelineStep,
    /// The run it belongs to.
    pub run: Run,
    /// When it was claimed: the compare-and-set stamp.
    pub stamp: Timestamp,
}

/// The result of an entry point's claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimAttempt {
    /// This caller holds the step now.
    Claimed(StepClaim),
    /// Another step holds the claim; `rescore_pending` was set instead, and the running step
    /// hands over to `score` when it commits.
    Busy,
    /// Nothing to claim: the scan's status claims nothing (a guided check on a `pending`,
    /// `queued` or `failed` scan), or it is being deleted.
    NotNeeded,
}

/// A photo answer's step claim (`question.step`, `step_at` and its `run`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestionStepClaim {
    /// The question.
    pub question: QuestionId,
    /// Its scan.
    pub scan: ScanId,
    /// The step.
    pub step: QuestionStep,
    /// The run it belongs to.
    pub run: Run,
    /// The compare-and-set stamp.
    pub stamp: Timestamp,
}

/// Where an image's bytes are, for the routes that stream them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRef {
    /// The storage key from the record.
    pub key: ObjectKey,
    /// Its content type.
    pub content_type: ContentType,
}

/// Every repository, for `Arc<dyn Store>`. Blanket-implemented, so the store crate implements
/// each area trait and gets this for free. Trait upcasting (stable since Rust 1.86) turns an
/// `Arc<dyn Store>` into an `Arc<dyn PlanRepo>` for a component that needs one area only.
pub trait Store:
    AccessRepo
    + IdentityRepo
    + GardenRepo
    + PlantRepo
    + ScanRepo
    + EventRepo
    + AnalysisRepo
    + StepRepo
    + PlanRepo
    + ChatRepo
    + KnowledgeRepo
    + JobRepo
    + IdempotencyRepo
    + UsageRepo
    + WeatherCacheRepo
    + DeletionRepo
    + MapRepo
    + FarmRecordStore
{
}

impl<T> Store for T where
    T: AccessRepo
        + IdentityRepo
        + GardenRepo
        + PlantRepo
        + ScanRepo
        + EventRepo
        + AnalysisRepo
        + StepRepo
        + PlanRepo
        + ChatRepo
        + KnowledgeRepo
        + JobRepo
        + IdempotencyRepo
        + UsageRepo
        + WeatherCacheRepo
        + DeletionRepo
        + MapRepo
        + FarmRecordStore
{
}
```

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

```rust
//! The lookups behind [`crate::authz`].

use crate::{
    id::{
        ConnectorId, ConversationId, GardenId, InviteId, MemoryId, PendingActionId, PlanId,
        PlantId, ScanId, TaskId, UserId,
    },
    ports::StoreError,
    wire::Role,
};

/// A record whose garden decides access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GardenResource {
    /// A plant.
    Plant(PlantId),
    /// A scan.
    Scan(ScanId),
    /// A plan version.
    Plan(PlanId),
    /// A task.
    Task(TaskId),
    /// A memory.
    Memory(MemoryId),
    /// An invite.
    Invite(InviteId),
    /// A farm connector; one being deleted (`status: deleting`) reads as missing.
    Connector(ConnectorId),
}

/// A conversation's owner and garden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationOwnerRow {
    /// The owner (`conversation.user`).
    pub user: UserId,
    /// The garden.
    pub garden: GardenId,
}

/// The reads every authorization check makes ([database 4], membership).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AccessRepo: Send + Sync {
    /// A direct read of `member_of:[user, garden]`; `None` for a non-member or a garden being
    /// deleted.
    async fn role_in(&self, user: UserId, garden: GardenId) -> Result<Option<Role>, StoreError>;

    /// The record's garden; `None` when it doesn't exist or is being deleted (its own
    /// `deleting_at`, its plant's or its garden's).
    async fn garden_of(&self, resource: GardenResource) -> Result<Option<GardenId>, StoreError>;

    /// The conversation's owner and garden; `None` when it doesn't exist.
    async fn conversation_owner(
        &self,
        conversation: ConversationId,
    ) -> Result<Option<ConversationOwnerRow>, StoreError>;

    /// The conversation a pending action belongs to.
    async fn pending_action_conversation(
        &self,
        action: PendingActionId,
    ) -> Result<Option<ConversationId>, StoreError>;
}
```

#### Identity

```rust
//! Users and push tokens ([contracts 9], account; architecture 4, identity).

use crate::{
    garden::account::{Me, Platform, PushTokenRequest, UpdateMeRequest},
    id::{AuthId, PushToken, UserId},
    ports::StoreError,
};

/// A device to push to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    /// Its user.
    pub user: UserId,
    /// The Expo token.
    pub token: PushToken,
    /// Its platform; only iOS and Android are delivered.
    pub platform: Platform,
}

/// The `user` and `push_token` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait IdentityRepo: Send + Sync {
    /// Creates or finds the user for an auth id: an upsert on the unique `auth_id`, so two
    /// first requests racing each other agree (the `user.created` webhook creates nothing).
    /// Refused with `StoreError::NotFound` while the account's tombstone
    /// (`deleted_user:[auth_id]`, [`AccountDeletionMarker`](crate::ports::AccountDeletionMarker))
    /// exists, checked in the same transaction as the create and before an existing user is
    /// returned, so a deleted account's still-valid session token neither recreates the user
    /// nor keeps using it during the cascade.
    async fn upsert_user(
        &self,
        auth_id: &AuthId,
        display_name: Option<String>,
    ) -> Result<UserId, StoreError>;

    /// The user of an auth id, without creating one (the `user.deleted` webhook).
    async fn user_by_auth(&self, auth_id: &AuthId) -> Result<Option<UserId>, StoreError>;

    /// `GET /me`.
    async fn me(&self, user: UserId) -> Result<Me, StoreError>;

    /// `PATCH /me`. Withdrawing consent rounds the location of every garden the user owns to
    /// about 10 km (`about_10_km`) in the same transaction.
    async fn update_me(&self, user: UserId, patch: UpdateMeRequest) -> Result<Me, StoreError>;

    /// `POST /me/push-tokens`: an upsert on `push_token:[token]`, so registering again is a
    /// no-op.
    async fn register_push_token(
        &self,
        user: UserId,
        request: PushTokenRequest,
    ) -> Result<(), StoreError>;

    /// `DELETE /me/push-tokens/{token}`.
    async fn remove_push_token(&self, user: UserId, token: &PushToken) -> Result<(), StoreError>;

    /// The deliverable tokens of these users.
    async fn push_targets(&self, users: &[UserId]) -> Result<Vec<PushTarget>, StoreError>;

    /// Removes tokens Expo reported as `DeviceNotRegistered`.
    async fn forget_push_tokens(&self, tokens: &[PushToken]) -> Result<(), StoreError>;
}
```

#### Gardens, members and invites

```rust
//! Gardens, members and invites ([contracts 9], gardens, members and invites).

use crate::{
    authz::{Membership, Owner, Viewer},
    garden::{
        gardens::{Garden, SoilProfileStatus, TimeZone},
        members::{Invite, Member},
    },
    id::{ConversationId, GardenId, InviteCode, InviteId, UserId},
    knowledge::SoilProfile,
    ports::{ObjectPrefix, StoreError},
    records::weather::{Coordinates, LocationPrecision, WeatherCell},
    wire::{Page, PageRequest, Role, Timestamp},
};

/// A location the geocoder resolved, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLocation {
    /// `garden.centroid`.
    pub coordinates: Coordinates,
    /// `garden.location_precision`.
    pub precision: LocationPrecision,
    /// `garden.location_label`.
    pub label: String,
    /// `garden.time_zone`.
    pub time_zone: TimeZone,
}

/// `POST /gardens`, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct NewGarden {
    /// Its id.
    pub id: GardenId,
    /// Its name.
    pub name: String,
    /// Its location.
    pub location: ResolvedLocation,
}

/// `PATCH /gardens/{id}`, resolved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GardenPatch {
    /// A new name.
    pub name: Option<String>,
    /// A new location.
    pub location: Option<ResolvedLocation>,
}

/// What the pipeline and the scheduler need of a garden.
#[derive(Debug, Clone, PartialEq)]
pub struct GardenContext {
    /// The garden.
    pub id: GardenId,
    /// Its name.
    pub name: String,
    /// Its centroid.
    pub coordinates: Coordinates,
    /// How precise it is.
    pub precision: LocationPrecision,
    /// Its weather cache cell.
    pub weather_cell: WeatherCell,
    /// Its time zone.
    pub time_zone: TimeZone,
    /// `SoilGrids`, when fetched.
    pub soil_profile: Option<SoilProfile>,
    /// The current map version, 0 for none.
    pub map_version: u32,
}

/// A new invite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInvite {
    /// Its id.
    pub id: InviteId,
    /// 26 random base32 characters.
    pub code: InviteCode,
    /// The role it grants.
    pub role: Role,
    /// 7 days from now.
    pub expires_at: Timestamp,
}

/// What accepting an invite did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedInvite {
    /// The garden.
    pub garden: GardenId,
    /// False for an existing member, whose role is unchanged.
    pub joined: bool,
}

/// What removing a member leaves for the shared-garden step of the cascade ([contracts 9],
/// removing a member): their conversations in the garden, with their chat image prefixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedMember {
    /// The garden.
    pub garden: GardenId,
    /// The member.
    pub user: UserId,
    /// Their conversations in the garden.
    pub conversations: Vec<ConversationId>,
    /// Their chat images' prefixes, deleted before the records.
    pub chat_prefixes: Vec<ObjectPrefix>,
}

/// What a role change did: the member as they are now, and whether an owner lost the role, in
/// which case the transaction also marked the farm connectors they created in the garden
/// `deleting` ([connectors 8]), for the deletion runner to finish after the commit.
///
/// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleChange {
    /// The member after the change.
    pub member: Member,
    /// The member was an owner and no longer is.
    pub demoted_owner: bool,
}

/// The `garden`, `member_of` and `invite` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait GardenRepo: Send + Sync {
    /// `POST /gardens`: the garden and the caller's `owner` membership in one transaction.
    async fn create_garden(&self, owner: UserId, garden: NewGarden) -> Result<Garden, StoreError>;

    /// `GET /gardens`: the caller's, by name.
    async fn gardens(&self, user: UserId, page: PageRequest) -> Result<Page<Garden>, StoreError>;

    /// `GET /gardens/{id}`.
    async fn garden(&self, member: Membership<Viewer>) -> Result<Garden, StoreError>;

    /// `PATCH /gardens/{id}`.
    async fn update_garden(
        &self,
        owner: Membership<Owner>,
        patch: GardenPatch,
    ) -> Result<Garden, StoreError>;

    /// The garden as the pipeline and scheduler read it.
    async fn garden_context(&self, garden: GardenId) -> Result<GardenContext, StoreError>;

    /// Stores the `SoilGrids` profile, or marks it unavailable for a later retry.
    async fn set_soil_profile(
        &self,
        garden: GardenId,
        status: SoilProfileStatus,
        profile: Option<SoilProfile>,
    ) -> Result<(), StoreError>;

    /// Gardens whose `SoilGrids` fetch is pending or failed, for the background retry.
    async fn gardens_without_soil_profile(
        &self,
        limit: u32,
    ) -> Result<Vec<(GardenId, Coordinates)>, StoreError>;

    /// A failed `SoilGrids` fetch ([contracts 9], the soil profile): `soil_profile_status:
    /// unavailable` and the earliest next attempt in `garden.soil_retry_at`, which
    /// [`crate::rules::limits::soil_retry_after`] sets 15 minutes ahead after an outage and 7
    /// days ahead after no data. Storing a `ready` profile clears it.
    ///
    /// The default body stores the status without the time, so every failed garden is due again
    /// at once; the store overrides it.
    async fn soil_profile_failed(
        &self,
        garden: GardenId,
        retry_at: Timestamp,
    ) -> Result<(), StoreError> {
        let _ = retry_at;
        self.set_soil_profile(garden, SoilProfileStatus::Unavailable, None)
            .await
    }

    /// Gardens due a `SoilGrids` attempt, for the scheduler's `soil` task: `pending`, or
    /// `unavailable` with no `soil_retry_at` or one at or before `now`; never a garden being
    /// deleted; the longest-waiting first; at most `limit`.
    ///
    /// The default body ignores `soil_retry_at` (it is
    /// [`gardens_without_soil_profile`](Self::gardens_without_soil_profile)); the store
    /// overrides it.
    async fn gardens_due_soil_profile(
        &self,
        now: Timestamp,
        limit: u32,
    ) -> Result<Vec<(GardenId, Coordinates)>, StoreError> {
        let _ = now;
        self.gardens_without_soil_profile(limit).await
    }

    /// `GET /gardens/{id}/members`, by `since`.
    async fn members(
        &self,
        member: Membership<Viewer>,
        page: PageRequest,
    ) -> Result<Page<Member>, StoreError>;

    /// Every member's user id, for alert pushes and the garden-deleted push.
    async fn member_ids(&self, garden: GardenId) -> Result<Vec<UserId>, StoreError>;

    /// `GET /gardens/{id}/invites`: unused and unexpired at `now`, newest first.
    async fn invites(
        &self,
        owner: Membership<Owner>,
        page: PageRequest,
        now: Timestamp,
    ) -> Result<Page<Invite>, StoreError>;

    /// `POST /gardens/{id}/invites`.
    async fn create_invite(
        &self,
        owner: Membership<Owner>,
        invite: NewInvite,
    ) -> Result<Invite, StoreError>;

    /// `DELETE /gardens/{id}/invites/{invite_id}`: sets `revoked_at`.
    async fn revoke_invite(
        &self,
        owner: Membership<Owner>,
        invite: InviteId,
        now: Timestamp,
    ) -> Result<(), StoreError>;

    /// `POST /invites/{code}/accept`: `INSERT RELATION` into `member_of`, which fails on an
    /// existing member so a second accept never changes a role ([database 4], membership).
    /// Unknown, used, revoked or expired codes are `NotFound`.
    async fn accept_invite(
        &self,
        user: UserId,
        code: &InviteCode,
        now: Timestamp,
    ) -> Result<AcceptedInvite, StoreError>;

    /// `PATCH /gardens/{id}/members/{user_id}`. Demoting the last owner is
    /// `Rejected(InvalidState)`, checked in the transaction; demoting any other owner marks
    /// their connectors in the garden `deleting` in it ([`RoleChange::demoted_owner`]).
    async fn change_role(
        &self,
        owner: Membership<Owner>,
        member: UserId,
        role: Role,
    ) -> Result<RoleChange, StoreError>;

    /// `DELETE /gardens/{id}/members/{user_id}`: by an owner, or the member themselves (the
    /// caller checks which). The last owner can't leave. Deletes the membership and returns what
    /// the shared-garden cascade step must delete next.
    async fn remove_member(
        &self,
        member: Membership<Viewer>,
        user: UserId,
    ) -> Result<RemovedMember, StoreError>;
}
```

#### Plants

```rust
//! Plants, the timeline and the growth series ([contracts 9], plants; [contracts 3], species
//! confirmation).

use crate::{
    authz::{Editor, Membership, Viewer},
    garden::plants::{GrowthQuantity, MeasurementPoint, TimelineEntry, UpdatePlantRequest},
    id::{GardenId, PlantId, ScanId, SpeciesId},
    ports::StoreError,
    scan::{Plant, PlantStatus, ScanStatus, SpeciesCandidate, SpeciesChoice, SpeciesStatus},
    url::Stored,
    wire::{Page, PageRequest, Timestamp},
};

/// `POST /plants`, with its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlant {
    /// Its id.
    pub id: PlantId,
    /// Its name.
    pub name: String,
    /// Its species, confirmed when given.
    pub species: Option<SpeciesId>,
}

/// The plant's most recent scan, which may follow a species change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentScan {
    /// The scan.
    pub scan: ScanId,
    /// Its status.
    pub status: ScanStatus,
    /// When it was created.
    pub created_at: Timestamp,
    /// The species its run works with.
    pub working_species: Option<SpeciesId>,
}

/// What `POST /plants/{id}/species` stored, and the scan that may follow it
/// ([`crate::rules::scan_state::species_follow`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeciesConfirmation {
    /// The updated plant.
    pub plant: Plant<Stored>,
    /// Its most recent live scan.
    pub recent_scan: Option<RecentScan>,
}

/// The `plant` table and the reads built on it.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait PlantRepo: Send + Sync {
    /// `GET /plants?garden_id=`, by name, with `latest_scan` computed.
    async fn plants(
        &self,
        member: Membership<Viewer>,
        page: PageRequest,
    ) -> Result<Page<Plant<Stored>>, StoreError>;

    /// `GET /plants/{id}`.
    async fn plant(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
    ) -> Result<Plant<Stored>, StoreError>;

    /// `POST /plants`.
    async fn create_plant(
        &self,
        editor: Membership<Editor>,
        plant: NewPlant,
    ) -> Result<Plant<Stored>, StoreError>;

    /// `PATCH /plants/{id}`.
    async fn update_plant(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        patch: UpdatePlantRequest,
    ) -> Result<Plant<Stored>, StoreError>;

    /// `POST /plants/{id}/species`: sets the species (`confirmed`) or the custom name, and
    /// returns the most recent live scan for the follow-up decision.
    async fn confirm_species(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<SpeciesConfirmation, StoreError>;

    /// The pipeline's species suggestion: candidates and `suggested`, unless already confirmed.
    async fn suggest_species(
        &self,
        plant: PlantId,
        candidates: Vec<SpeciesCandidate<Stored>>,
        working: Option<SpeciesId>,
        status: SpeciesStatus,
    ) -> Result<(), StoreError>;

    /// The plant's status after a scan's diagnosis.
    async fn set_plant_status(&self, plant: PlantId, status: PlantStatus)
    -> Result<(), StoreError>;

    /// `GET /plants/{id}/timeline`, newest first, skipping scans being deleted.
    async fn timeline(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
        page: PageRequest,
    ) -> Result<Page<TimelineEntry<Stored>>, StoreError>;

    /// `GET /plants/{id}/measurements`, oldest first: one point per scan and quantity, the
    /// highest-confidence current measurement of medium confidence or more, dated by the scan
    /// ([contracts 7], growth; index `observation_growth`).
    async fn growth_series(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
        quantity: Option<GrowthQuantity>,
        page: PageRequest,
    ) -> Result<Page<MeasurementPoint>, StoreError>;

    /// Every live plant of a garden (none being deleted), for the pipeline's neighbour list
    /// ([contracts 7], neighbours for the plan writer): names and species of the pinned plants.
    /// A system call with no membership proof, like `garden_context`.
    async fn garden_plants(&self, garden: GardenId) -> Result<Vec<Plant<Stored>>, StoreError>;

    /// The plant's whole height series, for fusion's `growth_stalled` rule.
    async fn heights(&self, plant: PlantId) -> Result<Vec<MeasurementPoint>, StoreError>;
}
```

#### Scans, images and masks

```rust
//! Scans, their images and masks ([contracts 3]).

use crate::{
    authz::{Editor, Membership, Reviewer, Viewer},
    events::{RunReason, SelectedCandidate},
    id::{
        FeedbackId, GuidedCheckId, ImageId, MaskId, PlantId, QuestionId, RegionId, ScanId,
        SpeciesId, UserId,
    },
    jobs::RleMask,
    ports::{GardenContext, NewJob, ObjectKey, ObjectRef, QuestionStepClaim, StoreError},
    records::{
        diagnosis::{Diagnosis, IndicatorEvidence},
        observation::Observation,
        plan::ActionPlan,
        weather::WeatherSummary,
    },
    rules::uploads::UploadRoute,
    scan::{
        DepthMeta, FeedbackRequest, ImageKind, NewPlantInput, Plant, Question, Scan, ScanDetail,
        ScanFailure, ScanImage, ScanStatus,
    },
    url::Stored,
    wire::{Run, Timestamp},
};

/// `POST /scans`, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScan {
    /// Its id.
    pub id: ScanId,
    /// The plant, or the plant to create with its new id.
    pub plant: NewScanPlant,
    /// The creator.
    pub created_by: UserId,
    /// Now; also the UTC day the daily scan limit counts in.
    pub created_at: Timestamp,
}

/// The plant of a new scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewScanPlant {
    /// An existing, live plant of the garden.
    Existing(PlantId),
    /// A plant created in the same transaction.
    New {
        /// Its id.
        id: PlantId,
        /// Name and species.
        plant: NewPlantInput,
    },
}

/// A depth map stored with its photo ([contracts 3], depth; [database 3] `scan_image.depth_*`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredDepth {
    /// The 16-bit depth PNG.
    pub depth_key: ObjectKey,
    /// The 8-bit confidence PNG, when sent.
    pub confidence_key: Option<ObjectKey>,
    /// The metadata, transformed into the stored image's frame.
    pub meta: DepthMeta,
    /// The stored depth map's width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// The quality scores of a stored image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageQuality {
    /// Variance of the Laplacian.
    pub blur_score: f64,
    /// Mean exposure.
    pub exposure: f64,
}

/// An image the API processed and wrote to `RustFS`, to record ([storage 4]: the object first,
/// then the record).
#[derive(Debug, Clone, PartialEq)]
pub struct NewScanImage {
    /// Its id.
    pub id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// The question it answers.
    pub question: Option<QuestionId>,
    /// Stored width.
    pub width: u32,
    /// Stored height.
    pub height: u32,
    /// Its storage key.
    pub storage_key: ObjectKey,
    /// Its quality scores.
    pub quality: ImageQuality,
    /// Its depth, when stored.
    pub depth: Option<StoredDepth>,
    /// With `question`: the photo answer's first step, decided by the pipeline before the
    /// upload is recorded (`Pipeline::photo_answer_start`), so the question's step and its job
    /// are written in the same transaction as the image.
    pub photo_answer_start: Option<PhotoAnswerStart>,
    /// Now.
    pub uploaded_at: Timestamp,
}

/// A photo answer's first step ([contracts 3], a photo answer that stalls; [contracts 7],
/// photos sent for a question). Recorded with the image, so an `answering` question always
/// has either a job in flight (`segmenting`) or a step the sweep recovers (`measuring`).
#[derive(Debug, Clone, PartialEq)]
pub enum PhotoAnswerStart {
    /// `segmenting`: the segmentation-only `perception` job, queued in the same transaction.
    /// Boxed: a `NewJob` is about 290 bytes and `Measure` carries nothing, so the box keeps
    /// every `NewScanImage` small (`clippy::large_enum_variant`).
    Segment(Box<NewJob>),
    /// `measuring`: `api`'s own step, with no job; it measures what needs no mask (or nothing)
    /// and queues `verify`, or claims `score`.
    Measure,
}

/// What recording an upload did. The transaction re-checks the scan's status and `deleting_at`,
/// the kind limit and the question's state, so a racing delete or a second photo for the same
/// question loses.
#[derive(Debug, Clone, PartialEq)]
pub struct AddedImage {
    /// The image.
    pub image: ScanImage<Stored>,
    /// Where it went.
    pub route: UploadRoute,
    /// The new guided check, with its `guided` job queued in the same transaction.
    pub guided_check: Option<GuidedCheckId>,
    /// A question photo: the question is now `answering`, in its first step.
    pub photo_answer: Option<QuestionStepClaim>,
}

/// A run to start: submit, resubmit or species change ([contracts 3]). One transaction moves
/// the status, increments `run`, cancels the old run's queued and leased run jobs, supersedes
/// the open question, releases the step claim and queues the first job.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStart {
    /// Why.
    pub reason: RunReason,
    /// The status the decision was made from; the transaction refuses any other.
    pub expected_status: ScanStatus,
    /// The run being replaced (the scan's current `run`).
    pub expected_run: Run,
    /// The working species, when known.
    pub working_species: Option<SpeciesId>,
    /// The run's first job (the `perception` job).
    pub first_job: NewJob,
    /// Now.
    pub now: Timestamp,
}

/// What starting a run did, for the events the per-scan writer sends next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedRun {
    /// The new run.
    pub run: Run,
    /// The status before.
    pub from: ScanStatus,
    /// The status now (`queued` or `analysing`).
    pub to: ScanStatus,
    /// The question superseded, if one was open or `answering`.
    pub superseded_question: Option<QuestionId>,
    /// Jobs cancelled; workers learn it from their next heartbeat.
    pub cancelled_jobs: Vec<crate::id::JobId>,
}

/// What failing a scan did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedScan {
    /// The status before.
    pub from: ScanStatus,
    /// The question superseded in the same transaction: a failed scan never holds one.
    pub superseded_question: Option<QuestionId>,
}

/// A mask to store: the API encoded the worker's RLE as a PNG and wrote it to `RustFS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMask {
    /// Its id.
    pub id: MaskId,
    /// The image.
    pub image: ImageId,
    /// The region it outlines.
    pub region_id: RegionId,
    /// Its storage key.
    pub storage_key: ObjectKey,
    /// The worker's mask, kept for measurement.
    pub rle: RleMask,
}

/// Everything a step reads when it starts ([contracts 3]: every step re-reads its inputs).
#[derive(Debug, Clone, PartialEq)]
pub struct ScanSnapshot {
    /// The scan, its images, guided checks and open question.
    pub scan: Scan<Stored>,
    /// Its plant.
    pub plant: Plant<Stored>,
    /// Its garden.
    pub garden: GardenContext,
    /// The species the run works with.
    pub working_species: Option<SpeciesId>,
    /// "Something else" was chosen.
    pub custom_species: bool,
    /// The current observations ([contracts 3], observations across runs).
    pub observations: Vec<Observation<Stored>>,
    /// Every stored indicator answer of the run, of every source (`user`, `measurement`,
    /// `vlm`); the scoring keeps one per indicator, ranked user > measurement > vlm.
    pub evidence: Vec<IndicatorEvidence>,
    /// The run's candidate set as `retrieve` selected it, plus any a `verify` job added
    /// (`StepOutput::Retrieve.candidates`, `StepOutput::Score.added_candidates`).
    pub candidates: Vec<SelectedCandidate>,
    /// The run's weather snapshot (`scan.weather`), stored by `retrieve`.
    pub weather: Option<WeatherSummary>,
    /// The open question's photo-answer step claim (`question.step`, `step_at`, its `run`)
    /// while it is `answering` with a step. `None` with `answering` means its `verify` job has
    /// completed: the store clears the step in the job's completion transaction, and the next
    /// `score` resolves the question. The broker path uses the claim to move or drop a photo
    /// answer whose job failed for good.
    pub open_question_claim: Option<QuestionStepClaim>,
    /// The indicator the open question asks (`question.indicator`), for an `indicator`
    /// question.
    pub open_question_indicator: Option<crate::id::IndicatorId>,
    /// The masks of the scan's regions, for measurement.
    pub masks: Vec<NewMask>,
    /// Each image's stored depth.
    pub depth: Vec<(ImageId, StoredDepth)>,
    /// Questions asked in the current run.
    pub questions_asked: u32,
    /// Every question of the run, open or resolved.
    pub questions: Vec<Question>,
    /// The current diagnosis.
    pub diagnosis: Option<Diagnosis<Stored>>,
    /// The current plan.
    pub plan: Option<ActionPlan>,
    /// The diagnosis the current plan was written from (`plan.diagnosis_id`), for the
    /// re-planning rules: its run (a species change) and its soil texture (a guided check).
    pub plan_diagnosis: Option<Diagnosis<Stored>>,
    /// New input arrived while a step held the claim.
    pub rescore_pending: bool,
}

/// `POST /scans/{id}/feedback`, with its author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFeedback {
    /// Its id.
    pub id: FeedbackId,
    /// The author; deleted with their account.
    pub user: UserId,
    /// The body.
    pub request: FeedbackRequest,
}

/// The `scan`, `scan_image` and `mask` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait ScanRepo: Send + Sync {
    /// `POST /scans`: creates the scan in `pending` (and the new plant), and counts it in
    /// `usage_daily` in the same transaction ([database 4], limits): over 30 is
    /// `Rejected(BudgetExceeded)` and nothing is stored.
    async fn create_scan(
        &self,
        editor: Membership<Editor>,
        scan: NewScan,
    ) -> Result<Scan<Stored>, StoreError>;

    /// The scan, skipping one being deleted.
    async fn scan(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
    ) -> Result<Scan<Stored>, StoreError>;

    /// `GET /scans/{id}`.
    async fn scan_detail(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
    ) -> Result<ScanDetail<Stored>, StoreError>;

    /// `POST /scans/{id}/images`, after the object is written.
    async fn add_image(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        image: NewScanImage,
    ) -> Result<AddedImage, StoreError>;

    /// `DELETE /scans/{id}/images/{image_id}` while `pending`: deletes the record and returns
    /// the keys (photo, depth, confidence) to delete in the background.
    async fn remove_image(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        image: ImageId,
    ) -> Result<Vec<ObjectKey>, StoreError>;

    /// Submit, resubmit and species change: see [`RunStart`].
    async fn start_run(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        start: RunStart,
    ) -> Result<StartedRun, StoreError>;

    /// Fails the scan's run `run` (job attempts exhausted, `no_plant_found`,
    /// `photos_unusable`, `upstream_unavailable`): releases the claim and supersedes its
    /// question in the same transaction. Refused when the run is no longer current.
    async fn fail_scan(
        &self,
        scan: ScanId,
        run: Run,
        failure: ScanFailure,
        now: Timestamp,
    ) -> Result<FailedScan, StoreError>;

    /// `POST /scans/{id}/feedback`; refused while `pending`.
    async fn record_feedback(
        &self,
        reviewer: Membership<Reviewer>,
        scan: ScanId,
        feedback: NewFeedback,
    ) -> Result<FeedbackId, StoreError>;

    /// `GET /scans/{id}/images/{image_id}`: the object behind a signed URL (the signature is
    /// the authorization). Depth maps are never returned.
    async fn image_object(
        &self,
        scan: ScanId,
        image: ImageId,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// `GET /scans/{id}/masks/{mask_id}`.
    async fn mask_object(
        &self,
        scan: ScanId,
        mask: MaskId,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// Everything a step reads, in one consistent read.
    async fn snapshot(&self, scan: ScanId) -> Result<ScanSnapshot, StoreError>;
}
```

#### Events

```rust
//! Scan events ([contracts 4]; [database 4], scan events).

use crate::{
    authz::{Membership, Viewer},
    events::{ScanEvent, ScanEventBody},
    id::ScanId,
    ports::StoreError,
    rules::stream::StreamState,
    url::Stored,
    wire::{EventSeq, Run, Timestamp},
};

/// An event to append. `seq` is assigned by the store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    /// Its run.
    pub run: Run,
    /// When it is stored.
    pub ts: Timestamp,
    /// The body, with image and mask ids only.
    pub body: ScanEventBody<Stored>,
}

/// The `scan_event` table and `scan.last_event_seq`.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait EventRepo: Send + Sync {
    /// Appends one event: `UPDATE scan SET last_event_seq += 1` and
    /// `CREATE scan_event:[scan, seq]` in one transaction, so `seq` is gap-free. Called only by
    /// the scan's serialised writer, which then broadcasts the returned event.
    async fn append_event(
        &self,
        scan: ScanId,
        event: NewEvent,
    ) -> Result<ScanEvent<Stored>, StoreError>;

    /// Replay: a key-range read of `scan_event:[scan, after + 1]..=[scan, ..]`.
    async fn events_after(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
        after: EventSeq,
    ) -> Result<Vec<ScanEvent<Stored>>, StoreError>;

    /// What the stream's closing rule needs ([contracts 4], when the stream closes).
    async fn stream_state(&self, scan: ScanId) -> Result<StreamState, StoreError>;
}
```

#### Observations, questions and guided checks

```rust
//! Observations, questions, guided checks and indicator evidence ([contracts 3], [contracts 7]).

use crate::{
    authz::{Editor, Membership, Viewer},
    chat::SimilarCase,
    events::Stage,
    id::{ImageId, ObservationId, QuestionId, ScanId},
    ports::{ClaimAttempt, Lease, NewJob, NewMask, QuestionStepClaim, StepClaim, StoreError},
    records::{diagnosis::IndicatorEvidence, observation::Observation},
    rules::questions::QuestionStep,
    scan::{AnswerRequest, GuidedCheckStatus, ScanStatus},
    url::Stored,
    wire::{Run, Timestamp},
};

/// A stage's observations, replacing that stage's earlier ones for the images it covers and
/// those tied to no image ([database 5]: replacement per image; [contracts 3], observations
/// across runs).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationWrite {
    /// The pipeline stage that produced them.
    pub stage: Stage,
    /// The run.
    pub run: Run,
    /// The images the stage covered; their earlier observations of this stage are replaced.
    pub images: Vec<ImageId>,
    /// Also replace this stage's observations tied to no image (a run does; a question photo
    /// doesn't).
    pub replace_imageless: bool,
    /// The new observations.
    pub observations: Vec<Observation<Stored>>,
    /// Masks to record with them.
    pub masks: Vec<NewMask>,
}

/// What storing a stage's output changed, for `finding` and `finding_retracted` events
/// ([contracts 5], partial results).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObservationDiff {
    /// New findings.
    pub added: Vec<Observation<Stored>>,
    /// Findings the final result didn't keep (`retracted_at` set).
    pub retracted: Vec<ObservationId>,
}

/// What answering an indicator question did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnswerOutcome {
    /// A late answer: the scan is `diagnosed` or `planned` and the scheduler had continued.
    pub late: bool,
    /// The `score` claim taken in the same transaction, or `Busy`.
    pub claim: ClaimAttempt,
}

/// A question due for continuation ([contracts 3]: 10 minutes without an answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueQuestion {
    /// The question.
    pub question: QuestionId,
    /// Its scan.
    pub scan: ScanId,
    /// Its run.
    pub run: Run,
}

/// What a photo answer's step does next ([contracts 3], a photo answer that stalls).
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "built once per step and moved straight into one transaction"
)]
pub enum QuestionStepOutput {
    /// Segmentation stored: move to `measuring` (in the result handler's transaction).
    Measure,
    /// Measured: store the measurements and evidence, then queue `verify` and move to
    /// `verifying`.
    Verify {
        /// The photo's measurements.
        observations: ObservationWrite,
        /// Indicators the measurements settled.
        evidence: Vec<IndicatorEvidence>,
        /// The `verify` job.
        job: NewJob,
    },
    /// Measured and nothing left to ask: store them and claim the scan's `score` step.
    Score {
        /// The photo's measurements.
        observations: ObservationWrite,
        /// Indicators the measurements settled.
        evidence: Vec<IndicatorEvidence>,
    },
}

/// What advancing a photo answer's step did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionAdvance {
    /// The question's next step, under a new stamp (`measuring` or `verifying`).
    Next(QuestionStepClaim),
    /// The measurements settled everything ([`QuestionStepOutput::Score`]): the question's
    /// step is cleared (it stays `answering` until `score` resolves it) and the scan's `score`
    /// step was claimed in the same transaction, or `rescore_pending` set (`Busy`), so a crash
    /// between the two can't strand the question.
    Scored(ClaimAttempt),
}

/// A finished guided check ([contracts 7], guided checks and the diagnosis). The store finds
/// the check from the leased job (its payload's image), so no id is passed.
#[derive(Debug, Clone, PartialEq)]
pub struct GuidedOutcome {
    /// `done` or `failed`.
    pub status: GuidedCheckStatus,
    /// The texture observation; none for a failed check or an unsettled jar.
    pub observation: Option<Observation<Stored>>,
}

/// What finishing a guided check did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidedFinish {
    /// The scan's status at that moment.
    pub scan_status: ScanStatus,
    /// The `score` claim on a `diagnosed` or `planned` scan; `NotNeeded` otherwise.
    pub claim: ClaimAttempt,
}

/// The `observation`, `question` and `guided_check` tables, and the indicator evidence.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait AnalysisRepo: Send + Sync {
    /// A `partial_result` from a worker: stores the stage's output once, under the lease.
    async fn store_partial(
        &self,
        lease: &Lease,
        write: ObservationWrite,
    ) -> Result<ObservationDiff, StoreError>;

    /// `api`'s own observations (measurement, organism lengths, VLM findings it converts),
    /// under the step claim.
    async fn store_observations(
        &self,
        claim: &StepClaim,
        write: ObservationWrite,
    ) -> Result<ObservationDiff, StoreError>;

    /// The scan's current observations.
    async fn observations(&self, scan: ScanId) -> Result<Vec<Observation<Stored>>, StoreError>;

    /// `POST /scans/{id}/answers`: stores the user's evidence, resolves the question and claims
    /// `score` (or flags `rescore_pending`) in one transaction. A question that is answered,
    /// superseded or `answering` is `Rejected(InvalidState)`.
    async fn answer_question(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        answer: AnswerRequest,
        evidence: Vec<IndicatorEvidence>,
        now: Timestamp,
    ) -> Result<AnswerOutcome, StoreError>;

    /// Moves a photo answer to its next step under its claim; refused (`QuestionStepLost`)
    /// unless the question is still `answering` with the same step and stamp, its run is the
    /// scan's, and the plant isn't being deleted.
    async fn advance_question_step(
        &self,
        claim: &QuestionStepClaim,
        output: QuestionStepOutput,
        now: Timestamp,
    ) -> Result<QuestionAdvance, StoreError>;

    /// A photo answer that couldn't be analysed: `dropped` when late, reopened otherwise.
    async fn drop_photo_answer(
        &self,
        claim: &QuestionStepClaim,
        now: Timestamp,
    ) -> Result<(), StoreError>;

    /// `measuring` steps older than 2 minutes (index `question_stalled`).
    async fn stalled_question_steps(
        &self,
        older_than: Timestamp,
    ) -> Result<Vec<QuestionStepClaim>, StoreError>;

    /// The sweep's re-claim of a stalled step, by its old stamp.
    async fn reclaim_question_step(
        &self,
        stale: &QuestionStepClaim,
        step: QuestionStep,
        now: Timestamp,
    ) -> Result<Option<QuestionStepClaim>, StoreError>;

    /// Open questions whose `continue_at` has passed (index `question_sweep`), skipping those
    /// whose photo is being analysed.
    async fn questions_due(&self, now: Timestamp) -> Result<Vec<DueQuestion>, StoreError>;

    /// The scheduler continues without an answer: sets `continued_at` and claims `score`.
    async fn continue_without_answer(
        &self,
        due: DueQuestion,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError>;

    /// `find_similar_cases` ([contracts 8]): an exact cosine search over `scan_image.embedding`
    /// within the proof's garden ([database 5]), excluding the query image's own scan; at most
    /// `k` (5 or fewer), best first. `NotFound` when the image isn't a scan image of the garden;
    /// an empty list when it has no embedding.
    async fn similar_cases(
        &self,
        member: Membership<Viewer>,
        image: ImageId,
        k: u32,
    ) -> Result<Vec<SimilarCase>, StoreError>;

    /// A `guided` job's result, under its lease at `now`: updates the check, stores its
    /// observation and claims `score` when the scan is `diagnosed` or `planned`.
    async fn finish_guided_check(
        &self,
        lease: &Lease,
        outcome: GuidedOutcome,
        now: Timestamp,
    ) -> Result<GuidedFinish, StoreError>;
}
```

#### Steps between jobs

```rust
//! `api`'s steps between jobs ([contracts 3], steps between jobs; [database 4], steps between
//! jobs).

use crate::{
    events::SelectedCandidate,
    id::{QuestionId, ScanId},
    ports::{ClaimAttempt, NewJob, ObservationWrite, PlanWrite, StepClaim, StoreError},
    records::{
        diagnosis::{Diagnosis, IndicatorEvidence},
        weather::WeatherSummary,
    },
    rules::steps::PipelineStep,
    scan::{Question, ScanStatus},
    url::Stored,
    wire::{Run, Timestamp},
};

/// A question the `score` step asks.
#[derive(Debug, Clone, PartialEq)]
pub struct NewQuestion {
    /// The question as the app sees it (`status: open`).
    pub question: Question,
    /// The indicator an `indicator` question asks (`question.indicator`, server-side only):
    /// the answer is stored as that indicator's `user` evidence.
    pub indicator: Option<crate::id::IndicatorId>,
}

/// A step's output, committed in one transaction that first re-checks the claim.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutput {
    /// `retrieve`: measurements, the indicators they settled, and the `vlm` job (or none when
    /// the run goes straight to scoring). Hands over by releasing the claim when a job is
    /// queued, else to `score`.
    Retrieve {
        /// The measurement stage's observations.
        observations: Option<ObservationWrite>,
        /// Measured answers.
        evidence: Vec<IndicatorEvidence>,
        /// The run's candidate set, stored for `score` (`scan.candidates`).
        candidates: Vec<SelectedCandidate>,
        /// The weather snapshot (`scan.weather`), when it could be fetched.
        weather: Option<WeatherSummary>,
        /// The `vlm` (or `verify`) job; checked against an existing job of the run first.
        job: Option<NewJob>,
    },
    /// `score`: the diagnosis (a compare-and-set on `scan.diagnosis` like plans). Hands over to
    /// `plan` when `needs_plan`, else releases.
    Score {
        /// The new diagnosis.
        diagnosis: Diagnosis<Stored>,
        /// The status it moves to, when it moves (`diagnosed`); late work moves nothing.
        status: Option<ScanStatus>,
        /// The question this score resolves: a photo answer whose `verify` job finished (an
        /// indicator answer is resolved by `AnalysisRepo::answer_question` itself).
        resolved_question: Option<QuestionId>,
        /// The step's own hand-over: release, or `plan`.
        needs_plan: bool,
    },
    /// `score`, ambiguous and the run may still ask: stores the question, moves the scan to
    /// `awaiting_answer`, increments `questions_asked` and releases the claim. No diagnosis is
    /// stored: the run's diagnosis comes after the answer or the 10-minute continuation.
    Ask {
        /// The question (`status: open`).
        question: NewQuestion,
        /// A photo answer resolved by the same score, as in `Score`.
        resolved_question: Option<QuestionId>,
    },
    /// `score`, when the VLM found an organism or damage pattern outside the candidate set: the
    /// candidates it adds to the run's set, the indicators a measurement settled for them, and
    /// the `verify` job for the rest (checked against an existing job of the run first).
    /// Releases the claim when a job is queued; with no job (measurements settled everything)
    /// hands over to `score`.
    Verify {
        /// The `verify` job (`reason: outside_candidate_set`), unless nothing is left to ask.
        job: Option<NewJob>,
        /// Measured answers for the added candidates.
        evidence: Vec<IndicatorEvidence>,
        /// Candidates added to the run's set (source `organism` or `damage`).
        added_candidates: Vec<SelectedCandidate>,
    },
    /// `plan`: the plan, written with its compare-and-set, then the claim is released.
    Plan(Box<PlanWrite>),
    /// Nothing to write (a re-run that finds its work already done, or a `score` that finds
    /// the run's VLM stage still in flight): releases the claim, or hands over to `score` when
    /// `rescore_pending` is set.
    Release,
}

/// A claim older than its step's limit, found by the sweep (index `scan_stalled`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleStep {
    /// The scan.
    pub scan: ScanId,
    /// The step.
    pub step: PipelineStep,
    /// The run of the claim.
    pub run: Run,
    /// The stamp the sweep saw.
    pub seen: Timestamp,
}

/// The step claim on `scan` ([database 4], steps between jobs).
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait StepRepo: Send + Sync {
    /// An entry point's claim: a compare-and-set on `pipeline_step = NONE` recording the
    /// scan's current run (claiming `score` clears `rescore_pending`); when another step holds
    /// it, sets `rescore_pending` and returns `Busy`.
    async fn claim_step(
        &self,
        scan: ScanId,
        step: PipelineStep,
        now: Timestamp,
    ) -> Result<ClaimAttempt, StoreError>;

    /// Commits a step's output under its claim and hands over: the next claim, or `None` when
    /// released. `Conflict::ClaimLost` when the claim, stamp or run no longer holds or the plant
    /// or garden is being deleted; nothing is written then.
    async fn commit_step(
        &self,
        claim: StepClaim,
        output: StepOutput,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError>;

    /// Claims older than their limit on scans that aren't `pending`, `failed` or being deleted.
    async fn stale_steps(&self, now: Timestamp) -> Result<Vec<StaleStep>, StoreError>;

    /// The sweep's re-claim, a compare-and-set on the old stamp and run; `None` when someone
    /// else has it.
    async fn reclaim_step(
        &self,
        stale: StaleStep,
        now: Timestamp,
    ) -> Result<Option<StepClaim>, StoreError>;
}
```

#### Plans, tasks and trigger rules

```rust
//! Plans, tasks and plan trigger rules ([contracts 7], action plan and task; [contracts 9],
//! tasks; [database 4], plan writes and alert dedupe).

use crate::{
    authz::{Editor, Membership, Viewer},
    garden::tasks::{TaskQuery, UpdateTaskRequest},
    id::{GardenId, PlanId, PlanTriggerId, PlantId, ScanId, StepId, TaskId, UserId},
    ports::{StepClaim, StoreError},
    records::{
        plan::{ActionPlan, PlanReason, TriggerKind},
        task::{AlertKey, Repeat, Task, TaskSource, TaskTrigger},
    },
    wire::{LocalDate, Page, PageRequest, Timestamp},
};

/// A task to create. Every write that creates a task with a plant checks, in its transaction,
/// that the plant is live in the garden ([contracts 7], tasks only on live plants).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    /// Its id.
    pub id: TaskId,
    /// The plant; checked live.
    pub plant: Option<PlantId>,
    /// The plan version.
    pub plan: Option<PlanId>,
    /// The step.
    pub plan_step: Option<StepId>,
    /// The scan (a jar-test task); checked live, of the garden and of the plant.
    pub scan: Option<ScanId>,
    /// Title.
    pub title: String,
    /// Detail.
    pub detail: Option<String>,
    /// Due date.
    pub due: Option<LocalDate>,
    /// Repeat.
    pub repeat: Option<Repeat>,
    /// Who made it.
    pub source: TaskSource,
    /// The rule that fired.
    pub trigger: Option<TaskTrigger>,
    /// The dedupe key, for alerts and plan triggers.
    pub alert_key: Option<AlertKey>,
    /// The user who made it.
    pub created_by: Option<UserId>,
    /// Now.
    pub created_at: Timestamp,
}

/// A plan trigger rule an `on_trigger` step registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlanTrigger {
    /// Its id.
    pub id: PlanTriggerId,
    /// The plant its tasks go to: the step's neighbour or the plan's plant; checked live.
    pub plant: PlantId,
    /// The step.
    pub step: StepId,
    /// The rule.
    pub trigger: TriggerKind,
    /// The last date it fires.
    pub until: Option<LocalDate>,
}

/// A plan version to store with its compare-and-set ([database 4], plan writes): the write
/// goes through only if `scan.plan` still equals `expected` (`None` for a first plan); a loser
/// gets `Conflict::StalePlan` and nothing is stored. The same transaction dismisses the
/// superseded plan's open tasks (`plan_superseded`), deactivates its trigger rules, and creates
/// the new tasks and rules, skipping any whose plant is gone.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanWrite {
    /// The plan as it will be read.
    pub plan: ActionPlan,
    /// Why it was written.
    pub reason: PlanReason,
    /// The plan it replaces.
    pub expected: Option<PlanId>,
    /// Tasks of `now` and `by_date` and `recurring` steps.
    pub tasks: Vec<NewTask>,
    /// Rules of `on_trigger` steps.
    pub triggers: Vec<NewPlanTrigger>,
}

/// An active plan trigger rule, for the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTriggerRule {
    /// The rule.
    pub id: PlanTriggerId,
    /// Its plan version.
    pub plan: PlanId,
    /// Its garden.
    pub garden: GardenId,
    /// The plant its tasks go to.
    pub plant: PlantId,
    /// The step.
    pub step: StepId,
    /// The rule.
    pub trigger: TriggerKind,
    /// The last date it fires.
    pub until: Option<LocalDate>,
    /// The step's title, read from its plan: the task's title when the rule fires.
    pub title: String,
    /// The step's detail: the task's detail.
    pub detail: String,
}

/// The `action_plan`, `task` and `plan_trigger` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait PlanRepo: Send + Sync {
    /// The scan's current plan.
    async fn current_plan(&self, scan: ScanId) -> Result<Option<ActionPlan>, StoreError>;

    /// A plan version of the garden.
    async fn plan(
        &self,
        member: Membership<Viewer>,
        plan: PlanId,
    ) -> Result<ActionPlan, StoreError>;

    /// The `plan` step's write, under its claim.
    async fn write_plan(
        &self,
        claim: &StepClaim,
        write: PlanWrite,
    ) -> Result<ActionPlan, StoreError>;

    /// A confirmed chat revision, stored exactly as previewed: the same compare-and-set with
    /// `expected = base_plan_id`, never retried ([contracts 7], re-planning). Runs on the
    /// scan's serialised writer.
    async fn write_revision(
        &self,
        editor: Membership<Editor>,
        write: PlanWrite,
    ) -> Result<ActionPlan, StoreError>;

    /// `GET /tasks`: by `due` (none last), then `created_at` (index `task_garden`).
    async fn tasks(
        &self,
        member: Membership<Viewer>,
        query: TaskQuery,
        page: PageRequest,
    ) -> Result<Page<Task>, StoreError>;

    /// A task of the garden.
    async fn task(&self, member: Membership<Viewer>, task: TaskId) -> Result<Task, StoreError>;

    /// `POST /tasks` and a confirmed `create_task`: a dead plant or scan is `NotFound`.
    async fn create_task(
        &self,
        editor: Membership<Editor>,
        task: NewTask,
    ) -> Result<Task, StoreError>;

    /// `PATCH /tasks/{id}`: completing a repeating task creates the next occurrence in the same
    /// transaction (`next` is its new id and due date, or `None` past `until`).
    async fn update_task(
        &self,
        editor: Membership<Editor>,
        task: TaskId,
        patch: UpdateTaskRequest,
        next: Option<NewTask>,
        now: Timestamp,
    ) -> Result<Task, StoreError>;

    /// `DELETE /tasks/{id}`: `user` and `chat` tasks only; others are
    /// `Rejected(InvalidState)`.
    async fn delete_task(&self, editor: Membership<Editor>, task: TaskId)
    -> Result<(), StoreError>;

    /// The scheduler's alert and plan-trigger tasks: the unique `alert_key` makes a duplicate
    /// insert fail, returned as `Ok(None)` ([database 4], alert dedupe). A dead plant also
    /// yields `None`.
    async fn create_alert_task(
        &self,
        garden: GardenId,
        task: NewTask,
    ) -> Result<Option<Task>, StoreError>;

    /// Active plan trigger rules of these gardens (index `plan_trigger_active`).
    async fn active_triggers(
        &self,
        gardens: &[GardenId],
    ) -> Result<Vec<PlanTriggerRule>, StoreError>;
}
```

#### Chat

```rust
//! Conversations, messages, chat images, pending actions and message feedback ([contracts 8]).

use crate::{
    authz::{ConversationOwner, Editor, Membership, Viewer},
    chat::{
        Anchor, ChatRoute, ConfirmResult, Conversation, ConversationDetail, ConversationQuery,
        Message, MessageFeedbackRequest, MessageRole, PendingAction,
    },
    connectors::FarmCitation,
    id::{ChatImageId, ConversationId, MessageId, PendingActionId, UserId},
    ports::{ContentPart, ObjectKey, ObjectRef, StoreError},
    records::diagnosis::Citation,
    url::Stored,
    wire::{Page, PageRequest, Timestamp},
};

/// `POST /conversations`, with its id and owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    /// Its id.
    pub id: ConversationId,
    /// Its owner.
    pub user: UserId,
    /// Its anchor; checked live and in the garden.
    pub anchor: Anchor,
    /// Now.
    pub created_at: Timestamp,
}

/// A chat image written to `RustFS`, to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChatImage {
    /// Its id.
    pub id: ChatImageId,
    /// Stored width.
    pub width: u32,
    /// Stored height.
    pub height: u32,
    /// Its storage key.
    pub storage_key: ObjectKey,
    /// Now.
    pub uploaded_at: Timestamp,
}

/// A message to store. Tool calls and results are stored but never sent to the app.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMessage {
    /// Its id.
    pub id: MessageId,
    /// Its author.
    pub role: MessageRole,
    /// The text.
    pub text: String,
    /// The user's images; each must belong to the conversation.
    pub attachments: Vec<ChatImageId>,
    /// Sources cited.
    pub citations: Vec<Citation>,
    /// Farm records an assistant message cites ([connectors 7]); empty for a user message.
    ///
    /// [connectors 7]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#7-the-agent
    pub farm_citations: Vec<FarmCitation>,
    /// Tool calls and results, in order.
    pub tool_calls: Vec<ContentPart>,
    /// The classifier's route.
    pub route: Option<ChatRoute>,
    /// The stream dropped before it finished.
    pub interrupted: bool,
    /// The model.
    pub model: Option<String>,
    /// The prompt version.
    pub prompt_version: Option<String>,
    /// Prompt tokens.
    pub tokens_in: u32,
    /// Completion tokens.
    pub tokens_out: u32,
    /// Now.
    pub created_at: Timestamp,
}

/// A stored message with its tool calls, for rebuilding the context.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMessage {
    /// The message as the app sees it.
    pub message: Message<Stored>,
    /// Its tool calls and results.
    pub tool_calls: Vec<ContentPart>,
}

/// A card to store with the message that proposed it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPendingAction {
    /// The card; `status: proposed`.
    pub action: PendingAction,
    /// The assistant message.
    pub message: MessageId,
}

/// A stored card with the result of its confirmation, replayed on a confirm retry.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingActionRecord {
    /// The card.
    pub action: PendingAction,
    /// What confirming stored.
    pub result: Option<ConfirmResult>,
}

/// The chat tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait ChatRepo: Send + Sync {
    /// `POST /conversations`: the anchor must be a live record of the garden.
    async fn create_conversation(
        &self,
        member: Membership<Viewer>,
        conversation: NewConversation,
    ) -> Result<Conversation, StoreError>;

    /// `GET /conversations`: the caller's own, newest `last_message_at` first.
    async fn conversations(
        &self,
        member: Membership<Viewer>,
        query: ConversationQuery,
        page: PageRequest,
    ) -> Result<Page<Conversation>, StoreError>;

    /// `GET /conversations/{id}`: messages newest first.
    async fn conversation_detail(
        &self,
        owner: ConversationOwner<Viewer>,
        page: PageRequest,
    ) -> Result<ConversationDetail<Stored>, StoreError>;

    /// `POST /conversations/{id}/images`, after the object is written.
    async fn add_chat_image(
        &self,
        owner: ConversationOwner<Viewer>,
        image: NewChatImage,
    ) -> Result<ChatImageId, StoreError>;

    /// `GET /conversations/{id}/images/{image_id}`, behind a signed URL.
    async fn chat_image_object(
        &self,
        conversation: ConversationId,
        image: ChatImageId,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// Stores a user or assistant message and sets `last_message_at`.
    async fn store_message(
        &self,
        owner: ConversationOwner<Viewer>,
        message: NewMessage,
    ) -> Result<Message<Stored>, StoreError>;

    /// The summary and the most recent messages, for the context builder.
    async fn context(
        &self,
        owner: ConversationOwner<Viewer>,
        recent: u32,
    ) -> Result<(Option<String>, Vec<ContextMessage>), StoreError>;

    /// The number of stored messages of the conversation (user and assistant), for the summary
    /// trigger (architecture 8.2).
    async fn message_count(&self, owner: ConversationOwner<Viewer>) -> Result<u32, StoreError>;

    /// Stores a new conversation summary.
    async fn set_summary(
        &self,
        owner: ConversationOwner<Viewer>,
        summary: String,
    ) -> Result<(), StoreError>;

    /// A write tool's card. Editors and owners only.
    async fn create_pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: NewPendingAction,
    ) -> Result<PendingAction, StoreError>;

    /// A card of the conversation, with its stored result.
    async fn pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
    ) -> Result<PendingActionRecord, StoreError>;

    /// Marks a `proposed` card `confirmed` with what it stored, in the transaction that stores
    /// it; a card already `confirmed` returns its stored result.
    async fn mark_confirmed(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
        result: ConfirmResult,
        now: Timestamp,
    ) -> Result<PendingAction, StoreError>;

    /// `POST /pending-actions/{id}/reject`.
    async fn reject_pending_action(
        &self,
        owner: ConversationOwner<Editor>,
        action: PendingActionId,
        now: Timestamp,
    ) -> Result<PendingAction, StoreError>;

    /// The cleanup: `proposed` cards past `expires_at` become `expired`.
    async fn expire_pending_actions(&self, now: Timestamp) -> Result<u32, StoreError>;

    /// `POST /messages/{id}/feedback`: an upsert on `message_feedback:[message]`, assistant
    /// messages only.
    async fn rate_message(
        &self,
        owner: ConversationOwner<Viewer>,
        message: MessageId,
        feedback: MessageFeedbackRequest,
    ) -> Result<(), StoreError>;

    /// The conversation a message belongs to, for the feedback route's check.
    async fn message_conversation(
        &self,
        message: MessageId,
    ) -> Result<Option<ConversationId>, StoreError>;
}
```

#### Knowledge reads

```rust
//! The knowledge tables as the `knowledge` crate reads them ([database 3], knowledge;
//! [database 5], vector search).

use crate::{
    chat::KnowledgeHit,
    garden::species::SpeciesPhoto,
    id::{ProblemId, SpeciesId},
    knowledge::{
        ClassifierLabels, OrganismSeed, ProblemSeed, ProductsSeed, SoilConditionSeed, SourceSeed,
        SpeciesSeed, TreatmentSeed,
    },
    ports::{ChunkFilter, Embedding, ObjectRef, StoreError, VlmOptions},
    url::PhotoHash,
};

/// The whole curated knowledge base, read once at start: a few hundred records that change
/// only with a migration. Edges are resolved: `looks_like` in both directions, `preys_on` on
/// each organism.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KnowledgeSnapshot {
    /// Every species.
    pub species: Vec<SpeciesSeed>,
    /// Each species' stored photos (hash and credit), in seed order.
    pub species_photos: Vec<(SpeciesId, Vec<SpeciesPhoto>)>,
    /// Every problem.
    pub problems: Vec<ProblemSeed>,
    /// Both directions of every `looks_like` edge: (problem, look-alike, how to tell).
    pub lookalikes: Vec<(ProblemId, ProblemId, String)>,
    /// Every organism, with `preys_on` filled.
    pub organisms: Vec<OrganismSeed>,
    /// Every treatment.
    pub treatments: Vec<TreatmentSeed>,
    /// Every source.
    pub sources: Vec<SourceSeed>,
    /// `products.yaml`.
    pub products: ProductsSeed,
    /// The current classifier labels.
    pub classifier_labels: ClassifierLabels,
    /// Every soil condition.
    pub soil_conditions: Vec<SoilConditionSeed>,
    /// The generated VLM options.
    pub vlm_options: VlmOptions,
}

/// The knowledge reads.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait KnowledgeRepo: Send + Sync {
    /// Every curated record, in one read.
    async fn load_knowledge(&self) -> Result<KnowledgeSnapshot, StoreError>;

    /// HNSW search over `knowledge_chunk.embedding` with the filters applied first:
    /// `WHERE … AND embedding <|k, 40|> $q`.
    async fn search_chunks(
        &self,
        query: &Embedding,
        filter: ChunkFilter,
        k: usize,
    ) -> Result<Vec<KnowledgeHit>, StoreError>;

    /// `GET /species/{id}/images/{hash}`: the photo's key under `knowledge/`.
    async fn species_photo_object(
        &self,
        species: &SpeciesId,
        hash: PhotoHash,
    ) -> Result<Option<ObjectRef>, StoreError>;
}
```

#### Jobs and workers

```rust
//! The job broker's storage ([contracts 5]; [database 4], job claim with `slots`).

use std::time::Duration;

use crate::{
    garden::ops::Status,
    id::{ImageId, JobId, QuestionId, ScanId},
    jobs::{Heartbeat, HeartbeatResponse, Job, JobError, JobKind, JobSpec},
    ports::{ClaimAttempt, Lease, ObjectRef, ObservationWrite, StoreError},
    records::diagnosis::IndicatorEvidence,
    rules::{jobs::ClaimPolicy, steps::PipelineStep},
    wire::{Run, Timestamp},
};

/// A job to queue.
#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    /// Its id (the payload's image URLs contain it).
    pub id: JobId,
    /// Its scan.
    pub scan: ScanId,
    /// Its run; `None` for guided jobs.
    pub run: Option<Run>,
    /// A photo answer's job: top-level `job.question`, so the fallback's claim can hold it back.
    pub question: Option<QuestionId>,
    /// Kind and payload.
    pub spec: JobSpec,
    /// The kind's `max_runtime` on a GPU worker (doubled at claim for the fallback).
    pub max_runtime: Duration,
    /// `queued_at`.
    pub queued_at: Timestamp,
}

/// A leased job, as the claim returns it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedJob {
    /// The job body for the worker.
    pub job: Job,
    /// The scan moved from `queued` to `analysing` in the claim's transaction (the run's first
    /// job), for the `status_changed` event.
    pub started_analysing: bool,
}

/// What the lease check found about a job, for `events` and `complete`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeasedJob {
    /// The job.
    pub job: JobId,
    /// Its kind.
    pub kind: JobKind,
    /// Its scan.
    pub scan: ScanId,
    /// Its run.
    pub run: Option<Run>,
    /// Its question.
    pub question: Option<QuestionId>,
}

/// A job's completion, stored under its lease with everything it produced and the step claim
/// it hands to, in one transaction ([contracts 3], claiming).
#[derive(Debug, Clone, PartialEq)]
pub enum JobCompletion {
    /// `status: done`.
    Done {
        /// The raw result, kept on the job row.
        result: serde_json::Value,
        /// The result's observations, per stage.
        observations: Vec<ObservationWrite>,
        /// The VLM's indicator answers.
        evidence: Vec<IndicatorEvidence>,
        /// The step to claim: `retrieve` after a run's perception result, `score` after `vlm`
        /// or a run `verify`; `None` for guided jobs and question photos (handled by their own
        /// writes).
        claim: Option<PipelineStep>,
    },
    /// `status: failed`.
    Failed {
        /// The worker's error.
        error: JobError,
    },
}

/// What completing did.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionOutcome {
    /// Stored; the claim taken, when one was asked.
    Stored {
        /// The claim.
        claim: ClaimAttempt,
        /// What the final result changed against the job's partial results, for `finding`
        /// and `finding_retracted` events ([contracts 5], partial results).
        diff: crate::ports::ObservationDiff,
    },
    /// The same lease token completed this job before: `204`, nothing changed.
    AlreadyCompleted,
    /// A retryable failure with attempts left: requeued.
    Requeued,
    /// Failed for good: the caller applies [`crate::rules::jobs::final_failure`].
    FailedForGood {
        /// The job.
        job: LeasedJob,
    },
}

/// A live worker as its last heartbeat described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveWorker {
    /// Its capabilities.
    pub caps: Vec<crate::jobs::Capability>,
    /// Its `versions` (contracts 5, `model_versions` keys), including `label_set_version` and
    /// `classifier_labels_version` on perception and fallback workers.
    pub versions: std::collections::BTreeMap<String, String>,
}

/// A job the scheduler requeued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequeuedJob {
    /// The job.
    pub job: LeasedJob,
    /// Out of attempts after this requeue: failed for good instead.
    pub exhausted: bool,
}

/// The `job` and `worker` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait JobRepo: Send + Sync {
    /// Queues a job on its own (a guided check's job; run jobs are queued in step commits and
    /// run starts). Signals the broker's `Notify` after commit; so does every other write that
    /// queues or requeues a job (step commits, run starts, `add_image`, a retryable failure in
    /// `complete_job`, `requeue_stale`).
    async fn enqueue_job(&self, job: NewJob) -> Result<JobId, StoreError>;

    /// The claim ([database 4]): counts the worker's leased jobs against `slots`, then leases
    /// the oldest eligible queued job of its kinds with a fresh `lease_token`, re-checking
    /// `status = 'queued'`, in one transaction. `None` when nothing is eligible.
    async fn claim_job(
        &self,
        worker: &crate::id::WorkerId,
        slots: u32,
        policy: ClaimPolicy,
        now: Timestamp,
    ) -> Result<Option<ClaimedJob>, StoreError>;

    /// A heartbeat: upserts `worker:[worker_id]`, extends each listed lease whose token matches
    /// and isn't cancelled (never past `claimed_at + max_runtime`), and returns the jobs to
    /// cancel.
    async fn heartbeat(
        &self,
        heartbeat: Heartbeat,
        now: Timestamp,
    ) -> Result<HeartbeatResponse, StoreError>;

    /// Checks a lease for `POST /internal/jobs/{id}/events` and `POST /internal/jobs/{id}/complete`
    /// and returns the job's head. Accepts a live lease (`leased`, same token and worker,
    /// `lease_until >= now`) **and** a job already `done` or `failed` with the same token and
    /// worker, so a retried `complete` reaches `complete_job`, which answers
    /// `CompletionOutcome::AlreadyCompleted` (contracts 5: `204`, nothing changes). Anything else
    /// (requeued, expired, cancelled, another token) is `Conflict::LeaseLost`.
    async fn check_lease(&self, lease: &Lease, now: Timestamp) -> Result<LeasedJob, StoreError>;

    /// Checks a lease for `POST /internal/jobs/{id}/events`: `Some` for a live lease, `None` for
    /// a job already `done` or `failed` with the same token and worker (late events after
    /// `complete`: `api` answers `204` and drops them, never forwarding them to the pipeline),
    /// and `Conflict::LeaseLost` for anything else, as [`check_lease`](Self::check_lease).
    ///
    /// The default body falls back to `check_lease` and treats every accepted job as live; the
    /// store overrides it with one query that reads the job's status.
    async fn live_lease(
        &self,
        lease: &Lease,
        now: Timestamp,
    ) -> Result<Option<LeasedJob>, StoreError> {
        self.check_lease(lease, now).await.map(Some)
    }

    /// `GET /internal/jobs/{job_id}/images/{image_id}`: the image, if it is in the leased job's
    /// payload.
    async fn job_image(
        &self,
        lease: &Lease,
        image: ImageId,
        now: Timestamp,
    ) -> Result<Option<ObjectRef>, StoreError>;

    /// `POST /internal/jobs/{id}/complete`.
    async fn complete_job(
        &self,
        lease: &Lease,
        completion: JobCompletion,
        now: Timestamp,
    ) -> Result<CompletionOutcome, StoreError>;

    /// Requeues leases past `lease_until` (index `job_lease`) and the jobs of workers silent for
    /// 30 seconds, resetting `queued_at` and counting attempts ([`crate::rules::jobs`]). Signals
    /// the broker's `Notify` after commit when any job went back to `queued`, like every other
    /// write that queues a job: the store's `Notify` is the claim long poll's only wake-up.
    async fn requeue_stale(&self, now: Timestamp) -> Result<Vec<RequeuedJob>, StoreError>;

    /// The capabilities and `versions` of every worker that heartbeated in the last 30 seconds,
    /// so a `perception` job never names a `label_set_version` or `classifier_labels_version`
    /// that no live perception or fallback worker has loaded ([contracts 5], versions a worker
    /// hasn't loaded).
    async fn live_workers(&self, now: Timestamp) -> Result<Vec<LiveWorker>, StoreError>;

    /// Whether a worker with the `perception` capability heartbeated in the last 30 seconds.
    async fn perception_alive(&self, now: Timestamp) -> Result<bool, StoreError>;

    /// `GET /status`.
    async fn status(&self, now: Timestamp) -> Result<Status, StoreError>;

    /// Cleanup: finished jobs older than the cutoff, in batches of 500.
    async fn delete_finished_jobs(&self, before: Timestamp) -> Result<u32, StoreError>;
}
```

#### Idempotency and usage

```rust
//! Idempotency keys and usage counters ([contracts 1], idempotency; [database 4],
//! idempotency keys and limits).

use crate::{
    chat::Usage,
    id::{IdempotencyKey, UserId},
    ports::StoreError,
    wire::Timestamp,
};

/// A key's scope: unique per (user, method, path), kept 24 hours.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyScope {
    /// The caller.
    pub user: UserId,
    /// `"POST /scans/01JAX…/images"`.
    pub route: String,
    /// The header's UUID.
    pub key: IdempotencyKey,
}

/// A stored response: `2xx` and the deterministic `4xx` only.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredResponse {
    /// The HTTP status.
    pub status: u16,
    /// The JSON body.
    pub body: serde_json::Value,
}

/// What `begin` found ([contracts 1], the rules table).
#[derive(Debug, Clone, PartialEq)]
pub enum IdempotencyBegin {
    /// First use: the caller runs the request, then `finish` or `release`.
    Started,
    /// Same fingerprint, finished: replay with the original status.
    Replay(StoredResponse),
    /// Same fingerprint, still running: `409 request_in_progress`.
    InProgress,
    /// Different fingerprint: `422 idempotency_conflict`.
    Conflict,
}

/// The `idempotency_key` table.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait IdempotencyRepo: Send + Sync {
    /// `CREATE idempotency_key:[user, route, key]`; on "already exists", reads the record and
    /// decides.
    async fn begin(
        &self,
        scope: &IdempotencyScope,
        fingerprint: &str,
        expires_at: Timestamp,
    ) -> Result<IdempotencyBegin, StoreError>;

    /// Stores the response and sets `state = 'done'`.
    async fn finish(
        &self,
        scope: &IdempotencyScope,
        response: StoredResponse,
    ) -> Result<(), StoreError>;

    /// Deletes the key: a `409`, `429` or `5xx`, or an interrupted chat message, so a retry runs
    /// again.
    async fn release(&self, scope: &IdempotencyScope) -> Result<(), StoreError>;

    /// Cleanup: expired keys, in batches of 500.
    async fn delete_expired(&self, now: Timestamp) -> Result<u32, StoreError>;
}

/// A user's usage on one UTC day (`usage_daily:[user, day]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UsageTotals {
    /// Scans created.
    pub scans: u32,
    /// Requests.
    pub requests: u32,
    /// Chat tokens in.
    pub tokens_in: u64,
    /// Chat tokens out.
    pub tokens_out: u64,
}

/// The `usage_daily` table. The scan count is incremented inside `ScanRepo::create_scan`'s
/// transaction; chat tokens here.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait UsageRepo: Send + Sync {
    /// The day's totals, for the budget check before a chat turn.
    async fn usage(&self, user: UserId, day: chrono::NaiveDate) -> Result<UsageTotals, StoreError>;

    /// `UPSERT … SET tokens_in += …, tokens_out += …` after a chat turn.
    async fn add_chat_usage(
        &self,
        user: UserId,
        day: chrono::NaiveDate,
        usage: Usage,
    ) -> Result<UsageTotals, StoreError>;
}
```

#### Weather cache

```rust
//! The weather cache ([contracts 7], fetching and caching; [database 3]
//! `weather_cache:[hour, cell]`).

use crate::{
    garden::gardens::TimeZone,
    id::GardenId,
    ports::StoreError,
    records::weather::{CellWeather, WeatherCell},
    wire::Timestamp,
};

/// The gardens that share a cell, for the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellGardens {
    /// The cell.
    pub cell: WeatherCell,
    /// Its time zone.
    pub time_zone: TimeZone,
    /// Its gardens.
    pub gardens: Vec<GardenId>,
}

/// The `weather_cache` table and the garden grouping.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait WeatherCacheRepo: Send + Sync {
    /// The cell's weather for `hour` (truncated to the hour), if cached.
    async fn cached(
        &self,
        cell: WeatherCell,
        hour: Timestamp,
    ) -> Result<Option<CellWeather>, StoreError>;

    /// Caches a cell's weather; the first write of the hour wins.
    async fn put(
        &self,
        cell: WeatherCell,
        hour: Timestamp,
        weather: CellWeather,
    ) -> Result<(), StoreError>;

    /// Every live garden grouped by cell (index `garden_cell`).
    async fn gardens_by_cell(&self) -> Result<Vec<CellGardens>, StoreError>;

    /// Cleanup: one key-range delete of hours before `cutoff`.
    async fn delete_before(&self, cutoff: Timestamp) -> Result<u32, StoreError>;
}
```

#### Garden map

```rust
//! The garden map ([contracts 9], garden map; [database 4], garden map versions).

use crate::{
    authz::{Editor, Membership, Viewer},
    garden::map::{GardenMap, MapUpload},
    id::{GardenId, PlantId},
    ports::StoreError,
    wire::Timestamp,
};

/// A checked upload with the ids of the plants it creates, in pin order.
#[derive(Debug, Clone, PartialEq)]
pub struct MapWrite {
    /// The checked upload.
    pub upload: MapUpload,
    /// One id per `new_plant` pin, in order.
    pub new_plant_ids: Vec<PlantId>,
    /// Now.
    pub created_at: Timestamp,
}

/// The `garden_map` table and `plant.position`.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait MapRepo: Send + Sync {
    /// `GET /gardens/{id}/map`: the current map, leaving out pins of plants being deleted.
    async fn map(&self, member: Membership<Viewer>) -> Result<Option<GardenMap>, StoreError>;

    /// `PUT /gardens/{id}/map`, in one transaction: checks `base_version` against
    /// `garden.map_version` (`Conflict::StaleMap` otherwise), drops pins of plants being
    /// deleted, refuses pins of plants not in the garden (`Rejected(UnknownPlants)` listing every
    /// one), creates the new plants, increments the version, writes the map, deletes older maps,
    /// and sets or unsets every plant's `position`.
    async fn put_map(
        &self,
        editor: Membership<Editor>,
        write: MapWrite,
    ) -> Result<GardenMap, StoreError>;

    /// The current map for the pipeline (neighbours, crowding) and `get_garden_map`.
    async fn current_map(&self, garden: GardenId) -> Result<Option<GardenMap>, StoreError>;
}
```

#### Farm connectors and records

The `connector` and `farm_record` tables ([connectors 4](../connectors/README.md#4-data)), implemented by the store in roadmap task E1.6 ([store 16](store.md#16-chat-memory-and-knowledge)). No route reaches them before E3.

```rust
//! Farm connectors and their synced records ([connectors 4], [connectors 6]): the `connector`
//! and `farm_record` tables. Owner routes take a `Membership<Owner>` from the connector's
//! garden (`GardenResource::Connector`); the `connectors` scheduler task, the cleanup and the
//! deletion runner call the internal methods, which take no proof.
//!
//! [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
//! [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync

use std::{collections::BTreeMap, fmt};

use crate::{
    authz::{Membership, Owner, Viewer},
    connectors::{
        Connector, ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource,
        ConnectorStatus, FarmRecordHit, FarmRecordKind, SyncErrorCode,
    },
    id::{ConnectorId, DeviceHash, FarmRecordId, GardenId, UserId},
    ports::{Embedding, StoreError, SyncCursor},
    wire::Timestamp,
};

/// A value sealed with AES-256-GCM under `CONNECTOR_KEY`, the connector's id as associated
/// data (`connector.secret`, the PKCE verifier in `oauth`). `Debug` never prints the bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed(pub bytes::Bytes);

impl fmt::Debug for Sealed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sealed({} bytes)", self.0.len())
    }
}

/// A pending connect's `oauth` field: kept until the exchange clears it, so a state works once.
#[derive(Clone, PartialEq, Eq)]
pub struct PendingConnect {
    /// The nonce the signed `state` carries.
    pub nonce: String,
    /// `SHA-256(device_verifier)` from the app that started the connect.
    pub device_hash: DeviceHash,
    /// Google's PKCE verifier, sealed; `None` for Slack.
    pub verifier: Option<Sealed>,
    /// 10 minutes after the start.
    pub expires_at: Timestamp,
    /// When a `complete` claimed the state ([`FarmRecordStore::claim_connect`]) to exchange the
    /// code; `None` until then. A claimed state is spent: the callback and every other
    /// `complete` refuse it.
    pub claimed_at: Option<Timestamp>,
}

impl fmt::Debug for PendingConnect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingConnect")
            .field("nonce", &"[redacted]")
            .field("device_hash", &self.device_hash)
            .field("verifier", &self.verifier)
            .field("expires_at", &self.expires_at)
            .field("claimed_at", &self.claimed_at)
            .finish()
    }
}

/// A connector to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConnector {
    /// Its id.
    pub id: ConnectorId,
    /// Slack or Google Drive.
    pub kind: ConnectorKind,
    /// `pending` for an owner's connect; `active` for the seeded demo connectors.
    pub status: ConnectorStatus,
    /// `vendor` for every route; `era` only from start-up seeding.
    pub endpoint: ConnectorEndpoint,
    /// The owner who consented.
    pub created_by: UserId,
    /// Known at once only for a seeded connector.
    pub account: Option<ConnectorAccount>,
    /// The sealed tokens; `None` until the exchange.
    pub secret: Option<Sealed>,
    /// A pending connect's state; `None` for a seeded connector.
    pub oauth: Option<PendingConnect>,
    /// The chosen sources; empty for an owner's connect.
    pub sources: Vec<ConnectorSource>,
    /// Now.
    pub created_at: Timestamp,
}

/// A connector with everything the sync and the routes need beside its wire shape. Never sent:
/// it holds the sealed tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorRow {
    /// The wire shape, with its record count.
    pub connector: Connector,
    /// The owner who consented; the connector goes when they stop being an owner.
    pub created_by: UserId,
    /// The sealed tokens.
    pub secret: Option<Sealed>,
    /// The pending connect's state.
    pub oauth: Option<PendingConnect>,
    /// Each source's sync cursor, by source id.
    pub cursors: BTreeMap<String, SyncCursor>,
    /// When the sync is due.
    pub next_sync_at: Timestamp,
    /// The last sync's failure, a fixed code.
    pub last_error: Option<SyncErrorCode>,
    /// The last accepted **Sync now** (decision 12): one per connector in 5 minutes.
    pub sync_requested_at: Option<Timestamp>,
}

/// What a successful exchange stores: `status: active`, `oauth` cleared, `next_sync_at` now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedConnect {
    /// The nonce the state carried: the write is a compare-and-set on the stored one, so a
    /// state works once.
    pub nonce: String,
    /// The connected workspace or account.
    pub account: ConnectorAccount,
    /// The sealed tokens.
    pub secret: Sealed,
    /// Now: also the first `next_sync_at`.
    pub now: Timestamp,
}

/// What a sync run stores when it stops, finished or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    /// The connector's `next_sync_at` as [`due_connectors`](FarmRecordStore::due_connectors)
    /// returned it. If the stored value is later when the run ends, a **Sync now** or a new set
    /// of sources landed during the run: the store keeps that time, and doesn't store a
    /// stopping failure (`too_large`, `source_missing`) over it.
    pub due_at: Timestamp,
    /// Each source's cursor after the run.
    pub cursors: BTreeMap<String, SyncCursor>,
    /// Set when every source was read to its end.
    pub synced_at: Option<Timestamp>,
    /// 15 minutes after a finished sync or an outage; now when pages are left.
    pub next_sync_at: Timestamp,
    /// The failure that stopped it, if any.
    pub last_error: Option<SyncErrorCode>,
}

/// What [`FarmRecordStore::request_sync`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRequest {
    /// Accepted: the connector is due now.
    Accepted,
    /// Within [`SYNC_NOW_INTERVAL`](crate::rules::limits::SYNC_NOW_INTERVAL) of the last
    /// accepted request.
    TooSoon {
        /// When the next request will be accepted (the last one plus the interval).
        allowed_at: Timestamp,
    },
}

/// A chunk's vector, sent only when its `text_hash` changed.
#[derive(Debug, Clone, PartialEq)]
pub struct FarmVector {
    /// The vector, embedded in document mode with the pinned text embedder.
    pub embedding: Embedding,
    /// The embedding model's name.
    pub model: String,
}

/// One chunk to upsert by `(connector, vendor_id, n)`.
#[derive(Debug, Clone, PartialEq)]
pub struct NewFarmRecord {
    /// `farm_record:[connector, vendor_id, n]`.
    pub id: FarmRecordId,
    /// A message or a document chunk.
    pub kind: FarmRecordKind,
    /// The connector source it came from: the Slack channel id or the Drive folder id.
    /// Dropping the source deletes by it.
    pub source: String,
    /// `"#field-reports"`, `"Spray log 2026"`.
    pub title: String,
    /// The author's display name.
    pub author: Option<String>,
    /// The permalink or `webViewLink`.
    pub link: String,
    /// At most 2,000 characters.
    pub text: String,
    /// The hash of `text`: an unchanged hash keeps the stored vector.
    pub text_hash: String,
    /// `None` when `text_hash` is unchanged, so the stored vector stays; required for a new
    /// record.
    pub vector: Option<FarmVector>,
    /// The message's time, or the document's `modifiedTime`.
    pub at: Timestamp,
}

/// `search_farm_records`' filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FarmRecordFilter {
    /// Messages or documents only.
    pub kind: Option<FarmRecordKind>,
    /// Records at or after this instant (the tool's `since` date at the garden's midnight).
    pub since: Option<Timestamp>,
}

/// A search hit with its similarity to the query.
#[derive(Debug, Clone, PartialEq)]
pub struct FarmRecordMatch {
    /// The text and its citation.
    pub hit: FarmRecordHit,
    /// Cosine similarity.
    pub similarity: f32,
}

/// The `connector` and `farm_record` tables.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait FarmRecordStore: Send + Sync {
    // ---- connectors ----

    /// `POST /gardens/{id}/connectors`: a `pending` connector with its `oauth` state. A garden
    /// holds at most 2 connectors (`Rejected`).
    async fn create_connector(
        &self,
        owner: Membership<Owner>,
        connector: NewConnector,
    ) -> Result<Connector, StoreError>;

    /// Start-up seeding of the demo garden's `era` connectors: idempotent by the fixed id.
    async fn seed_connector(
        &self,
        garden: GardenId,
        connector: NewConnector,
    ) -> Result<Connector, StoreError>;

    /// `GET /gardens/{id}/connectors`: the garden's connectors with their record counts,
    /// oldest first, leaving out those being deleted.
    async fn connectors(&self, member: Membership<Viewer>) -> Result<Vec<Connector>, StoreError>;

    /// One connector with its sealed tokens and sync state; `None` when it doesn't exist.
    async fn connector_row(
        &self,
        connector: ConnectorId,
    ) -> Result<Option<ConnectorRow>, StoreError>;

    /// `POST /connectors/{id}/complete`, before the code exchange: claims the pending state, a
    /// compare-and-set on `status = 'pending'`, the stored nonce, its `expires_at` being after
    /// `now` and no earlier claim, which sets `oauth.claimed_at`. Single use: of two
    /// concurrent `complete`s, one claims and the other gets `Conflict(AlreadyExists)`, so the
    /// code is spent at most once. `Conflict(AlreadyExists)` too when the state was used,
    /// replaced or expired; `NotFound` when the connector is gone.
    async fn claim_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        nonce: String,
        now: Timestamp,
    ) -> Result<(), StoreError>;

    /// Undoes [`claim_connect`](Self::claim_connect) when the exchange failed before the vendor
    /// gave tokens (an outage, a refused code), so a retry of the same `complete` can claim
    /// again. Only the claim of `nonce` on a connector still `pending` is cleared; anything
    /// else is left as it is.
    async fn release_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        nonce: String,
    ) -> Result<(), StoreError>;

    /// `POST /connectors/{id}/complete`, after the exchange: a compare-and-set on
    /// `status = 'pending'` and the stored nonce having been claimed; `Conflict(AlreadyExists)`
    /// when that no longer holds, and `NotFound` when the connector is gone (deleted, or
    /// removed by the pending cleanup while the exchange ran). The caller then revokes the
    /// tokens it was given, which nothing stores.
    async fn complete_connect(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        done: CompletedConnect,
    ) -> Result<Connector, StoreError>;

    /// `PUT /connectors/{id}/sources`: replaces the sources, drops the cursors of sources no
    /// longer chosen and makes the sync due now.
    async fn set_sources(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        sources: Vec<ConnectorSource>,
        now: Timestamp,
    ) -> Result<Connector, StoreError>;

    /// `POST /connectors/{id}/sync` (decision 12): a compare-and-set on `sync_requested_at`,
    /// accepted when it is unset or [`SYNC_NOW_INTERVAL`] old (`next_sync_at` becomes now);
    /// [`SyncRequest::TooSoon`] within it, with the instant the next is allowed, so the route
    /// can answer `429 rate_limited` with `retry_after_s`. Only an `active` connector syncs; any
    /// other status, and a connector stopped until new sources (`too_large`, `source_missing`),
    /// is `Rejected(InvalidState)`.
    ///
    /// [`SYNC_NOW_INTERVAL`]: crate::rules::limits::SYNC_NOW_INTERVAL
    async fn request_sync(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
        now: Timestamp,
    ) -> Result<SyncRequest, StoreError>;

    /// The `connectors` task: connectors whose `next_sync_at` has passed, `active` only
    /// (never `pending`, `reauth_needed` or `deleting`), on the `connector_due` index, earliest
    /// first. A connector stopped with `too_large` or `source_missing` waits until the owner
    /// changes its sources ([`set_sources`](Self::set_sources) clears the failure), and
    /// [`request_sync`](Self::request_sync) refuses it with `invalid_state`.
    async fn due_connectors(
        &self,
        now: Timestamp,
        limit: u32,
    ) -> Result<Vec<ConnectorRow>, StoreError>;

    /// Stores a sync run's cursors, times and failure.
    async fn finish_sync(
        &self,
        connector: ConnectorId,
        outcome: SyncOutcome,
    ) -> Result<(), StoreError>;

    /// Sets `status` and `last_error` (a refused token: `reauth_needed` with `auth_refused`).
    async fn set_status(
        &self,
        connector: ConnectorId,
        status: ConnectorStatus,
        last_error: Option<SyncErrorCode>,
    ) -> Result<(), StoreError>;

    /// `DELETE /connectors/{id}`: `status: deleting`, so the sync and the list skip it.
    /// Returns the row, whose tokens the caller revokes before deleting the records.
    async fn start_connector_deletion(
        &self,
        owner: Membership<Owner>,
        connector: ConnectorId,
    ) -> Result<ConnectorRow, StoreError>;

    /// One batch of a connector's deletion: its farm records, 500 at a time, then the connector
    /// itself. The caller repeats it until 0.
    async fn delete_connector_batch(&self, connector: ConnectorId) -> Result<u32, StoreError>;

    /// The connectors a member created in a garden: deleted when that member stops being an
    /// owner (demoted, removed, leaving or deleting the account). The store marks those
    /// `deleting` itself, in the transaction that changes the membership
    /// (`GardenRepo::change_role`, `GardenRepo::remove_member`, `DeletionRepo::transfer_ownership`;
    /// a garden's in `DeletionRepo::start_garden_deletion`).
    async fn connectors_created_by(
        &self,
        garden: GardenId,
        user: UserId,
    ) -> Result<Vec<ConnectorId>, StoreError>;

    /// How many other live connectors, in any garden, hold a token of this account: `vendor`
    /// connectors of `kind`, `active` or `reauth_needed`, not `except`, whose account is
    /// `account` (Slack's team id, or the Google account's email). A deletion revokes the token
    /// only when there are none ([connectors 5]): Slack gives one bot token per app and
    /// workspace, and revoking Google's refresh token ends the whole grant of that account to
    /// this OAuth client, so either would stop every connector of the account. `era`
    /// connectors hold the simulator's tokens and never count.
    ///
    /// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
    async fn shared_account_connectors(
        &self,
        kind: ConnectorKind,
        account: String,
        except: ConnectorId,
    ) -> Result<u32, StoreError>;

    /// Replaces a connector's sealed tokens with `new` (a refreshed Google access token) only
    /// when the stored value is still `read`, the one the caller unsealed, and the connector is
    /// `active` or `reauth_needed`: a compare-and-set, so of two refreshes at once one is
    /// stored and the other keeps its token for its own call. `true` when it was stored.
    /// Used by the source picker and the sync (E4).
    async fn replace_secret(
        &self,
        connector: ConnectorId,
        read: Sealed,
        new: Sealed,
    ) -> Result<bool, StoreError>;

    /// Connectors marked `deleting` whose deletion hasn't finished, with their sealed tokens,
    /// oldest id first: with `Some(garden)`, that garden's (a cascade or a demotion finishing
    /// them); with `None`, those of every garden not itself being deleted (`api`'s
    /// `deletions.resume()` after a restart; a garden being deleted finishes its own connectors
    /// in its cascade). The marker is what makes a deletion resumable: the status is set in the
    /// transaction that removed the owner, and the row keeps its token until `api` revoked it
    /// and deleted the records (EON E3.6).
    async fn deleting_connectors(
        &self,
        garden: Option<GardenId>,
    ) -> Result<Vec<ConnectorRow>, StoreError>;

    /// The cleanup: deletes `pending` connectors created before the cutoff (10 minutes ago).
    async fn delete_expired_pending(&self, before: Timestamp) -> Result<u32, StoreError>;

    // ---- farm records ----

    /// The stored `text_hash` of every chunk of these items, so the sync embeds only changed
    /// text.
    async fn record_hashes(
        &self,
        connector: ConnectorId,
        vendor_ids: Vec<String>,
    ) -> Result<Vec<(FarmRecordId, String)>, StoreError>;

    /// Upserts chunks by `(connector, vendor_id, n)`, keeping the stored vector when
    /// `vector` is `None`. Returns how many were written. `NotFound` when the connector is
    /// missing or being deleted, so no record outlives its connector's deletion.
    async fn upsert_records(
        &self,
        connector: ConnectorId,
        records: Vec<NewFarmRecord>,
    ) -> Result<u32, StoreError>;

    /// Deletes every chunk of these items (gone at the vendor). Returns how many went.
    async fn delete_records(
        &self,
        connector: ConnectorId,
        vendor_ids: Vec<String>,
    ) -> Result<u32, StoreError>;

    /// A document re-chunked into fewer chunks: deletes its chunks numbered `from` and up,
    /// leaving the others (and their vectors) for [`upsert_records`](Self::upsert_records).
    async fn delete_chunks_from(
        &self,
        connector: ConnectorId,
        vendor_id: String,
        from: u32,
    ) -> Result<u32, StoreError>;

    /// The vendor ids a source holds from `since` on (by `at`), for the daily 7-day re-read to
    /// find what is gone at the vendor ([connectors 6]). The source is the id stored on each
    /// record (`NewFarmRecord::source`).
    async fn source_vendor_ids_since(
        &self,
        connector: ConnectorId,
        source: String,
        since: Timestamp,
    ) -> Result<Vec<String>, StoreError>;

    /// One batch of 500 of a source's records, after the owner dropped it: those of the
    /// connector whose stored `source` is this channel or folder id. Another source's records,
    /// a new Drive folder's among them, stay.
    async fn delete_source_records(
        &self,
        connector: ConnectorId,
        source: String,
    ) -> Result<u32, StoreError>;

    /// A garden's record count, for the 10,000 limit.
    async fn garden_record_count(&self, garden: GardenId) -> Result<u32, StoreError>;

    /// `search_farm_records`: the `k` closest records of the proof's garden, searched exactly
    /// (no HNSW index, gap G7), each with its citation.
    async fn search_records(
        &self,
        member: Membership<Viewer>,
        query: &Embedding,
        filter: FarmRecordFilter,
        k: usize,
    ) -> Result<Vec<FarmRecordMatch>, StoreError>;

    /// The cleanup: one batch of 500 Slack messages older than the cutoff (180 days), on the
    /// `farm_record_kind_at` index. Documents are not deleted by age: Drive's sync removes a
    /// file when `changes.list` says it is gone.
    async fn delete_records_before(&self, cutoff: Timestamp) -> Result<u32, StoreError>;
}
```

#### Deletion cascades

```rust
//! Deletion cascades ([contracts 9], deletion cascades; [database 4], deletion; architecture 5,
//! deletion). Bottom-up, in batches of 500, never one large transaction: mark, cancel, delete
//! objects, then delete records children first, each step repeated until it deletes nothing.

use crate::{
    authz::{Editor, Membership, Owner},
    garden::account::GardenTransfer,
    id::{AuthId, ConversationId, GardenId, PlantId, ScanId, UserId},
    ports::{ObjectKey, ObjectPrefix, StoreError},
    wire::Timestamp,
};

/// What is being deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeletionTarget {
    /// A garden and everything in it.
    Garden(GardenId),
    /// A plant and everything under it.
    Plant(PlantId),
    /// A `pending` scan.
    PendingScan(ScanId),
    /// One member's data in one garden (removal, or account deletion step 3).
    Member {
        /// The garden.
        garden: GardenId,
        /// The member.
        user: UserId,
    },
}

/// The record steps, in the order [database 4] fixes: each holds a
/// `REFERENCE ON DELETE REJECT` to something a later step deletes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CascadeStep {
    /// Scan events (one range delete per scan) and the scans' jobs.
    EventsAndJobs,
    /// Tasks and plan trigger rules, including neighbour ones in both directions.
    TasksAndTriggers,
    /// Plans and diagnoses, after unsetting `scan.plan`, `scan.diagnosis` and
    /// `scan.open_question`; plans selected by equality, never `WHERE scan IN …`.
    PlansAndDiagnoses,
    /// Masks and guided checks.
    MasksAndGuidedChecks,
    /// Observations, questions and scan feedback.
    ObservationsQuestionsFeedback,
    /// Scan images, then scans.
    ImagesAndScans,
    /// Conversations with their pending actions, message feedback, messages and chat images.
    Conversations,
    /// Memories, then plants (a single plant first loses its pin), then the garden map.
    MemoriesPlantsMap,
    /// Farm records, then the farm connectors ([connectors 8], deletion): a record holds a
    /// reference to its connector, and both to the garden. A garden's only.
    ///
    /// [connectors 8]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#8-privacy-and-security
    FarmRecordsAndConnectors,
    /// Invites, memberships and the garden itself.
    Garden,
}

impl CascadeStep {
    /// Every step, in order.
    pub const ORDER: [Self; 10] = [
        Self::EventsAndJobs,
        Self::TasksAndTriggers,
        Self::PlansAndDiagnoses,
        Self::MasksAndGuidedChecks,
        Self::ObservationsQuestionsFeedback,
        Self::ImagesAndScans,
        Self::Conversations,
        Self::MemoriesPlantsMap,
        Self::FarmRecordsAndConnectors,
        Self::Garden,
    ];

    /// The steps a target goes through.
    pub fn steps_for(target: DeletionTarget) -> &'static [Self] {
        match target {
            DeletionTarget::Garden(_) => &Self::ORDER,
            DeletionTarget::Plant(_) => &Self::ORDER[..8],
            DeletionTarget::PendingScan(_) => &[
                Self::EventsAndJobs,
                Self::MasksAndGuidedChecks,
                Self::ObservationsQuestionsFeedback,
                Self::Conversations,
                Self::ImagesAndScans,
            ],
            DeletionTarget::Member { .. } => &[Self::Conversations, Self::MemoriesPlantsMap],
        }
    }
}

/// What starting a deletion did: `deleting_at` is set, jobs are cancelled and step claims and
/// question steps cleared in the same transaction, so running steps fail their commit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionStarted {
    /// The target.
    pub target: DeletionTarget,
    /// Object prefixes to delete before the records (a garden's, or each scan's).
    pub prefixes: Vec<ObjectPrefix>,
    /// Single objects to delete.
    pub keys: Vec<ObjectKey>,
    /// Members to tell with a `garden_deleted` push (garden only, not the deleter).
    pub notify: Vec<UserId>,
    /// The garden's name, for that push.
    pub garden_name: Option<String>,
}

/// What account deletion will do to each garden of the user (architecture 5, deletion steps
/// 1–3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountDeletionPlan {
    /// Gardens where the user is the only member, or the only owner with no editor to inherit.
    pub delete: Vec<GardenId>,
    /// Gardens handed to the longest-standing editor.
    pub transfer: Vec<GardenTransfer>,
    /// Shared gardens the user leaves (step 3).
    pub leave: Vec<GardenId>,
}

/// An account deletion's marker and tombstone, `deleted_user:[auth_id]` ([database 4],
/// deletion): written first, in the compare-and-set that starts the cascade, and kept until the
/// account is gone at the auth provider plus [`crate::rules::limits::TOMBSTONE_KEEP`]. While it
/// exists, `IdentityRepo::upsert_user` refuses the auth id, so a session token still valid after
/// the deletion can't create the user again; and an unfinished one resumes after a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDeletionMarker {
    /// The user being deleted (kept as an id, not a record link: the record goes first).
    pub user: UserId,
    /// The auth provider's id, the tombstone's key.
    pub auth_id: AuthId,
    /// When the cascade started.
    pub started_at: Timestamp,
    /// The `user` record is gone (cascade steps 1–5 done); only the auth provider's deletion is
    /// left.
    pub user_deleted: bool,
}

/// The cascade's writes.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait DeletionRepo: Send + Sync {
    /// `DELETE /gardens/{id}`: starts the cascade; `202` follows.
    async fn start_garden_deletion(
        &self,
        owner: Membership<Owner>,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError>;

    /// `DELETE /plants/{id}`: also removes its pin from the current map in place.
    async fn start_plant_deletion(
        &self,
        owner: Membership<Owner>,
        plant: PlantId,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError>;

    /// `DELETE /scans/{id}` of a `pending` scan (any other status is
    /// `Rejected(InvalidState)` with `status`).
    async fn start_scan_deletion(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        now: Timestamp,
    ) -> Result<DeletionStarted, StoreError>;

    /// One batch of one step: `DELETE (SELECT VALUE id FROM … LIMIT 500) RETURN VALUE
    /// $before.id` ([database 4], deletion). Returns how many records it deleted (the ids
    /// returned); the caller repeats the step until 0, then moves on. `RETURN NONE` can't be
    /// used: it returns `[]` for a full batch and an empty one alike.
    async fn delete_batch(
        &self,
        target: DeletionTarget,
        step: CascadeStep,
    ) -> Result<u32, StoreError>;

    /// Deletions started but not finished (after a restart), to resume.
    async fn unfinished_deletions(&self) -> Result<Vec<DeletionTarget>, StoreError>;

    /// Decides each garden's fate for `DELETE /me` and the `user.deleted` webhook.
    async fn plan_account_deletion(&self, user: UserId) -> Result<AccountDeletionPlan, StoreError>;

    /// Makes `to` an owner in place of `from` (step 2).
    async fn transfer_ownership(
        &self,
        transfer: GardenTransfer,
        from: UserId,
    ) -> Result<(), StoreError>;

    /// A member's conversations in a garden and their chat image prefixes, before the
    /// `Member` cascade.
    async fn member_conversations(
        &self,
        garden: GardenId,
        user: UserId,
    ) -> Result<Vec<(ConversationId, ObjectPrefix)>, StoreError>;

    /// Step 4: the user's scan feedback.
    async fn delete_user_feedback(&self, user: UserId) -> Result<u32, StoreError>;

    /// Step 5: the `user` record (push tokens cascade; author links unset).
    async fn delete_user(&self, user: UserId) -> Result<(), StoreError>;

    /// Starts an account deletion (`DELETE /me` or `user.deleted`): creates the tombstone
    /// `deleted_user:[auth_id]` with `user` and `started_at`, as a compare-and-set on its
    /// absence. `true` when this call created it, so the caller runs the cascade; `false` when
    /// a cascade for this account already started, so the second request is a no-op.
    ///
    /// The default body always answers `true` (no marker, no protection against a second
    /// cascade); the store overrides it.
    async fn start_account_deletion(
        &self,
        user: UserId,
        auth_id: &AuthId,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        let _ = (user, auth_id, now);
        Ok(true)
    }

    /// Account deletions whose auth-provider deletion hasn't been confirmed
    /// (`clerk_deleted_at` unset), to resume after a restart.
    ///
    /// The default body finds none; the store overrides it.
    async fn unfinished_account_deletions(&self) -> Result<Vec<AccountDeletionMarker>, StoreError> {
        Ok(Vec::new())
    }

    /// The account is gone at the auth provider: sets the tombstone's `clerk_deleted_at`. The
    /// tombstone itself stays for [`crate::rules::limits::TOMBSTONE_KEEP`].
    ///
    /// The default body does nothing; the store overrides it.
    async fn finish_account_deletion(
        &self,
        auth_id: &AuthId,
        now: Timestamp,
    ) -> Result<(), StoreError> {
        let _ = (auth_id, now);
        Ok(())
    }

    /// Cleanup: tombstones whose `clerk_deleted_at` is before the cutoff, in batches of 500.
    ///
    /// The default body deletes nothing; the store overrides it.
    async fn delete_finished_tombstones(&self, before: Timestamp) -> Result<u32, StoreError> {
        let _ = before;
        Ok(0)
    }

    /// The orphan sweep: which of these keys no record points to (every key field counts:
    /// `storage_key`, `depth_key`, `depth_confidence_key`, masks and chat images).
    async fn unreferenced_keys(&self, keys: &[ObjectKey]) -> Result<Vec<ObjectKey>, StoreError>;
}
```

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
