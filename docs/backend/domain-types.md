# PDM Backend: `domain` contract types

> **Status:** Draft v0.3.2, 2026-10-16. v0.3.2: the schema links point to `backend/migrations/schema.surql`. v0.3.1: `JobErrorCode`'s comments follow contracts 5: `max_runtime_exceeded` is settled (question 18), and `unsupported_version` and `model_error` carry the contract's full meaning. v0.3: `PastWeather` gains `longest_humid_run_days` (contracts v0.10, section 7), the longest run of humid days in the past week, so a past humid stretch is detectable. v0.2: follows contracts v0.9 and v0.10 where they touch shapes: `JobError.code` is a `JobErrorCode` (`max_runtime_exceeded` and `unknown_label_version` included), the `model_versions` keys are `ModelVersionKey` constants, `Invite.created_by` is optional, worker payloads never carry a mask URL and `complete` results are read by the job's kind (both as v0.1 had them); `BoxFormat`'s comments name the models of each format. v0.1: every shape of [contracts.md](../architecture/contracts.md) v0.8.2 as compiled Rust, section by section, with the serde and schema decisions explained and a TypeScript → Rust map.

Companion to the [backend architecture](README.md) and to the [`domain` crate](domain.md). Sections 1–10 follow [contracts.md](../architecture/contracts.md) 1–10, so "contracts 7" and "domain-types 7" describe the same shapes. Every listing is copied from the compiled source of `crates/domain` (`cargo check`, `cargo test` and `cargo clippy --all-targets` pass on Rust 1.99.1 with serde 1.0.229, utoipa 6.0.0, schemars 1.2.3, chrono 0.4.45 and ulid 3.0.0); the module paths are in [the map](#11-typescript-to-rust-map).

## Contents

1. [Conventions](#1-conventions)
2. [Errors](#2-errors)
3. [Scan REST API](#3-scan-rest-api)
4. [Scan events](#4-scan-events)
5. [Worker API and jobs](#5-worker-api-and-jobs)
6. [VLM structured output](#6-vlm-structured-output)
7. [Pipeline records](#7-pipeline-records)
8. [Chat and memory](#8-chat-and-memory)
9. [Gardens, plants, tasks and account](#9-gardens-plants-tasks-and-account)
10. [Knowledge seed format](#10-knowledge-seed-format)
11. [TypeScript to Rust map](#11-typescript-to-rust-map)
12. [Sources](#12-sources)

---

## 1. Conventions

[Contracts 1](../architecture/contracts.md#1-conventions).

### How a TypeScript shape becomes Rust

These rules hold for every type below; the per-section notes only cover departures from them.

| Contract | Rust | Why |
|---|---|---|
| `field: T` | `pub field: T` | Field names are already `snake_case` |
| `field?: T` | `Option<T>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` and `#[schema(nullable = false)]` | Omitted when absent, never `null` ([contracts 1, notation](../architecture/contracts.md#notation)). utoipa makes every `Option` nullable and optional; `nullable = false` keeps the generated TypeScript at `field?: T` |
| `field: T \| null` | `Option<T>` with `#[schema(required = true)]` and no `skip_serializing_if` | Always present, `null` when empty |
| `"a" \| "b"` | an enum with `#[serde(rename_all = "snake_case")]` | Unknown values fail to decode; the app treats an unknown value as "other" |
| `Id` | `Id<T>` (`ScanId`, `PlantId`, …) | The bare 26-character ULID; a `ScanId` can't be passed for a `PlantId` |
| `Slug` | `Slug<K>` (`SpeciesId`, `ProblemId`, …) | Checked format, typed by knowledge table |
| `Timestamp` | `Timestamp` | RFC 3339, `Z`, whole seconds on the wire |
| `LocalDate` | `LocalDate` | `YYYY-MM-DD` |
| `Confidence` + `confidence_band` | `Rated` (flattened) | The band is computed, never set |
| `v: 1` | `V1` | Serialises as `1`; any other value fails |
| `interface B extends A` | a struct with `#[serde(flatten)] a: A` | Same JSON; the OpenAPI schema is an `allOf` |
| A union tagged by a field | an enum with `#[serde(tag = "…")]` (or `tag` + `content`) | As the contract places the tag |
| A record with an image or mask URL | a type generic over `U: UrlState = Signed` | IDs when stored, signed URLs on the wire ([domain 5](domain.md#5-signed-url-typing)) |
| An app → API request body | `#[serde(deny_unknown_fields)]` | A misspelt field is a `400`, not a silent `None` |
| Records the API reads from workers | no `deny_unknown_fields` | Older and newer workers run side by side |

Every wire type derives `Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema`; the fixed shapes (the knowledge seed and the plan writer's output) also derive `schemars::JsonSchema`. Types generic over the URL state carry `#[serde(bound = "")]` (the bounds come from `UrlState`'s associated types) and `#[schema(bound = "U: WireState", as = Name)]`, so only the signed instantiation has a schema and it is named without its parameter.

### Timestamps, dates and confidence

`chrono` serialises a `DateTime<Utc>` with a fraction whenever it has one, and every clock reading has one, so `Timestamp` truncates to whole seconds on the wire and keeps full precision in memory. `LocalDate` is a newtype so the generators see `format: date` through it. `Confidence` rejects anything outside 0–1 when it is decoded.

```rust
/// RFC 3339 in UTC with a `Z` suffix, whole seconds on the wire: `"2026-10-18T14:03:22Z"`
/// ([contracts 1]). chrono's own `Serialize` adds a fraction whenever the value has one, and a
/// clock reading always has one, so the newtype truncates when serialising and keeps full
/// precision in memory (the store binds the inner `DateTime<Utc>`, [database 2]). There is no
/// `Timestamp::now()`: every reading goes through the `Clock` port so tests control time.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

/// A calendar date in the garden's time zone, `YYYY-MM-DD` ([contracts 1]). A newtype over
/// `chrono::NaiveDate` (whose serde form is already `"2026-10-20"`) so the `OpenAPI` and JSON
/// Schema generators see `format: date` through any alias, and so a UTC day can't be passed
/// where a garden-local one is meant without saying so.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(transparent)]
#[schema(value_type = String, format = Date, example = "2026-10-20")]
#[schemars(transparent)]
pub struct LocalDate(chrono::NaiveDate);

/// A number from 0 to 1 ([contracts 1]). Finite and in range by construction: deserialising
/// anything else fails.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Confidence(f64);

/// `"high" | "medium" | "low"`, computed by the API so every screen agrees ([contracts 1]):
/// **high** ≥ 0.8, **medium** ≥ 0.5, **low** below that.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    /// Below 0.5.
    Low,
    /// 0.5 up to 0.8.
    Medium,
    /// 0.8 or more.
    High,
}

/// `confidence` and `confidence_band` travelling together, flattened into the record that
/// carries them (`Observation`). The band is derived from the number when serialising and
/// recomputed when deserialising, so it can never be set by hand or disagree with the number.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rated(pub Confidence);
```

`Rated` is flattened into the record that carries a confidence (`Observation`). Its `Serialize` writes both fields from the number; its `Deserialize` reads the number and recomputes the band, so a stored band that disagrees can never come back.

### Boxes, regions and pages

```rust
/// `[x, y, w, h]`, each from 0 to 1, origin top-left, normalised to the stored image
/// ([contracts 1], regions). Whoever produces a box converts it before sending.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(into = "[f64; 4]")]
pub struct BBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// A box on an image ([contracts 1], `Region`). `region_id` is present for segmentation regions
/// (`"<image_id>.<n>"`) and referenced by later stages.
//
// The mask slot is `mask_id` when stored and `mask_url` (signed) on the wire: see `crate::url`.
// (A plain comment, so it stays out of the OpenAPI description.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Region)]
pub struct Region<U: UrlState = Signed> {
    /// Present for segmentation regions, unique within the scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub region_id: Option<RegionId>,
    /// The image the box is on.
    pub image_id: ImageId,
    /// The box.
    pub bbox: BBox,
    // `mask_id` (stored) or `mask_url` (signed), when segmentation produced a mask.
    #[serde(flatten)]
    #[schema(inline, value_type = SignedMask)]
    pub mask: U::Mask,
}

/// `{ items: T[], next_cursor?: string }` ([contracts 1], lists). Default page size 20, at most
/// 100 ([`PageRequest`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Page<T> {
    /// The items, in the order the endpoint states.
    pub items: Vec<T>,
    /// The cursor of the next page; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub next_cursor: Option<Cursor>,
}

/// `?cursor=&limit=` ([contracts 1], lists).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageRequest {
    /// From a previous page's `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
    /// 1–100; 20 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}
```

- `BBox` is `[x, y, w, h]` on the wire (`#[serde(into = "[f64; 4]")]`) and checks every value and `x + w ≤ 1`, `y + h ≤ 1` when decoded. utoipa ignores `into`, so the schema is written by hand: an array of exactly 4 numbers from 0 to 1.
- `Region`'s mask is a flattened slot: `mask_id` in `Region<Stored>`, `mask_url` in `Region<Signed>` ([domain 5](domain.md#5-signed-url-typing)). The doc comment on the slot is a plain `//` comment so it stays out of the OpenAPI description.
- `PageRequest` is the `?cursor=&limit=` query (`utoipa::IntoParams`); `limit()` applies the default 20 and the maximum 100.

### Runs, sequence numbers, literals and roles

```rust
/// A scan's run: 0 until submitted, 1 on submit, +1 on a resubmit or a species change
/// ([contracts 3], lifecycle). Jobs, events, observations and diagnoses carry it.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[serde(transparent)]
pub struct Run(pub u32);

/// A scan event's `seq` and SSE `id`: per scan, from 1, strictly increasing, without gaps,
/// across all runs ([contracts 4]).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[serde(transparent)]
pub struct EventSeq(pub u64);

/// The literal `v: 1` on every SSE event, worker payload and result, chat event and push
/// payload ([contracts 1], versioning). Serialises as `1`; deserialising any other number fails
/// with [`UnsupportedVersion`], which a worker turns into the job error `unsupported_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct V1;

/// A boolean literal type, for `passed: true` ([contracts 3], upload response) and the
/// `stored: true | false` union tag of `UploadImageResponse.depth`. Deserialising the other
/// value fails, which is what lets an untagged enum pick the right branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BoolLit<const B: bool>;

/// `true` as a type.
pub type True = BoolLit<true>;

/// `false` as a type.
pub type False = BoolLit<false>;

/// A garden member's role ([contracts 1], roles and access), ordered
/// `viewer < reviewer < editor < owner` so "editor or above" is `role >= Role::Editor`. The
/// wire order in contracts (`"owner" | "editor" | "viewer" | "reviewer"`) is not the rank.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Reads and chats.
    Viewer,
    /// A viewer who can also give scan feedback.
    Reviewer,
    /// Scans, answers, manages tasks and plants, maps the garden.
    Editor,
    /// Manages the garden and its members. A garden may have several.
    Owner,
}

/// `{ user_id: Id; display_name: string }`: `Scan.created_by` and `GardenMap.created_by`,
/// absent once that user is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserRef {
    /// The user.
    pub user_id: UserId,
    /// Their display name.
    pub display_name: String,
}
```

- `Run` and `EventSeq` are newtypes so a run can't be compared with a sequence number. Both serialise as plain numbers (`#[serde(transparent)]`).
- `V1` is what every versioned payload carries. A `v` of 2 fails to decode with `unsupported_version: v = 2`; workers read [`JobHead`](#5-worker-api-and-jobs) first so they can still fail the job.
- `BoolLit<B>` is a boolean literal type: `passed: true` in `QualityReport`, and the tag of `UploadImageResponse.depth`.
- `Role` derives `Ord` in rank order, `viewer < reviewer < editor < owner`, which is not the order contracts lists the values in.

### Identifiers

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

/// Crockford base32, 26 characters.
pub const ULID_PATTERN: &str = "^[0-9A-HJKMNP-TV-Z]{26}$";
```

`Id<T>` serialises as the bare ULID and parses case-insensitively, rejecting a `scan:` prefix. The markers and aliases:

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
    }
}
```

```rust
/// `user` record key.
pub type UserId = Id<table::User>;

/// `garden` record key.
pub type GardenId = Id<table::Garden>;

/// `invite` record key.
pub type InviteId = Id<table::Invite>;

/// `plant` record key.
pub type PlantId = Id<table::Plant>;

/// `garden_map` record key.
pub type GardenMapId = Id<table::GardenMap>;

/// `scan` record key.
pub type ScanId = Id<table::Scan>;

/// `scan_image` record key: the `image_id` of contracts 3 and 5.
pub type ImageId = Id<table::ScanImage>;

/// `mask` record key.
pub type MaskId = Id<table::Mask>;

/// `observation` record key.
pub type ObservationId = Id<table::Observation>;

/// `question` record key.
pub type QuestionId = Id<table::Question>;

/// `guided_check` record key.
pub type GuidedCheckId = Id<table::GuidedCheck>;

/// `diagnosis` record key.
pub type DiagnosisId = Id<table::Diagnosis>;

/// `scan_feedback` record key.
pub type FeedbackId = Id<table::ScanFeedback>;

/// `action_plan` record key.
pub type PlanId = Id<table::ActionPlan>;

/// `plan_trigger` record key.
pub type PlanTriggerId = Id<table::PlanTrigger>;

/// `task` record key.
pub type TaskId = Id<table::Task>;

/// `conversation` record key.
pub type ConversationId = Id<table::Conversation>;

/// `chat_image` record key.
pub type ChatImageId = Id<table::ChatImage>;

/// `message` record key.
pub type MessageId = Id<table::Message>;

/// `pending_action` record key.
pub type PendingActionId = Id<table::PendingAction>;

/// `memory` record key.
pub type MemoryId = Id<table::Memory>;

/// `job` record key.
pub type JobId = Id<table::Job>;
```

Knowledge ids are slugs, typed by table. `"healthy"` is reserved: it can't be a problem slug, because it takes a problem id's place in `problem_id: Slug | "healthy"`.

```rust
/// A knowledge-base table whose records are keyed by a slug ([contracts 10]).
pub trait SlugKind: 'static {
    /// The table name.
    const TABLE: &'static str;
}

/// A knowledge-base id: `[a-z][a-z0-9_]*`, at most 64 characters, unique across all tables and
/// never reused ([contracts 10]). Cheap to clone (`Arc<str>`), because candidate lists, scores
/// and prompts copy slugs often. The marker works as in [`Id`].
pub struct Slug<K: SlugKind> {
    value: Arc<str>,
    kind: PhantomData<fn() -> K>,
}

/// The slug format, as a regular expression.
pub const SLUG_PATTERN: &str = "^[a-z][a-z0-9_]{0,63}$";

/// `Slug | "healthy"` ([contracts 5] disease results, [contracts 10] classifier labels).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProblemOrHealthy {
    /// A problem of the knowledge base.
    Problem(ProblemId),
    /// The classifier's healthy class.
    Healthy,
}
```

Compound ids check both parts:

```rust
/// `<problem_id>.<indicator_slug>` ([contracts 10]), such as
/// `"tomato_early_blight.concentric_rings"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IndicatorId {
    problem: ProblemId,
    slug: Arc<str>,
}

/// `<image_id>.<n>` ([contracts 5], region ids): numbered per image by the worker that segments
/// it, unique within the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionId {
    /// The image the region is on.
    pub image: ImageId,
    /// The region's number on that image.
    pub n: u32,
}

/// `<source_id>.<n>`: a knowledge chunk ([contracts 10], `ChunkSeed.id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId {
    /// The source the chunk was cut from.
    pub source: SourceId,
    /// Its number within the source.
    pub n: u32,
}
```

The opaque strings (`WorkerId`, `LeaseToken`, `AuthId`, `InviteCode`, `PushToken`, `Cursor`, `StepId`) come from one macro with a check per type, and `IdempotencyKey` wraps a `uuid::Uuid`:

```rust
/// The `Idempotency-Key` header: a UUID the app generates per user action ([contracts 1],
/// idempotency). Parsed with `uuid` (`v4`, `serde`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(pub uuid::Uuid);
```

### Image URLs

A `url` the API returns is a `SignedUrl`; the stored form of the same record has no URL at all ([domain 5](domain.md#5-signed-url-typing)).

```rust
/// An API-signed, short-lived image URL: `{base}{path}?exp={unix seconds}&sig={hex}`
/// ([storage 3]). Built only by [`Signer::url`] in this crate; it deserialises (for clients and
/// tests), but nothing turns it back into a stored value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(transparent)]
pub struct SignedUrl(String);

/// The empty URL slot of a stored value. It is skipped when serialising, and deserialising
/// anything into it fails, so a stored record can't hold a URL even through JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NoUrl;
```

---

## 2. Errors

[Contracts 2](../architecture/contracts.md#2-errors). The HTTP status and default `retryable` of each code are data on `ErrorCode` ([domain 6](domain.md#6-errors)).

```rust
/// Every error code of contracts 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 400: malformed JSON, missing or wrong-typed field.
    InvalidRequest,
    /// 401: missing, expired or invalid JWT.
    Unauthenticated,
    /// 403: a member whose role is too low.
    Forbidden,
    /// 403: expired or tampered signed image URL.
    InvalidSignature,
    /// 404: unknown id, or one the caller may not see.
    NotFound,
    /// 409: the record isn't in a state that allows this call.
    InvalidState,
    /// 409: same `Idempotency-Key` while the first request still runs.
    RequestInProgress,
    /// 409: worker call for a job whose lease is gone.
    LeaseLost,
    /// 413: image over 12 MB.
    UploadTooLarge,
    /// 415: not JPEG, PNG, WebP or HEIC, or an undecodable HEIC.
    UnsupportedMediaType,
    /// 422: the blur or exposure check failed; the image was not stored.
    ImageQuality,
    /// 422: submit without a `whole_plant` image.
    MissingRequiredShot,
    /// 422: kind limit reached.
    TooManyImages,
    /// 422: reused `Idempotency-Key` with a different body.
    IdempotencyConflict,
    /// 429: request rate limit, including invite-code attempts.
    RateLimited,
    /// 429: daily chat token budget or daily scan limit used up.
    BudgetExceeded,
    /// 500: a bug.
    Internal,
    /// 503: face blurring or another required upload step failed; nothing stored.
    ProcessingUnavailable,
    /// 503: LLM API, weather or another dependency down.
    UpstreamUnavailable,
}

/// One hint per `image_quality` problem, for the app's retake prompt ([contracts 2]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityHint {
    /// `too_blurry`: hold the phone still and tap the subject to focus.
    TapToFocus,
    /// `too_dark`: move into better light or turn the plant to face it.
    MoreLight,
    /// `too_bright`: shade the plant or step out of direct sun.
    AvoidGlare,
}

/// `details.problem` of `image_quality`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityProblem {
    /// The blur score is under the threshold.
    TooBlurry,
    /// Underexposed.
    TooDark,
    /// Overexposed.
    TooBright,
}

/// `details.hint` of `invalid_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvalidStateHint {
    /// Submit of a scan that failed with `no_plant_found` or `photos_unusable`.
    NewScan,
    /// Confirming a plan revision whose step names a deleted neighbour.
    NeighbourRemoved,
    /// Confirming a `create_task` whose plant was deleted.
    PlantRemoved,
}

/// `details.budget` of `budget_exceeded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// The daily chat token budget.
    ChatTokens,
    /// The daily scan limit (30 per user).
    Scans,
}
```

`details` depends on `code`, so `code` and `details` are one adjacently tagged enum, flattened into the error object. Codes without details serialise with no `details` key.

```rust
/// `details` of `invalid_request`. `plant_ids` is set only for the map upload's
/// `problem: "unknown_plant"` ([contracts 9], garden map).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InvalidRequestDetails {
    /// The field at fault, as a dotted path (`"location.lat"`, `"pins"`).
    pub field: String,
    /// What is wrong with it.
    pub problem: String,
    /// Every pinned plant that isn't a plant of this garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_ids: Option<Vec<PlantId>>,
}

/// `details` of `invalid_state`. Every field is optional; the record's state is reported as a
/// string because the record may be a scan, question, plan, task, invite, pending action or map.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct InvalidStateDetails {
    /// The record's current state, such as `"planned"` or `"rejected"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub status: Option<String>,
    /// The states that would allow the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub allowed: Option<Vec<String>>,
    /// A plan revision's base is no longer current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub current_plan_id: Option<PlanId>,
    /// A map upload's `base_version` is stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub current_map_version: Option<u32>,
    /// What the app should offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub hint: Option<InvalidStateHint>,
}

/// The `code` and `details` pair of an error body, tagged as contracts 2 lays them out: `code`
/// beside the other fields, and `details` only for the codes that define it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "code", content = "details", rename_all = "snake_case")]
pub enum ErrorKind {
    /// 400 `{ field, problem }`.
    InvalidRequest(InvalidRequestDetails),
    /// 401.
    Unauthenticated,
    /// 403 `{ required_role }`.
    Forbidden {
        /// The lowest role that may make the call.
        required_role: Role,
    },
    /// 403.
    InvalidSignature,
    /// 404.
    NotFound,
    /// 409, with the state details.
    InvalidState(InvalidStateDetails),
    /// 409.
    RequestInProgress,
    /// 409 `{ job_id }`.
    LeaseLost {
        /// The job whose lease is gone.
        job_id: JobId,
    },
    /// 413 `{ max_bytes }`.
    UploadTooLarge {
        /// The limit.
        max_bytes: u64,
    },
    /// 415 `{ accepted }`.
    UnsupportedMediaType {
        /// The accepted media types.
        accepted: Vec<String>,
    },
    /// 422 `{ problem, blur_score, exposure, hint }`.
    ImageQuality {
        /// Which check failed.
        problem: QualityProblem,
        /// The blur score (variance of the Laplacian).
        blur_score: f64,
        /// Mean exposure, 0–1.
        exposure: f64,
        /// The retake hint.
        hint: QualityHint,
    },
    /// 422 `{ missing }`.
    MissingRequiredShot {
        /// The required kinds that are missing.
        missing: Vec<ImageKind>,
    },
    /// 422 `{ kind, max }`.
    TooManyImages {
        /// The kind at its limit.
        kind: ImageKind,
        /// Its limit.
        max: u32,
    },
    /// 422.
    IdempotencyConflict,
    /// 429.
    RateLimited,
    /// 429 `{ budget, resets_at }`.
    BudgetExceeded {
        /// Which budget.
        budget: Budget,
        /// When it resets.
        resets_at: Timestamp,
    },
    /// 500.
    Internal,
    /// 503.
    ProcessingUnavailable,
    /// 503 `{ upstream }`.
    UpstreamUnavailable {
        /// The dependency that failed, such as `"llm"` or `"open_meteo"`.
        upstream: String,
    },
}

/// `ErrorBody.error` ([contracts 2]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ErrorObject {
    /// `code` and `details`.
    #[serde(flatten)]
    pub kind: ErrorKind,
    /// Human-readable, safe to show; English for now.
    pub message: String,
    /// The same request may succeed later.
    pub retryable: bool,
    /// With `409 request_in_progress`, `429 rate_limited` and `503`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub retry_after_s: Option<u32>,
    /// Matches the server log line: `req_<ULID>`.
    pub request_id: String,
}

/// Every non-2xx response body ([contracts 2]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// The error.
    pub error: ErrorObject,
}
```

- `retry_after_s` is a sibling of `code`, not part of `details`, as in the contract.
- `invalid_request` carries `plant_ids` only for the map upload's `unknown_plant` ([contracts 9, garden map](../architecture/contracts.md#garden-map)).
- `invalid_state.status` is a string: the record may be a scan, question, plan, task, invite, pending action or map, each with its own states.

The contract's example round-trips exactly:

```rust
#[test]
fn error_body_matches_the_contract_example() {
    let json = r#"{
      "error": {
        "code": "image_quality",
        "message": "This photo is too blurry. Hold the phone still and tap the leaf to focus.",
        "retryable": true,
        "details": { "problem": "too_blurry", "blur_score": 41.2, "exposure": 0.52, "hint": "tap_to_focus" },
        "request_id": "req_01JAX4B2Q8"
      }
    }"#;
    let body: ErrorBody = serde_json::from_str(json).unwrap();
    assert_eq!(body.http_status(), 422);
    assert_eq!(
        body.error.kind,
        ErrorKind::ImageQuality {
            problem: QualityProblem::TooBlurry,
            blur_score: 41.2,
            exposure: 0.52,
            hint: QualityHint::TapToFocus,
        }
    );
    let again: serde_json::Value = serde_json::to_value(&body).unwrap();
    let expected: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(again, expected);
}
```

---

## 3. Scan REST API

[Contracts 3](../architecture/contracts.md#3-scan-rest-api).

### Status, kinds and images

```rust
/// `ScanStatus` ([contracts 3], scan lifecycle). The transitions are in
/// [`crate::rules::scan_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    /// Created; photos being uploaded.
    Pending,
    /// Submitted; the `perception` job waits for a worker.
    Queued,
    /// The run's jobs and steps are working.
    Analysing,
    /// A question is open; at most 2 per run.
    AwaitingAnswer,
    /// A diagnosis exists; the plan is being written.
    Diagnosed,
    /// A plan exists. Late work never changes this status.
    Planned,
    /// The run failed with a `ScanFailure`.
    Failed,
}

/// `ImageKind` ([contracts 3]). The kind limits are in [`crate::rules::uploads`].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    /// Required, at most 1.
    WholePlant,
    /// At most 2.
    LeafCloseup,
    /// At most 1.
    LeafUnderside,
    /// At most 1.
    SoilSurface,
    /// At most 1.
    FlowerFruit,
    /// Guided soil check: at most 1 before submit, any number after.
    SqueezeTest,
    /// Guided soil check: at most 1 before submit, any number after.
    JarTest,
}

/// `ScanImage.depth`: present for `LiDAR` shots; the depth map itself is never served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DepthInfo {
    /// Depth map width, pixels.
    pub width: u32,
    /// Depth map height, pixels.
    pub height: u32,
    /// Plant height and canopy width need gravity.
    pub has_gravity: bool,
}

/// `ScanImage` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ScanImage)]
pub struct ScanImage<U: UrlState = Signed> {
    /// The image.
    pub id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// A photo sent to answer a question; doesn't count toward kind limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub answers_question_id: Option<QuestionId>,
    /// Stored (upright) width, pixels.
    pub width: u32,
    /// Stored (upright) height, pixels.
    pub height: u32,
    /// Signed, short-lived.
    #[serde(default, skip_serializing_if = "UrlSlot::is_absent")]
    #[schema(value_type = SignedUrl, required = true)]
    pub url: U::Url,
    /// When it was uploaded.
    pub uploaded_at: Timestamp,
    /// `LiDAR` phones only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub depth: Option<DepthInfo>,
}
```

`ScanImage.url` is `U::Url`: `SignedUrl` on the wire, `NoUrl` (never serialised) when stored. `#[schema(required = true)]` keeps it required in the OpenAPI document even though `skip_serializing_if` is set for the stored form.

### Species and plants

```rust
/// `SpeciesRef` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesRef {
    /// The species.
    pub id: SpeciesId,
    /// "Solanum lycopersicum".
    pub scientific_name: String,
    /// "Tomato".
    pub common_name: String,
}

/// `SpeciesCandidate.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    /// `BioCLIP` on the perception worker.
    Bioclip,
    /// The VLM's species guess.
    Vlm,
    /// The user's choice.
    User,
}

/// A species photo with its credit: `SpeciesCandidate.reference_image` and
/// `SpeciesSummary.reference_images[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ReferenceImage)]
pub struct ReferenceImage<U: UrlState = Signed> {
    /// Signed.
    #[serde(default, skip_serializing_if = "UrlSlot::is_absent")]
    #[schema(value_type = SignedUrl, required = true)]
    pub url: U::Url,
    /// "Photo: A. Author, CC BY-SA 4.0"; shown with the photo.
    pub credit: String,
}

/// `SpeciesCandidate` ([contracts 3]). `reference_image` is computed from `species_id` when a
/// candidate is read or sent and never stored: a stored candidate always has `None`, and
/// [`Sign`] fills it from the species photo index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = SpeciesCandidate)]
pub struct SpeciesCandidate<U: UrlState = Signed> {
    /// `null` when the name isn't in the knowledge base.
    #[schema(required = true)]
    pub species_id: Option<SpeciesId>,
    /// The name shown.
    pub name: String,
    /// The candidate's score.
    pub score: Confidence,
    /// Who proposed it.
    pub source: CandidateSource,
    /// The species' first reference photo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<ReferenceImage>)]
    pub reference_image: Option<ReferenceImage<U>>,
}

/// `Plant.species_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpeciesStatus {
    /// Nothing identified yet.
    Unknown,
    /// Suggested by the models; the app shows the confirmation card.
    Suggested,
    /// Confirmed by the user; later scans skip identification.
    Confirmed,
}

