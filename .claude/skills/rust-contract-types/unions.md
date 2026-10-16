# Tagged unions: SSE events, jobs, results, worker events, depth, chat

Part of [rust-contract-types](SKILL.md). Each union below is shaped to produce exactly the JSON of its contract, and each block proves it. Every type named after a contract shape carries every field the contract gives it. The blocks carry exact copies of `V1` and `Timestamp` from [primitives.md](primitives.md#1-the-code) and use `String` for IDs and `f64` for `Confidence` so they compile on their own; real code uses the ID newtypes and `Confidence` from primitives.md. Shapes owned by another section of the contracts (`VlmFindings`, `Citation`, `PendingAction`, …) are `serde_json::Value` here.

## Contents

1. [SSE envelope: flattened common fields, internally tagged body](#1-sse-envelope-flattened-common-fields-internally-tagged-body)
2. [Jobs: `kind` and `payload` are adjacent tags](#2-jobs-kind-and-payload-are-adjacent-tags)
3. [The worker checks `v` first](#3-the-worker-checks-v-first)
4. [`complete`: tagged by `status`, result decoded by the job's kind](#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind)
5. [Worker events: unknown types ignored, `PartialResult` tagged by `stage`](#5-worker-events-unknown-types-ignored-partialresult-tagged-by-stage)
6. [`deny_unknown_fields` breaks `flatten`](#6-deny_unknown_fields-breaks-flatten)
7. [A union tagged by a boolean: the depth result](#7-a-union-tagged-by-a-boolean-the-depth-result)
8. [Chat stream events: the name is on the `event:` line only](#8-chat-stream-events-the-name-is-on-the-event-line-only)

## 1. SSE envelope: flattened common fields, internally tagged body

[Contracts 4](../../../docs/architecture/contracts.md#envelope) writes every event as one flat object: the `ScanEventBase` fields, then `type`, then the event's own fields (`{"v":1,"seq":17,"run":1,"scan_id":"01JAX...","ts":"…","type":"finding","observation":{…}}`). A struct with the common fields and a `#[serde(flatten)]` body that is internally tagged by `type` produces exactly that, in that key order. The body is a separate enum so the serialised writer can build a body and assign `seq` and `ts` later. The block shows three events of the catalogue; the others follow the same pattern.

`SpeciesCandidate.reference_image` is never stored in the event payload; it is filled in from `species_id` as the event is sent ([contracts 4, transport](../../../docs/architecture/contracts.md#transport)), so the stored body has it as `None` and the sender sets it.

Sources: [internally tagged](https://serde.rs/enum-representations.html#internally-tagged), [`flatten`](https://serde.rs/attr-flatten.html), [`skip_serializing_if`](https://serde.rs/field-attrs.html#skip_serializing_if).

```rust
use serde::Serialize;
use utoipa::ToSchema;

/// contracts 4 `Stage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    PlantCheck,
    Segmentation,
    Species,
    Disease,
    ImageEmbedding,
    Measurement,
    Context,
    Retrieve,
    VlmFindings,
    VlmIndicators,
    Guided,
    Verify,
    Scoring,
    Fusion,
    Plan,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Skipped {
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpeciesSource {
    Bioclip,
    Vlm,
    User,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ReferenceImage {
    /// Signed, short-lived.
    pub url: String,
    pub credit: String,
}

/// contracts 3 `SpeciesCandidate`.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SpeciesCandidate {
    /// `Slug | null`: always present, `null` when the name isn't in the knowledge base.
    #[schema(required = true)]
    pub species_id: Option<String>,
    pub name: String,
    pub score: f64,
    pub source: SpeciesSource,
    /// `reference_image?`: filled in as the event is sent, never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub reference_image: Option<ReferenceImage>,
}

/// `ScanEventBase` + body (contracts 4). Serialize only: the API writes events, the app reads them.
#[derive(Clone, Debug, Serialize)]
pub struct ScanEvent {
    pub v: V1,
    pub seq: u64,
    pub run: u32,
    pub scan_id: String,
    pub ts: Timestamp,
    #[serde(flatten)]
    pub body: ScanEventBody,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScanEventBody {
    StageStarted {
        stage: Stage,
    },
    StageCompleted {
        stage: Stage,
        duration_ms: u64,
        /// `skipped?`: omitted when absent, never `null`.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        skipped: Option<Skipped>,
    },
    SpeciesSuggested {
        candidates: Vec<SpeciesCandidate>,
        /// `Slug | null`: always present.
        #[schema(required = true)]
        working_species_id: Option<String>,
        needs_confirmation: bool,
    },
}

impl ScanEventBody {
    /// The SSE `event:` line, which must equal the JSON `type`.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::StageStarted { .. } => "stage_started",
            Self::StageCompleted { .. } => "stage_completed",
            Self::SpeciesSuggested { .. } => "species_suggested",
        }
    }
}

// copy of: rust-contract-types/primitives.md V1
/// The `"v": 1` on every SSE event, job payload and result (contracts 1, versioning).
/// Serializes as `1`; any other number fails to deserialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct V1;

impl serde::Serialize for V1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> serde::Deserialize<'de> for V1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if v == 1 {
            Ok(Self)
        } else {
            let unexpected = serde::de::Unexpected::Unsigned(v);
            Err(serde::de::Error::invalid_value(unexpected, &"1"))
        }
    }
}

impl utoipa::PartialSchema for V1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}

impl utoipa::ToSchema for V1 {}

impl schemars::JsonSchema for V1 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "V1".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "const": 1 })
    }
}
// end: V1

// copy of: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1).
/// Keeps full precision in memory; only the wire form is truncated. Whole seconds are
/// this skill's choice, not the contracts' (contracts 12, question 12).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use serde_json::json;

    fn event(body: ScanEventBody) -> ScanEvent {
        let ts = chrono::Utc
            .with_ymd_and_hms(2026, 10, 18, 14, 3, 22)
            .unwrap();
        ScanEvent {
            v: V1,
            seq: 5,
            run: 1,
            scan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".to_owned(),
            ts: Timestamp::from(ts + chrono::Duration::milliseconds(381)),
            body,
        }
    }

    #[test]
    fn exact_json_and_key_order() {
        let e = event(ScanEventBody::StageCompleted {
            stage: Stage::PlantCheck,
            duration_ms: 812,
            skipped: None,
        });
        // A string comparison, so key order and the omitted `skipped` are checked too.
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"v":1,"seq":5,"run":1,"scan_id":"01JAX3K9T6R2B8V0QZ4M7D1E5F","ts":"2026-10-18T14:03:22Z","type":"stage_completed","stage":"plant_check","duration_ms":812}"#
        );
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], e.body.event_type());
    }

    #[test]
    fn nullable_field_is_sent_as_null() {
        let e = event(ScanEventBody::SpeciesSuggested {
            candidates: vec![SpeciesCandidate {
                species_id: None,
                name: "Mystery vine".to_owned(),
                score: 0.4,
                source: SpeciesSource::Vlm,
                reference_image: None,
            }],
            working_species_id: None,
            needs_confirmation: true,
        });
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["working_species_id"], json!(null));
        assert_eq!(
            json["candidates"][0],
            json!({ "species_id": null, "name": "Mystery vine", "score": 0.4, "source": "vlm" })
        );
        assert_eq!(json["type"], e.body.event_type());
    }

    #[test]
    fn openapi_says_required_nullable_and_optional_non_null() {
        let schema =
            serde_json::to_value(<ScanEventBody as utoipa::PartialSchema>::schema()).unwrap();
        let completed = &schema["oneOf"][1];
        assert_eq!(
            completed["properties"]["type"],
            json!({ "type": "string", "enum": ["stage_completed"] })
        );
        assert_eq!(
            completed["required"],
            json!(["stage", "duration_ms", "type"])
        );
        assert_eq!(
            completed["properties"]["skipped"]["$ref"],
            json!("#/components/schemas/Skipped") // a plain $ref: no `null` branch
        );
        let suggested = &schema["oneOf"][2];
        assert_eq!(
            suggested["required"],
            json!([
                "candidates",
                "working_species_id",
                "needs_confirmation",
                "type"
            ])
        );
        assert_eq!(
            suggested["properties"]["working_species_id"]["type"],
            json!(["string", "null"])
        );
        let candidate =
            serde_json::to_value(<SpeciesCandidate as utoipa::PartialSchema>::schema()).unwrap();
        assert_eq!(
            candidate["required"],
            json!(["species_id", "name", "score", "source"])
        );
    }
}
```

Without the `#[schema(...)]` attributes, utoipa renders `working_species_id` as optional and `skipped` as nullable, because it treats every `Option` as both ([schema-attrs.md](schema-attrs.md#2-optional-and-nullable)).

