---
name: rust-contract-types
description: "The domain crate's wire types: serde for contracts.md (snake_case, optional vs null, v: 1, tagged unions for SSE events, jobs and results), IDs, timestamps, unknown fields, and utoipa ToSchema and schemars JsonSchema. Use when writing or reviewing any type that crosses a boundary."
---

# Contract types (the `domain` crate)

The `domain` crate holds every shape that crosses a boundary: app ↔ API, API ↔ workers, worker ↔ VLM, ingestion ↔ database ([architecture 3, Rust crates](../../../docs/architecture/README.md#rust-crates)); its design is [backend domain](../../../docs/backend/domain.md), and every wire type is listed in [backend domain-types](../../../docs/backend/domain-types.md), whose [§1 conventions](../../../docs/backend/domain-types.md#1-conventions) this skill follows. **The project documents win.** The types must produce exactly the JSON in [contracts.md](../../../docs/architecture/contracts.md): [§1 conventions](../../../docs/architecture/contracts.md#1-conventions), [§2 errors](../../../docs/architecture/contracts.md#2-errors), [§4 SSE](../../../docs/architecture/contracts.md#4-scan-events-sse), [§5 worker API](../../../docs/architecture/contracts.md#5-worker-api-and-jobs), [§6 VLM output](../../../docs/architecture/contracts.md#6-vlm-structured-output), and [database 2](../../../docs/database/README.md#2-conventions) for how stored values relate. When this skill, a crate or habit disagrees with them, follow the documents and report the disagreement.

Versions: serde 1.0.229, serde_json 1.0.154, utoipa 6.0.0 (derive 6.0.1), schemars 1.2.3, chrono 0.4.45, ulid 3.0.0 ([versions.md](../rust/versions.md)). Every claim below is proved by a test block in this skill or its reference files:

| File | What it holds |
|---|---|
| [primitives.md](primitives.md) | `V1`, `Timestamp`, `Id<T>` and its aliases, `Slug<K>`, `LocalDate`, `Confidence` and its band, `BBox`, units |
| [unions.md](unions.md) | The SSE envelope, jobs, the worker's version check (the canonical `JobHead`), `complete`, worker events, `deny_unknown_fields` with `flatten`, the boolean-tagged depth result, chat stream events |
| [schema-attrs.md](schema-attrs.md) | Which serde attributes utoipa and schemars honour, optional vs nullable in each, and the proofs |

## Decisions at a glance

| Concern | Rule | Details |
|---|---|---|
| Field and enum names | Rust field names are the wire names; every enum has `#[serde(rename_all = "snake_case")]` | [Rule 1](#1-names-and-enums) |
| `field?: T` | `Option<T>` + `#[serde(default, skip_serializing_if = "Option::is_none")]` + `#[schema(nullable = false)]` | [Rule 2](#2-optional-and-nullable) |
| `field: T \| null` | `Option<T>` + `#[schema(required = true)]`, no `skip_serializing_if` | [Rule 2](#2-optional-and-nullable) |
| IDs | One generic `Id<T: Table>` over `ulid::Ulid` with a `PhantomData<fn() -> T>` marker and an alias per table (`ScanId = Id<table::Scan>`), a bare 26-character string; knowledge IDs are `Slug<K>` ([backend domain 4](../../../docs/backend/domain.md#4-ids-and-primitives)). A malformed ID is `404` in a path and `400` naming the field in a body or query (settled, [contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions)) | [primitives.md](primitives.md#2-why-each-type-looks-like-this) |
| Timestamps | `Timestamp(DateTime<Utc>)`: whole seconds with `Z` on the wire, full precision in memory and in the database; chrono alone emits fractions. Settled ([contracts 12, question 12](../../../docs/architecture/contracts.md#12-open-questions); [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format)). No `Timestamp::now()`: time comes from the `Clock` port. This is [backend domain 4](../../../docs/backend/domain.md#wire-primitives)'s newtype; [domain 1](../../../docs/backend/domain.md#1-purpose-and-boundaries) notes it as the one deliberate difference from the shared backend brief, which gave `chrono::DateTime<Utc>` | [primitives.md](primitives.md#2-why-each-type-looks-like-this) |
| Local dates | `LocalDate(NaiveDate)` → `"2026-10-20"`, `format: date` | [primitives.md](primitives.md#2-why-each-type-looks-like-this) |
| `v: 1` | The `V1` unit type; the worker reads `v` before the rest of a job | [Rule 3](#3-versioned-payloads) |
| Tagged unions | Pick the serde representation that matches the contract; never `untagged` for decoding | [Rule 4](#4-tagged-unions) |
| Unknown fields | `deny_unknown_fields` on app request bodies, the model's tool arguments and config; never on worker traffic. Settled for app requests ([contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions); [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)) | [Rule 5](#5-unknown-fields-and-unknown-values-by-direction) |
| Schemas | `ToSchema` on everything in the OpenAPI spec; `JsonSchema` only on the fixed shapes | [Rule 6](#6-openapi-and-json-schema) |
| A contract change | Rust type, contracts.md and (if stored) schema.surql in one pull request | [Rule 7](#7-the-source-of-truth) |

## 1. Names and enums

- Field names are `snake_case` on the wire, as Rust fields already are, so structs need no `rename_all`. Enums need `#[serde(rename_all = "snake_case")]`: `AwaitingAnswer` → `"awaiting_answer"` ([contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format); [serde `rename_all`](https://serde.rs/container-attrs.html#rename_all)).
- utoipa and schemars both honour `rename_all` on containers ([schema-attrs.md](schema-attrs.md#1-which-serde-attributes-each-generator-honours)).
- A Rust name that can't match the wire (a keyword such as `type`) gets `#[serde(rename = "...")]`, or a raw identifier (`r#where`), whose `r#` serde strips ([unions.md 2](unions.md#2-jobs-kind-and-payload-are-adjacent-tags), `Indicator`). Both generators honour field-level `rename` ([utoipa, partial serde attributes](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html#partial-serde-attributes-support); [schemars, supported serde attributes](https://graham.cool/schemars/deriving/attributes/#supported-serde-attributes)). Where the field is a discriminator, use a tag (`#[serde(tag = "type")]`, [Rule 4](#4-tagged-unions)).
- Unit suffixes stay in the field name and the type is a plain number: `duration_ms: u64`, `temp_c: f64`, never a `Duration` ([contracts 1](../../../docs/architecture/contracts.md#wire-format)).

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Pending,
    Queued,
    Analysing,
    AwaitingAnswer,
    Diagnosed,
    Planned,
    Failed,
}

#[test]
fn enum_values_are_snake_case() {
    assert_eq!(
        serde_json::to_string(&ScanStatus::AwaitingAnswer).unwrap(),
        r#""awaiting_answer""#
    );
    assert!(serde_json::from_str::<ScanStatus>(r#""AwaitingAnswer""#).is_err());
}
```

## 2. Optional and nullable

[Contracts 1, notation](../../../docs/architecture/contracts.md#notation): `?` is omitted when absent, never `null`; `| null` is always present. Both are `Option<T>` in Rust; the attributes make the difference, and utoipa needs telling, because it renders every `Option` as nullable *and* not required ([utoipa rules](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html#field-nullability-and-required-rules); proof and the schemars side in [schema-attrs.md 2](schema-attrs.md#2-optional-and-nullable)).

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The `species_confirmed` event body: `species_id: Slug | null, custom_name?: string`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesConfirmed {
    #[schema(required = true)]
    pub species_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub custom_name: Option<String>,
    pub rerun: bool,
}

#[test]
fn null_is_sent_and_absent_is_omitted() {
    let other = SpeciesConfirmed {
        species_id: None,
        custom_name: Some("Grandma's bean".to_owned()),
        rerun: true,
    };
    assert_eq!(
        serde_json::to_string(&other).unwrap(),
        r#"{"species_id":null,"custom_name":"Grandma's bean","rerun":true}"#
    );
    let known = SpeciesConfirmed {
        species_id: Some("tomato".to_owned()),
        custom_name: None,
        rerun: false,
    };
    assert_eq!(
        serde_json::to_string(&known).unwrap(),
        r#"{"species_id":"tomato","rerun":false}"#
    );
}
```

- Keep `default` on every `?` field. serde already reads a missing `Option` as `None`, but not once the field has `with` or `deserialize_with` ([schema-attrs.md 2](schema-attrs.md#2-optional-and-nullable)).
- On input, `null` reads as `None` too, so a `PATCH` with plain `Option` fields can't clear a value. No contract `PATCH` needs to today; the double-`Option` pattern for one that will is in [schema-attrs.md 2](schema-attrs.md#2-optional-and-nullable).
- Lists that may be empty are `Vec<T>` and always sent (`items: []`), unless the contract marks them `?`: only `?` fields are omitted ([contracts 1, notation](../../../docs/architecture/contracts.md#notation)).
- The database stores an absent value as `NONE`, never `NULL` ([database 2](../../../docs/database/README.md#2-conventions)). Store records are separate `SurrealValue` types ([database 6](../../../docs/database/README.md#6-rust-sdk)), so a wire `null` never reaches the database by accident.

## 3. Versioned payloads

Every SSE event, job payload and worker result carries `"v": 1` ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)).

- Type the field as the `V1` unit type, everywhere, never `u8` or `u64`: it always serializes as `1`, and any other value fails with ``invalid value: integer `2`, expected 1`` ([primitives.md](primitives.md#1-the-code), the canonical `V1` region; other blocks carry copies).
- **Worker side.** A worker that receives an unsupported `v` fails the job with `unsupported_version`, not retryable. It can only do that if it can still read the `job_id` and `lease_token`, so it decodes a small head (`v`, `job_id`, `lease_token`) first and the full `Job` only when `v` is 1. A `v: 1` job whose kind or payload this build can't read is failed the same way (settled, [contracts 12, questions 19 and 21](../../../docs/architecture/contracts.md#12-open-questions); [backend worker 16](../../../docs/backend/worker.md#16-decisions-on-the-contracts-open-questions)); a head that doesn't decode can't be reported at all, and is logged and left to the lease ([unions.md 3](unions.md#3-the-worker-checks-v-first), the one implementation, `JobHead`).
- **API side.** `WorkerEvents` and `CompleteJob` carry `V1`; another version fails the JSON extractor, which is `400 invalid_request`.
- REST bodies have no `v` and no version prefix; their changes are additive only.

## 4. Tagged unions

Choose the representation from the JSON the contract shows, then prove it with an exact-JSON test.

| Shape | Contract | Representation | Proof |
|---|---|---|---|
| SSE event | `ScanEventBase` fields, then `type`, then the event's fields, in one object ([§4 envelope](../../../docs/architecture/contracts.md#envelope)) | Struct with the common fields + `#[serde(flatten)] body`, body `#[serde(tag = "type")]` | [unions.md 1](unions.md#1-sse-envelope-flattened-common-fields-internally-tagged-body) (string comparison, key order included) |
| Job payload | `kind` and `payload` side by side ([§5 claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)) | `#[serde(tag = "kind", content = "payload")]`, flattened into `Job` | [unions.md 2](unions.md#2-jobs-kind-and-payload-are-adjacent-tags) |
| `complete` | `status`, with `result` for `done` and `error` for `failed` ([§5 complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete)) | `#[serde(tag = "status")]` enum, flattened | [unions.md 4](unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind) |
| Job result | Untagged union of four result types ([§5 results](../../../docs/architecture/contracts.md#results)) | `serde_json::Value`, decoded by the job's kind from the job record | [unions.md 4](unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind) (untagged accepts a result of the wrong kind, and its answer depends on declaration order) |
| Worker event | `type` per event; `PartialResult` tagged by `stage` ([§5 events](../../../docs/architecture/contracts.md#post-internaljobsidevents)) | `#[serde(tag = "type")]` with a `#[serde(other)]` unit variant; `#[serde(tag = "stage")]` | [unions.md 5](unions.md#5-worker-events-unknown-types-ignored-partialresult-tagged-by-stage) |
| Depth result | `{ stored: true } \| { stored: false, problem }`, tagged by a boolean ([§3 upload](../../../docs/architecture/contracts.md#post-scansidimages)) | A struct with `stored: bool` and an optional `problem`, private fields, a checked `try_from`, a hand-written `oneOf` schema. Not `#[serde(tag = "stored")]`, which writes `"stored":"true"` | [unions.md 7](unions.md#7-a-union-tagged-by-a-boolean-the-depth-result) |
| Chat stream event | `data` is `{ v: 1, … }` with no `type`; the name is on the SSE `event:` line ([§8 sending a message](../../../docs/architecture/contracts.md#sending-a-message)) | `#[serde(untagged)]` enum, **Serialize only**, flattened under a `v` wrapper, with `name()` for the `event:` line | [unions.md 8](unions.md#8-chat-stream-events-the-name-is-on-the-event-line-only) |
| `PlanWhen`, `GuidedTexture` | `kind` plus the variant's fields ([§7 action plan](../../../docs/architecture/contracts.md#actionplan), [§6 after decoding](../../../docs/architecture/contracts.md#after-decoding)) | `#[serde(tag = "kind")]` | same pattern as worker events |

- The SSE `event:` line must equal the body's `type`; give the body enum an `event_type()` and test that it matches serde's tag ([unions.md 1](unions.md#1-sse-envelope-flattened-common-fields-internally-tagged-body)).
- Don't use `#[serde(untagged)]` for anything the system decodes: serde takes the first variant that fits, and with unknown fields ignored a larger shape fits a smaller one ([serde, untagged](https://serde.rs/enum-representations.html#untagged)). Serializing with it is fine where the contract has no tag (chat events, [unions.md 8](unions.md#8-chat-stream-events-the-name-is-on-the-event-line-only)).

## 5. Unknown fields and unknown values, by direction

"Clients ignore fields they don't know" and changes are additive ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)). serde ignores unknown fields unless the type says `deny_unknown_fields` ([serde](https://serde.rs/container-attrs.html#deny_unknown_fields)).

| Direction | Examples | `deny_unknown_fields` | Unknown enum value |
|---|---|---|---|
| App → API request bodies | `CreateScanRequest`, `answers` | **Yes**: a misspelt `plant_ld` would otherwise become `None` silently. The answer is `400 invalid_request` with `details.field` naming the JSON path, which `api` reads with `serde_path_to_error`. Settled ([contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions); [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning), [2](../../../docs/architecture/contracts.md#2-errors); [backend api 8](../../../docs/backend/api.md#8-errors), [22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions)). A multipart upload's unknown form fields are skipped, not refused ([backend api 12](../../../docs/backend/api.md#12-uploads)) | Error (`400`) |
| Model → agent tool arguments | each tool's `Args` | **Yes**: a call with a field the tool doesn't know is refused, and the reason goes back to the model as an `is_error` tool result, not to the user ([contracts 8](../../../docs/architecture/contracts.md#8-chat-and-memory); [backend agent 12](../../../docs/backend/agent.md#typed-tools-erased-once)) | Refused to the model |
| Configuration files | worker or API config structs | **Yes**, this skill's choice: a misspelt setting fails start-up instead of silently taking its default | Error at start-up |
| API → app (responses, SSE) | `Scan`, `ScanEvent` | No (the app is the reader) | The app treats it as "other" |
| API → worker | `Job`, heartbeat and claim responses | No: a newer API may add fields; workers ignore unknown fields in jobs and responses ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)) | A `kind` this build can't read: the job fails with `unsupported_version`, not retryable ([contracts 12, question 21](../../../docs/architecture/contracts.md#12-open-questions)) |
| Worker → API | `WorkerEvents`, `CompleteJob`, results | **No**: older and newer workers run side by side; the API ignores unknown fields in worker requests (settled, [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)) | Unknown event `type`: `#[serde(other)] Unknown`, ignored. Unknown value inside a known event: error, because the API must store it ([unions.md 5](unions.md#5-worker-events-unknown-types-ignored-partialresult-tagged-by-stage)) |
| VLM output → worker | `VlmFindingsRaw` | **No** | Error: retry the call once, then fail the job with `retryable: true` ([contracts 6, after decoding](../../../docs/architecture/contracts.md#after-decoding)) |
| Seed files (schema only) | knowledge profiles | Yes, so the JSON Schema says `additionalProperties: false` and the ingestion check catches typos | Error |
| Plan writer output | plan steps | No on the type; `#[schemars(extend("additionalProperties" = false))]` on the schema ([schema-attrs.md 1](schema-attrs.md#1-which-serde-attributes-each-generator-honours)) | Error: the plan is sent back once ([contracts 7](../../../docs/architecture/contracts.md#actionplan)) |

- `deny_unknown_fields` doesn't work with `flatten`, on either side: a flattened tagged enum's own tag counts as unknown and valid input is rejected ([unions.md 6](unions.md#6-deny_unknown_fields-breaks-flatten)). Request bodies therefore never use `flatten`.
- With `deny_unknown_fields` on requests, a new optional request field must reach the API before an app sends it: **deploy the API first** ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning); [contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions)).
- Rust enums the API sends are closed; there is no `Other` variant to send. "Apps treat an unknown enum value as other" ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)) is for values from a newer API, not something the API sends.

## 6. OpenAPI and JSON Schema

- Derive `utoipa::ToSchema` on every type in a public endpoint or event. `utoipa` generates the spec and `openapi-typescript` the app's types from it ([contracts 1, source of truth](../../../docs/architecture/contracts.md#source-of-truth)).
- Derive `schemars::JsonSchema` only on the fixed shapes: the knowledge seed types ([contracts 10](../../../docs/architecture/contracts.md#10-knowledge-seed-format)) and the plan writer's output. The VLM's schema is built per job by the worker from the payload ([contracts 6, per-job schema](../../../docs/architecture/contracts.md#per-job-schema); [rust-workers](../rust-workers/SKILL.md)).
- The generators don't honour the same serde attributes. The ones that bite: utoipa ignores `transparent` on braced structs, `try_from`/`from`/`into` and `with`; schemars needs `with` to name a type; neither knows what `other` means. Check [the table](schema-attrs.md#1-which-serde-attributes-each-generator-honours) for any attribute beyond `rename_all`, `tag`, `content` and `skip_serializing_if`, and assert the generated schema in a test when you use one.
- Types utoipa can't derive (`Id<T>` and `Slug<K>`, the `V1` constant, `BBox`'s length) get `#[schema(value_type = ...)]` or a hand-written `PartialSchema` ([primitives.md](primitives.md#1-the-code); [`PartialSchema`](https://docs.rs/utoipa/6.0.0/utoipa/trait.PartialSchema.html)). A generic type's hand-written `ToSchema::name` is shared by every instantiation (`Id`, `Slug`), which is fine when they all have one schema.
- Doc comments on fields become `description` in both generators ([utoipa, `ToSchema`](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html); [schemars, doc comments](https://graham.cool/schemars/deriving/attributes/#doc)). Keep them to what the app team needs.

## 7. The source of truth

The `domain` crate's types are the contract, and contracts.md explains them ([AGENTS.md, sources of truth](../../../AGENTS.md#sources-of-truth)). A contract change, in one pull request:

1. changes the Rust type and its exact-JSON test;
2. updates contracts.md to match, bumps its status line, and adds a row to [contracts 11](../../../docs/architecture/contracts.md#11-changes-to-the-architecture-document) if the architecture document changes too;
3. changes [schema.surql](../../../backend/migrations/schema.surql) if the shape is stored;
4. regenerates `backend/openapi.json` (`cargo run -p api -- openapi > openapi.json` in `backend/`; `api`'s test `the_committed_openapi_document_matches_the_types` fails until it does) and, once `app/` exists, the app's types.

## Review checklist

- [ ] Every new or changed shape has every field its contract gives it, and a test comparing its serialized JSON to the contract, as a string where key order matters (SSE) ([Rule 4](#4-tagged-unions)).
- [ ] Enums: `rename_all = "snake_case"`. Structs: no renames unless a name must differ.
- [ ] `?` fields: `default` + `skip_serializing_if` + `#[schema(nullable = false)]`. `| null` fields: no `skip_serializing_if`, `#[schema(required = true)]`.
- [ ] IDs are `Id<T>` aliases (`ScanId`, …) or `Slug<K>`, never `String` or `RecordId`; timestamps are `Timestamp`, never a bare `DateTime<Utc>` on the wire, and never `Utc::now()` outside the `Clock` port; dates are `LocalDate`.
- [ ] `confidence_band` is computed from `confidence`, never read from a worker.
- [ ] Events, jobs and results carry `V1` (never `u8`); the worker checks `v` before decoding the job, with the `JobHead` region ([Rule 3](#3-versioned-payloads)).
- [ ] No `untagged` on anything decoded; job results decoded by the job's recorded kind.
- [ ] `deny_unknown_fields` on app request bodies, tool arguments and config only, never on worker traffic, and never together with `flatten`.
- [ ] Any serde attribute outside the safe four is checked against [the table](schema-attrs.md#1-which-serde-attributes-each-generator-honours), with a schema assertion.
- [ ] contracts.md (and schema.surql if stored) changed in the same pull request.

## Related skills

- [rust](../rust/SKILL.md): versions and the example checker.
- [rust-axum-api](../rust-axum-api/SKILL.md): extractors, mapping a serde rejection to `400 invalid_request`, the SSE endpoint, OpenAPI routes.
- [rust-errors](../rust-errors/SKILL.md): the `ErrorBody` codes and error enums.
- [rust-surrealdb](../rust-surrealdb/SKILL.md): store records and converting `RecordId` to and from `Id<T>` (`T::NAME`).
- [rust-workers](../rust-workers/SKILL.md): the per-job VLM schema, claiming and completing jobs.
- [rust-testing](../rust-testing/SKILL.md): snapshot tests of the JSON shapes.
- [rust-tracing](../rust-tracing/SKILL.md): which fields of these types may be logged.