/// `Plant.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlantStatus {
    /// No problem found.
    Healthy,
    /// Worth watching.
    Watch,
    /// A problem was diagnosed.
    Sick,
    /// Dead.
    Dead,
}

/// `Plant.position`: from the garden map ([contracts 9]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlantPosition {
    /// The map version the pin is on.
    pub map_version: u32,
    /// Metres on the map's ground plane.
    pub x_m: f64,
    /// Metres on the map's ground plane.
    pub y_m: f64,
}

/// `Plant.latest_scan`: the plant's most recent scan, in any status, skipping scans being
/// deleted ([contracts 3], deleting a scan).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = LatestScan)]
pub struct LatestScan<U: UrlState = Signed> {
    /// The scan.
    pub scan_id: ScanId,
    /// When it was created.
    pub created_at: Timestamp,
    /// Its status.
    pub status: ScanStatus,
    /// Its `whole_plant` shot, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub thumbnail_image_id: Option<ImageId>,
    /// Signed URL of that shot.
    #[serde(default, skip_serializing_if = "url_absent")]
    #[schema(nullable = false, value_type = Option<SignedUrl>)]
    pub thumbnail_url: Option<U::Url>,
}

/// `Plant` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Plant)]
pub struct Plant<U: UrlState = Signed> {
    /// The plant.
    pub id: PlantId,
    /// Its garden.
    pub garden_id: GardenId,
    /// "Tomato by the fence".
    pub name: String,
    /// The working species, suggested or confirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species: Option<SpeciesRef>,
    /// "Something else" in the confirmation card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub custom_species_name: Option<String>,
    /// Whether the species is suggested or confirmed.
    pub species_status: SpeciesStatus,
    /// The candidates behind the suggestion.
    #[schema(value_type = Vec<SpeciesCandidate>)]
    pub species_candidates: Vec<SpeciesCandidate<U>>,
    /// Health status.
    pub status: PlantStatus,
    /// From the garden map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub position: Option<PlantPosition>,
    /// The most recent scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<LatestScan>)]
    pub latest_scan: Option<LatestScan<U>>,
    /// When it was created.
    pub created_at: Timestamp,
}
```

- `SpeciesCandidate.reference_image` is computed from `species_id` each time a candidate is read or sent, never stored ([contracts 9, species](../architecture/contracts.md#species)). A stored candidate has `None`, and `Sign` fills it from the in-memory species photo index:

```rust
impl Sign for SpeciesCandidate<Stored> {
    type Signed = SpeciesCandidate<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> SpeciesCandidate<Signed> {
        let reference_image = self.species_id.as_ref().and_then(|species| {
            signer
                .photos()
                .photos(species)
                .into_iter()
                .next()
                .map(|photo| ReferenceImage {
                    url: signer.url(&ImagePath::SpeciesPhoto {
                        species: species.clone(),
                        hash: photo.hash,
                    }),
                    credit: photo.credit,
                })
        });
        SpeciesCandidate {
            species_id: self.species_id,
            name: self.name,
            score: self.score,
            source: self.source,
            reference_image,
        }
    }
}
```

- `LatestScan` carries both `thumbnail_image_id` (stored and sent) and `thumbnail_url` (signed, from that id).

### Failures, questions and guided checks

```rust
/// `ScanFailureCode` ([contracts 3]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanFailureCode {
    /// Not retryable: start a new scan with new photos.
    NoPlantFound,
    /// Not retryable: start a new scan with new photos.
    PhotosUnusable,
    /// Retryable: job attempts exhausted.
    ProcessingError,
    /// Retryable: LLM API or weather down.
    UpstreamUnavailable,
}

/// `ScanFailure` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScanFailure {
    /// The reason.
    pub code: ScanFailureCode,
    /// Safe to show.
    pub message: String,
    /// Equal to `code.retryable()`.
    pub retryable: bool,
}

/// `"yes" | "no" | "unsure"`: question options, indicator answers and evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// The indicator is present.
    Yes,
    /// The indicator is absent.
    No,
    /// Can't tell; counts for nothing in scoring.
    Unsure,
}

/// `Question.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// Answered with an option.
    Indicator,
    /// Answered by uploading a photo with `question_id`.
    PhotoRequest,
}

/// `Question.options[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QuestionOption {
    /// The option.
    pub id: Answer,
    /// Its label.
    pub label: String,
}

/// `Question.status`: what the app sees. The stored lifecycle has three more states, in
/// [`crate::rules::questions::QuestionState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    /// Waiting for an answer.
    Open,
    /// A photo for it is being analysed; further answers and photos get `409`.
    Answering,
}

/// `Question` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Question {
    /// The question.
    pub id: QuestionId,
    /// The run that asked it.
    pub run: Run,
    /// How it is answered.
    pub kind: QuestionKind,
    /// "Do the spots have rings, like a target?"
    pub text: String,
    /// Indicator questions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub options: Option<Vec<QuestionOption>>,
    /// `photo_request`: which shot to take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub photo_kind: Option<ImageKind>,
    /// "Turn the leaf over and photograph the underside".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub photo_hint: Option<String>,
    /// The problems this question separates.
    pub candidates: Vec<ProblemId>,
    /// When it was asked.
    pub asked_at: Timestamp,
    /// When the scheduler continues without an answer.
    pub continue_at: Timestamp,
    /// `open` or `answering`.
    pub status: QuestionStatus,
}

/// `GuidedCheck.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidedKind {
    /// Squeeze a handful of moist soil.
    SqueezeTest,
    /// Shake soil with water in a jar and let it settle.
    JarTest,
}

/// `GuidedCheck.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidedCheckStatus {
    /// Its `guided` job is queued or running.
    Analysing,
    /// Finished; `result` is set unless a jar hadn't settled.
    Done,
    /// The job failed for good; the check is photographed again as a new check.
    Failed,
}

/// `GuidedCheck` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = GuidedCheck)]
pub struct GuidedCheck<U: UrlState = Signed> {
    /// The check's photo.
    pub image_id: ImageId,
    /// Which check.
    pub kind: GuidedKind,
    /// Its progress.
    pub status: GuidedCheckStatus,
    /// A soil observation with aspect `texture` ([contracts 7]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Observation>)]
    pub result: Option<Observation<U>>,
}
```

- `Answer` is the `"yes" | "no" | "unsure"` used by options, indicator answers and evidence.
- `QuestionStatus` is what the app sees (`open`, `answering`). The stored lifecycle adds `answered`, `superseded` and `dropped` ([`rules::questions::QuestionState`](domain.md#7-rules)).

### Scan and scan detail

```rust
/// `Scan` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Scan)]
pub struct Scan<U: UrlState = Signed> {
    /// The scan.
    pub id: ScanId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its plant.
    pub plant_id: PlantId,
    /// Its status.
    pub status: ScanStatus,
    /// 0 until submitted.
    pub run: Run,
    /// Every plant, soil and guided-check photo.
    #[schema(value_type = Vec<ScanImage>)]
    pub images: Vec<ScanImage<U>>,
    /// Every guided check.
    #[schema(value_type = Vec<GuidedCheck>)]
    pub guided_checks: Vec<GuidedCheck<U>>,
    /// When it was created.
    pub created_at: Timestamp,
    /// Absent once that user is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub created_by: Option<UserRef>,
    /// When it was first submitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub submitted_at: Option<Timestamp>,
    /// Present while a question is unanswered, in any status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub open_question: Option<Question>,
    /// The current diagnosis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub diagnosis_id: Option<DiagnosisId>,
    /// The current plan version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_id: Option<PlanId>,
    /// With status `failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub failure: Option<ScanFailure>,
    /// The highest `scan_event.seq` so far.
    pub last_event_seq: EventSeq,
}

/// `ScanDetail extends Scan` ([contracts 3]): `GET /scans/{id}`. `extends` is a flattened
/// `scan` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ScanDetail)]
pub struct ScanDetail<U: UrlState = Signed> {
    /// Every field of `Scan`.
    #[serde(flatten)]
    #[schema(value_type = Scan)]
    pub scan: Scan<U>,
    /// The scan's plant.
    #[schema(value_type = Plant)]
    pub plant: Plant<U>,
    /// The latest observations of each stage, across runs ([contracts 3], observations across
    /// runs).
    #[schema(value_type = Vec<Observation>)]
    pub observations: Vec<Observation<U>>,
    /// The run's weather snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub weather: Option<WeatherSummary>,
    /// The current diagnosis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Diagnosis>)]
    pub diagnosis: Option<Diagnosis<U>>,
    /// The current plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan: Option<ActionPlan>,
}
```

`ScanDetail extends Scan` is a flattened `scan` field, so the JSON is one object.

### Requests

```rust
/// `new_plant` of `CreateScanRequest` and `GardenMapInput.pins[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPlantInput {
    /// The plant's name.
    pub name: String,
    /// Its species, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
}

/// `POST /scans` ([contracts 3]). Exactly one of `plant_id` and `new_plant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateScanRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// An existing plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// A plant to create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub new_plant: Option<NewPlantInput>,
}

/// The plant a new scan is for, once "exactly one of" is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanPlant {
    /// An existing plant of the garden.
    Existing(PlantId),
    /// A plant created with the scan.
    New(NewPlantInput),
}
```

"Exactly one of `plant_id` and `new_plant`" can't be expressed in serde without an untagged enum, so the request keeps both fields and `plant()` turns it into `ScanPlant` or a `400`:

```rust
impl CreateScanRequest {
    /// Checks "exactly one of `plant_id` and `new_plant`".
    pub fn plant(self) -> Result<ScanPlant, DomainError> {
        match (self.plant_id, self.new_plant) {
            (Some(id), None) => Ok(ScanPlant::Existing(id)),
            (None, Some(new)) => Ok(ScanPlant::New(new)),
            _ => Err(DomainError::invalid(
                "plant_id",
                "exactly one of plant_id and new_plant is required",
            )),
        }
    }
}
```

### Upload with depth

```rust
/// `DepthMeta.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DepthSource {
    /// The `LiDAR` depth camera or `ARKit` scene depth.
    Lidar,
}

/// `DepthMeta.intrinsics`: pixels of a reference size, in the sensor frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Intrinsics {
    /// Focal length x, pixels.
    pub fx: f64,
    /// Focal length y, pixels.
    pub fy: f64,
    /// Principal point x, pixels.
    pub cx: f64,
    /// Principal point y, pixels.
    pub cy: f64,
    /// The reference width the values are in.
    pub ref_width: u32,
    /// The reference height the values are in.
    pub ref_height: u32,
}

/// `DepthMeta.distortion`: only if the day-1 check finds the photo and depth distorted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Distortion {
    /// Reference-size pixels, sensor frame.
    pub center: [f64; 2],
    /// The calibration's inverse lens distortion table.
    pub inverse_lookup: Vec<f64>,
}

/// `DepthMeta` ([contracts 3], depth): the `depth_meta` multipart part, JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DepthMeta {
    /// Always `lidar`.
    pub source: DepthSource,
    /// Camera intrinsics.
    pub intrinsics: Intrinsics,
    /// Lens distortion, when sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub distortion: Option<Distortion>,
    /// Unit vector towards the ground, camera coordinates (x right, y down, z forward), sensor
    /// frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub gravity: Option<[f64; 3]>,
    /// "iPhone16,1".
    pub device_model: String,
    /// The phone filled holes in the depth map.
    pub filtered: bool,
}

/// `UploadImageResponse.quality`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct QualityReport {
    /// Variance of the Laplacian.
    pub blur_score: f64,
    /// Mean exposure, 0–1.
    pub exposure: f64,
    /// Always `true`: a failing shot gets `422 image_quality` instead.
    pub passed: True,
}

/// Why depth sent with a photo was dropped ([contracts 3]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DepthProblem {
    /// Not the photo's aspect ratio within 1%.
    AspectMismatch,
    /// Fewer than 20% of the pixels have depth.
    TooSparse,
    /// `depth_meta` is invalid, or the orientation is mirrored.
    InvalidMeta,
    /// Not a 16-bit grayscale PNG.
    UnsupportedFormat,
    /// Over 4 MB.
    TooLarge,
    /// Sent with a `squeeze_test` or `jar_test`.
    NotForThisKind,
    /// Writing the depth or confidence map failed after the photo was stored.
    StorageError,
}

/// `UploadImageResponse.depth`: `{ stored: true } | { stored: false, problem }`. The union is
/// tagged by a boolean, which serde can't tag on, so it is untagged with [`True`] and [`False`]
/// literal fields that only deserialise from their own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum DepthOutcome {
    /// The depth (and confidence map) were stored.
    Stored {
        /// `true`.
        stored: True,
    },
    /// The photo was stored without its depth.
    Dropped {
        /// `false`.
        stored: False,
        /// Why.
        problem: DepthProblem,
    },
}

/// `201` of `POST /scans/{id}/images` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = UploadImageResponse)]
pub struct UploadImageResponse<U: UrlState = Signed> {
    /// The stored image.
    #[schema(value_type = ScanImage)]
    pub image: ScanImage<U>,
    /// The quality scores.
    pub quality: QualityReport,
    /// Present when depth parts were sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub depth: Option<DepthOutcome>,
}
```

- `DepthOutcome` is `{ stored: true } | { stored: false, problem }`: tagged by a boolean, which serde can't tag on. It is untagged with `True` and `False` literal fields; each only decodes from its own value, so the branches can't be confused. The API never decodes it; the app does.
- The `depth_meta` multipart part is a `DepthMeta` (JSON), with `deny_unknown_fields`.

### Answers, species confirmation and feedback

```rust
/// `POST /scans/{id}/answers` ([contracts 3]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnswerRequest {
    /// An `indicator` question.
    pub question_id: QuestionId,
    /// The answer.
    pub option_id: Answer,
}

/// `POST /plants/{id}/species` ([contracts 3]). Exactly one of the two fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmSpeciesRequest {
    /// A knowledge-base species.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
    /// "Something else", at most 80 characters: the plant is treated as unsupported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub custom_name: Option<String>,
}

/// The user's species choice, once "exactly one of" and the length are checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeciesChoice {
    /// A knowledge-base species.
    Known(SpeciesId),
    /// "Something else": skip species identification.
    Custom(String),
}