## 2. Jobs: `kind` and `payload` are adjacent tags

`Job` carries `kind` and, as a sibling, `payload` ([contracts 5, claim](../../../docs/architecture/contracts.md#post-internaljobsclaim)). That is serde's adjacently tagged representation, flattened into the job, so `kind` and `payload` come last in the JSON rather than in the contract's order (JSON objects are unordered; nothing reads a job by position). A worker that gets a `kind` it doesn't know fails to decode the job.

**Not in the contracts yet:** an unknown `kind` is treated as a bug and the job is failed; contracts 5 doesn't say what `complete` reports ([contracts 12, question 21](../../../docs/architecture/contracts.md#12-open-questions)).

`lease_token` is a `String` here; in real code it is the redacting `LeaseToken` of [rust-tracing](../rust-tracing/json-logs.md#5-secrets-cant-be-logged-by-accident), so a `Debug` of the job can't print it.

Sources: [adjacently tagged](https://serde.rs/enum-representations.html#adjacently-tagged), [`tag` + `content`](https://serde.rs/container-attrs.html#tag--content), [contracts 5, payloads](../../../docs/architecture/contracts.md#payloads).

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    WholePlant,
    LeafCloseup,
    LeafUnderside,
    SoilSurface,
    FlowerFruit,
    SqueezeTest,
    JarTest,
}

/// contracts 5 `JobImage`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JobImage {
    pub image_id: String,
    pub kind: ImageKind,
    pub width: u32,
    pub height: u32,
    /// `GET /internal/jobs/{job_id}/images/{image_id}`.
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionStage {
    PlantCheck,
    Segmentation,
    Species,
    Disease,
    ImageEmbedding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionClass {
    Plant,
    Leaf,
    Soil,
    Surroundings,
}

/// `Region & { region_id: string; class: RegionClass }`: a region from an earlier run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PriorRegion {
    pub region_id: String,
    pub image_id: String,
    pub bbox: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_url: Option<String>,
    pub class: RegionClass,
}

/// contracts 5 `PerceptionPayload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerceptionPayload {
    pub images: Vec<JobImage>,
    pub stages: Vec<PerceptionStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub species_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_species_identification: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regions: Option<Vec<PriorRegion>>,
    pub label_set_version: String,
    pub classifier_labels_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorWhere {
    LowerLeaves,
    UpperLeaves,
    WholePlant,
    LeafUnderside,
    Stem,
    Fruit,
    Soil,
}

/// contracts 5 `Indicator`. serde strips the `r#` of a raw identifier: the key is `where`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Indicator {
    pub indicator_id: String,
    pub question: String,
    pub r#where: IndicatorWhere,
    pub image_kinds: Vec<ImageKind>,
}

/// contracts 5 `CandidatePrompt`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidatePrompt {
    pub problem_id: String,
    pub name: String,
    pub indicators: Vec<Indicator>,
}

/// `{ id: Slug; name: string }`, an allowed species or organism.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedOption {
    pub id: String,
    pub name: String,
}

/// `{ id: Slug | null; name: string }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkingSpecies {
    pub id: Option<String>,
    pub name: String,
}

/// contracts 5 `VlmPayload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VlmPayload {
    pub findings_images: Vec<JobImage>,
    pub indicator_images: Vec<JobImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_species: Option<WorkingSpecies>,
    pub species_options: Vec<NamedOption>,
    pub organism_options: Vec<NamedOption>,
    pub candidates: Vec<CandidatePrompt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyReason {
    OutsideCandidateSet,
    NewPhoto,
}

/// contracts 5 `VerifyPayload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifyPayload {
    pub reason: VerifyReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<String>,
    pub images: Vec<JobImage>,
    pub candidates: Vec<CandidatePrompt>,
}

/// contracts 5 `GuidedPayload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuidedPayload {
    pub image: JobImage,
}