/// `FeedbackRequest.verdict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The diagnosis was right.
    Correct,
    /// It was wrong.
    Wrong,
    /// Can't say.
    Unsure,
}

/// `POST /scans/{id}/feedback` ([contracts 3]): stored with the author's user id; deleted with
/// their account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRequest {
    /// The verdict.
    pub verdict: Verdict,
    /// Free text, or a slug when picked from a list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub corrected_label: Option<String>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub notes: Option<String>,
}

/// `{ id: Id }`, the `201` of `POST /scans/{id}/feedback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "", as = Created)]
pub struct Created<T: Table> {
    /// The new record.
    #[schema(value_type = String)]
    pub id: crate::id::Id<T>,
}
```

`ConfirmSpeciesRequest::choice` checks "exactly one of" and trims `custom_name` to 1–80 characters.

---

## 4. Scan events

[Contracts 4](../architecture/contracts.md#4-scan-events-sse).

```rust
/// `Stage` ([contracts 4]): every pipeline stage, in pipeline order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Perception: is a plant present?
    PlantCheck,
    /// Perception: leaf (and plant, soil, surroundings) regions, with masks when available.
    Segmentation,
    /// Perception: `BioCLIP` species identification.
    Species,
    /// Perception: disease classification per leaf region.
    Disease,
    /// Perception: image vectors for similar cases.
    ImageEmbedding,
    /// `api`: `LiDAR` measurement, only when a photo has depth.
    Measurement,
    /// `api`: weather and soil context.
    Context,
    /// `api`: the pre-VLM candidate set and its indicators.
    Retrieve,
    /// VLM: findings call.
    VlmFindings,
    /// VLM: indicators call.
    VlmIndicators,
    /// VLM: a guided soil check.
    Guided,
    /// VLM: a `verify` job.
    Verify,
    /// `api`: deterministic candidate scoring.
    Scoring,
    /// `api`: fusion risk rules.
    Fusion,
    /// `api` → agent core: the plan writer.
    Plan,
}

/// `ProblemKind` ([contracts 4]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// Fungal disease.
    Fungal,
    /// Bacterial disease.
    Bacterial,
    /// Viral disease.
    Viral,
    /// Insect and other pests.
    Pest,
    /// Larger animals.
    Wildlife,
    /// Non-living causes: nutrients, water, sun, cold.
    Abiotic,
}

/// `run_started.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunReason {
    /// Run 1.
    Submit,
    /// A retryable failure was resubmitted.
    Resubmit,
    /// The user picked a different species.
    SpeciesChanged,
}

/// `stage_completed.skipped` and the worker's `stage_completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Skipped {
    /// Such as `"species_not_supported"`, `"nothing_measurable"`, `"error"`, `"all_measured"`.
    pub reason: String,
}

/// `candidates_selected.candidates[].source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    /// The disease classifier's top 3.
    Classifier,
    /// `susceptible_to` the working species.
    Species,
    /// A look-alike of a top-ranked candidate.
    Lookalike,
    /// An organism the VLM found, added by a `verify` job.
    Organism,
    /// A damage pattern the VLM found, added by a `verify` job.
    Damage,
}

/// `candidates_selected.candidates[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SelectedCandidate {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
    /// Why it is in the set.
    pub source: SelectionSource,
}

/// `question_resolved.by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedBy {
    /// The user answered.
    User,
    /// A photo answered it.
    Photo,
    /// A new run, or a failed scan, superseded it.
    Superseded,
    /// The photo sent for it couldn't be analysed; the plan stays.
    Dropped,
}

/// `plan_superseded.reason` ([contracts 7], re-planning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupersedeReason {
    /// An answer or photo after the scheduler continued without it.
    LateAnswer,
    /// A species-change run reached `planned` again.
    SpeciesChanged,
    /// A confirmed `revise_plan` card.
    ChatRevision,
    /// A guided check changed the texture.
    GuidedCheck,
}
```

The catalogue is one enum, internally tagged by `type`, flattened into the envelope. `ScanEvent<Stored>` is what `scan_event.payload` holds; `ScanEvent<Signed>` (`ScanEvent`) is what the SSE `data:` line carries.

```rust
/// The event catalogue ([contracts 4]), tagged by `type` and flattened into [`ScanEvent`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", bound = "")]
#[schema(as = ScanEventBody)]
pub enum ScanEventBody<U: UrlState = Signed> {
    /// Reset the listed stages; clear their findings.
    RunStarted {
        /// Why the run started.
        reason: RunReason,
        /// The stages the run repeats.
        rerun_stages: Vec<Stage>,
    },
    /// Update the status chip.
    StatusChanged {
        /// The old status.
        from: ScanStatus,
        /// The new status.
        to: ScanStatus,
    },
    /// Show the stage as in progress.
    StageStarted {
        /// The stage.
        stage: Stage,
    },
    /// Tick the stage.
    StageCompleted {
        /// The stage.
        stage: Stage,
        /// How long it took.
        duration_ms: u64,
        /// Why it was skipped, if it was.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        skipped: Option<Skipped>,
    },
    /// Add the finding; draw its region and mask.
    Finding {
        /// The finding.
        #[schema(value_type = Observation)]
        observation: Observation<U>,
    },
    /// Remove a finding the job's final result didn't keep.
    FindingRetracted {
        /// The retracted observation.
        observation_id: ObservationId,
    },
    /// Show the weather strip.
    WeatherLoaded {
        /// The run's weather.
        weather: WeatherSummary,
    },
    /// "Checking 9 possible problems".
    CandidatesSelected {
        /// The candidate set.
        candidates: Vec<SelectedCandidate>,
    },
    /// Show the confirmation card when `needs_confirmation`.
    SpeciesSuggested {
        /// `BioCLIP` and VLM candidates.
        #[schema(value_type = Vec<SpeciesCandidate>)]
        candidates: Vec<SpeciesCandidate<U>>,
        /// The species the run continues with.
        #[schema(required = true)]
        working_species_id: Option<SpeciesId>,
        /// Whether to ask the user.
        needs_confirmation: bool,
    },
    /// Hide the card.
    SpeciesConfirmed {
        /// `null` for "something else".
        #[schema(required = true)]
        species_id: Option<SpeciesId>,
        /// "Something else".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        custom_name: Option<String>,
        /// A new run follows (announced by `run_started`).
        rerun: bool,
    },
    /// Show the question card.
    Question {
        /// The question.
        question: Question,
    },
    /// Hide the card.
    QuestionResolved {
        /// The question.
        question_id: QuestionId,
        /// How it resolved.
        by: ResolvedBy,
    },
    /// Show the texture result, or the failure.
    GuidedCheckDone {
        /// The check.
        #[schema(value_type = GuidedCheck)]
        check: GuidedCheck<U>,
    },
    /// Show the result.
    Diagnosis {
        /// The diagnosis.
        #[schema(value_type = Diagnosis)]
        diagnosis: Diagnosis<U>,
    },
    /// Fetch `GET /scans/{id}` and show the plan.
    PlanReady {
        /// The plan.
        plan_id: PlanId,
        /// Written from an `uncertain` diagnosis.
        provisional: bool,
    },
    /// Replace the plan, show a notice.
    PlanSuperseded {
        /// The old plan.
        old_plan_id: PlanId,
        /// The new plan.
        new_plan_id: PlanId,
        /// Why.
        reason: SupersedeReason,
    },
    /// Show the error; offer "retry" when retryable, otherwise "new scan".
    Failed {
        /// The failure.
        failure: ScanFailure,
    },
}

/// `ScanEventType`: the `type` values, for the `scan_event.type` column and the SSE `event:`
/// line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanEventType {
    /// `run_started`
    RunStarted,
    /// `status_changed`
    StatusChanged,
    /// `stage_started`
    StageStarted,
    /// `stage_completed`
    StageCompleted,
    /// `finding`
    Finding,
    /// `finding_retracted`
    FindingRetracted,
    /// `weather_loaded`
    WeatherLoaded,
    /// `candidates_selected`
    CandidatesSelected,
    /// `species_suggested`
    SpeciesSuggested,
    /// `species_confirmed`
    SpeciesConfirmed,
    /// `question`
    Question,
    /// `question_resolved`
    QuestionResolved,
    /// `guided_check_done`
    GuidedCheckDone,
    /// `diagnosis`
    Diagnosis,
    /// `plan_ready`
    PlanReady,
    /// `plan_superseded`
    PlanSuperseded,
    /// `failed`
    Failed,
}

/// `ScanEventBase` plus its body ([contracts 4], envelope): what the SSE `data:` line carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ScanEvent)]
pub struct ScanEvent<U: UrlState = Signed> {
    /// `1`.
    pub v: V1,
    /// The SSE `id`.
    pub seq: EventSeq,
    /// The run the event belongs to.
    pub run: Run,
    /// The scan.
    pub scan_id: ScanId,
    /// When it was stored.
    pub ts: Timestamp,
    /// `type` and the extra fields.
    #[serde(flatten)]
    #[schema(value_type = ScanEventBody)]
    pub body: ScanEventBody<U>,
}
```

- `#[schema(as = ScanEventBody)]` without `bound`: utoipa 6 rejects `bound` on enums ("unexpected attribute: bound"), so the enum uses the default `U: ToSchema`, which only `Signed` implements.
- `event_type()` is the SSE `event:` line and the stored `type` column; a test checks it equals the serde tag.

The envelope of contracts 4, signed as it is sent (the test also checks the key order):

```rust
#[test]
fn finding_envelope_is_signed_when_sent() {
    let signer = FakeSigner;
    let photos = FixedPhotos::default();
    let now: crate::wire::Timestamp = "2026-10-18T14:03:22Z".parse().unwrap();
    let sent = stored_finding().sign((), &Signer::new(&signer, &photos, now));
    assert_eq!(sent.event_name(), "finding");
    let text = serde_json::to_string(&sent).unwrap();
    // The envelope of contracts 4, in order: v, seq, run, scan_id, ts, type, then the body.
    assert!(text.starts_with(
        r#"{"v":1,"seq":17,"run":1,"scan_id":"01JAX3K9T6R2B8V0QZ4M7D1E5F","ts":"2026-10-18T14:03:22Z","type":"finding","observation":{"#
    ));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let obs = &json["observation"];
    assert_eq!(obs["confidence"], 0.71);
    assert_eq!(obs["confidence_band"], "medium");
    assert_eq!(
        obs["ref"],
        serde_json::json!({"table": "disease", "id": "tomato_early_blight"})
    );
    let mask_url = obs["region"]["mask_url"].as_str().unwrap();
    let exp = now.unix_seconds() + 900;
    assert!(mask_url.starts_with(&format!(
        "https://api.test/scans/01JAX3K9T6R2B8V0QZ4M7D1E5F/masks/01JAX7MMMMMMMMMMMMMMMMMMMM?exp={exp}&sig="
    )));
    assert!(obs["region"].get("mask_id").is_none());
    let back: ScanEvent = serde_json::from_str(&text).unwrap();
    assert_eq!(back.body, sent.body);
    assert_eq!(back.ts.to_string(), "2026-10-18T14:03:22Z");
}
```

---

## 5. Worker API and jobs

[Contracts 5](../architecture/contracts.md#5-worker-api-and-jobs).

### Heartbeat and claim

```rust
/// `Capability` ([contracts 5]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// The perception worker (RTX 3070): `perception` jobs.
    Perception,
    /// The VLM worker (RTX 5070 Ti): `vlm`, `verify` and `guided` jobs.
    Vlm,
    /// The fallback worker (LLM API): every kind, 15 seconds after `queued_at`.
    Fallback,
}

/// `Job.kind` ([contracts 5]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// Plant check, segmentation, species, disease, image embeddings.
    Perception,
    /// The findings and indicators calls.
    Vlm,
    /// Indicators for a candidate outside the set, or a new photo.
    Verify,
    /// One guided soil check; belongs to the scan, not a run.
    Guided,
}

/// `Heartbeat.gpu`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GpuInfo {
    /// "NVIDIA `GeForce` RTX 3070".
    pub name: String,
    /// VRAM in use, MB.
    pub vram_used_mb: u32,
    /// Total VRAM, MB.
    pub vram_total_mb: u32,
}

/// `Heartbeat.current_jobs[]`: the leases to extend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HeldJob {
    /// The job.
    pub job_id: JobId,
    /// Its lease token.
    pub lease_token: LeaseToken,
}

/// `POST /internal/workers/heartbeat` ([contracts 5]), every 10 seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Heartbeat {
    /// Stable per process.
    pub worker_id: WorkerId,
    /// What it claims.
    pub caps: Vec<Capability>,
    /// The most jobs it holds at once.
    pub slots: u32,
    /// The GPU, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub gpu: Option<GpuInfo>,
    /// Extends these leases, within `max_runtime`.
    pub current_jobs: Vec<HeldJob>,
    /// Model name → version, shown on `/status`.
    pub versions: BTreeMap<String, String>,
}

/// `200` of the heartbeat.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct HeartbeatResponse {
    /// Stop these jobs now.
    pub cancel_job_ids: Vec<JobId>,
}

/// `POST /internal/jobs/claim` ([contracts 5]), a long poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClaimRequest {
    /// The worker.
    pub worker_id: WorkerId,
    /// Its capabilities.
    pub caps: Vec<Capability>,
    /// As in the heartbeat; a worker already holding `slots` jobs gets `204`.
    pub slots: u32,
    /// Default 25, at most 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub wait_s: Option<u32>,
}
```

### Job and payloads

`JobImage.url` is the worker route `/internal/jobs/{job_id}/images/{image_id}`, authorised by the lease token: not signed and never expiring, so it is a `WorkerImagePath` built from the two ids, and it may be stored in `job.payload`.

```rust
/// `JobImage` ([contracts 5]). `url` is the worker route
/// `GET /internal/jobs/{job_id}/images/{image_id}`, authorised by the lease token; it is not
/// signed and never expires, so it is a [`WorkerImagePath`] and may be stored in the payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobImage {
    /// The image.
    pub image_id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// Stored width.
    pub width: u32,
    /// Stored height.
    pub height: u32,
    /// The worker route for its bytes.
    pub url: WorkerImagePath,
}

/// `/internal/jobs/{job_id}/images/{image_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct WorkerImagePath(String);

/// `PerceptionPayload.stages[]`: the perception stages only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionStage {
    /// Plant present?
    PlantCheck,
    /// Regions and masks.
    Segmentation,
    /// `BioCLIP`.
    Species,
    /// The disease classifier.
    Disease,
    /// Image vectors.
    ImageEmbedding,
}

/// `RegionClass` ([contracts 5]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegionClass {
    /// The whole plant.
    Plant,
    /// A leaf (the reliable class).
    Leaf,
    /// Soil.
    Soil,
    /// Surroundings.
    Surroundings,
}

/// `PerceptionPayload.regions[]`: `Region & { region_id: string; class: RegionClass }` from an
/// earlier run, when segmentation isn't re-run. The optional `mask_url` of `Region` is never
/// sent: workers have no route to masks, and species and disease work from the boxes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PriorRegion {
    /// Always present here.
    pub region_id: RegionId,
    /// The image.
    pub image_id: ImageId,
    /// The box.
    pub bbox: BBox,
    /// The class.
    pub class: RegionClass,
}

/// `PerceptionPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PerceptionPayload {
    /// Every plant and soil image of the scan, or only a question's photo; never guided-check
    /// images, never depth.
    pub images: Vec<JobImage>,
    /// The stages to run.
    pub stages: Vec<PerceptionStage>,
    /// Confirmed or user-chosen species: skip identification, use this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
    /// True when the user chose "something else" (no `species_id`). Omitted when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_species_identification: bool,
    /// From an earlier run, when segmentation isn't re-run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub regions: Option<Vec<PriorRegion>>,
    /// `BioCLIP` label list version.
    pub label_set_version: String,
    /// Classifier class → problem mapping version.
    pub classifier_labels_version: String,
    /// Set on a question photo's segmentation-only job: `stages` is `["segmentation"]`,
    /// `images` holds that photo alone, and nothing it returns can fail the scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub question_id: Option<QuestionId>,
}

/// `Indicator` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Indicator {
    /// `"tomato_early_blight.concentric_rings"`.
    pub indicator_id: IndicatorId,
    /// Asked to the VLM verbatim.
    pub question: String,
    /// Where it shows.
    pub r#where: IndicatorWhere,
    /// Which shots can show it.
    pub image_kinds: Vec<ImageKind>,
}

/// `CandidatePrompt` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CandidatePrompt {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// Its 3 discriminating indicators, minus those a measurement settled; a candidate with
    /// none left is not sent.
    pub indicators: Vec<Indicator>,
}

/// `VlmPayload.working_species`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkingSpecies {
    /// `null` for a name outside the knowledge base.
    #[schema(required = true)]
    pub id: Option<SpeciesId>,
    /// Its name.
    pub name: String,
}

/// `VlmPayload.species_options[]`: allowed `species_guess` values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesOption {
    /// The species.
    pub id: SpeciesId,
    /// Its name.
    pub name: String,
}

/// `VlmPayload.organism_options[]`: allowed organism labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OrganismOption {
    /// The organism.
    pub id: OrganismId,
    /// Its name.
    pub name: String,
}

/// `VlmPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VlmPayload {
    /// At most 4, fixed order ([`crate::rules::routing`]).
    pub findings_images: Vec<JobImage>,
    /// At most 4, chosen from the indicators; empty when measurements answered all.
    pub indicator_images: Vec<JobImage>,
    /// The working species.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub working_species: Option<WorkingSpecies>,
    /// Allowed `species_guess` values.
    pub species_options: Vec<SpeciesOption>,
    /// Allowed organism labels.
    pub organism_options: Vec<OrganismOption>,
    /// At most 9; empty when measurements answered every indicator.
    pub candidates: Vec<CandidatePrompt>,
}

/// `VerifyPayload.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerifyReason {
    /// The VLM found an organism or damage pattern outside the candidate set.
    OutsideCandidateSet,
    /// A photo sent for a question.
    NewPhoto,
}

/// `VerifyPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VerifyPayload {
    /// Why.
    pub reason: VerifyReason,
    /// With `new_photo`: the question the photo answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub question_id: Option<QuestionId>,
    /// At most 4; for `new_photo`, the new shot first.
    pub images: Vec<JobImage>,
    /// The candidates to ask about.
    pub candidates: Vec<CandidatePrompt>,
}

/// `GuidedPayload` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GuidedPayload {
    /// One `squeeze_test` or `jar_test` image, alone.
    pub image: JobImage,
}
```

- `PerceptionPayload.regions` is `(Region & { region_id; class })[]` in the contract. `PriorRegion` sends `region_id`, `image_id`, `bbox` and `class`; the optional `mask_url` is never sent, because workers have no route to masks and species and disease work from the boxes.
- `skip_species_identification?: boolean` is a `bool` that is omitted when false (`skip_serializing_if = "std::ops::Not::not"`), which reads the same.
- `Indicator.where` is a Rust keyword, so the field is `r#where`; serde and utoipa both strip the `r#`.

`kind` and `payload` sit side by side in the job, so the payload union is adjacently tagged and flattened:

```rust
/// `kind` and `payload` side by side ([contracts 5], `Job`): adjacently tagged, flattened into
/// [`Job`]. Stored as `job.kind` and `job.payload`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum JobSpec {
    /// A `perception` job.
    Perception(PerceptionPayload),
    /// A `vlm` job.
    Vlm(VlmPayload),
    /// A `verify` job.
    Verify(VerifyPayload),
    /// A `guided` job.
    Guided(GuidedPayload),
}

/// `Job` ([contracts 5]): `200` of the claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Job {
    /// `1`.
    pub v: V1,
    /// The job.
    pub job_id: JobId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Absent for guided jobs, which don't belong to a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub run: Option<Run>,
    /// 1 on the first try.
    pub attempt: u32,
    /// Sent as `X-Lease-Token` on every later call.
    pub lease_token: LeaseToken,
    /// When the lease ends unless a heartbeat extends it.
    pub lease_until: Timestamp,
    /// The kind's `max_runtime`, doubled on the fallback worker.
    pub max_runtime_s: u32,
    /// `kind` and `payload`.
    #[serde(flatten)]
    pub spec: JobSpec,
}

/// The fields a worker needs to fail a job, readable from any version: the worker decodes this
/// first, and the full [`Job`] only when `v` is 1 ([contracts 1], versioning).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JobHead {
    /// The payload's version.
    pub v: u64,
    /// The job.
    pub job_id: JobId,
    /// Its lease token.
    pub lease_token: LeaseToken,
}

/// What a worker makes of a claimed body.
#[derive(Debug, Clone, PartialEq)]
pub enum ClaimedJob {
    /// A job this worker can read.
    Job(Box<Job>),
    /// Complete it as failed with `unsupported_version`.
    Unsupported(JobHead),
}
```

A worker decodes `JobHead` first: a `v` it doesn't support still leaves it the `job_id` and `lease_token` it needs to fail the job with `unsupported_version` ([contracts 1, versioning](../architecture/contracts.md#versioning)).

### Events and completion

```rust
/// `PartialResult` ([contracts 5]): the complete output of one finished stage, tagged by its
/// own `stage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum PartialResult {
    /// The plant check.
    PlantCheck {
        /// Plant present?
        plant_present: bool,
    },
    /// Segmentation.
    Segmentation {
        /// Its regions.
        regions: Vec<PerceptionRegion>,
    },
    /// `BioCLIP`.
    Species {
        /// Its candidates.
        species: SpeciesResult,
    },
    /// The classifier.
    Disease {
        /// Its results.
        disease: Vec<DiseaseResult>,
    },
    /// The VLM's findings call.
    VlmFindings {
        /// The findings.
        findings: VlmFindings,
    },
}

/// `WorkerEvents.events[]` ([contracts 5]), tagged by `type`. An event type from a newer
/// worker decodes as `Unknown` and is ignored; an unknown value inside a known event is still
/// an error, because the API must store it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerEvent {
    /// A stage started.
    StageStarted {
        /// The stage.
        stage: Stage,
    },
    /// A stage completed.
    StageCompleted {
        /// The stage.
        stage: Stage,
        /// How long it took.
        duration_ms: u64,
        /// Why it was skipped.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        skipped: Option<Skipped>,
    },
    /// One finished stage's output.
    PartialResult {
        /// The stage.
        stage: Stage,
        /// Its output.
        result: PartialResult,
    },
    /// An event type this API doesn't know. Ignored; never sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// `POST /internal/jobs/{id}/events` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WorkerEvents {
    /// `1`.
    pub v: V1,
    /// The events, in order.
    pub events: Vec<WorkerEvent>,
}
```

- An event type from a newer worker decodes as `WorkerEvent::Unknown` (`#[serde(other, skip_serializing)]`) and is ignored; an unknown value inside a known event is an error, because the API must store it.
- `PartialResult` is tagged by its own `stage`; the API checks it equals the event's `stage`.

```rust
/// `JobErrorCode` ([contracts 5], complete): the codes a worker sends in `CompleteJob.error`.
/// The contract types the field as `JobErrorCode | string`, so a code this build doesn't know
/// (from a newer worker) still decodes, as [`JobErrorCode::Other`], and is stored as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobErrorCode {
    /// A payload or job `v` the worker doesn't support, a job kind it doesn't know, or a payload
    /// it can't read; `retryable: false`.
    UnsupportedVersion,
    /// A `label_set_version` or `classifier_labels_version` the worker hasn't loaded;
    /// `retryable: false`.
    UnknownLabelVersion,
    /// Inference failed, or its output failed validation twice; `retryable: true`, except
    /// `false` when the worker's adapter can't do the job's kind at all.
    ModelError,
    /// An image URL failed after retries; `retryable: true`.
    ImageFetchFailed,
    /// The worker stopped the job itself at its soft deadline, before `max_runtime_s` (a hung
    /// or slow model); `retryable: true`. Settled by contracts 12, question 18.
    MaxRuntimeExceeded,
    /// Any other code, kept verbatim.
    Other(String),
}
```

```rust
impl JobErrorCode {
    /// The wire string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnknownLabelVersion => "unknown_label_version",
            Self::ModelError => "model_error",
            Self::ImageFetchFailed => "image_fetch_failed",
            Self::MaxRuntimeExceeded => "max_runtime_exceeded",
            Self::Other(code) => code,
        }
    }

    /// Whether the contract says a failure with this code may be retried; `None` for
    /// [`JobErrorCode::Other`], whose `retryable` flag alone decides.
    pub const fn retryable(&self) -> Option<bool> {
        match self {
            Self::UnsupportedVersion | Self::UnknownLabelVersion => Some(false),
            Self::ModelError | Self::ImageFetchFailed | Self::MaxRuntimeExceeded => Some(true),
            Self::Other(_) => None,
        }
    }
}
```

`JobErrorCode` serialises as its bare string and decodes any string, so a code from a newer worker is kept as `Other` and stored as written. `retryable()` is the contract's rule for each known code ([contracts 5, complete](../architecture/contracts.md#post-internaljobsidcomplete)); `None` means the worker's own `retryable` flag decides.

```rust
/// The keys of `CompleteJob.model_versions` and the heartbeat's `versions` ([contracts 5],
/// `model_versions` keys). Plain constants: the map stays `BTreeMap<String, String>`, because
/// each model a job used adds its own `<model name>` key beside these.
pub struct ModelVersionKey;

/// `CompleteJob.error` ([contracts 5]): job errors travel here, never as HTTP status codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobError {
    /// A [`JobErrorCode`], or another string.
    pub code: JobErrorCode,
    /// Detail for the log.
    pub message: String,
    /// Counts toward `attempts` when true.
    pub retryable: bool,
}

/// `status` with `result` (done) or `error` (failed), flattened into [`CompleteJob`]. The tag
/// makes the other combinations unrepresentable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobOutcome {
    /// The job finished.
    Done {
        /// The kind's result, decoded with [`JobResult::decode`].
        #[schema(value_type = Object)]
        result: serde_json::Value,
    },
    /// The job failed.
    Failed {
        /// Why.
        error: JobError,
    },
}

/// `POST /internal/jobs/{id}/complete` ([contracts 5]). A repeat with the same lease token for
/// a completed job returns `204` and changes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CompleteJob {
    /// `1`.
    pub v: V1,
    /// `status`, with `result` or `error`.
    #[serde(flatten)]
    pub outcome: JobOutcome,
    /// Model name → version.
    pub model_versions: BTreeMap<String, String>,
    /// Stage → milliseconds.
    pub timings_ms: BTreeMap<String, u64>,
}
```

`status`, `result` and `error` are one enum tagged by `status`, so `done` without `result` or `failed` without `error` can't be decoded. `model_versions` stays a `BTreeMap<String, String>`: `ModelVersionKey` names the fixed keys, and each model a job used adds its own manifest-entry key beside them.

### Results

```rust
/// `RleMask` ([contracts 5]): COCO-style compressed run-length encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RleMask {
    /// Height, width of the stored image.
    pub size: [u32; 2],
    /// The compressed counts.
    pub counts: String,
}

/// `PerceptionResult.regions[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PerceptionRegion {
    /// `"<image_id>.<n>"`.
    pub region_id: RegionId,
    /// The image.
    pub image_id: ImageId,
    /// The class.
    pub class: RegionClass,
    /// The box.
    pub bbox: BBox,
    /// The API stores it as a PNG and serves it through a signed URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub mask: Option<RleMask>,
    /// The detector's confidence.
    pub confidence: Confidence,
}

/// `PerceptionResult.species.candidates[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesLabel {
    /// `null` for a label outside the knowledge base.
    #[schema(required = true)]
    pub species_id: Option<SpeciesId>,
    /// The `BioCLIP` label.
    pub label: String,
    /// Its score.
    pub score: Confidence,
}

/// `PerceptionResult.species`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesResult {
    /// Top candidates.
    pub candidates: Vec<SpeciesLabel>,
}

/// `PerceptionResult.disease[].top[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProblemScore {
    /// A problem, or `"healthy"`.
    pub problem_id: ProblemOrHealthy,
    /// Its confidence.
    pub confidence: Confidence,
}

/// `PerceptionResult.disease[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DiseaseResult {
    /// The image.
    pub image_id: ImageId,
    /// Absent when the classifier ran on the whole close-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub region_id: Option<RegionId>,
    /// Top 3, restricted to the working species' labels.
    pub top: Vec<ProblemScore>,
}

/// `PerceptionResult.disease_skipped.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiseaseSkipReason {
    /// The classifier doesn't cover the species.
    SpeciesNotSupported,
    /// No leaf region and no close-up.
    NoLeavesFound,
}

/// `PerceptionResult.disease_skipped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiseaseSkipped {
    /// Why.
    pub reason: DiseaseSkipReason,
}

/// `PerceptionResult.image_embeddings[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ImageEmbedding {
    /// The image.
    pub image_id: ImageId,
    /// The embedding model.
    pub model: String,
    /// The vector.
    pub vector: Vec<f32>,
}

/// `PerceptionResult` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PerceptionResult {
    /// Present when `plant_check` was requested; `false` fails the scan with `no_plant_found`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_present: Option<bool>,
    /// Regions found.
    pub regions: Vec<PerceptionRegion>,
    /// `BioCLIP`'s candidates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species: Option<SpeciesResult>,
    /// The classifier's results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub disease: Option<Vec<DiseaseResult>>,
    /// Why disease classification didn't run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub disease_skipped: Option<DiseaseSkipped>,
    /// Image vectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub image_embeddings: Option<Vec<ImageEmbedding>>,
}

/// `VlmResult` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct VlmResult {
    /// Aliases already mapped back to ids.
    pub findings: VlmFindings,
    /// Empty when the indicators call was skipped.
    pub indicators: VlmIndicatorAnswers,
}

/// `VerifyResult` ([contracts 5]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VerifyResult {
    /// The answers.
    pub indicators: VlmIndicatorAnswers,
}

/// `GuidedResult` ([contracts 5]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GuidedResult {
    /// The texture reading.
    pub texture: GuidedTexture,
}
```

The contract sends `result` with no tag (`PerceptionResult | VlmResult | VerifyResult | GuidedResult`) and, since v0.9, says it is read by the job's kind. An untagged decoder takes the first variant that fits and, ignoring unknown fields, reads a `VlmResult` as a `VerifyResult` (a subset of its fields). So `JobOutcome::Done` keeps the raw JSON, and the API decodes it for the kind on the job's record:

```rust
/// A decoded job result. contracts 5 sends `result` with no tag
/// (`PerceptionResult | VlmResult | VerifyResult | GuidedResult`), and an untagged decoder would
/// take a `VlmResult` for a `VerifyResult` (a subset of its fields), so the API keeps the raw
/// JSON and decodes it for the kind it has on record ([`JobResult::decode`]).
#[derive(Debug, Clone, PartialEq)]
pub enum JobResult {
    /// From a `perception` job.
    Perception(PerceptionResult),
    /// From a `vlm` job.
    Vlm(VlmResult),
    /// From a `verify` job.
    Verify(VerifyResult),
    /// From a `guided` job.
    Guided(GuidedResult),
}
```

A job of each kind round-trips:

```rust
#[test]
fn job_with_each_payload_kind() {
    let perception = job(
        JobSpec::Perception(PerceptionPayload {
            images: vec![image(JOB, IMG, ImageKind::WholePlant)],
            stages: PerceptionStage::ALL.to_vec(),
            species_id: None,
            skip_species_identification: false,
            regions: None,
            label_set_version: "2026-10-20".to_owned(),
            classifier_labels_version: "plantdoc-v3".to_owned(),
            question_id: None,
        }),
        Some(1),
    );
    round_trip(&perception, "perception");
    let json = serde_json::to_value(&perception).unwrap();
    assert!(json["payload"].get("skip_species_identification").is_none());
    assert_eq!(
        json["payload"]["images"][0]["url"],
        format!("/internal/jobs/{JOB}/images/{IMG}")
    );

    let candidate = CandidatePrompt {
        problem_id: "tomato_early_blight".parse().unwrap(),
        name: "Early blight".to_owned(),
        indicators: vec![Indicator {
            indicator_id: "tomato_early_blight.concentric_rings".parse().unwrap(),
            question: "Do the brown spots have rings inside them, like a target?".to_owned(),
            r#where: IndicatorWhere::LowerLeaves,
            image_kinds: vec![ImageKind::LeafCloseup],
        }],
    };
    let vlm = job(
        JobSpec::Vlm(VlmPayload {
            findings_images: vec![image(JOB, IMG, ImageKind::WholePlant)],
            indicator_images: vec![image(JOB, IMG, ImageKind::WholePlant)],
            working_species: Some(WorkingSpecies {
                id: Some("tomato".parse().unwrap()),
                name: "Tomato".to_owned(),
            }),
            species_options: vec![],
            organism_options: vec![],
            candidates: vec![candidate.clone()],
        }),
        Some(1),
    );
    round_trip(&vlm, "vlm");
    let json = serde_json::to_value(&vlm).unwrap();
    assert_eq!(
        json["payload"]["candidates"][0]["indicators"][0]["where"],
        "lower_leaves"
    );

    let verify = job(
        JobSpec::Verify(VerifyPayload {
            reason: VerifyReason::NewPhoto,
            question_id: Some("01JAX8DDDDDDDDDDDDDDDDDDDD".parse().unwrap()),
            images: vec![image(JOB, IMG, ImageKind::LeafCloseup)],
            candidates: vec![candidate],
        }),
        Some(2),
    );
    round_trip(&verify, "verify");

    let guided = job(
        JobSpec::Guided(GuidedPayload {
            image: image(JOB, IMG, ImageKind::JarTest),
        }),
        None,
    );
    round_trip(&guided, "guided");
    assert!(serde_json::to_value(&guided).unwrap().get("run").is_none());
}
```

---

## 6. VLM structured output

[Contracts 6](../architecture/contracts.md#6-vlm-structured-output). The raw shapes are what the model writes under guided decoding; the worker builds each call's JSON Schema from the payload, so they derive serde only. The converted shapes are what the worker returns and the API stores.

### Shared vocabularies

```rust
/// The VLM's certainty as a word ([contracts 6]): a quantized 7–8B model's own probabilities are
/// poorly calibrated, so the worker maps the word to a fixed confidence (open question 2 of
/// contracts 12). Measurements use the same values (contracts 7, measurement confidence).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// 0.4.
    Low,
    /// 0.65.
    Medium,
    /// 0.85.
    High,
}
```

```rust
/// `DamagePattern` ([contracts 6]).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DamagePattern {
    /// Small round holes (flea beetles).
    ShotHoles,
    /// Ragged holes (slugs).
    RaggedHoles,
    /// Chewed leaf margins.
    ChewedMargins,
    /// Tunnels inside leaves (leaf miners).
    LeafMines,
    /// Honeydew (aphids).
    StickyResidue,
    /// Black mould on honeydew.
    SootyMould,
    /// Webbing (spider mites).
    Webbing,
    /// Slime trails (slugs, snails).
    SlimeTrails,
    /// Insect droppings.
    Frass,
    /// Only the veins left.
    SkeletonisedLeaves,
    /// Cleanly cut stems (rabbits).
    CutStems,
    /// Torn stems (deer).
    TornStems,
    /// Digging.
    Digging,
    /// Vole or mole tunnels and mounds.
    TunnelsOrMounds,
    /// Tracks.
    Tracks,
    /// Animal droppings.
    Droppings,
    /// Birds pecking fruit.
    PeckedFruit,
    /// Anything else, with `other_description`.
    Other,
}

/// `count` of an organism: `"one" | "few" | "many"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrganismCount {
    /// One.
    One,
    /// A few.
    Few,
    /// Many.
    Many,
}

/// `SoilAspect` ([contracts 6], after decoding). `texture` comes from guided checks.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SoilAspect {
    /// Moisture.
    Moisture,
    /// Surface.
    Surface,
    /// Structure.
    Structure,
    /// Colour.
    Colour,
    /// Cover.
    Cover,
    /// Deposits.
    Deposits,
    /// Erosion.
    Erosion,
    /// Weeds.
    Weeds,
    /// From guided checks.
    Texture,
}

/// `img1` … `img4`: an image as the model sees it in one call ([contracts 6], per-job schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageAlias(u8);

/// The species and organism enums of a call end in this extra value ([contracts 6]).
pub const OTHER: &str = "other";
```

### Raw output

```rust
/// `soil.moisture` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilMoisture {
    /// Dry.
    Dry,
    /// Moist.
    Moist,
    /// Wet.
    Wet,
    /// Waterlogged.
    Waterlogged,
}

/// `soil.surface` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilSurface {
    /// Normal.
    Normal,
    /// Crusted.
    Crusted,
    /// Cracked.
    Cracked,
}

/// `soil.structure` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilStructure {
    /// Crumbly.
    Crumbly,
    /// Cloddy.
    Cloddy,
    /// Compacted.
    Compacted,
    /// Unclear.
    Unclear,
}

/// `soil.colour` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilColour {
    /// Dark.
    Dark,
    /// Medium.
    Medium,
    /// Pale.
    Pale,
    /// Grey and mottled (waterlogging).
    GreyMottled,
}

/// `soil.cover` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilCover {
    /// Bare.
    Bare,
    /// Mulched.
    Mulched,
    /// Crop residue.
    Residue,
    /// Ground cover plants.
    GroundCover,
}

/// `soil.deposits` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilDeposits {
    /// None.
    None,
    /// Salt crust.
    SaltCrust,
    /// Moss.
    Moss,
    /// Algae.
    Algae,
    /// Mould.
    Mould,
}

/// `soil.erosion` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilErosion {
    /// None.
    None,
    /// Rills.
    Rills,
    /// Exposed roots.
    ExposedRoots,
}

/// `soil.weeds` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoilWeeds {
    /// None.
    None,
    /// A few.
    Few,
    /// Many.
    Many,
}

/// One soil reading: `{ value, image, certainty }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoilReadingValue<V> {
    /// The value.
    pub value: V,
    /// The shot that showed it.
    pub image: ImageAlias,
    /// How sure the model is.
    pub certainty: Certainty,
}

/// `SoilReading<V>`: `null` when not visible. Always present in the object.
pub type SoilReading<V> = Option<SoilReadingValue<V>>;

/// `VlmFindingsRaw.species_guess`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeciesGuessRaw {
    /// A `species_options` slug, or `"other"`.
    pub species: String,
    /// With `"other"`; `null` otherwise.
    pub other_name: Option<String>,
    /// How sure.
    pub certainty: Certainty,
}

/// `VlmFindingsRaw.soil`: every aspect present, `null` when not visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoilRaw {
    /// Moisture.
    pub moisture: SoilReading<SoilMoisture>,
    /// Surface.
    pub surface: SoilReading<SoilSurface>,
    /// Structure.
    pub structure: SoilReading<SoilStructure>,
    /// Colour.
    pub colour: SoilReading<SoilColour>,
    /// Cover.
    pub cover: SoilReading<SoilCover>,
    /// Deposits.
    pub deposits: SoilReading<SoilDeposits>,
    /// Erosion.
    pub erosion: SoilReading<SoilErosion>,
    /// Weeds.
    pub weeds: SoilReading<SoilWeeds>,
}

/// `VlmFindingsRaw.organisms[]` (at most 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganismRaw {
    /// An `organism_options` slug, or `"other"`.
    pub label: String,
    /// With `"other"`.
    pub other_name: Option<String>,
    /// How many.
    pub count: OrganismCount,
    /// The shot.
    pub image: ImageAlias,
    /// Integers, in the adapter's `box_format`.
    pub bbox_2d: [i64; 4],
    /// How sure.
    pub certainty: Certainty,
}

/// `VlmFindingsRaw.damage[]` (at most 10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageRaw {
    /// The pattern.
    pub pattern: DamagePattern,
    /// With `"other"`, at most 100 characters.
    pub other_description: Option<String>,
    /// The shot.
    pub image: ImageAlias,
    /// Integers in the adapter's `box_format`, or `null`.
    pub bbox_2d: Option<[i64; 4]>,
    /// How sure.
    pub certainty: Certainty,
}

/// `VlmFindingsRaw` ([contracts 6], findings call).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VlmFindingsRaw {
    /// The model's own species guess.
    pub species_guess: SpeciesGuessRaw,
    /// Soil, every aspect.
    pub soil: SoilRaw,
    /// Organisms.
    pub organisms: Vec<OrganismRaw>,
    /// Damage.
    pub damage: Vec<DamageRaw>,
    /// At most 200 characters.
    pub notes: String,
}

/// One value of `VlmIndicatorsRaw`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndicatorAnswerRaw {
    /// At most 60 characters; written first, so the answer follows from it.
    pub reason: String,
    /// The answer; `"unsure"` only when none of the indicator's kinds is in the call.
    pub answer: Answer,
    /// The shot that decided it.
    pub image: Option<ImageAlias>,
}

/// `VlmIndicatorsRaw`: one required key per indicator, `q<n>_<slug cut to 24 characters>`.
pub type VlmIndicatorsRaw = BTreeMap<String, IndicatorAnswerRaw>;

/// The key of the `n`-th indicator (1-based) in an indicators call: `q3_concentric_rings`.
pub fn indicator_key(n: usize, indicator: &IndicatorId) -> String {
    let slug: String = indicator.slug().chars().take(24).collect();
    format!("q{n}_{slug}")
}

/// `squeeze_test.result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SqueezeResult {
    /// Sandy.
    Crumbles,
    /// Loamy.
    HoldsBall,
    /// Clayey.
    StickyRibbon,
    /// Can't tell.
    Unclear,
}

/// `jar_test.texture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JarTexture {
    /// Sandy.
    Sandy,
    /// Loamy.
    Loamy,
    /// Silty.
    Silty,
    /// Clayey.
    Clayey,
    /// Can't tell.
    Unclear,
}

/// `VlmGuidedRaw` ([contracts 6], guided-check call): the schema has only the branch for the
/// image's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VlmGuidedRaw {
    /// A squeeze test.
    SqueezeTest {
        /// What the soil did.
        result: SqueezeResult,
        /// How sure.
        certainty: Certainty,
    },
    /// A jar test.
    JarTest {
        /// Whether the layers had settled.
        settled: bool,
        /// The texture the layers show.
        texture: JarTexture,
        /// How sure.
        certainty: Certainty,
    },
}