/// `kind` and `payload` of a job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum JobSpec {
    Perception(PerceptionPayload),
    Vlm(VlmPayload),
    Verify(VerifyPayload),
    Guided(GuidedPayload),
}

/// contracts 5 `Job`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub v: V1,
    pub job_id: String,
    pub scan_id: String,
    /// Absent for guided jobs, which don't belong to a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<u32>,
    pub attempt: u32,
    pub lease_token: String,
    pub lease_until: Timestamp,
    pub max_runtime_s: u32,
    #[serde(flatten)]
    pub spec: JobSpec,
}

// copy of: rust-contract-types/primitives.md V1
/// The `"v": 1` on every SSE event, job payload and result (contracts 1, versioning).
/// Serializes as `1`; any other number fails to deserialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct V1;

impl serde::Serialize for V1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> serde::Deserialize<'de> for V1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if v == 1 {
            Ok(Self)
        } else {
            let unexpected = serde::de::Unexpected::Unsigned(v);
            Err(serde::de::Error::invalid_value(unexpected, &"1"))
        }
    }
}

impl utoipa::PartialSchema for V1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}

impl utoipa::ToSchema for V1 {}

impl schemars::JsonSchema for V1 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "V1".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "const": 1 })
    }
}
// end: V1

// copy of: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1).
/// Keeps full precision in memory; only the wire form is truncated. Whole seconds are
/// this skill's choice, not the contracts' (contracts 12, question 12).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use serde_json::json;

    fn image(kind: ImageKind) -> JobImage {
        JobImage {
            image_id: "01JAX6BBBBBBBBBBBBBBBBBBBB".to_owned(),
            kind,
            width: 3024,
            height: 4032,
            url: "/internal/jobs/01JAX5AAAAAAAAAAAAAAAAAAAA/images/01JAX6BBBBBBBBBBBBBBBBBBBB"
                .to_owned(),
        }
    }

    #[test]
    fn guided_job_json() {
        let lease_until = chrono::Utc
            .with_ymd_and_hms(2026, 10, 18, 14, 5, 22)
            .unwrap();
        let job = Job {
            v: V1,
            job_id: "01JAX5AAAAAAAAAAAAAAAAAAAA".to_owned(),
            scan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".to_owned(),
            run: None,
            attempt: 1,
            lease_token: "lt_9f2c41".to_owned(),
            lease_until: Timestamp::from(lease_until),
            max_runtime_s: 180,
            spec: JobSpec::Guided(GuidedPayload {
                image: image(ImageKind::JarTest),
            }),
        };
        let value = serde_json::to_value(&job).unwrap();
        assert_eq!(
            value,
            json!({
                "v": 1,
                "job_id": "01JAX5AAAAAAAAAAAAAAAAAAAA",
                "scan_id": "01JAX3K9T6R2B8V0QZ4M7D1E5F",
                "attempt": 1,
                "lease_token": "lt_9f2c41",
                "lease_until": "2026-10-18T14:05:22Z",
                "max_runtime_s": 180,
                "kind": "guided",
                "payload": { "image": {
                    "image_id": "01JAX6BBBBBBBBBBBBBBBBBBBB",
                    "kind": "jar_test",
                    "width": 3024,
                    "height": 4032,
                    "url": "/internal/jobs/01JAX5AAAAAAAAAAAAAAAAAAAA/images/01JAX6BBBBBBBBBBBBBBBBBBBB"
                } }
            })
        );
        assert_eq!(serde_json::from_value::<Job>(value).unwrap(), job);
    }

    #[test]
    fn perception_job_with_every_field_decodes() {
        let body = json!({
            "v": 1, "job_id": "J", "scan_id": "S", "run": 2, "attempt": 1,
            "lease_token": "t", "lease_until": "2026-10-18T14:05:22Z", "max_runtime_s": 120,
            "kind": "perception",
            "payload": {
                "images": [{ "image_id": "I", "kind": "leaf_closeup", "width": 2048,
                             "height": 1536, "url": "/internal/jobs/J/images/I" }],
                "stages": ["species", "disease"],
                "species_id": "tomato",
                "regions": [{ "region_id": "I.1", "image_id": "I", "bbox": [0.1, 0.2, 0.3, 0.4],
                              "class": "leaf" }],
                "label_set_version": "2026-10-17",
                "classifier_labels_version": "2026-10-17"
            }
        });
        let job: Job = serde_json::from_value(body.clone()).unwrap();
        let JobSpec::Perception(payload) = &job.spec else {
            panic!("expected a perception job, got {:?}", job.spec);
        };
        assert_eq!(payload.regions.as_ref().map(Vec::len), Some(1));
        assert_eq!(serde_json::to_value(&job).unwrap(), body);
    }

    #[test]
    fn unknown_fields_pass_unknown_kinds_fail() {
        let guided = || {
            json!({
                "v": 1, "job_id": "J", "scan_id": "S", "attempt": 2, "lease_token": "t",
                "lease_until": "2026-10-18T14:05:22Z", "max_runtime_s": 180, "kind": "guided",
                "payload": { "image": { "image_id": "I", "kind": "jar_test", "width": 1,
                                        "height": 1, "url": "u" } }
            })
        };
        // A field no contract has yet: a newer API may add one.
        let mut newer = guided();
        newer["priority"] = json!(5);
        assert!(serde_json::from_value::<Job>(newer).is_ok());
        let mut unknown = guided();
        unknown["kind"] = json!("soil_probe");
        let err = serde_json::from_value::<Job>(unknown).unwrap_err();
        assert!(err.to_string().starts_with("unknown variant `soil_probe`"));
        // Another version doesn't decode at all, which is why section 3 reads a head first.
        let mut v2 = guided();
        v2["v"] = json!(2);
        assert!(serde_json::from_value::<Job>(v2).is_err());
    }
}
```

## 3. The worker checks `v` first

"A worker that receives a `v` it doesn't support fails the job with the error code `unsupported_version`" ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)). To fail a job the worker needs its `job_id` and `lease_token`, and a `v: 2` job may not decode as today's `Job` at all. So the worker decodes a small head first, with no `deny_unknown_fields`, checks `v`, and only then decodes the whole job. The outcomes:

| Claim body | Result | What the worker does |
|---|---|---|
| A head with `v: 1`, and the job decodes | `Claimed::Job` | Runs it |
| A head with any other number in `v` | `Claimed::Unsupported(head)` | Completes the job as `failed` with `unsupported_version`, using `head.lease_token` ([rust-errors, jobs](../rust-errors/jobs.md)) |
| No numeric `v`, or no `job_id` or `lease_token` | `ClaimError::Head` | Can't complete the job, because it can't name it or prove its lease. Logs the error (without the body) and drops the job; the lease expires and the broker requeues it, and a job that keeps coming back runs out of attempts ([contracts 5, leases](../../../docs/architecture/contracts.md#leases-runs-and-attempts)). Not `unsupported_version`: there is no version to report |
| A head with `v: 1`, but the job doesn't decode (an unknown `kind`, a missing field) | `ClaimError::Job { head, .. }` | A bug: fails the job with `head` (contracts 12, question 21, above) |

This is the one implementation of the head check: [rust-errors jobs.md](../rust-errors/jobs.md) carries an exact copy of the `JobHead` region.

```rust
use serde::Deserialize;