/// How a model adapter writes boxes ([contracts 6], box formats per model adapter).
/// Every format is corners `[x1, y1, x2, y2]` in `bbox_2d`. The format belongs to the adapter
/// instance (model, server and preparation together), not to the model family alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxFormat {
    /// Pixels of the prepared image: divide by its size. The Qwen2.5-VL baseline on vLLM, and
    /// the fallback worker's hosted LLM (sent images it won't resize) until one live call
    /// confirms it (contracts open question 26).
    AbsPxPrepared,
    /// 0–1000: divide by 1000. The chosen Qwen3-VL-class models (Qwen3-VL, Qwen3.5) on vLLM or
    /// llama.cpp; the `vlm_box_format` default.
    Norm1000,
    /// 0–1: no conversion. Models that return 0–1 coordinates.
    #[serde(rename = "norm_1")]
    Norm1,
}
```

- `SoilReading<V>` is `Option<SoilReadingValue<V>>` and always present: `null` when not visible.
- `bbox_2d` is read as corners `[x1, y1, x2, y2]`, the order Qwen2.5-VL grounding emits. Contracts 6 gives the units per `box_format` but not the order; this is listed as an ambiguity in [domain 12](domain.md#12-open-questions).

### After decoding

```rust
/// `VlmFindings.species_guess`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesGuess {
    /// `null` for `"other"`.
    #[schema(required = true)]
    pub species_id: Option<SpeciesId>,
    /// With `"other"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub other_name: Option<String>,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings.soil[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilFinding {
    /// The aspect.
    pub aspect: SoilAspect,
    /// Its value, such as `"dry"`.
    pub value: String,
    /// The shot.
    pub image_id: ImageId,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings.organisms[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OrganismFinding {
    /// `null` for `"other"`.
    #[schema(required = true)]
    pub organism_id: Option<OrganismId>,
    /// With `"other"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub other_name: Option<String>,
    /// How many.
    pub count: OrganismCount,
    /// The box; VLM regions never have a mask.
    #[schema(value_type = Region)]
    pub region: Region<Stored>,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings.damage[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DamageFinding {
    /// The pattern.
    pub pattern: DamagePattern,
    /// With `"other"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub other_description: Option<String>,
    /// The box, when the model gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Region>)]
    pub region: Option<Region<Stored>>,
    /// The shot.
    pub image_id: ImageId,
    /// From the certainty word.
    pub confidence: Confidence,
}

/// `VlmFindings` ([contracts 6]): aliases mapped back to ids, certainty words to numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct VlmFindings {
    /// The species guess.
    pub species_guess: SpeciesGuess,
    /// Soil readings that were visible.
    pub soil: Vec<SoilFinding>,
    /// Organisms.
    pub organisms: Vec<OrganismFinding>,
    /// Damage.
    pub damage: Vec<DamageFinding>,
    /// Notes.
    pub notes: String,
}

/// `VlmIndicatorAnswers.answers[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IndicatorAnswer {
    /// The indicator.
    pub indicator_id: IndicatorId,
    /// The answer.
    pub answer: Answer,
    /// Why.
    pub reason: String,
    /// The shot that decided it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub image_id: Option<ImageId>,
}

/// `VlmIndicatorAnswers` ([contracts 6]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct VlmIndicatorAnswers {
    /// One answer per indicator of the payload.
    pub answers: Vec<IndicatorAnswer>,
}

/// `GuidedTexture` ([contracts 6]), tagged by `kind`. A jar test with `settled: false` makes no
/// texture observation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GuidedTexture {
    /// A squeeze test.
    SqueezeTest {
        /// What the soil did.
        result: SqueezeResult,
        /// From the certainty word.
        confidence: Confidence,
    },
    /// A jar test.
    JarTest {
        /// Whether the layers had settled.
        settled: bool,
        /// The texture.
        texture: JarTexture,
        /// From the certainty word.
        confidence: Confidence,
    },
}
```

- VLM regions never have masks, so `OrganismFinding.region` and `DamageFinding.region` are `Region<Stored>`: they become stored observations as they are.
- `From<VlmGuidedRaw> for GuidedTexture` maps the certainty word to its fixed confidence.

---

## 7. Pipeline records

[Contracts 7](../architecture/contracts.md#7-pipeline-records).

### Observation

```rust
/// `Observation.stage`: what kind of finding it is (not the pipeline stage that produced it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStage {
    /// Plant present or not.
    PlantCheck,
    /// A segmentation region.
    Segmentation,
    /// A species candidate or guess.
    Species,
    /// A disease classifier result.
    Disease,
    /// A soil aspect, from the VLM or a guided check.
    Soil,
    /// An organism.
    Organism,
    /// A damage pattern.
    Damage,
    /// A `LiDAR` measurement.
    Measurement,
}

/// `Observation.ref`: the knowledge-base record, when there is one. Adjacently tagged, so it
/// serialises as `{ "table": "disease", "id": "tomato_early_blight" }`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "table", content = "id", rename_all = "snake_case")]
pub enum KnowledgeRef {
    /// A species.
    Species(SpeciesId),
    /// A problem (the `disease` table).
    Disease(ProblemId),
    /// An organism.
    Organism(OrganismId),
}

/// `Observation.role` and `Diagnosis.organisms[].role`: organisms only; `unknown` for "other".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrganismRole {
    /// Harms the plant.
    Pest,
    /// Eats pests or pollinates; never targeted.
    Beneficial,
    /// Neither.
    Neutral,
    /// An invasive species, with reporting guidance.
    Invasive,
    /// Reported as "other": cannot be targeted.
    Unknown,
}

/// `Observation.detail`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct ObservationDetail {
    /// Soil observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub aspect: Option<SoilAspect>,
    /// The soil aspect's value, such as `"dry"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub value: Option<String>,
    /// Damage observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub pattern: Option<DamagePattern>,
    /// Organism observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub count: Option<OrganismCount>,
    /// Segmentation observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub region_class: Option<RegionClass>,
    /// Guided-check observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub guided_kind: Option<GuidedKind>,
    /// Measurement observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub measurement: Option<Measurement>,
    /// Organisms on a photo with depth; shown, not scored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub length_mm: Option<f64>,
}

/// `Observation.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSource {
    /// The segmentation models.
    Segmentation,
    /// `BioCLIP`.
    Bioclip,
    /// The disease classifier.
    Classifier,
    /// The VLM.
    Vlm,
    /// The fallback worker's LLM.
    LlmFallback,
    /// The user.
    User,
    /// The measurement stage.
    Measurement,
}

/// `Observation` ([contracts 7]). `confidence` and `confidence_band` travel together as the
/// flattened [`Rated`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Observation)]
pub struct Observation<U: UrlState = Signed> {
    /// The observation.
    pub id: ObservationId,
    /// Its scan.
    pub scan_id: ScanId,
    /// The run that produced it.
    pub run: Run,
    /// What kind of finding it is.
    pub stage: ObservationStage,
    /// The pipeline stage that produced it; runs replace observations by this, per image.
    pub pipeline_stage: Stage,
    /// Display text: "Early blight", "Ladybug larva", "Soil: dry", "Leaf".
    pub label: String,
    /// The knowledge-base record, when there is one.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub knowledge_ref: Option<KnowledgeRef>,
    /// Organisms only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub role: Option<OrganismRole>,
    /// Kind-specific detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<ObservationDetail>,
    /// `confidence` and its computed `confidence_band`.
    #[serde(flatten)]
    #[schema(inline)]
    pub confidence: Rated,
    /// Where on which image; always present for measurements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false, value_type = Option<Region>)]
    pub region: Option<Region<U>>,
    /// Who produced it.
    pub source: ObservationSource,
    /// For measurements: the algorithm version and `depth_meta.device_model`.
    pub model_version: String,
    /// When it was stored.
    pub created_at: Timestamp,
}
```

- `ref` is a Rust keyword, so the field is `knowledge_ref` with `#[serde(rename = "ref")]`.
- `KnowledgeRef` is adjacently tagged: `{ "table": "disease", "id": "tomato_early_blight" }`, with the slug typed by table.
- `confidence` is the flattened `Rated`.

### Measurement

```rust
/// `MeasureQuantity` ([contracts 7]). The unit is in the name.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MeasureQuantity {
    /// From `whole_plant`: one value; needs gravity and a ground plane.
    PlantHeightCm,
    /// From `whole_plant`: one value; needs gravity.
    CanopyWidthCm,
    /// Per leaf with a mask.
    LeafLengthCm,
    /// Per leaf with a mask.
    LeafAreaCm2,
    /// Per spot at least 4 pixels across.
    LesionDiameterMm,
    /// Spots per leaf with a mask.
    LesionCount,
    /// Share of each leaf covered by spots and their yellowing.
    AffectedAreaPct,
    /// Per hole at least 4 pixels across.
    HoleDiameterMm,
    /// Holes per leaf with a mask, of any size.
    HoleCount,
}

/// A quantity's unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeasureUnit {
    /// Centimetres.
    Cm,
    /// Square centimetres.
    Cm2,
    /// Millimetres.
    Mm,
    /// A count.
    Count,
    /// A percentage.
    Pct,
}

/// `Measurement` ([contracts 7]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Measurement {
    /// What was measured.
    pub quantity: MeasureQuantity,
    /// The median over the items, or the single value.
    pub value: f64,
    /// Over the items; equal to `value` when `n` = 1. Always sent.
    pub min: f64,
    /// Over the items; equal to `value` when `n` = 1. Always sent.
    pub max: f64,
    /// The number of items; 1 for height and canopy width.
    pub n: u32,
    /// Median camera distance over what was measured, metres.
    pub distance_m: f64,
}
```

`measurable_from` encodes the table of quantities in contracts 7, which the knowledge loader also checks against an indicator's `image_kinds`.

### WeatherSummary

```rust
/// `WeatherSummary.provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeatherProviderName {
    /// Open-Meteo.
    OpenMeteo,
}

/// `location_precision` of `WeatherSummary` and `Garden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocationPrecision {
    /// GPS with consent.
    Exact,
    /// GPS rounded with consent.
    #[serde(rename = "about_1_km")]
    About1Km,
    /// Rounded after consent was withdrawn.
    #[serde(rename = "about_10_km")]
    About10Km,
    /// Geocoded from a postal code.
    PostalCode,
    /// Geocoded from a city.
    City,
}

/// `WeatherSummary.current`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CurrentWeather {
    /// °C.
    pub temp_c: f64,
    /// Relative humidity, %.
    pub rh_pct: f64,
    /// km/h.
    pub wind_kmh: f64,
    /// mm over the last 24 hours.
    pub precip_mm_last_24h: f64,
}

/// `WeatherSummary.past_7_days`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PastWeather {
    /// mm.
    pub precip_mm: f64,
    /// %.
    pub mean_rh_pct: f64,
    /// Hours with RH over 90%: the leaf-wetness stand-in.
    pub hours_rh_over_90: u32,
    /// The longest run of consecutive humid days in the past 7, each a local day with at least
    /// `WeatherThresholds::humid_day_hours` hours over 90% (0–7), computed from the hourly
    /// history when the cell is fetched (`WeatherThresholds::longest_humid_run`), so a humid
    /// stretch in the past week is detectable ([contracts 9], crowding; architecture 6.8).
    /// `#[serde(default)]` reads weather cached or snapshotted before the field existed as 0;
    /// the wire always carries it.
    #[serde(default)]
    #[schema(required = true)]
    pub longest_humid_run_days: u8,
}

/// `WeatherSummary.forecast[]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ForecastDay {
    /// In the garden's time zone.
    pub date: LocalDate,
    /// °C.
    pub t_min_c: f64,
    /// °C.
    pub t_max_c: f64,
    /// mm.
    pub precip_mm: f64,
    /// %.
    pub mean_rh_pct: f64,
    /// Hours with RH over 90% (`WeatherThresholds::humid_hours`).
    pub hours_rh_over_90: u32,
    /// Frost expected: the 2 m minimum at or below `WeatherThresholds::frost_min_c` (0.5 °C by
    /// default), set when the cell is fetched (`domain::rules::weather`).
    pub frost_risk: bool,
}

/// `WeatherSummary.soil`: Open-Meteo's modelled soil conditions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilNow {
    /// m³/m³, 0–9 cm.
    pub moisture_0_9cm_m3m3: f64,
    /// m³/m³, 9–27 cm.
    pub moisture_9_27cm_m3m3: f64,
    /// °C at 6 cm.
    pub temp_6cm_c: f64,
    /// FAO reference evapotranspiration, mm over the last 24 hours.
    pub et0_mm_last_24h: f64,
}

/// `WeatherSummary` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WeatherSummary {
    /// Always `open_meteo`.
    pub provider: WeatherProviderName,
    /// When the cell was fetched.
    pub fetched_at: Timestamp,
    /// The garden's location precision.
    pub location_precision: LocationPrecision,
    /// Now.
    pub current: CurrentWeather,
    /// The past 7 days.
    pub past_7_days: PastWeather,
    /// 7–14 days ahead.
    pub forecast: Vec<ForecastDay>,
    /// Soil now.
    pub soil: SoilNow,
}

/// What the weather cache holds for one cell and hour: a `WeatherSummary` without the garden's
/// `location_precision`, which differs between gardens that share a cell
/// ([contracts 7], fetching and caching).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellWeather {
    /// Always `open_meteo`.
    pub provider: WeatherProviderName,
    /// When the cell was fetched.
    pub fetched_at: Timestamp,
    /// Now.
    pub current: CurrentWeather,
    /// The past 7 days.
    pub past_7_days: PastWeather,
    /// 7–14 days ahead, dated in the cell's time zone.
    pub forecast: Vec<ForecastDay>,
    /// Soil now.
    pub soil: SoilNow,
}

/// WGS 84 coordinates. Raw coordinates are never returned to the app ([contracts 9]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    /// Latitude, degrees, −90 to 90.
    pub lat: f64,
    /// Longitude, degrees, −180 to 180.
    pub lon: f64,
}

/// A weather cache cell: coordinates rounded to 0.01° (about 1 km), stored on the garden as
/// `"lat,lon"` such as `"43.65,-79.38"` ([contracts 7], [database 3] `garden.weather_cell`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WeatherCell {
    lat_centi: i32,
    lon_centi: i32,
}
```

- `about_1_km` and `about_10_km` are renamed by hand: `rename_all = "snake_case"` would write `about1_km`.
- `CellWeather` is what the cache holds per cell and hour: a `WeatherSummary` without `location_precision`, which belongs to the garden, not the cell. `for_garden` adds it back.
- `WeatherCell` formats as `"43.65,-79.38"`, the stored `garden.weather_cell`.

### Diagnosis

```rust
/// An indicator's weight, 1 to 3 ([contracts 7], [contracts 10]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(into = "u8")]
#[schema(value_type = u8, minimum = 1, maximum = 3)]
pub struct Weight(u8);

/// `IndicatorEvidence.source`, ranked `user` > `measurement` > `vlm`: each indicator keeps one
/// answer ([contracts 7], measurable indicators).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// The VLM; lowest rank.
    Vlm,
    /// A `LiDAR` measurement.
    Measurement,
    /// The user; highest rank.
    User,
}

/// `IndicatorEvidence.measured`: with source `measurement`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MeasuredValue {
    /// The quantity.
    pub quantity: MeasureQuantity,
    /// The value the indicator's `stat` picked.
    pub value: f64,
    /// The image it came from.
    pub image_id: ImageId,
}

/// `IndicatorEvidence` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IndicatorEvidence {
    /// The indicator.
    pub indicator_id: IndicatorId,
    /// "Concentric rings in the spots".
    pub text: String,
    /// 1–3.
    pub weight: Weight,
    /// The answer.
    pub answer: Answer,
    /// Why.
    pub reason: String,
    /// Who answered.
    pub source: EvidenceSource,
    /// With source `measurement`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub measured: Option<MeasuredValue>,
}

/// `CandidateScore.weather_fit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeatherFit {
    /// The weather matches the profile's favourable conditions.
    Favourable,
    /// Neither.
    Neutral,
    /// The weather works against it.
    Unfavourable,
}

/// `CandidateScore` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CandidateScore {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
    /// Sum of matched weights minus contradicted weights.
    pub score: i32,
    /// Answered `yes`.
    pub matched: Vec<IndicatorEvidence>,
    /// Answered `no`.
    pub contradicted: Vec<IndicatorEvidence>,
    /// Answered `unsure`.
    pub unsure: Vec<IndicatorEvidence>,
    /// The weather against the profile.
    pub weather_fit: WeatherFit,
}

/// `Citation` ([contracts 7]): a curated source. Its `url` is the source's public page, not an
/// image, so it is a plain string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Citation {
    /// The source.
    pub source_id: SourceId,
    /// Its title.
    pub title: String,
    /// Its publisher.
    pub publisher: String,
    /// Its page.
    pub url: String,
}

/// `Diagnosis.status`, decided in the order of contracts 7's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisStatus {
    /// A clear winner.
    Confirmed,
    /// Ambiguous; the plan is provisional.
    Uncertain,
    /// Only on positive evidence; a short care plan.
    Healthy,
}

/// `Diagnosis.organisms[].preys_on[]`: computed from the `preys_on` graph when read; never
/// stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Prey {
    /// The pest.
    pub organism_id: OrganismId,
    /// Its name.
    pub name: String,
}

/// `Diagnosis.organisms[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiagnosisOrganism {
    /// The finding.
    pub observation_id: ObservationId,
    /// `null` for "other".
    #[schema(required = true)]
    pub organism_id: Option<OrganismId>,
    /// Its name.
    pub name: String,
    /// Its role.
    pub role: OrganismRole,
    /// Invasive only: the app shows "How to report".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub reporting_url: Option<String>,
    /// Beneficial only: the pests it eats. Filled in when read, never stored: the store drops it
    /// on write and `api` fills it from `KnowledgeStore::prey_of`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub preys_on: Option<Vec<Prey>>,
}

/// `Diagnosis.risks[].level`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// `Diagnosis.risks[]`. `risk` is an open vocabulary in contracts 7; the known values are the
/// constants below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Risk {
    /// Such as `"fungal_spread"`.
    pub risk: String,
    /// Its level.
    pub level: RiskLevel,
    /// "4 days over 90% humidity forecast", "Crowded: nearest plant 30 cm away".
    pub drivers: Vec<String>,
}

/// `Diagnosis.evidence_basis` (architecture 6.4: user input, then photo, then modelled data).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct EvidenceBasis {
    /// From shared memories only ([contracts 8]).
    pub user_input: Vec<String>,
    /// Measurements included: "Median spot 2.1 mm".
    pub photo: Vec<String>,
    /// Weather and soil models.
    pub modelled: Vec<String>,
}

/// `Diagnosis` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Diagnosis)]
pub struct Diagnosis<U: UrlState = Signed> {
    /// The diagnosis.
    pub id: DiagnosisId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Its run.
    pub run: Run,
    /// The earlier diagnosis of the same scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub supersedes: Option<DiagnosisId>,
    /// The status.
    pub status: DiagnosisStatus,
    /// `uncertain`, and nothing points to any problem.
    pub low_evidence: bool,
    /// Absent when healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub primary: Option<CandidateScore>,
    /// Ranked, at most 9.
    pub candidates: Vec<CandidateScore>,
    /// Organisms found.
    pub organisms: Vec<DiagnosisOrganism>,
    /// The soil observations used, guided checks included.
    #[schema(value_type = Vec<Observation>)]
    pub soil: Vec<Observation<U>>,
    /// Fusion's risks.
    pub risks: Vec<Risk>,
    /// What the diagnosis rests on.
    pub evidence_basis: EvidenceBasis,
    /// Every source cited.
    pub citations: Vec<Citation>,
    /// Model name → version.
    pub model_versions: BTreeMap<String, String>,
    /// When it was stored.
    pub created_at: Timestamp,
}
```

- `Weight` only decodes 1, 2 or 3.
- `EvidenceSource` derives `Ord` in rank order (`vlm < measurement < user`): each indicator keeps the highest-ranked answer.
- `DiagnosisOrganism.preys_on` is computed from the `preys_on` graph when read and never stored.
- `Risk.risk` is an open vocabulary in the contract (a `string`); the known values are constants.

### ActionPlan

```rust
/// The fixed note the server adds to every `chemical` step; the plan writer never writes it
/// ([contracts 7], plan validation).
pub const SAFETY_NOTE: &str =
    "Use only a product registered for this use where you live, and follow its label exactly.";

/// `TriggerKind` ([contracts 7]): one vocabulary for forecast alerts, plan triggers and
/// `Task.trigger`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Frost forecast.
    Frost,
    /// Heat forecast.
    Heat,
    /// A humid stretch: fungal risk.
    HumidStretch,
    /// Heavy rain forecast.
    HeavyRain,
    /// After the next rain.
    AfterRain,
    /// Dry soil: a watering reminder from soil moisture and ET₀.
    DrySoil,
}

/// `PlanWhen` ([contracts 7]), tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanWhen {
    /// A task due today.
    Now,
    /// A task due on that date.
    ByDate {
        /// The due date.
        due: LocalDate,
    },
    /// A repeating task: "check for new spots every 3 days".
    Recurring {
        /// At least 1.
        every_days: u32,
        /// The last date a task is created for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        until: Option<LocalDate>,
    },
    /// No task yet: a plan trigger rule creates it when it fires.
    OnTrigger {
        /// The rule.
        trigger: TriggerKind,
        /// The last date the rule fires.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        until: Option<LocalDate>,
    },
}

/// `PlanStep.category`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StepCategory {
    /// Cultural.
    Cultural,
    /// Biological.
    Biological,
    /// Mechanical.
    Mechanical,
    /// Chemical: always carries [`SAFETY_NOTE`].
    Chemical,
    /// Monitoring; the only category a neighbour step may have.
    Monitoring,
    /// Humane wildlife deterrent.
    WildlifeDeterrent,
}

/// `PlanStep.targets[]`: what the step acts on, `{ table, id }`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(tag = "table", content = "id", rename_all = "snake_case")]
pub enum PlanTarget {
    /// A problem.
    Disease(ProblemId),
    /// An organism; never a beneficial one.
    Organism(OrganismId),
}

/// `PlanStep` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PlanStep {
    /// Unique within the plan only.
    pub id: StepId,
    /// Position in the plan.
    pub order: u32,
    /// "Remove the lowest infected leaves".
    pub title: String,
    /// Detail.
    pub detail: String,
    /// The knowledge-base treatment the step applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub treatment_id: Option<TreatmentId>,
    /// From the treatment's `kind` when there is one; `chemical` when a product is named.
    pub category: StepCategory,
    /// When.
    pub when: PlanWhen,
    /// What it acts on; may be empty.
    pub targets: Vec<PlanTarget>,
    /// A neighbour this monitoring step is about; its task goes to that plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// Source ids; at least one.
    pub citations: Vec<SourceId>,
    /// Added by the server to every chemical step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub safety_note: Option<String>,
}

/// `ActionPlan.constraints[]`: from confirmed chat revisions; carried into every later version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlanConstraint {
    /// "No copper products".
    pub text: String,
    /// The confirmed card; an id, not a link (the card may be deleted with its conversation).
    pub from_pending_action_id: PendingActionId,
    /// When it was confirmed.
    pub created_at: Timestamp,
}

/// `ActionPlan` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ActionPlan {
    /// The plan version.
    pub id: PlanId,
    /// Its scan.
    pub scan_id: ScanId,
    /// Its plant.
    pub plant_id: PlantId,
    /// 1, 2, …; unique per scan.
    pub version: u32,
    /// The version it replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub supersedes: Option<PlanId>,
    /// True exactly when the diagnosis it was written from is `uncertain`.
    pub provisional: bool,
    /// The diagnosis this version was written from.
    pub diagnosis_id: DiagnosisId,
    /// The question still open when this version was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub open_question: Option<Question>,
    /// From confirmed chat revisions.
    pub constraints: Vec<PlanConstraint>,
    /// Summary.
    pub summary: String,
    /// Steps.
    pub steps: Vec<PlanStep>,
    /// "New spots on upper leaves within 5 days".
    pub watch_for: Vec<String>,
    /// Every source the steps cite, resolved.
    pub citations: Vec<Citation>,
    /// The tasks the steps created.
    pub task_ids: Vec<TaskId>,
    /// The LLM that wrote it.
    pub llm_model: String,
    /// The prompt version.
    pub prompt_version: String,
    /// When it was stored.
    pub created_at: Timestamp,
}

/// Why a plan version was written: the stored `action_plan.reason` ([database 3]), which adds
/// `initial` to the re-planning reasons of contracts 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanReason {
    /// A run's first plan.
    Initial,
    /// A late answer or photo.
    LateAnswer,
    /// A species-change run.
    SpeciesChanged,
    /// A guided check changed the texture.
    GuidedCheck,
    /// A confirmed `revise_plan` card.
    ChatRevision,
}
```

`PlanReason` is the stored `action_plan.reason`, which adds `initial` to the four re-planning reasons ([database 3](../database/README.md#3-tables)).

The plan writer's output is the fixed shape whose JSON Schema the agent sends to the LLM API. It isn't a type in contracts 7; it is `ActionPlan`'s model-written fields, without `safety_note`, which the server adds:

```rust
/// A neighbour given to the plan writer ([contracts 7], neighbours for the plan writer): up to
/// 5, nearest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanNeighbour {
    /// The neighbour.
    pub plant_id: PlantId,
    /// Its name.
    pub name: String,
    /// Its species' common name.
    pub species_name: String,
    /// Centre to centre, metres.
    pub distance_m: f64,
    /// The spreading problems its species is `susceptible_to`.
    pub problems: Vec<ProblemId>,
}

/// One step as the plan writer writes it: a `PlanStep` without `safety_note`, which the server
/// adds. The validator then fixes `category` from the treatment and the product list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct PlanStepDraft {
    /// Unique within the plan.
    pub id: StepId,
    /// Position.
    pub order: u32,
    /// Title; dose-checked.
    pub title: String,
    /// Detail; dose-checked.
    pub detail: String,
    /// The treatment applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub treatment_id: Option<TreatmentId>,
    /// Proposed category.
    pub category: StepCategory,
    /// When.
    pub when: PlanWhen,
    /// Targets.
    pub targets: Vec<PlanTarget>,
    /// A neighbour, for monitoring steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plant_id: Option<PlantId>,
    /// At least one source.
    pub citations: Vec<SourceId>,
}

/// The plan writer's structured output: the fixed shape whose JSON Schema the agent sends to
/// the LLM API ([contracts 1], source of truth). Not in contracts 7 as a type: it is
/// `ActionPlan`'s model-written fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct PlanWriterOutput {
    /// Summary; dose-checked.
    pub summary: String,
    /// Steps.
    pub steps: Vec<PlanStepDraft>,
    /// What to watch for; dose-checked.
    pub watch_for: Vec<String>,
}
```

The schema says `additionalProperties: false` (`#[schemars(extend(…))]`) so the model is told no extra keys are allowed, while serde still accepts and drops one, so a near-miss output reaches validation instead of failing to parse.

### Task

```rust
/// `Task.repeat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Repeat {
    /// At least 1.
    pub every_days: u32,
    /// The last date an occurrence is created for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub until: Option<LocalDate>,
}

/// `Task.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    /// A plan step; dismissed, never deleted.
    Plan,
    /// A forecast alert; dismissed, never deleted.
    ForecastAlert,
    /// A confirmed `create_task` card.
    Chat,
    /// `POST /tasks`.
    User,
}

/// `Task.trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskTrigger {
    /// The rule that fired.
    pub kind: TriggerKind,
    /// The local date it fired for.
    pub date: LocalDate,
}

/// `Task.dismissed_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DismissedReason {
    /// The user dismissed it.
    User,
    /// Its plan was superseded.
    PlanSuperseded,
}

/// `Task` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Task {
    /// The task.
    pub id: TaskId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its plant, always a live one when created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// The plan version that made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_id: Option<PlanId>,
    /// The step that made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_step_id: Option<StepId>,
    /// A guided-check task's scan: the jar test photographed later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub scan_id: Option<ScanId>,
    /// Title.
    pub title: String,
    /// Detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    /// Due date, local to the garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub due: Option<LocalDate>,
    /// Completing it creates the next occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub repeat: Option<Repeat>,
    /// Who made it.
    pub source: TaskSource,
    /// The rule that created it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub trigger: Option<TaskTrigger>,
    /// When it was completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub done_at: Option<Timestamp>,
    /// When it was dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub dismissed_at: Option<Timestamp>,
    /// Why it was dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub dismissed_reason: Option<DismissedReason>,
    /// When it was created.
    pub created_at: Timestamp,
}

/// `?state=open|done|dismissed`, stored as `task.state` ([database 3]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Neither done nor dismissed.
    Open,
    /// Completed.
    Done,
    /// Dismissed.
    Dismissed,
}

/// `task.alert_key`, server-side only, under one unique index ([contracts 7]):
/// `<garden_id>:<trigger>:<date>` for forecast alerts and
/// `plan:<plan_id>:<plan_step_id>:<date>` for plan triggers. A second insert with the same key
/// fails, which makes the scheduler's dedupe race-free ([database 4], alert dedupe).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AlertKey {
    /// A forecast alert.
    Forecast {
        /// The garden.
        garden: GardenId,
        /// The rule.
        trigger: TriggerKind,
        /// The local date.
        date: LocalDate,
    },
    /// A plan trigger rule.
    PlanTrigger {
        /// The plan version.
        plan: PlanId,
        /// The step.
        step: StepId,
        /// The local date.
        date: LocalDate,
    },
}
```

`AlertKey` never leaves the server; its `Display` is the stored `task.alert_key`.

---

## 8. Chat and memory

[Contracts 8](../architecture/contracts.md#8-chat-and-memory).

### Conversations and messages

```rust
/// `anchor`: `{ type: "scan" | "plant" | "garden"; id: Id }`, adjacently tagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum Anchor {
    /// "Ask about this result".
    Scan(ScanId),
    /// The plant timeline.
    Plant(PlantId),
    /// Garden home.
    Garden(GardenId),
}

/// `anchor_type` query values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorType {
    /// A scan.
    Scan,
    /// A plant.
    Plant,
    /// The garden.
    Garden,
}

/// `POST /conversations` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateConversationRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// What the conversation is about; must be in the garden.
    pub anchor: Anchor,
}

/// `Conversation` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Conversation {
    /// The conversation.
    pub id: ConversationId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its anchor.
    pub anchor: Anchor,
    /// A title, once one is generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub title: Option<String>,
    /// When it was created.
    pub created_at: Timestamp,
    /// When the last message was sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub last_message_at: Option<Timestamp>,
}

/// `GET /conversations?garden_id=&anchor_type=&anchor_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConversationQuery {
    /// The garden.
    pub garden_id: GardenId,
    /// Filter by anchor type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_type: Option<AnchorType>,
    /// Filter by anchor id (a ULID of the anchor type's table).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub anchor_id: Option<ulid::Ulid>,
}

/// `Message.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// The user.
    User,
    /// The assistant.
    Assistant,
}

/// `Message.attachments[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Attachment)]
pub struct Attachment<U: UrlState = Signed> {
    /// The chat image.
    pub image_id: ChatImageId,
    /// Signed.
    #[serde(default, skip_serializing_if = "UrlSlot::is_absent")]
    #[schema(value_type = SignedUrl, required = true)]
    pub url: U::Url,
}

/// `Message` ([contracts 8]). Tool calls and results are stored but never sent to the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Message)]
pub struct Message<U: UrlState = Signed> {
    /// The message.
    pub id: MessageId,
    /// Who wrote it.
    pub role: MessageRole,
    /// The text, dose-checked when the assistant wrote it.
    pub text: String,
    /// The user's images.
    #[schema(value_type = Vec<Attachment>)]
    pub attachments: Vec<Attachment<U>>,
    /// Sources cited.
    pub citations: Vec<Citation>,
    /// Confirmation cards.
    pub pending_actions: Vec<PendingAction>,
    /// The stream dropped before `message_completed`.
    pub interrupted: bool,
    /// When it was stored.
    pub created_at: Timestamp,
}

/// `ConversationDetail extends Conversation` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = ConversationDetail)]
pub struct ConversationDetail<U: UrlState = Signed> {
    /// Every field of `Conversation`.
    #[serde(flatten)]
    pub conversation: Conversation,
    /// Newest first; `?cursor=` for older.
    #[schema(value_type = Page<Message>)]
    pub messages: Page<Message<U>>,
}

/// `201` of `POST /conversations/{id}/images`: `{ image_id, url }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChatImageUploaded {
    /// The new chat image.
    pub image_id: ChatImageId,
    /// Signed.
    pub url: SignedUrl,
}

/// `POST /messages/{id}/feedback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    /// Thumbs up.
    Up,
    /// Thumbs down.
    Down,
}

/// `POST /messages/{id}/feedback`: owner only, assistant messages only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageFeedbackRequest {
    /// The rating.
    pub rating: Rating,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub notes: Option<String>,
}

/// `SendMessageRequest` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SendMessageRequest {
    /// At most 2,000 characters.
    pub text: String,
    /// From `POST /conversations/{id}/images`, at most 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub image_ids: Option<Vec<ChatImageId>>,
}

/// The replay of a finished chat message under its idempotency key: `200 { message }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = MessageReplay)]
pub struct MessageReplay<U: UrlState = Signed> {
    /// The stored assistant message.
    #[schema(value_type = Message)]
    pub message: Message<U>,
}
```

- `anchor` is adjacently tagged: `{ "type": "plant", "id": "01JAX…" }`.
- An attachment's URL needs the conversation id, which `Message` doesn't carry, so `Sign for Message<Stored>` takes it as its scope.

### Chat SSE events

```rust
/// `message_started.route` and `message.route`: the text classifier's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChatRoute {
    /// About a diagnosis.
    DiagnosisQuestion,
    /// A care question.
    CareQuestion,
    /// Write tools are likely.
    TaskChange,
    /// Pesticide doses, a person or pet poisoned: the safety policy first.
    SafetySensitive,
    /// A short, polite redirect.
    OffTopic,
}

/// `message_completed.usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct Usage {
    /// Prompt tokens.
    pub tokens_in: u32,
    /// Completion tokens.
    pub tokens_out: u32,
}

/// `message_started` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MessageStarted {
    /// `1`.
    pub v: V1,
    /// The assistant message being written.
    pub message_id: MessageId,
    /// Its route.
    pub route: ChatRoute,
}

/// `text_delta` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TextDelta {
    /// `1`.
    pub v: V1,
    /// One dose-checked sentence or part of one.
    pub text: String,
}

/// `tool_started` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolStarted {
    /// `1`.
    pub v: V1,
    /// The call.
    pub call_id: String,
    /// The tool name.
    pub tool: String,
    /// For the UI: "Checking the forecast".
    pub label: String,
}

/// `tool_finished` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolFinished {
    /// `1`.
    pub v: V1,
    /// The call.
    pub call_id: String,
    /// Whether it succeeded.
    pub ok: bool,
}

/// `citation` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CitationEvent {
    /// `1`.
    pub v: V1,
    /// The source.
    pub citation: Citation,
}

/// `pending_action` data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PendingActionEvent {
    /// `1`.
    pub v: V1,
    /// The card.
    pub pending_action: PendingAction,
}

/// `memory_saved` data: shown as "Remembered: …" with Undo and Share with garden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MemorySaved {
    /// `1`.
    pub v: V1,
    /// The private chat memory.
    pub memory: Memory,
}

/// `rescan_requested` data: the app offers the capture flow for a new scan of that plant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RescanRequested {
    /// `1`.
    pub v: V1,
    /// The plant.
    pub plant_id: PlantId,
    /// Which shot.
    pub photo_kind: ImageKind,
    /// How to take it; dose-checked.
    pub instructions: String,
}

/// `message_completed` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MessageCompleted {
    /// `1`.
    pub v: V1,
    /// The stored message.
    pub message_id: MessageId,
    /// Token counts.
    pub usage: Usage,
}

/// `error` data: an error after the stream started; the stream ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChatErrorEvent {
    /// `1`.
    pub v: V1,
    /// A contracts 2 code.
    pub code: ErrorCode,
    /// Safe to show.
    pub message: String,
    /// Whether resending may work.
    pub retryable: bool,
}

/// One chat SSE event ([contracts 8], sending a message). The `data:` line carries the
/// variant's fields only, and the SSE `event:` line is [`ChatEvent::event_name`]; the stream is
/// not resumable, so events have no `id:`. Serialize only: the API writes these and the app
/// reads them.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(untagged)]
pub enum ChatEvent {
    /// `message_started`
    MessageStarted(MessageStarted),
    /// `text_delta`
    TextDelta(TextDelta),
    /// `tool_started`
    ToolStarted(ToolStarted),
    /// `tool_finished`
    ToolFinished(ToolFinished),
    /// `citation`
    Citation(CitationEvent),
    /// `pending_action`
    PendingAction(PendingActionEvent),
    /// `memory_saved`
    MemorySaved(MemorySaved),
    /// `rescan_requested`
    RescanRequested(RescanRequested),
    /// `message_completed`
    MessageCompleted(MessageCompleted),
    /// `error`
    Error(ChatErrorEvent),
}
```

The `data:` line has no `type` field; the event name is the SSE `event:` line. `ChatEvent` is therefore untagged and serialise-only: the API writes these and the app reads them, so the untagged decoding hazard doesn't arise.

### Pending actions

```rust
/// `PendingAction.tool`: the write tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteTool {
    /// Add a task.
    CreateTask,
    /// Complete a task.
    CompleteTask,
    /// Propose a new plan version.
    RevisePlan,
}

/// `PendingAction.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    /// Waiting for the user.
    Proposed,
    /// Confirmed; a retry returns the same result.
    Confirmed,
    /// Rejected.
    Rejected,
    /// Past `expires_at`.
    Expired,
}

/// `PendingAction.card`; the labels are written by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Card {
    /// "Add a task?"
    pub title: String,
    /// "Spray the tomato with water to knock off aphids".
    pub body: String,
    /// "Add task".
    pub confirm_label: String,
    /// "Not now".
    pub reject_label: String,
}

/// `PendingAction.preview`, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    /// A task to create or complete.
    Task {
        /// The task as it would be stored.
        task: Box<Task>,
    },
    /// A plan revision, stored exactly as shown when confirmed.
    Plan {
        /// The plan the revision is based on.
        base_plan_id: PlanId,
        /// The new summary.
        plan_summary: String,
        /// Replace the base plan's step with the same id, or add a new one.
        changed_steps: Vec<PlanStep>,
        /// Steps dropped.
        removed_step_ids: Vec<StepId>,
        /// "No copper products".
        constraints_added: Vec<String>,
    },
}

/// `PendingAction` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PendingAction {
    /// The card.
    pub id: PendingActionId,
    /// Its conversation.
    pub conversation_id: ConversationId,
    /// The write tool.
    pub tool: WriteTool,
    /// Its status.
    pub status: PendingStatus,
    /// What the card shows.
    pub card: Card,
    /// What confirming stores.
    pub preview: Preview,
    /// 24 hours after it was made.
    pub expires_at: Timestamp,
}

/// `result` of a confirm.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct ConfirmResult {
    /// The created or completed task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub task: Option<Task>,
    /// The new plan version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan: Option<ActionPlan>,
}

/// `200` of `POST /pending-actions/{id}/confirm`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ConfirmResponse {
    /// The card, now `confirmed`.
    pub pending_action: PendingAction,
    /// What was stored.
    pub result: ConfirmResult,
}

/// `200` of `POST /pending-actions/{id}/reject`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RejectResponse {
    /// The card, now `rejected`.
    pub pending_action: PendingAction,
}
```

`Preview::Task` boxes its `Task` to keep the two variants close in size.

### Memory

```rust
/// `Memory.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// "It's in a container".
    Fact,
    /// "Applied neem oil Oct 18".
    Event,
    /// The pipeline's scan summary.
    Summary,
}

/// `Memory.source` ([contracts 8], memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// The pipeline after each scan; every member sees it; the plan writer uses it.
    Pipeline,
    /// Extraction or `remember`; private to its author until shared.
    Chat,
    /// A shared chat memory or an editor's garden note.
    User,
}

/// `Memory` ([contracts 8]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Memory {
    /// The memory.
    pub id: MemoryId,
    /// Its garden.
    pub garden_id: GardenId,
    /// Its plant; absent for a garden-level fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// What kind.
    pub kind: MemoryKind,
    /// The text.
    pub text: String,
    /// Where it came from.
    pub source: MemorySource,
    /// `false` for chat memories until their author shares them.
    pub shared: bool,
    /// When it was stored.
    pub created_at: Timestamp,
}

/// `GET /memories?garden_id=&plant_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MemoryQuery {
    /// The garden.
    pub garden_id: GardenId,
    /// One plant's memories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plant_id: Option<PlantId>,
}

/// `POST /memories`: a garden note, `source: "user"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMemoryRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// The plant, or none for the garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// The note.
    pub text: String,
}
```

### Tool return shapes

```rust
/// The agent's tools ([contracts 8], tool return shapes and scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    /// `ScanDetail` without image URLs.
    GetScan,
    /// The image to the model.
    ViewImage,
    /// `WeatherSummary["forecast"]`.
    GetForecast,
    /// `Memory[]`.
    SearchMemory,
    /// [`SimilarCase`] list.
    FindSimilarCases,
    /// [`KnowledgeHit`] list.
    SearchKnowledge,
    /// `SpeciesSeed`.
    GetSpeciesProfile,
    /// `ProblemSeed`.
    GetDiseaseProfile,
    /// `OrganismSeed`.
    GetOrganismProfile,
    /// [`Lookalike`] list.
    CompareLookalikes,
    /// [`SoilProfileView`].
    GetSoilProfile,
    /// `MeasurementPoint[]`.
    GetPlantMeasurements,
    /// [`GardenMapView`].
    GetGardenMap,
    /// `Task[]`.
    ListTasks,
    /// `Memory` and a `memory_saved` event.
    Remember,
    /// [`RescanRequest`] and a `rescan_requested` event.
    RequestRescan,
    /// A `PendingAction` (editors and owners).
    CreateTask,
    /// A `PendingAction` (editors and owners).
    CompleteTask,
    /// A `PendingAction` (editors and owners).
    RevisePlan,
}

/// `find_similar_cases` items, at most 5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarCase {
    /// The past scan.
    pub scan_id: ScanId,
    /// When it was taken.
    pub created_at: Timestamp,
    /// Its primary problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<ProblemRef>,
    /// Cosine similarity.
    pub similarity: f64,
    /// Its `whole_plant` shot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_image_id: Option<ImageId>,
}

/// `{ problem_id, name }`: `SimilarCase.primary` and `TimelineEntry.primary`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProblemRef {
    /// The problem.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
}

/// `search_knowledge` items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeHit {
    /// The chunk text.
    pub text: String,
    /// Its source.
    pub citation: Citation,
}

/// `compare_lookalikes` items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lookalike {
    /// The look-alike.
    pub problem_id: ProblemId,
    /// Its name.
    pub name: String,
    /// How to tell them apart.
    pub how_to_tell: String,
}

/// `get_soil_profile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoilProfileView {
    /// `SoilGrids`, `null` until fetched.
    pub soil_profile: Option<SoilProfile>,
    /// Open-Meteo's soil now.
    pub latest: SoilNow,
    /// The latest guided-check texture observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<Observation<Stored>>,
}