// canonical: rust-contract-types/unions.md JobHead
/// The fields needed to fail a claimed job, readable from any version: no
/// `deny_unknown_fields`, nothing but what `complete` needs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct JobHead {
    pub v: u64,
    pub job_id: String,
    pub lease_token: String,
}

/// A `200` claim body, read head first (contracts 1, versioning).
#[derive(Debug)]
pub enum Claimed<J> {
    /// `v` is 1 and the whole job decoded.
    Job(J),
    /// `v` isn't 1: complete the job as `failed` with `unsupported_version`.
    Unsupported(JobHead),
}

/// A claim body the worker can't run. Neither message includes the body.
#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    /// No numeric `v`, `job_id` or `lease_token`: the job can't be completed. Log it and
    /// let the lease expire (contracts 5, leases).
    #[error("claim body has no readable job head")]
    Head(#[source] serde_json::Error),
    /// `v` is 1 but the job doesn't decode: a bug, failed with `head`.
    // Not in the contracts yet: what `complete` reports for it (contracts 12, question 21).
    #[error("job {} does not decode as v1", .head.job_id)]
    Job {
        head: JobHead,
        #[source]
        source: serde_json::Error,
    },
}

/// Decodes a claim body: the head, then `v`, then the whole job.
pub fn decode_claim<J: serde::de::DeserializeOwned>(body: &[u8]) -> Result<Claimed<J>, ClaimError> {
    let head: JobHead = serde_json::from_slice(body).map_err(ClaimError::Head)?;
    if head.v != 1 {
        return Ok(Claimed::Unsupported(head));
    }
    serde_json::from_slice(body)
        .map(Claimed::Job)
        .map_err(|source| ClaimError::Job { head, source })
}
// end: JobHead

/// A stand-in for section 2's `Job`: one kind, so an unknown one fails.
#[derive(Debug, Deserialize)]
pub struct Job {
    pub job_id: String,
    pub kind: Kind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Guided,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_job_is_failed_not_guessed() {
        let body = br#"{"v":2,"job_id":"J","lease_token":"t","task":{"new":"shape"}}"#;
        match decode_claim::<Job>(body).unwrap() {
            Claimed::Unsupported(head) => {
                assert_eq!(head.v, 2);
                assert_eq!(head.job_id, "J");
                assert_eq!(head.lease_token, "t");
            }
            Claimed::Job(job) => panic!("decoded a v2 job as v1: {job:?}"),
        }
    }

    #[test]
    fn v1_job_decodes() {
        let body = br#"{"v":1,"job_id":"J","lease_token":"t","kind":"guided","payload":{}}"#;
        assert!(matches!(
            decode_claim::<Job>(body).unwrap(),
            Claimed::Job(Job {
                kind: Kind::Guided,
                ..
            })
        ));
    }

    #[test]
    fn an_unreadable_head_is_not_unsupported_version() {
        for body in [
            br#"{"v":"2","job_id":"J","lease_token":"t"}"#.as_slice(),
            br#"{"job_id":"J","lease_token":"t"}"#,
            br#"{"v":1,"job_id":"J"}"#,
            b"not json",
        ] {
            let err = decode_claim::<Job>(body).unwrap_err();
            assert!(matches!(err, ClaimError::Head(_)), "{err}");
        }
    }

    #[test]
    fn a_v1_job_of_unknown_kind_keeps_its_head() {
        let body = br#"{"v":1,"job_id":"J","lease_token":"t","kind":"soil_probe","payload":{}}"#;
        let err = decode_claim::<Job>(body).unwrap_err();
        assert_eq!(err.to_string(), "job J does not decode as v1");
        let ClaimError::Job { head, .. } = err else {
            panic!("expected ClaimError::Job");
        };
        assert_eq!(head.lease_token, "t");
    }
}
```

The worker sends `unsupported_version` with `retryable: false`. **Not in the contracts yet:** contracts 1 doesn't say whether the error is retryable; `false` is this skill's choice, because a retry on the same worker can't succeed ([contracts 12, question 19](../../../docs/architecture/contracts.md#12-open-questions)).

The API side is the mirror: `WorkerEvents` and `CompleteJob` carry `v: V1`, so a worker sending another version gets `400 invalid_request` from the JSON extractor, which is correct for a body the API can't read ([contracts 2](../../../docs/architecture/contracts.md#2-errors)).

## 4. `complete`: tagged by `status`, result decoded by the job's kind

`CompleteJob` has `status: "done" | "failed"`, with `result` only for `done` and `error` only for `failed` ([contracts 5, complete](../../../docs/architecture/contracts.md#post-internaljobsidcomplete)). An enum internally tagged by `status`, flattened, makes the other combinations unrepresentable.

`result` itself is `PerceptionResult | VlmResult | VerifyResult | GuidedResult` with **no tag** ([contracts 5, results](../../../docs/architecture/contracts.md#results)). The API knows the job's kind from its own record, so it keeps `result` as `serde_json::Value` and decodes it as that kind's type. Not as `#[serde(untagged)]`, which tries the variants in order and takes the first that fits:

- **It accepts a result of the wrong kind.** A `verify` result sent for a `vlm` job fits `VerifyResult`, and untagged takes it whatever the order; it has no way to know which kind was asked for. Decoding by the recorded kind rejects it, because `VlmResult` needs `findings`.
- **Its answer depends on the declaration order and on the fields.** The same `vlm` body decodes as `Verify` or `Vlm` depending on which variant is listed first, and adding an optional field to one result type can change which variant a body lands in. Decoding by kind gives the same answer whatever is declared and whatever a newer worker adds.

The tests prove both. Sources: [untagged](https://serde.rs/enum-representations.html#untagged) ("Serde will try to match the data against each variant in order and the first one that deserializes successfully is the one returned"), [internally tagged](https://serde.rs/enum-representations.html#internally-tagged).

```rust
use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `CompleteJob.error`: `{ code, message, retryable }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompleteError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Done { result: Value },
    Failed { error: CompleteError },
}

/// contracts 5 `CompleteJob`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompleteJob {
    pub v: V1,
    #[serde(flatten)]
    pub outcome: Outcome,
    pub model_versions: BTreeMap<String, String>,
    pub timings_ms: BTreeMap<String, u64>,
}

/// The job's kind, from the job record.
#[derive(Clone, Copy, Debug)]
pub enum JobKind {
    Perception,
    Vlm,
    Verify,
    Guided,
}

/// contracts 5 `PerceptionResult`; the nested shapes are typed in section 5.
#[derive(Debug, Deserialize)]
pub struct PerceptionResult {
    #[serde(default)]
    pub plant_present: Option<bool>,
    pub regions: Vec<Value>,
    #[serde(default)]
    pub species: Option<Value>,
    #[serde(default)]
    pub disease: Option<Vec<Value>>,
    #[serde(default)]
    pub disease_skipped: Option<Value>,
    #[serde(default)]
    pub image_embeddings: Option<Vec<Value>>,
}

/// contracts 5 `VlmResult`: `VlmFindings` and `VlmIndicatorAnswers` are contracts 6's.
#[derive(Debug, Deserialize)]
pub struct VlmResult {
    pub findings: Value,
    pub indicators: Value,
}

/// contracts 5 `VerifyResult`.
#[derive(Debug, Deserialize)]
pub struct VerifyResult {
    pub indicators: Value,
}

/// contracts 5 `GuidedResult`: `GuidedTexture` is contracts 6's.
#[derive(Debug, Deserialize)]
pub struct GuidedResult {
    pub texture: Value,
}

#[derive(Debug)]
pub enum JobResult {
    Perception(PerceptionResult),
    Vlm(VlmResult),
    Verify(VerifyResult),
    Guided(GuidedResult),
}

fn decode<T: DeserializeOwned>(v: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(v)
}

/// The kind comes from the job record, never from the body.
pub fn decode_result(kind: JobKind, result: Value) -> Result<JobResult, serde_json::Error> {
    match kind {
        JobKind::Perception => decode(result).map(JobResult::Perception),
        JobKind::Vlm => decode(result).map(JobResult::Vlm),
        JobKind::Verify => decode(result).map(JobResult::Verify),
        JobKind::Guided => decode(result).map(JobResult::Guided),
    }
}

// copy of: rust-contract-types/primitives.md V1
/// The `"v": 1` on every SSE event, job payload and result (contracts 1, versioning).
/// Serializes as `1`; any other number fails to deserialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct V1;

impl serde::Serialize for V1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> serde::Deserialize<'de> for V1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if v == 1 {
            Ok(Self)
        } else {
            let unexpected = serde::de::Unexpected::Unsigned(v);
            Err(serde::de::Error::invalid_value(unexpected, &"1"))
        }
    }
}

impl utoipa::PartialSchema for V1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}

impl utoipa::ToSchema for V1 {}

impl schemars::JsonSchema for V1 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "V1".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "const": 1 })
    }
}
// end: V1

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum VlmFirst {
        Vlm(VlmResult),
        Verify(VerifyResult),
    }

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum VerifyFirst {
        Verify(VerifyResult),
        Vlm(VlmResult),
    }

    fn vlm_body() -> Value {
        json!({ "findings": { "notes": "two lesions" }, "indicators": { "answers": [] } })
    }

    fn verify_body() -> Value {
        json!({ "indicators": { "answers": [] } })
    }

    #[test]
    fn failed_complete_round_trips() {
        let body = json!({
            "v": 1,
            "status": "failed",
            "error": { "code": "unsupported_version", "message": "v2", "retryable": false },
            "model_versions": {},
            "timings_ms": {}
        });
        let c: CompleteJob = serde_json::from_value(body.clone()).unwrap();
        assert!(matches!(c.outcome, Outcome::Failed { .. }));
        assert_eq!(serde_json::to_value(&c).unwrap(), body);
        // `done` without `result` is rejected, so the invalid combination can't be stored.
        let bad = json!({ "v": 1, "status": "done", "model_versions": {}, "timings_ms": {} });
        assert!(serde_json::from_value::<CompleteJob>(bad).is_err());
        // Another version is rejected too: `400 invalid_request` at the extractor.
        let v2 = json!({ "v": 2, "status": "done", "result": {}, "model_versions": {}, "timings_ms": {} });
        assert!(serde_json::from_value::<CompleteJob>(v2).is_err());
    }

    #[test]
    fn a_result_of_the_wrong_kind_is_rejected() {
        // A verify result sent for a vlm job: untagged accepts it, whatever the order.
        assert!(matches!(
            serde_json::from_value::<VlmFirst>(verify_body()).unwrap(),
            VlmFirst::Verify(_)
        ));
        assert!(decode_result(JobKind::Vlm, verify_body()).is_err());
        assert!(decode_result(JobKind::Guided, vlm_body()).is_err());
    }

    #[test]
    fn untagged_depends_on_order_decoding_by_kind_does_not() {
        // The same vlm body: Verify when Verify is declared first, its findings dropped.
        assert!(matches!(
            serde_json::from_value::<VerifyFirst>(vlm_body()).unwrap(),
            VerifyFirst::Verify(_)
        ));
        assert!(matches!(
            serde_json::from_value::<VlmFirst>(vlm_body()).unwrap(),
            VlmFirst::Vlm(_)
        ));
        // By kind: a vlm result, also with a field a newer worker added.
        let mut newer = vlm_body();
        newer["latency_ms"] = json!(812);
        let Ok(JobResult::Vlm(result)) = decode_result(JobKind::Vlm, newer) else {
            panic!("a vlm job's result decodes as VlmResult");
        };
        assert_eq!(result.findings["notes"], "two lesions");
    }
}
```

## 5. Worker events: unknown types ignored, `PartialResult` tagged by `stage`

`WorkerEvents.events` is a list tagged by `type` ([contracts 5, events](../../../docs/architecture/contracts.md#post-internaljobsidevents)). A newer worker may send an event type this API doesn't know yet. The events are progress, not results, so the API ignores an unknown one rather than rejecting the batch, in the spirit of "apps ignore unknown event types" ([contracts 1, versioning](../../../docs/architecture/contracts.md#versioning)). serde's documented tool is `#[serde(other)]` on a unit variant of an internally tagged enum. `skip_serializing` on that variant makes serializing it an error, and also hides it from utoipa ([schema-attrs.md](schema-attrs.md#1-which-serde-attributes-each-generator-honours)).