/// `get_garden_map`: the map and each pinned plant's neighbours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenMapView {
    /// `null` when the garden has no map.
    pub map: Option<GardenMap>,
    /// Plant → plants within `NEIGHBOUR_RADIUS_M`.
    pub neighbours: BTreeMap<PlantId, Vec<PlantId>>,
}

/// `request_rescan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RescanRequest {
    /// The plant.
    pub plant_id: PlantId,
    /// Which shot.
    pub photo_kind: ImageKind,
    /// How to take it.
    pub instructions: String,
}
```

These go to the model, never to the app, so they derive serde only. `get_scan` returns `ScanDetail<Stored>`: "without image URLs", which is exactly the stored URL state.

---

## 9. Gardens, plants, tasks and account

[Contracts 9](../architecture/contracts.md#9-gardens-plants-tasks-and-account).

### Gardens

```rust
/// `GardenLocationInput.precision`: what the phone's location consent allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsentPrecision {
    /// Exact GPS.
    Exact,
    /// Rounded GPS.
    #[serde(rename = "about_1_km")]
    About1Km,
}

/// `GardenLocationInput` ([contracts 9]): exactly one of `lat` and `lon`, `postal_code`, `city`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GardenLocationInput {
    /// With location consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub lat: Option<f64>,
    /// With location consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub lon: Option<f64>,
    /// With `lat` and `lon`; `exact` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub precision: Option<ConsentPrecision>,
    /// Without consent; geocoded once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub postal_code: Option<String>,
    /// Without consent, instead of a postal code: "Guelph, ON"; geocoded once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub city: Option<String>,
}

/// A garden location after "exactly one of" is checked: what the geocoder resolves.
#[derive(Debug, Clone, PartialEq)]
pub enum LocationQuery {
    /// GPS coordinates with consent.
    Coordinates {
        /// Where.
        at: Coordinates,
        /// How precise.
        precision: ConsentPrecision,
    },
    /// A postal code.
    PostalCode(String),
    /// A city.
    City(String),
}

/// `Garden.soil_profile_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilProfileStatus {
    /// `SoilGrids` is being fetched in the background.
    Pending,
    /// Fetched.
    Ready,
    /// `SoilGrids` failed; retried later.
    Unavailable,
}

/// An IANA time zone name, such as `"America/Toronto"`: sets every `LocalDate` of the garden.
/// Checked by the geocoder that derives it; `domain` keeps it as text so it needs no time-zone
/// database.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct TimeZone(pub String);

/// `Garden` ([contracts 9]). Raw coordinates are never returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Garden {
    /// The garden.
    pub id: GardenId,
    /// Its name.
    pub name: String,
    /// The caller's role.
    pub my_role: Role,
    /// "Toronto, ON".
    pub location_label: String,
    /// How precise the stored location is.
    pub location_precision: LocationPrecision,
    /// IANA, derived from the location.
    pub time_zone: TimeZone,
    /// `SoilGrids` progress.
    pub soil_profile_status: SoilProfileStatus,
    /// How many members.
    pub member_count: u32,
    /// When it was created.
    pub created_at: Timestamp,
}

/// `POST /gardens`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateGardenRequest {
    /// Its name.
    pub name: String,
    /// Its location.
    pub location: GardenLocationInput,
}

/// `PATCH /gardens/{id}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateGardenRequest {
    /// A new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// A new location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub location: Option<GardenLocationInput>,
}

/// `200` of `GET /gardens/{id}/weather`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GardenWeather {
    /// The garden's cache cell for the current hour.
    pub weather: WeatherSummary,
}
```

`GardenLocationInput::query` checks "exactly one of `lat` and `lon`, `postal_code`, `city`" and the coordinate ranges, and gives the geocoder a `LocationQuery`.

### Members and invites

```rust
/// `GET /gardens/{id}/members` items: `{ user_id, display_name, role, since }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Member {
    /// The member.
    pub user_id: UserId,
    /// Their display name.
    pub display_name: String,
    /// Their role.
    pub role: Role,
    /// When they joined.
    pub since: Timestamp,
}

/// `Invite` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Invite {
    /// The invite.
    pub id: InviteId,
    /// 26 random base32 characters (128 bits); shared as a link.
    pub code: InviteCode,
    /// The role the invitee gets.
    pub role: Role,
    /// Who made it; absent once that user is deleted (contracts v0.9, `created_by?: Id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub created_by: Option<UserId>,
    /// 7 days after it was made.
    pub expires_at: Timestamp,
}

/// `POST /gardens/{id}/invites`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateInviteRequest {
    /// The role.
    pub role: Role,
}

/// `PATCH /gardens/{id}/members/{user_id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMemberRequest {
    /// The new role; making someone `owner` adds an owner.
    pub role: Role,
}
```

### Plants, timeline and growth

```rust
/// `GET /plants?garden_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlantsQuery {
    /// The garden.
    pub garden_id: GardenId,
}

/// `POST /plants`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePlantRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// Its name.
    pub name: String,
    /// Its species, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
}

/// `PATCH /plants/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePlantRequest {
    /// A new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// A new status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub status: Option<PlantStatus>,
}

/// `TimelineEntry.measurements`: medium confidence or more.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct TimelineMeasurements {
    /// cm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_height_cm: Option<f64>,
    /// cm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub canopy_width_cm: Option<f64>,
}

/// `TimelineEntry` ([contracts 9]): newest first, skipping scans being deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = TimelineEntry)]
pub struct TimelineEntry<U: UrlState = Signed> {
    /// The scan.
    pub scan_id: ScanId,
    /// When it was created.
    pub created_at: Timestamp,
    /// Its status.
    pub status: ScanStatus,
    /// The current diagnosis' status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub diagnosis_status: Option<DiagnosisStatus>,
    /// The primary problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub primary: Option<ProblemRef>,
    /// The `whole_plant` shot, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub thumbnail_image_id: Option<ImageId>,
    /// Signed URL of that shot.
    #[serde(default, skip_serializing_if = "url_absent")]
    #[schema(nullable = false, value_type = Option<SignedUrl>)]
    pub thumbnail_url: Option<U::Url>,
    /// With status `failed`: "Failed: no plant found".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub failure_code: Option<ScanFailureCode>,
    /// The current plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_id: Option<PlanId>,
    /// Height and width, medium confidence or more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub measurements: Option<TimelineMeasurements>,
}

/// `MeasurementPoint.quantity`: the growth quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrowthQuantity {
    /// Plant height.
    PlantHeightCm,
    /// Canopy width.
    CanopyWidthCm,
}

/// `MeasurementPoint` ([contracts 9]): one per scan and quantity, oldest first.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MeasurementPoint {
    /// The scan.
    pub scan_id: ScanId,
    /// The scan's `created_at`, when the photos were taken.
    pub created_at: Timestamp,
    /// Which quantity.
    pub quantity: GrowthQuantity,
    /// cm.
    pub value: f64,
    /// Medium or high; low never enters the series.
    pub confidence_band: ConfidenceBand,
}

/// `GET /plants/{id}/measurements?quantity=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MeasurementsQuery {
    /// One quantity, or both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<GrowthQuantity>,
}
```

### Species

```rust
/// `SpeciesSummary.profile` and `SpeciesSeed.profile`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SpeciesProfileKind {
    /// Care profile and problems.
    Full,
    /// Recognised by `BioCLIP`, no care profile or problems yet.
    LabelOnly,
}

/// `SpeciesSummary extends SpeciesRef` ([contracts 9]). Only exists on the wire: it is built
/// from the knowledge record, whose photos are storage keys, so it has no stored form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesSummary {
    /// `id`, `scientific_name`, `common_name`.
    #[serde(flatten)]
    pub species: SpeciesRef,
    /// `full` or `label_only`.
    pub profile: SpeciesProfileKind,
    /// The disease classifier covers it.
    pub classifier_supported: bool,
    /// At most 3, in seed order.
    pub reference_images: Vec<ReferenceImage<Signed>>,
}

/// A species photo as the knowledge base records it: the content hash in its key and its
/// credit ([contracts 10], `SpeciesSeed.reference_images`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesPhoto {
    /// The first 16 hex digits of the WebP's SHA-256.
    pub hash: PhotoHash,
    /// "Photo: A. Author, CC BY-SA 4.0".
    pub credit: String,
}

/// `GET /species?q=`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SpeciesQuery {
    /// Matches common and scientific names, case-insensitively, anywhere in the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
}
```

`SpeciesSummary` exists only on the wire: the knowledge record holds storage keys, and `signed_images` builds the signed photos from them.

### Garden map

```rust
/// `GardenMapInput.beds[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BedInput {
    /// The app's own id, unique within the map.
    pub id: String,
    /// Its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// Ground plane, metres; 3–50 points, a simple polygon.
    pub polygon_m: Vec<[f64; 2]>,
}

/// `GardenMapInput.pins[]`: exactly one of `plant_id` and `new_plant`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PinInput {
    /// An existing plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// A plant created with the upload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub new_plant: Option<NewPlantInput>,
    /// Within 200 m of the origin.
    pub x_m: f64,
    /// Within 200 m of the origin.
    pub y_m: f64,
    /// From the reconstructed mesh; shown, not used by any rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub canopy_radius_m: Option<f64>,
    /// The bed it stands in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub bed_id: Option<String>,
}

/// `PUT /gardens/{id}/map` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GardenMapInput {
    /// The version current when the mapping session started; 0 when the garden had none.
    pub base_version: u32,
    /// At most 20.
    pub beds: Vec<BedInput>,
    /// At most 200; a plant appears at most once.
    pub pins: Vec<PinInput>,
    /// 0–360, clockwise from the map's +y axis to north; only with a compass accuracy of 20° or
    /// better.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub north_deg: Option<f64>,
}

/// `GardenMap.beds[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Bed {
    /// The app's id.
    pub id: String,
    /// Its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// Ground plane, metres.
    pub polygon_m: Vec<[f64; 2]>,
    /// Computed from the polygon.
    pub area_m2: f64,
}

/// `GardenMap.pins[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Pin {
    /// The plant.
    pub plant_id: PlantId,
    /// Metres.
    pub x_m: f64,
    /// Metres.
    pub y_m: f64,
    /// Shown only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub canopy_radius_m: Option<f64>,
    /// The bed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub bed_id: Option<String>,
}

/// `GardenMap` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GardenMap {
    /// The garden.
    pub garden_id: GardenId,
    /// The version; only the current one is kept.
    pub version: u32,
    /// Beds.
    pub beds: Vec<Bed>,
    /// Pins, leaving out plants being deleted.
    pub pins: Vec<Pin>,
    /// North, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub north_deg: Option<f64>,
    /// Absent once that user is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub created_by: Option<UserRef>,
    /// When it was uploaded.
    pub created_at: Timestamp,
}

/// `200` of `GET /gardens/{id}/map`: `{ map: GardenMap | null }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MapResponse {
    /// `null` when the garden has no map.
    #[schema(required = true)]
    pub map: Option<GardenMap>,
}

/// A pin's plant, once "exactly one of" is checked.
#[derive(Debug, Clone, PartialEq)]
pub enum PinPlant {
    /// An existing plant; the store checks it belongs to the garden.
    Existing(PlantId),
    /// A plant created in the upload's transaction.
    New(NewPlantInput),
}

/// A checked pin.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidPin {
    /// Its plant.
    pub plant: PinPlant,
    /// Metres.
    pub x_m: f64,
    /// Metres.
    pub y_m: f64,
    /// Shown only.
    pub canopy_radius_m: Option<f64>,
    /// The bed.
    pub bed_id: Option<String>,
}

/// A checked upload, ready for the map transaction ([database 4], garden map versions).
#[derive(Debug, Clone, PartialEq)]
pub struct MapUpload {
    /// Checked against `garden.map_version` in the transaction.
    pub base_version: u32,
    /// Beds with their areas.
    pub beds: Vec<Bed>,
    /// Pins.
    pub pins: Vec<ValidPin>,
    /// North.
    pub north_deg: Option<f64>,
}
```

`GardenMapInput::validate` checks everything that needs no database (counts, simple polygons, 200 m, one pin per plant, exactly one of `plant_id` and `new_plant`, known bed ids, the north range) and computes each bed's area; the store checks the rest in the map transaction ([domain 9](domain.md#9-boundary-traits)).

### Tasks

```rust
/// `GET /tasks?garden_id=&source=&state=`: by `due` (tasks without one last), then
/// `created_at`. `?source=forecast_alert` is the alert inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskQuery {
    /// The garden.
    pub garden_id: GardenId,
    /// Filter by source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TaskSource>,
    /// Filter by state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<TaskState>,
}

/// `POST /tasks`: `source: "user"`. A `plant_id` or `scan_id` that isn't live in the garden
/// gets `404`, checked in the insert's transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// A live plant of the garden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// A live scan of the garden (and of that plant): the jar-test task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub scan_id: Option<ScanId>,
    /// Title.
    pub title: String,
    /// Detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    /// Due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub due: Option<LocalDate>,
    /// Repeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub repeat: Option<Repeat>,
}

/// `PATCH /tasks/{id}`. Only `user` and `chat` tasks can change `title`, `detail`, `due` and
/// `repeat`; completing a repeating task creates the next one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTaskRequest {
    /// Complete or reopen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub done: Option<bool>,
    /// Dismiss or restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub dismissed: Option<bool>,
    /// New title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub title: Option<String>,
    /// New detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub detail: Option<String>,
    /// New due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub due: Option<LocalDate>,
    /// New repeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub repeat: Option<Repeat>,
}
```

### Account

```rust
/// `GET /me` and `PATCH /me`: `{ id, display_name, location_consent, created_at }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Me {
    /// The user.
    pub id: UserId,
    /// Their display name.
    pub display_name: String,
    /// Whether gardens may store GPS coordinates.
    pub location_consent: bool,
    /// When the account was created.
    pub created_at: Timestamp,
}

/// `PATCH /me`. Withdrawing consent rounds every owned garden to `about_10_km`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMeRequest {
    /// New display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub display_name: Option<String>,
    /// New consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub location_consent: Option<bool>,
}

/// `DELETE /me` `transferred_gardens[]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GardenTransfer {
    /// The garden.
    pub garden_id: GardenId,
    /// The longest-standing editor, now owner.
    pub new_owner_user_id: UserId,
}

/// `202` of `DELETE /me`. The cascade runs in the background (architecture 5, deletion).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct AccountDeletion {
    /// Gardens deleted outright.
    pub deleted_gardens: Vec<GardenId>,
    /// Gardens handed to another member.
    pub transferred_gardens: Vec<GardenTransfer>,
}

/// `POST /me/push-tokens` `platform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Delivered through Expo Push.
    Ios,
    /// Delivered through Expo Push.
    Android,
    /// Stored, not yet used: Expo Push doesn't deliver to the web.
    Web,
}

/// `POST /me/push-tokens`: registering the same token again is a no-op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushTokenRequest {
    /// The Expo push token.
    pub token: PushToken,
    /// Its platform.
    pub platform: Platform,
}
```

### Push notifications

```rust
/// `PushPayload.reason` for `kind: "scan"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PushScanReason {
    /// A run reached `planned`.
    PlanReady,
    /// A late answer, a guided check or a species change produced a new plan version (not for
    /// `chat_revision`).
    PlanSuperseded,
    /// A run asked a question.
    Question,
    /// The scan failed.
    Failed,
}

/// `PushPayload` ([contracts 9]): the `data` of an Expo push message, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PushPayload {
    /// To the scan's creator, while still a member.
    Scan {
        /// `1`.
        v: V1,
        /// Why.
        reason: PushScanReason,
        /// The garden.
        garden_id: GardenId,
        /// The plant.
        plant_id: PlantId,
        /// The scan.
        scan_id: ScanId,
    },
    /// A forecast alert's task, to every member.
    Alert {
        /// `1`.
        v: V1,
        /// The garden.
        garden_id: GardenId,
        /// The alert task.
        task_id: TaskId,
        /// The rule.
        trigger: TriggerKind,
    },
    /// To every other member the garden had.
    GardenDeleted {
        /// `1`.
        v: V1,
        /// Its name.
        garden_name: String,
    },
}

/// One push message for the `Notifier`: a short English title and body, and the payload as
/// `data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMessage {
    /// The device token (iOS or Android).
    pub to: PushToken,
    /// Title.
    pub title: String,
    /// Body.
    pub body: String,
    /// `data`.
    pub data: PushPayload,
}
```

`PushPayload` is tagged by `kind` and every variant carries `v`.

### Auth webhook

```rust
/// `AuthWebhookEvent.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum AuthEventType {
    /// Upsert the user on `auth_id`.
    #[serde(rename = "user.created")]
    UserCreated,
    /// Run the account deletion cascade.
    #[serde(rename = "user.deleted")]
    UserDeleted,
}

/// `AuthWebhookEvent.data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuthEventData {
    /// The provider's user id, stored as `user.auth_id`.
    pub id: AuthId,
}

/// `AuthWebhookEvent` ([contracts 9]). `204`, idempotent. Clerk's payload has more fields; they
/// are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuthWebhookEvent {
    /// `user.created` or `user.deleted`.
    #[serde(rename = "type")]
    pub event_type: AuthEventType,
    /// The user.
    pub data: AuthEventData,
}

/// Svix's three headers, borrowed from the request for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebhookHeaders<'a> {
    /// `svix-id`.
    pub id: &'a str,
    /// `svix-timestamp`, Unix seconds.
    pub timestamp: &'a str,
    /// `svix-signature`: space-separated `v1,<base64>` entries.
    pub signature: &'a str,
}

/// Svix rejects a timestamp more than 5 minutes from now.
pub const WEBHOOK_TOLERANCE: std::time::Duration = std::time::Duration::from_mins(5);
```

### Ops

```rust
/// `Status.workers[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkerStatus {
    /// The worker.
    pub worker_id: WorkerId,
    /// Its capabilities.
    pub caps: Vec<Capability>,
    /// Heartbeat within 30 seconds.
    pub healthy: bool,
    /// Its last heartbeat.
    pub last_heartbeat: Timestamp,
    /// Its GPU's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub gpu: Option<String>,
}

/// `Status.queue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct QueueCounts {
    /// Jobs waiting.
    pub queued: u32,
    /// Jobs leased.
    pub leased: u32,
}

/// `GET /status` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Status {
    /// Every known worker.
    pub workers: Vec<WorkerStatus>,
    /// The queue.
    pub queue: QueueCounts,
}

/// `GET /healthz`: `200 { ok: true }` when `SurrealDB` and `RustFS` are reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct Healthz {
    /// `true`.
    pub ok: True,
}
```

---

## 10. Knowledge seed format

[Contracts 10](../architecture/contracts.md#10-knowledge-seed-format). Every seed type derives `schemars::JsonSchema` and denies unknown fields, so the JSON Schema says `additionalProperties: false` and a misspelt key in a reviewed YAML file fails validation.

### Problem profile

```rust
/// `IndicatorWhere` ([contracts 10]): where on the plant an indicator shows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorWhere {
    /// Lower leaves.
    LowerLeaves,
    /// Upper leaves.
    UpperLeaves,
    /// The whole plant.
    WholePlant,
    /// The leaf underside.
    LeafUnderside,
    /// The stem.
    Stem,
    /// The fruit.
    Fruit,
    /// The soil.
    Soil,
}
```

```rust
/// `ProblemSeed.prevalence`: the tie-breaker in the candidate ranking.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Prevalence {
    /// Common.
    Common,
    /// Occasional.
    Occasional,
    /// Rare.
    Rare,
}

/// `IndicatorMeasure.stat`: which value of the measurement the range applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasureStat {
    /// `value`.
    Median,
    /// `max`.
    Max,
    /// `min`.
    Min,
}

/// `IndicatorMeasure` ([contracts 10]): at least one of `min` and `max`, in the quantity's
/// unit, `min` < `max`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndicatorMeasure {
    /// The quantity.
    pub quantity: MeasureQuantity,
    /// Which statistic.
    pub stat: MeasureStat,
    /// Lower bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Upper bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// `ProblemSeed.indicators[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndicatorSeed {
    /// `"<problem_id>.<slug>"`.
    pub id: IndicatorId,
    /// "Brown spots with concentric rings, like a target".
    pub text: String,
    /// Asked to the VLM verbatim.
    pub question: String,
    /// 1–3.
    pub weight: Weight,
    /// Exactly 3 per problem.
    pub discriminating: bool,
    /// Where it shows.
    pub r#where: IndicatorWhere,
    /// Which shots can show it.
    pub image_kinds: Vec<ImageKind>,
    /// Answered from `LiDAR` measurements when possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<IndicatorMeasure>,
}

/// `ProblemSeed.conditions.temp_c` and `SpeciesSeed.care.soil_ph`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Range {
    /// Lower bound.
    pub min: f64,
    /// Upper bound.
    pub max: f64,
}

/// `ProblemSeed.conditions`: the weather that favours the problem.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    /// °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<Range>,
    /// Relative humidity, %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rh_pct_min: Option<f64>,
    /// Hours with RH over 90% in the past 7 days (leaf-wetness proxy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub humid_hours_min: Option<u32>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// `ProblemSeed.looks_like[]`, written once in either problem's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LooksLikeSeed {
    /// The look-alike.
    pub problem: ProblemId,
    /// How to tell them apart.
    pub how_to_tell: String,
}