`PartialResult` is tagged by its own `stage` field, with one variant per stage the contract lists; its `species`, `disease` and `regions` are the shapes of `PerceptionResult` ([contracts 5, results](../../../docs/architecture/contracts.md#results)). The event repeats the stage outside it, so the API checks the two agree.

Sources: [`other`](https://serde.rs/variant-attrs.html#other), [`skip_serializing`](https://serde.rs/variant-attrs.html#skip_serializing).

```rust
use serde::{Deserialize, Serialize};

/// contracts 4 `Stage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    PlantCheck,
    Segmentation,
    Species,
    Disease,
    ImageEmbedding,
    Measurement,
    Context,
    Retrieve,
    VlmFindings,
    VlmIndicators,
    Guided,
    Verify,
    Scoring,
    Fusion,
    Plan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skipped {
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionClass {
    Plant,
    Leaf,
    Soil,
    Surroundings,
}

/// contracts 5 `RleMask`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RleMask {
    /// Height, width of the stored image.
    pub size: [u32; 2],
    pub counts: String,
}

/// An element of `PerceptionResult.regions`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerceptionRegion {
    pub region_id: String,
    pub image_id: String,
    pub class: RegionClass,
    pub bbox: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<RleMask>,
    pub confidence: f64,
}

/// An element of `PerceptionResult.species.candidates`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeciesScore {
    /// `Slug | null`: always sent.
    pub species_id: Option<String>,
    pub label: String,
    pub score: f64,
}

/// `PerceptionResult.species`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeciesScores {
    pub candidates: Vec<SpeciesScore>,
}

/// `{ problem_id: Slug | "healthy"; confidence }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiseaseTop {
    pub problem_id: String,
    pub confidence: f64,
}

/// An element of `PerceptionResult.disease`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiseaseResult {
    pub image_id: String,
    /// Absent when the classifier ran on the whole close-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    pub top: Vec<DiseaseTop>,
}

/// contracts 5 `PartialResult`. `VlmFindings` is contracts 6's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum PartialResult {
    PlantCheck { plant_present: bool },
    Segmentation { regions: Vec<PerceptionRegion> },
    Species { species: SpeciesScores },
    Disease { disease: Vec<DiseaseResult> },
    VlmFindings { findings: serde_json::Value },
}

impl PartialResult {
    pub fn stage(&self) -> Stage {
        match self {
            Self::PlantCheck { .. } => Stage::PlantCheck,
            Self::Segmentation { .. } => Stage::Segmentation,
            Self::Species { .. } => Stage::Species,
            Self::Disease { .. } => Stage::Disease,
            Self::VlmFindings { .. } => Stage::VlmFindings,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerEvent {
    StageStarted {
        stage: Stage,
    },
    StageCompleted {
        stage: Stage,
        duration_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skipped: Option<Skipped>,
    },
    PartialResult {
        stage: Stage,
        result: PartialResult,
    },
    /// An event type from a newer worker. Ignored; never sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// contracts 5 `WorkerEvents`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerEvents {
    pub v: V1,
    pub events: Vec<WorkerEvent>,
}

// copy of: rust-contract-types/primitives.md V1
/// The `"v": 1` on every SSE event, job payload and result (contracts 1, versioning).
/// Serializes as `1`; any other number fails to deserialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct V1;

impl serde::Serialize for V1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> serde::Deserialize<'de> for V1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if v == 1 {
            Ok(Self)
        } else {
            let unexpected = serde::de::Unexpected::Unsigned(v);
            Err(serde::de::Error::invalid_value(unexpected, &"1"))
        }
    }
}

impl utoipa::PartialSchema for V1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}

impl utoipa::ToSchema for V1 {}

impl schemars::JsonSchema for V1 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "V1".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "const": 1 })
    }
}
// end: V1

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_event_type_is_skipped_not_fatal() {
        let body = json!({ "v": 1, "events": [
            { "type": "stage_started", "stage": "plant_check" },
            { "type": "gpu_temperature", "celsius": 71 },
            { "type": "partial_result", "stage": "plant_check",
              "result": { "stage": "plant_check", "plant_present": true } },
            { "type": "stage_completed", "stage": "disease", "duration_ms": 3,
              "skipped": { "reason": "species_not_supported" } }
        ] });
        let batch: WorkerEvents = serde_json::from_value(body).unwrap();
        assert_eq!(batch.events[1], WorkerEvent::Unknown);
        match &batch.events[2] {
            WorkerEvent::PartialResult { stage, result } => assert_eq!(*stage, result.stage()),
            other => panic!("expected a partial result, got {other:?}"),
        }
        assert!(matches!(
            &batch.events[3],
            WorkerEvent::StageCompleted { skipped: Some(s), .. } if s.reason == "species_not_supported"
        ));
        assert!(serde_json::to_string(&WorkerEvent::Unknown).is_err());
    }

    #[test]
    fn species_and_disease_partial_results_decode() {
        let species = json!({ "stage": "species", "species": { "candidates": [
            { "species_id": null, "label": "Solanum sp.", "score": 0.61 }
        ] } });
        let species: PartialResult = serde_json::from_value(species).unwrap();
        assert_eq!(species.stage(), Stage::Species);
        // `region_id` absent: the classifier ran on the whole close-up.
        let json = json!({ "stage": "disease", "disease": [
            { "image_id": "I", "top": [{ "problem_id": "healthy", "confidence": 0.9 }] }
        ] });
        let disease: PartialResult = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(disease.stage(), Stage::Disease);
        assert_eq!(serde_json::to_value(&disease).unwrap(), json);
    }

    #[test]
    fn unknown_stage_and_other_versions_are_still_errors() {
        let body = json!({ "type": "stage_started", "stage": "thermal_imaging" });
        assert!(serde_json::from_value::<WorkerEvent>(body).is_err());
        let v2 = json!({ "v": 2, "events": [] });
        assert!(serde_json::from_value::<WorkerEvents>(v2).is_err());
    }
}
```

The third test is deliberate: `other` only covers the event's `type`. An unknown value *inside* a known event, such as a new `Stage`, still fails, because the API must store it and the schema's literal unions would reject it anyway ([database 2](../../../docs/database/README.md#2-conventions)). Add enum values on the API first, then on the workers.

## 6. `deny_unknown_fields` breaks `flatten`

serde's docs: "this attribute is not supported in combination with flatten, neither on the outer struct nor on the flattened field" ([container attributes](https://serde.rs/container-attrs.html#deny_unknown_fields)). With serde 1.0.229 the failure is total: the flattened enum's own tag counts as an unknown field, so even a valid body is rejected.

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Body {
    StageStarted { stage: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strict {
    pub seq: u64,
    #[serde(flatten)]
    pub body: Body,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_is_rejected() {
        let err = serde_json::from_str::<Strict>(r#"{"seq":1,"type":"stage_started","stage":"x"}"#)
            .unwrap_err();
        assert!(err.to_string().starts_with("unknown field `type`"));
    }
}
```