/// `ProblemSeed` ([contracts 10]): `problems/<problem_id>.yaml`, the `disease` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProblemSeed {
    /// The slug.
    pub id: ProblemId,
    /// "Early blight".
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
    /// Species → `susceptible_to` edges.
    pub affects: Vec<SpeciesId>,
    /// Organisms, for pest and wildlife problems.
    pub caused_by: Vec<OrganismId>,
    /// Patterns that point to this problem.
    pub damage_patterns: Vec<DamagePattern>,
    /// Tie-breaker in the ranking.
    pub prevalence: Prevalence,
    /// Indicators; exactly 3 discriminating.
    pub indicators: Vec<IndicatorSeed>,
    /// Favourable weather.
    pub conditions: Conditions,
    /// Look-alikes.
    pub looks_like: Vec<LooksLikeSeed>,
    /// `manages` edges.
    pub treatments: Vec<TreatmentId>,
    /// Its sources.
    pub sources: Vec<SourceId>,
    /// The teammate who reviewed it.
    pub reviewed_by: String,
    /// When.
    pub reviewed_at: LocalDate,
}
```

The contract's example profile, as the JSON its YAML parses to, round-trips:

```rust
#[test]
fn problem_seed_example_round_trips() {
    let seed: ProblemSeed = serde_json::from_str(EARLY_BLIGHT).unwrap();
    assert_eq!(seed.discriminating().count(), 3);
    assert_eq!(seed.indicators[0].r#where, IndicatorWhere::LowerLeaves);
    let again = serde_json::to_value(&seed).unwrap();
    assert_eq!(
        serde_json::from_value::<ProblemSeed>(again.clone()).unwrap(),
        seed
    );
    let expected: serde_json::Value = serde_json::from_str(EARLY_BLIGHT).unwrap();
    let keys =
        |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&again), keys(&expected));
    assert_eq!(again["indicators"], expected["indicators"]);
}
```

### Other entity shapes

```rust
/// `SourceSeed` ([contracts 10]): `sources.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceSeed {
    /// The slug.
    pub id: SourceId,
    /// Title.
    pub title: String,
    /// Publisher.
    pub publisher: String,
    /// Its page.
    pub url: String,
    /// Reuse terms; required for photo sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    /// When it was retrieved.
    pub retrieved_at: LocalDate,
}

/// `SpeciesSeed.care.water`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaterNeed {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// `SpeciesSeed.care.light`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LightNeed {
    /// Full sun.
    FullSun,
    /// Part shade.
    PartShade,
    /// Shade.
    Shade,
}

/// `SpeciesSeed.care`: required when `profile` is `full`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Care {
    /// Water.
    pub water: WaterNeed,
    /// Light.
    pub light: LightNeed,
    /// Soil pH.
    pub soil_ph: Range,
    /// Canadian plant hardiness zones, such as `"annual"` or `"4-8"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardiness_zones: Option<String>,
    /// Recommended distance between plants; the crowding rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_cm: Option<f64>,
    /// The `growth_stalled` rule stops here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mature_height_cm: Option<f64>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// `SpeciesSeed.reference_images[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceImageSeed {
    /// Under `ml/knowledge/seed/images/`, e.g. `"species/tomato-1.jpg"`.
    pub file: String,
    /// A source whose licence allows reuse with credit.
    pub source_id: SourceId,
    /// "Photo: A. Author, CC BY-SA 4.0", shown with the photo.
    pub credit: String,
}

/// `SpeciesSeed` ([contracts 10]): `species/<species_id>.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeciesSeed {
    /// The slug.
    pub id: SpeciesId,
    /// "Solanum lycopersicum".
    pub scientific_name: String,
    /// The first is the common name shown.
    pub common_names: Vec<String>,
    /// `label_only`: recognised by `BioCLIP`, no care profile or problems yet.
    pub profile: SpeciesProfileKind,
    /// Required when `full`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub care: Option<Care>,
    /// Checked against `classifier_labels.yaml` at ingestion.
    pub classifier_supported: bool,
    /// The text prompt `BioCLIP` scores against.
    pub bioclip_label: String,
    /// May be empty for `label_only`.
    pub sources: Vec<SourceId>,
    /// At most 3, for the confirmation card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_images: Option<Vec<ReferenceImageSeed>>,
    /// The reviewer.
    pub reviewed_by: String,
}

/// `OrganismSeed.taxon_group`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaxonGroup {
    /// Insects.
    Insect,
    /// Mites.
    Mite,
    /// Slugs and snails.
    Mollusc,
    /// Worms.
    Worm,
    /// Mammals.
    Mammal,
    /// Birds.
    Bird,
}

/// `OrganismSeed.role`: the seeded roles; `unknown` exists only for "other" findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SeedOrganismRole {
    /// Pest.
    Pest,
    /// Beneficial.
    Beneficial,
    /// Neutral.
    Neutral,
    /// Invasive: `reporting_url` required.
    Invasive,
}

/// `OrganismSeed` ([contracts 10]): `organisms/<organism_id>.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganismSeed {
    /// The slug.
    pub id: OrganismId,
    /// "Ladybug larva".
    pub name: String,
    /// Scientific name.
    pub scientific_name: String,
    /// Group.
    pub taxon_group: TaxonGroup,
    /// Role.
    pub role: SeedOrganismRole,
    /// How to recognise it in a photo.
    pub recognition: String,
    /// Beneficial → pest edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preys_on: Option<Vec<OrganismId>>,
    /// Required when invasive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporting_url: Option<String>,
    /// `BioCLIP` prompt.
    pub bioclip_label: String,
    /// Sources.
    pub sources: Vec<SourceId>,
    /// Reviewer.
    pub reviewed_by: String,
}

/// `TreatmentSeed.kind`: a step that applies the treatment takes its category from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TreatmentKind {
    /// Cultural.
    Cultural,
    /// Biological.
    Biological,
    /// Mechanical.
    Mechanical,
    /// Chemical.
    Chemical,
}

/// `TreatmentSeed` ([contracts 10]): no doses, ever; checked like plan steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreatmentSeed {
    /// The slug.
    pub id: TreatmentId,
    /// Category.
    pub kind: TreatmentKind,
    /// The advice.
    pub text: String,
    /// Sources.
    pub sources: Vec<SourceId>,
}

/// `ProductsSeed.products[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductSeed {
    /// Matched case-insensitively; a step naming one becomes chemical.
    pub name: String,
    /// Harms beneficial insects too.
    pub broad_spectrum: bool,
}

/// `ProductsSeed` ([contracts 10]): `products.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductsSeed {
    /// Every pest-control product name.
    pub products: Vec<ProductSeed>,
}

/// `ClassifierLabels.labels[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassifierLabel {
    /// The classifier's output index.
    pub index: u32,
    /// The training dataset's class name.
    pub label: String,
    /// The species.
    pub species_id: SpeciesId,
    /// The problem, or `"healthy"`.
    pub problem_id: ProblemOrHealthy,
}

/// `ClassifierLabels` ([contracts 10]): `classifier_labels.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassifierLabels {
    /// The classifier version these labels belong to.
    pub model_version: String,
    /// One per output.
    pub labels: Vec<ClassifierLabel>,
}

/// `ChunkSeed.topic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChunkTopic {
    /// Symptoms.
    Symptoms,
    /// Conditions.
    Conditions,
    /// Management.
    Management,
    /// Care.
    Care,
    /// Recognition.
    Recognition,
}

/// `ChunkSeed` ([contracts 10]): one line of `chunks/<source_id>.jsonl`. Embeddings are
/// computed at load time with the pinned model, never stored in the seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkSeed {
    /// `"<source_id>.<n>"`.
    pub id: ChunkId,
    /// The source.
    pub source_id: SourceId,
    /// The crop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<SpeciesId>,
    /// The problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<ProblemId>,
    /// The organism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organism: Option<OrganismId>,
    /// The topic.
    pub topic: ChunkTopic,
    /// A short excerpt, at most about 200 words.
    pub text: String,
}

/// One entry of `soil_conditions.yaml`: a description for each `SoilAspect` and value, with
/// what to do and its sources ([contracts 10]; the file's shape is not spelled out there, so
/// this follows `schema.surql`'s `soil_condition` table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoilConditionSeed {
    /// The aspect.
    pub aspect: SoilAspect,
    /// The value, such as `"compacted"`.
    pub value: String,
    /// What it means.
    pub description: String,
    /// What to do.
    pub what_to_do: String,
    /// Sources.
    pub sources: Vec<SourceId>,
}
```

- `SeedOrganismRole` has the four seeded roles; `unknown` exists only for findings reported as "other" (`OrganismRole`).
- `soil_conditions.yaml`'s shape isn't spelled out in contracts 10; `SoilConditionSeed` follows the `soil_condition` table of [schema.surql](../../backend/migrations/schema.surql).

### Soil profile

```rust
/// `SoilProfile.depths[].depth_cm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum DepthBand {
    /// 0–5 cm.
    #[serde(rename = "0-5")]
    D0To5,
    /// 5–15 cm.
    #[serde(rename = "5-15")]
    D5To15,
    /// 15–30 cm.
    #[serde(rename = "15-30")]
    D15To30,
    /// 30–60 cm.
    #[serde(rename = "30-60")]
    D30To60,
    /// 60–100 cm.
    #[serde(rename = "60-100")]
    D60To100,
    /// 100–200 cm.
    #[serde(rename = "100-200")]
    D100To200,
}

/// `SoilProfile.depths[]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilDepth {
    /// The depth band.
    pub depth_cm: DepthBand,
    /// pH in water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub ph_h2o: Option<f64>,
    /// %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub clay_pct: Option<f64>,
    /// %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub sand_pct: Option<f64>,
    /// %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub silt_pct: Option<f64>,
    /// g/kg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub organic_carbon_g_kg: Option<f64>,
    /// g/kg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub nitrogen_g_kg: Option<f64>,
    /// cmol/kg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub cec_cmol_kg: Option<f64>,
    /// kg/dm³.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub bulk_density_kg_dm3: Option<f64>,
}

/// `SoilProfile` ([contracts 10]): `garden.soil_profile`, from `SoilGrids`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilProfile {
    /// When it was fetched.
    pub fetched_at: Timestamp,
    /// Per depth band.
    pub depths: Vec<SoilDepth>,
}
```

---

## 11. TypeScript to Rust map

`domain::` is omitted from the paths. `U` marks a type generic over the URL state (`X` is the signed form, `X<Stored>` the stored one).

| Contract (TypeScript) | Rust type | Module |
|---|---|---|
| `Id` | `Id<T>` and its aliases (`ScanId`, …) | `id` |
| `Slug` | `Slug<K>` (`SpeciesId`, `ProblemId`, `OrganismId`, `TreatmentId`, `SourceId`) | `id` |
| `Slug \| "healthy"` | `ProblemOrHealthy` | `id` |
| `"<problem_id>.<slug>"`, `"<image_id>.<n>"`, `"<source_id>.<n>"` | `IndicatorId`, `RegionId`, `ChunkId` | `id` |
| `Timestamp`, `LocalDate` | `Timestamp`, `LocalDate` | `wire` |
| `Confidence`, `ConfidenceBand` | `Confidence`, `ConfidenceBand`; the pair as `Rated` | `wire` |
| `BBox`, `Region`, `Page<T>` | `BBox`, `Region<U>`, `Page<T>` | `wire` |
| `?cursor=&limit=` | `PageRequest` | `wire` |
| `v: 1` | `V1` | `wire` |
| `"owner" \| "editor" \| "viewer" \| "reviewer"` | `Role` | `wire` |
| `{ user_id, display_name }` | `UserRef` | `wire` |
| `ErrorBody`, `ErrorCode`, `QualityHint` | `ErrorBody`, `ErrorObject`, `ErrorKind`, `ErrorCode`, `QualityHint` | `error` |
| `ScanStatus`, `ImageKind` | `ScanStatus`, `ImageKind` | `scan` |
| `ScanImage`, `ScanImage.depth` | `ScanImage<U>`, `DepthInfo` | `scan` |
| `SpeciesRef`, `SpeciesCandidate` | `SpeciesRef`, `SpeciesCandidate<U>`, `ReferenceImage<U>` | `scan` |
| `Plant`, `Plant.position`, `Plant.latest_scan` | `Plant<U>`, `PlantPosition`, `LatestScan<U>` | `scan` |
| `ScanFailureCode`, `ScanFailure` | `ScanFailureCode`, `ScanFailure` | `scan` |
| `Question`, its options | `Question`, `QuestionOption`, `Answer`, `QuestionKind`, `QuestionStatus` | `scan` |
| `GuidedCheck` | `GuidedCheck<U>`, `GuidedKind`, `GuidedCheckStatus` | `scan` |
| `Scan`, `ScanDetail` | `Scan<U>`, `ScanDetail<U>` | `scan` |
| `CreateScanRequest` | `CreateScanRequest`, `NewPlantInput`, `ScanPlant` | `scan` |
| `DepthMeta` | `DepthMeta`, `DepthSource`, `Intrinsics`, `Distortion` | `scan` |
| `UploadImageResponse` | `UploadImageResponse<U>`, `QualityReport`, `DepthOutcome`, `DepthProblem` | `scan` |
| `AnswerRequest`, `ConfirmSpeciesRequest`, `FeedbackRequest` | `AnswerRequest`, `ConfirmSpeciesRequest` (`SpeciesChoice`), `FeedbackRequest`, `Verdict` | `scan` |
| `{ id: Id }` | `Created<T>` | `scan` |
| `ScanEventBase` + catalogue | `ScanEvent<U>`, `ScanEventBody<U>`, `ScanEventType` | `events` |
| `Stage`, `ProblemKind` | `Stage`, `ProblemKind` | `events` |
| event fields | `RunReason`, `Skipped`, `SelectedCandidate`, `SelectionSource`, `ResolvedBy`, `SupersedeReason` | `events` |
| `Capability`, `Heartbeat`, `HeartbeatResponse`, `ClaimRequest` | same names, `GpuInfo`, `HeldJob` | `jobs` |
| `Job`, `JobImage` | `Job`, `JobSpec`, `JobKind`, `JobImage`, `WorkerImagePath`, `JobHead`, `ClaimedJob` | `jobs` |
| `PerceptionPayload`, `VlmPayload`, `VerifyPayload`, `GuidedPayload` | same names, `PerceptionStage`, `PriorRegion`, `WorkingSpecies`, `SpeciesOption`, `OrganismOption`, `VerifyReason` | `jobs` |
| `Indicator`, `CandidatePrompt` | `Indicator`, `CandidatePrompt` | `jobs` |
| `WorkerEvents`, `PartialResult` | `WorkerEvents`, `WorkerEvent`, `PartialResult` | `jobs` |
| `CompleteJob` | `CompleteJob`, `JobOutcome`, `JobError`, `JobErrorCode`, `ModelVersionKey`, `JobResult` | `jobs` |
| `RegionClass`, `RleMask`, `PerceptionResult` | same names, `PerceptionRegion`, `SpeciesResult`, `SpeciesLabel`, `DiseaseResult`, `ProblemScore`, `DiseaseSkipped`, `ImageEmbedding` | `jobs` |
| `VlmResult`, `VerifyResult`, `GuidedResult` | same names | `jobs` |
| `Certainty` | `Certainty` | `wire` |
| `ImageAlias`, `SoilReading<V>`, `VlmFindingsRaw`, `VlmIndicatorsRaw`, `VlmGuidedRaw` | same names and the soil value enums, `BoxFormat` | `vlm` |
| `DamagePattern`, `SoilAspect`, `count` | `DamagePattern`, `SoilAspect`, `OrganismCount` | `vlm` |
| `VlmFindings`, `VlmIndicatorAnswers`, `GuidedTexture` | same names, `SpeciesGuess`, `SoilFinding`, `OrganismFinding`, `DamageFinding`, `IndicatorAnswer` | `vlm` |
| `Observation` | `Observation<U>`, `ObservationStage`, `KnowledgeRef`, `OrganismRole`, `ObservationDetail`, `ObservationSource` | `records::observation` |
| `MeasureQuantity`, `Measurement` | same names, `MeasureUnit` | `records::measurement` |
| `WeatherSummary` | `WeatherSummary`, `CellWeather`, `LocationPrecision`, `CurrentWeather`, `PastWeather`, `ForecastDay`, `SoilNow`, `WeatherCell` | `records::weather` |
| `IndicatorEvidence`, `CandidateScore`, `Citation`, `Diagnosis` | same names, `Weight`, `EvidenceSource`, `MeasuredValue`, `WeatherFit`, `DiagnosisStatus`, `DiagnosisOrganism`, `Prey`, `Risk`, `RiskLevel`, `EvidenceBasis` | `records::diagnosis` |
| `PlanWhen`, `TriggerKind`, `PlanStep`, `ActionPlan` | same names, `StepCategory`, `PlanTarget`, `PlanConstraint`, `PlanReason`, `PlanWriterOutput` | `records::plan` |
| `Task` | `Task`, `Repeat`, `TaskSource`, `TaskTrigger`, `DismissedReason`, `TaskState`, `AlertKey` | `records::task` |
| `CreateConversationRequest`, `Conversation`, `Message`, `ConversationDetail` | same names, `Anchor`, `Attachment<U>`, `MessageRole`, `MessageReplay<U>` | `chat` |
| `SendMessageRequest`, message feedback | `SendMessageRequest`, `MessageFeedbackRequest`, `Rating` | `chat` |
| chat SSE events | `ChatEvent` and one struct per event, `ChatRoute`, `Usage` | `chat` |
| `PendingAction` | `PendingAction`, `WriteTool`, `PendingStatus`, `Card`, `Preview`, `ConfirmResponse`, `ConfirmResult`, `RejectResponse` | `chat` |
| `Memory` | `Memory`, `MemoryKind`, `MemorySource`, `MemoryQuery`, `CreateMemoryRequest` | `chat` |
| tool return shapes | `SimilarCase`, `ProblemRef`, `KnowledgeHit`, `Lookalike`, `SoilProfileView`, `GardenMapView`, `RescanRequest`, `ToolName` | `chat` |
| `GardenLocationInput`, `Garden` | same names, `ConsentPrecision`, `LocationQuery`, `SoilProfileStatus`, `TimeZone`, `CreateGardenRequest`, `UpdateGardenRequest`, `GardenWeather` | `garden::gardens` |
| members, `Invite` | `Member`, `Invite`, `CreateInviteRequest`, `UpdateMemberRequest` | `garden::members` |
| `TimelineEntry`, `MeasurementPoint` | `TimelineEntry<U>`, `MeasurementPoint`, `GrowthQuantity`, `TimelineMeasurements`, `CreatePlantRequest`, `UpdatePlantRequest` | `garden::plants` |
| `SpeciesSummary` | `SpeciesSummary`, `SpeciesProfileKind`, `SpeciesPhoto` | `garden::species` |
| `GardenMapInput`, `GardenMap` | same names, `BedInput`, `PinInput`, `Bed`, `Pin`, `MapResponse`, `MapUpload` | `garden::map` |
| task requests | `TaskQuery`, `CreateTaskRequest`, `UpdateTaskRequest` | `garden::tasks` |
| account | `Me`, `UpdateMeRequest`, `AccountDeletion`, `GardenTransfer`, `PushTokenRequest`, `Platform` | `garden::account` |
| `PushPayload` | `PushPayload`, `PushScanReason`, `PushMessage` | `garden::push` |
| `AuthWebhookEvent` | `AuthWebhookEvent`, `AuthEventType`, `AuthEventData` | `garden::webhook` |
| `Status`, `/healthz` | `Status`, `WorkerStatus`, `QueueCounts`, `Healthz` | `garden::ops` |
| `IndicatorWhere` | `IndicatorWhere` | `jobs` |
| `ProblemSeed`, `IndicatorMeasure` | same names, `IndicatorSeed`, `Conditions`, `Range`, `LooksLikeSeed`, `Prevalence`, `MeasureStat` | `knowledge` |
| `SourceSeed`, `SpeciesSeed`, `OrganismSeed`, `TreatmentSeed`, `ProductsSeed`, `ClassifierLabels`, `ChunkSeed` | same names and their parts | `knowledge` |
| `SoilProfile` | `SoilProfile`, `SoilDepth`, `DepthBand` | `knowledge` |

---

## 12. Sources

**Project documents**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [database](../database/README.md) and [schema](../../backend/migrations/schema.surql) · [storage](../storage/README.md) · [the `domain` crate](domain.md)

**serde and schema generators**
- [serde enum representations](https://serde.rs/enum-representations.html) · [`flatten`](https://serde.rs/attr-flatten.html) · [container attributes](https://serde.rs/container-attrs.html) · [field attributes](https://serde.rs/field-attrs.html) · [variant attributes](https://serde.rs/variant-attrs.html)
- [utoipa 6 `ToSchema` derive](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html) (field nullability and required rules, generics, `bound`) · [`PartialSchema`](https://docs.rs/utoipa/6.0.0/utoipa/trait.PartialSchema.html) · [`IntoParams`](https://docs.rs/utoipa/6.0.0/utoipa/derive.IntoParams.html)
- [schemars 1.2.3](https://docs.rs/schemars/1.2.3/schemars/) · [schemars attributes](https://graham.cool/schemars/deriving/attributes/)

**Crates at the pinned versions**
- [chrono 0.4.45 `DateTime`](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html) · [ulid 3.0.0 `Ulid`](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html) · [uuid 1.28.0](https://docs.rs/uuid/1.28.0/uuid/)

**Project skills** (in the main checkout, `.claude/skills/`)
- `rust-contract-types` (optional vs nullable, unions, `V1`, `JobHead`, `Timestamp`) · `rust-errors` · `rust/versions.md`