So the inbound types that use `deny_unknown_fields` (app requests, config) never use `flatten`; the types that use `flatten` (events, jobs, `complete`) are the ones that must tolerate unknown fields anyway.

## 7. A union tagged by a boolean: the depth result

`UploadImageResponse.depth` is `{ stored: true } | { stored: false, problem }` ([contracts 3, `POST /scans/{id}/images`](../../../docs/architecture/contracts.md#post-scansidimages)). The tag is a JSON boolean. serde's `tag` writes the variant's name, which is always a string, so `#[serde(tag = "stored")]` with variants renamed `"true"` and `"false"` sends `"stored":"true"` and can't read `"stored":true` (the trap test below). The right representation is a struct with `stored: bool` and an optional `problem`, private fields so only the two valid combinations can be built, and a checked `Deserialize`. utoipa would render the struct as one object with an optional `problem`, so its schema is written by hand as the two-branch `oneOf`, which `openapi-typescript` turns into the contract's union.

Sources: [internally tagged](https://serde.rs/enum-representations.html#internally-tagged) (the tag is the variant name), [`try_from`](https://serde.rs/container-attrs.html#try_from), [`PartialSchema`](https://docs.rs/utoipa/6.0.0/utoipa/trait.PartialSchema.html).

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Schema, Type};
use utoipa::openapi::{Ref, RefOr};

/// Why the depth was dropped (contracts 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DepthProblem {
    AspectMismatch,
    TooSparse,
    InvalidMeta,
    UnsupportedFormat,
    TooLarge,
    NotForThisKind,
    StorageError,
}

/// `{ stored: true } | { stored: false, problem }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DepthWire")]
pub struct DepthResult {
    stored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<DepthProblem>,
}

impl DepthResult {
    pub const STORED: Self = Self {
        stored: true,
        problem: None,
    };

    pub const fn dropped(problem: DepthProblem) -> Self {
        Self {
            stored: false,
            problem: Some(problem),
        }
    }

    pub const fn problem(self) -> Option<DepthProblem> {
        self.problem
    }
}

/// The wire shape as read, before the combination is checked.
#[derive(Deserialize)]
struct DepthWire {
    stored: bool,
    #[serde(default)]
    problem: Option<DepthProblem>,
}

impl TryFrom<DepthWire> for DepthResult {
    type Error = &'static str;

    fn try_from(wire: DepthWire) -> Result<Self, Self::Error> {
        match (wire.stored, wire.problem) {
            (true, None) => Ok(Self::STORED),
            (false, Some(problem)) => Ok(Self::dropped(problem)),
            (true, Some(_)) => Err("a stored depth map has no problem"),
            (false, None) => Err("a dropped depth map needs a problem"),
        }
    }
}

impl utoipa::PartialSchema for DepthResult {
    fn schema() -> RefOr<Schema> {
        let stored = |value: bool| {
            ObjectBuilder::new()
                .schema_type(Type::Boolean)
                .enum_values(Some([value]))
        };
        let kept = ObjectBuilder::new()
            .property("stored", stored(true))
            .required("stored");
        let dropped = ObjectBuilder::new()
            .property("stored", stored(false))
            .property("problem", Ref::from_schema_name("DepthProblem"))
            .required("stored")
            .required("problem");
        OneOfBuilder::new().item(kept).item(dropped).into()
    }
}

impl ToSchema for DepthResult {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        let problem = <DepthProblem as utoipa::PartialSchema>::schema();
        schemas.push((DepthProblem::name().into_owned(), problem));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_json_both_directions() {
        let stored = r#"{"stored":true}"#;
        let dropped = r#"{"stored":false,"problem":"too_sparse"}"#;
        assert_eq!(serde_json::to_string(&DepthResult::STORED).unwrap(), stored);
        assert_eq!(
            serde_json::to_string(&DepthResult::dropped(DepthProblem::TooSparse)).unwrap(),
            dropped
        );
        assert_eq!(
            serde_json::from_str::<DepthResult>(stored).unwrap(),
            DepthResult::STORED
        );
        assert_eq!(
            serde_json::from_str::<DepthResult>(dropped)
                .unwrap()
                .problem(),
            Some(DepthProblem::TooSparse)
        );
        assert!(serde_json::from_str::<DepthResult>(r#"{"stored":false}"#).is_err());
        assert!(
            serde_json::from_str::<DepthResult>(r#"{"stored":true,"problem":"too_large"}"#)
                .is_err()
        );
    }

    #[test]
    fn a_serde_tag_is_a_string_not_a_boolean() {
        #[derive(Debug, Serialize, Deserialize)]
        #[serde(tag = "stored")]
        enum Tagged {
            #[serde(rename = "true")]
            Stored,
            #[serde(rename = "false")]
            Dropped { problem: DepthProblem },
        }
        assert_eq!(
            serde_json::to_string(&Tagged::Stored).unwrap(),
            r#"{"stored":"true"}"#
        );
        assert!(serde_json::from_str::<Tagged>(r#"{"stored":true}"#).is_err());
    }

    #[test]
    fn schema_is_the_two_branch_union() {
        let schema =
            serde_json::to_value(<DepthResult as utoipa::PartialSchema>::schema()).unwrap();
        assert_eq!(
            schema["oneOf"][0]["properties"]["stored"],
            json!({ "type": "boolean", "enum": [true] })
        );
        assert_eq!(schema["oneOf"][1]["required"], json!(["stored", "problem"]));
        assert_eq!(
            schema["oneOf"][1]["properties"]["problem"],
            json!({ "$ref": "#/components/schemas/DepthProblem" })
        );
    }
}
```

## 8. Chat stream events: the name is on the `event:` line only

The chat stream's events ([contracts 8, sending a message](../../../docs/architecture/contracts.md#sending-a-message)) differ from the scan events: each `data` is `{ v: 1, …the event's fields }` with **no `type` field**; the event's name is only on the SSE `event:` line, and there is no `id:` line. An internally tagged enum would add a `"type"` key the contract doesn't have. So the body enum is `#[serde(untagged)]`, **for `Serialize` only**: untagged writes a variant's fields without a name, which is exactly the shape, and never decodes anything ([Rule 4](SKILL.md#4-tagged-unions)). A wrapper adds `v` in front, and `name()` gives the `event:` line. The app reads the events, choosing the shape by the `event:` name.

Sources: [untagged](https://serde.rs/enum-representations.html#untagged), [`flatten`](https://serde.rs/attr-flatten.html), [axum `sse::Event`](https://docs.rs/axum/0.8.9/axum/response/sse/struct.Event.html).

```rust
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    DiagnosisQuestion,
    CareQuestion,
    TaskChange,
    SafetySensitive,
    OffTopic,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Usage {
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// contracts 8 chat stream events, one variant per row. `Citation`, `PendingAction` and
/// `Memory` are contracts 8's shapes; `photo_kind` is an `ImageKind`, `code` an `ErrorCode`.
/// Serialize only: an untagged enum is never decoded (rust-contract-types, rule 4).
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ChatEvent {
    MessageStarted {
        message_id: String,
        route: Route,
    },
    TextDelta {
        text: String,
    },
    ToolStarted {
        call_id: String,
        tool: String,
        label: String,
    },
    ToolFinished {
        call_id: String,
        ok: bool,
    },
    Citation {
        citation: Value,
    },
    PendingAction {
        pending_action: Value,
    },
    MemorySaved {
        memory: Value,
    },
    RescanRequested {
        plant_id: String,
        photo_kind: String,
        instructions: String,
    },
    MessageCompleted {
        message_id: String,
        usage: Usage,
    },
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

/// `data`: `v` first, then the event's fields.
#[derive(Serialize)]
struct ChatData<'a> {
    v: V1,
    #[serde(flatten)]
    event: &'a ChatEvent,
}

impl ChatEvent {
    /// The SSE `event:` line.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::MessageStarted { .. } => "message_started",
            Self::TextDelta { .. } => "text_delta",
            Self::ToolStarted { .. } => "tool_started",
            Self::ToolFinished { .. } => "tool_finished",
            Self::Citation { .. } => "citation",
            Self::PendingAction { .. } => "pending_action",
            Self::MemorySaved { .. } => "memory_saved",
            Self::RescanRequested { .. } => "rescan_requested",
            Self::MessageCompleted { .. } => "message_completed",
            Self::Error { .. } => "error",
        }
    }

    /// The SSE `data:` line.
    pub fn data(&self) -> serde_json::Result<String> {
        serde_json::to_string(&ChatData { v: V1, event: self })
    }

    /// No `id`: the chat stream isn't resumable (contracts 8).
    pub fn to_sse(&self) -> serde_json::Result<axum::response::sse::Event> {
        Ok(axum::response::sse::Event::default()
            .event(self.name())
            .data(self.data()?))
    }
}

// copy of: rust-contract-types/primitives.md V1
/// The `"v": 1` on every SSE event, job payload and result (contracts 1, versioning).
/// Serializes as `1`; any other number fails to deserialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct V1;

impl serde::Serialize for V1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> serde::Deserialize<'de> for V1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if v == 1 {
            Ok(Self)
        } else {
            let unexpected = serde::de::Unexpected::Unsigned(v);
            Err(serde::de::Error::invalid_value(unexpected, &"1"))
        }
    }
}

impl utoipa::PartialSchema for V1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}

impl utoipa::ToSchema for V1 {}

impl schemars::JsonSchema for V1 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "V1".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "const": 1 })
    }
}
// end: V1

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn data_is_v_then_the_fields_without_a_type() {
        let started = ChatEvent::MessageStarted {
            message_id: "01JAX7MMMMMMMMMMMMMMMMMMMM".to_owned(),
            route: Route::CareQuestion,
        };
        assert_eq!(started.name(), "message_started");
        assert_eq!(
            started.data().unwrap(),
            r#"{"v":1,"message_id":"01JAX7MMMMMMMMMMMMMMMMMMMM","route":"care_question"}"#
        );
        let error = ChatEvent::Error {
            code: "upstream_unavailable".to_owned(),
            message: "A service we depend on is unavailable. Try again shortly.".to_owned(),
            retryable: true,
        };
        assert_eq!(
            serde_json::from_str::<Value>(&error.data().unwrap()).unwrap(),
            json!({
                "v": 1,
                "code": "upstream_unavailable",
                "message": "A service we depend on is unavailable. Try again shortly.",
                "retryable": true
            })
        );
        let delta = ChatEvent::TextDelta {
            text: "Water at the base.".to_owned(),
        };
        let data: Value = serde_json::from_str(&delta.data().unwrap()).unwrap();
        assert!(data.get("type").is_none());
        assert!(delta.to_sse().is_ok());
    }

    #[test]
    fn an_internal_tag_would_add_a_field() {
        #[derive(Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Tagged {
            TextDelta { text: String },
        }
        let text = "Water at the base.".to_owned();
        assert_eq!(
            serde_json::to_string(&Tagged::TextDelta { text }).unwrap(),
            r#"{"type":"text_delta","text":"Water at the base."}"#
        );
    }
}
```
