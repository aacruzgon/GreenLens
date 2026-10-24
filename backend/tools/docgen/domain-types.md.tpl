# PDM Backend: `domain` contract types

> **Status:** Draft v0.3.8, 2026-10-24. v0.3.8: `DamageRaw.trace`, the animal or the trace it left, written before the pattern ([contracts 6](../architecture/contracts.md#findings-call); vision dev-roadmap P11). v0.3.7: answer blocks ([contracts 8](../architecture/contracts.md#answer-blocks)): `AnswerBlock` (`call_id` and a `BlockBody` tagged by `kind`), `AnswerBlockKind`, `BlockPlant`, `FrostPlant`, the `answer_block` event (`AnswerBlockEvent`), `Message.blocks` (omitted when empty) and `ToolName::ShowBlock`; `Lookalike` derives `ToSchema`, since a `candidates` block carries it. v0.3.6: `SpeciesSeed.frost`, a species' frost tenderness (`tender` or `hardy`), top level and absent when the sources don't say (backend S5 work 9). v0.3.5: `PlanStepDraft.farm_citations`, the farm records a plan-writer step cites, optional and omitted when empty, strings that validation parses so a bad id refuses the step and not the answer (EON E5.4). v0.3.4: the farm connectors' wire types ([connectors](../connectors/README.md), roadmap E1.1): `Connector` and its enums, the connect requests, `FarmCitation`, `FarmRecordId` and `ConnectorId`; `PlanStep.farm_citations`, `ActionPlan.farm_records` and `Message.farm_citations`, absent when empty; the `search_farm_records` tool. v0.3.3: memories gain a `topic`, and the beliefs mirrored into Agent Memory get their wire types (`Belief`, `PlantBeliefs`, `BeliefHit`, `MemorySearch`). v0.3.2: the schema links point to `backend/migrations/schema.surql`. v0.3.1: `JobErrorCode`'s comments follow contracts 5: `max_runtime_exceeded` is settled (question 18), and `unsupported_version` and `model_error` carry the contract's full meaning. v0.3: `PastWeather` gains `longest_humid_run_days` (contracts v0.10, section 7), the longest run of humid days in the past week, so a past humid stretch is detectable. v0.2: follows contracts v0.9 and v0.10 where they touch shapes: `JobError.code` is a `JobErrorCode` (`max_runtime_exceeded` and `unknown_label_version` included), the `model_versions` keys are `ModelVersionKey` constants, `Invite.created_by` is optional, worker payloads never carry a mask URL and `complete` results are read by the job's kind (both as v0.1 had them); `BoxFormat`'s comments name the models of each format. v0.1: every shape of [contracts.md](../architecture/contracts.md) v0.8.2 as compiled Rust, section by section, with the serde and schema decisions explained and a TypeScript → Rust map.

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

@@items wire.rs Timestamp LocalDate Confidence ConfidenceBand Rated@@

`Rated` is flattened into the record that carries a confidence (`Observation`). Its `Serialize` writes both fields from the number; its `Deserialize` reads the number and recomputes the band, so a stored band that disagrees can never come back.

### Boxes, regions and pages

@@items wire.rs BBox Region Page PageRequest@@

- `BBox` is `[x, y, w, h]` on the wire (`#[serde(into = "[f64; 4]")]`) and checks every value and `x + w ≤ 1`, `y + h ≤ 1` when decoded. utoipa ignores `into`, so the schema is written by hand: an array of exactly 4 numbers from 0 to 1.
- `Region`'s mask is a flattened slot: `mask_id` in `Region<Stored>`, `mask_url` in `Region<Signed>` ([domain 5](domain.md#5-signed-url-typing)). The doc comment on the slot is a plain `//` comment so it stays out of the OpenAPI description.
- `PageRequest` is the `?cursor=&limit=` query (`utoipa::IntoParams`); `limit()` applies the default 20 and the maximum 100.

### Runs, sequence numbers, literals and roles

@@items wire.rs Run EventSeq V1 BoolLit True False Role UserRef@@

- `Run` and `EventSeq` are newtypes so a run can't be compared with a sequence number. Both serialise as plain numbers (`#[serde(transparent)]`).
- `V1` is what every versioned payload carries. A `v` of 2 fails to decode with `unsupported_version: v = 2`; workers read [`JobHead`](#5-worker-api-and-jobs) first so they can still fail the job.
- `BoolLit<B>` is a boolean literal type: `passed: true` in `QualityReport`, and the tag of `UploadImageResponse.depth`.
- `Role` derives `Ord` in rank order, `viewer < reviewer < editor < owner`, which is not the order contracts lists the values in.

### Identifiers

@@items id.rs Table Id ULID_PATTERN@@

`Id<T>` serialises as the bare ULID and parses case-insensitively, rejecting a `scan:` prefix. The markers and aliases:

@@items id.rs mod:table@@

@@items id.rs UserId GardenId InviteId PlantId GardenMapId ScanId ImageId MaskId ObservationId QuestionId GuidedCheckId DiagnosisId FeedbackId PlanId PlanTriggerId TaskId ConversationId ChatImageId MessageId PendingActionId MemoryId JobId ConnectorId@@

Knowledge ids are slugs, typed by table. `"healthy"` is reserved: it can't be a problem slug, because it takes a problem id's place in `problem_id: Slug | "healthy"`.

@@items id.rs SlugKind Slug SLUG_PATTERN ProblemOrHealthy@@

Compound ids check both parts. A farm record's key is `farm_record:[connector, vendor_id, n]`, and its wire form puts the vendor id last, because a Slack `ts` holds a dot:

@@items id.rs IndicatorId RegionId ChunkId FarmRecordId@@

The opaque strings (`WorkerId`, `LeaseToken`, `AuthId`, `InviteCode`, `PushToken`, `Cursor`, `StepId`, `DeviceVerifier`, `DeviceHash`) come from one macro with a check per type, and `IdempotencyKey` wraps a `uuid::Uuid`:

@@items id.rs IdempotencyKey@@

### Image URLs

A `url` the API returns is a `SignedUrl`; the stored form of the same record has no URL at all ([domain 5](domain.md#5-signed-url-typing)).

@@items url.rs SignedUrl NoUrl@@

---

## 2. Errors

[Contracts 2](../architecture/contracts.md#2-errors). The HTTP status and default `retryable` of each code are data on `ErrorCode` ([domain 6](domain.md#6-errors)).

@@items error.rs ErrorCode QualityHint QualityProblem InvalidStateHint Budget@@

`details` depends on `code`, so `code` and `details` are one adjacently tagged enum, flattened into the error object. Codes without details serialise with no `details` key.

@@items error.rs InvalidRequestDetails InvalidStateDetails ErrorKind ErrorObject ErrorBody@@

- `retry_after_s` is a sibling of `code`, not part of `details`, as in the contract.
- `invalid_request` carries `plant_ids` only for the map upload's `unknown_plant` ([contracts 9, garden map](../architecture/contracts.md#garden-map)).
- `invalid_state.status` is a string: the record may be a scan, question, plan, task, invite, pending action or map, each with its own states.

The contract's example round-trips exactly:

@@test error.rs error_body_matches_the_contract_example@@

---

## 3. Scan REST API

[Contracts 3](../architecture/contracts.md#3-scan-rest-api).

### Status, kinds and images

@@items scan.rs ScanStatus ImageKind DepthInfo ScanImage@@

`ScanImage.url` is `U::Url`: `SignedUrl` on the wire, `NoUrl` (never serialised) when stored. `#[schema(required = true)]` keeps it required in the OpenAPI document even though `skip_serializing_if` is set for the stored form.

### Species and plants

@@items scan.rs SpeciesRef CandidateSource ReferenceImage SpeciesCandidate SpeciesStatus PlantStatus PlantPosition LatestScan Plant@@

- `SpeciesCandidate.reference_image` is computed from `species_id` each time a candidate is read or sent, never stored ([contracts 9, species](../architecture/contracts.md#species)). A stored candidate has `None`, and `Sign` fills it from the in-memory species photo index:

@@impl scan.rs Sign for SpeciesCandidate<Stored>@@

- `LatestScan` carries both `thumbnail_image_id` (stored and sent) and `thumbnail_url` (signed, from that id).

### Failures, questions and guided checks

@@items scan.rs ScanFailureCode ScanFailure Answer QuestionKind QuestionOption QuestionStatus Question GuidedKind GuidedCheckStatus GuidedCheck@@

- `Answer` is the `"yes" | "no" | "unsure"` used by options, indicator answers and evidence.
- `QuestionStatus` is what the app sees (`open`, `answering`). The stored lifecycle adds `answered`, `superseded` and `dropped` ([`rules::questions::QuestionState`](domain.md#7-rules)).

### Scan and scan detail

@@items scan.rs Scan ScanDetail@@

`ScanDetail extends Scan` is a flattened `scan` field, so the JSON is one object.

### Requests

@@items scan.rs NewPlantInput CreateScanRequest ScanPlant@@

"Exactly one of `plant_id` and `new_plant`" can't be expressed in serde without an untagged enum, so the request keeps both fields and `plant()` turns it into `ScanPlant` or a `400`:

@@impl scan.rs impl CreateScanRequest@@

### Upload with depth

@@items scan.rs DepthSource Intrinsics Distortion DepthMeta QualityReport DepthProblem DepthOutcome UploadImageResponse@@

- `DepthOutcome` is `{ stored: true } | { stored: false, problem }`: tagged by a boolean, which serde can't tag on. It is untagged with `True` and `False` literal fields; each only decodes from its own value, so the branches can't be confused. The API never decodes it; the app does.
- The `depth_meta` multipart part is a `DepthMeta` (JSON), with `deny_unknown_fields`.

### Answers, species confirmation and feedback

@@items scan.rs AnswerRequest ConfirmSpeciesRequest SpeciesChoice Verdict FeedbackRequest Created@@

`ConfirmSpeciesRequest::choice` checks "exactly one of" and trims `custom_name` to 1–80 characters.

---

## 4. Scan events

[Contracts 4](../architecture/contracts.md#4-scan-events-sse).

@@items events.rs Stage ProblemKind RunReason Skipped SelectionSource SelectedCandidate ResolvedBy SupersedeReason@@

The catalogue is one enum, internally tagged by `type`, flattened into the envelope. `ScanEvent<Stored>` is what `scan_event.payload` holds; `ScanEvent<Signed>` (`ScanEvent`) is what the SSE `data:` line carries.

@@items events.rs ScanEventBody ScanEventType ScanEvent@@

- `#[schema(as = ScanEventBody)]` without `bound`: utoipa 6 rejects `bound` on enums ("unexpected attribute: bound"), so the enum uses the default `U: ToSchema`, which only `Signed` implements.
- `event_type()` is the SSE `event:` line and the stored `type` column; a test checks it equals the serde tag.

The envelope of contracts 4, signed as it is sent (the test also checks the key order):

@@test events.rs finding_envelope_is_signed_when_sent@@

---

## 5. Worker API and jobs

[Contracts 5](../architecture/contracts.md#5-worker-api-and-jobs).

### Heartbeat and claim

@@items jobs.rs Capability JobKind GpuInfo HeldJob Heartbeat HeartbeatResponse ClaimRequest@@

### Job and payloads

`JobImage.url` is the worker route `/internal/jobs/{job_id}/images/{image_id}`, authorised by the lease token: not signed and never expiring, so it is a `WorkerImagePath` built from the two ids, and it may be stored in `job.payload`.

@@items jobs.rs JobImage WorkerImagePath PerceptionStage RegionClass PriorRegion PerceptionPayload Indicator CandidatePrompt WorkingSpecies SpeciesOption OrganismOption VlmPayload VerifyReason VerifyPayload GuidedPayload@@

- `PerceptionPayload.regions` is `(Region & { region_id; class })[]` in the contract. `PriorRegion` sends `region_id`, `image_id`, `bbox` and `class`; the optional `mask_url` is never sent, because workers have no route to masks and species and disease work from the boxes.
- `skip_species_identification?: boolean` is a `bool` that is omitted when false (`skip_serializing_if = "std::ops::Not::not"`), which reads the same.
- `Indicator.where` is a Rust keyword, so the field is `r#where`; serde and utoipa both strip the `r#`.

`kind` and `payload` sit side by side in the job, so the payload union is adjacently tagged and flattened:

@@items jobs.rs JobSpec Job JobHead ClaimedJob@@

A worker decodes `JobHead` first: a `v` it doesn't support still leaves it the `job_id` and `lease_token` it needs to fail the job with `unsupported_version` ([contracts 1, versioning](../architecture/contracts.md#versioning)).

### Events and completion

@@items jobs.rs PartialResult WorkerEvent WorkerEvents@@

- An event type from a newer worker decodes as `WorkerEvent::Unknown` (`#[serde(other, skip_serializing)]`) and is ignored; an unknown value inside a known event is an error, because the API must store it.
- `PartialResult` is tagged by its own `stage`; the API checks it equals the event's `stage`.

@@items jobs.rs JobErrorCode@@

@@impl jobs.rs impl JobErrorCode@@

`JobErrorCode` serialises as its bare string and decodes any string, so a code from a newer worker is kept as `Other` and stored as written. `retryable()` is the contract's rule for each known code ([contracts 5, complete](../architecture/contracts.md#post-internaljobsidcomplete)); `None` means the worker's own `retryable` flag decides.

@@items jobs.rs ModelVersionKey JobError JobOutcome CompleteJob@@

`status`, `result` and `error` are one enum tagged by `status`, so `done` without `result` or `failed` without `error` can't be decoded. `model_versions` stays a `BTreeMap<String, String>`: `ModelVersionKey` names the fixed keys, and each model a job used adds its own manifest-entry key beside them.

### Results

@@items jobs.rs RleMask PerceptionRegion SpeciesLabel SpeciesResult ProblemScore DiseaseResult DiseaseSkipReason DiseaseSkipped ImageEmbedding PerceptionResult VlmResult VerifyResult GuidedResult@@

The contract sends `result` with no tag (`PerceptionResult | VlmResult | VerifyResult | GuidedResult`) and, since v0.9, says it is read by the job's kind. An untagged decoder takes the first variant that fits and, ignoring unknown fields, reads a `VlmResult` as a `VerifyResult` (a subset of its fields). So `JobOutcome::Done` keeps the raw JSON, and the API decodes it for the kind on the job's record:

@@items jobs.rs JobResult@@

A job of each kind round-trips:

@@test jobs.rs job_with_each_payload_kind@@

---

## 6. VLM structured output

[Contracts 6](../architecture/contracts.md#6-vlm-structured-output). The raw shapes are what the model writes under guided decoding; the worker builds each call's JSON Schema from the payload, so they derive serde only. The converted shapes are what the worker returns and the API stores.

### Shared vocabularies

@@items wire.rs Certainty@@

@@items vlm.rs DamagePattern OrganismCount SoilAspect ImageAlias OTHER@@

### Raw output

@@items vlm.rs SoilMoisture SoilSurface SoilStructure SoilColour SoilCover SoilDeposits SoilErosion SoilWeeds SoilReadingValue SoilReading SpeciesGuessRaw SoilRaw OrganismRaw DamageRaw VlmFindingsRaw IndicatorAnswerRaw VlmIndicatorsRaw indicator_key SqueezeResult JarTexture VlmGuidedRaw BoxFormat@@

- `SoilReading<V>` is `Option<SoilReadingValue<V>>` and always present: `null` when not visible.
- `bbox_2d` is read as corners `[x1, y1, x2, y2]`, the order Qwen2.5-VL grounding emits. Contracts 6 gives the units per `box_format` but not the order; this is listed as an ambiguity in [domain 12](domain.md#12-open-questions).

### After decoding

@@items vlm.rs SpeciesGuess SoilFinding OrganismFinding DamageFinding VlmFindings IndicatorAnswer VlmIndicatorAnswers GuidedTexture@@

- VLM regions never have masks, so `OrganismFinding.region` and `DamageFinding.region` are `Region<Stored>`: they become stored observations as they are.
- `From<VlmGuidedRaw> for GuidedTexture` maps the certainty word to its fixed confidence.

---

## 7. Pipeline records

[Contracts 7](../architecture/contracts.md#7-pipeline-records).

### Observation

@@items records/observation.rs ObservationStage KnowledgeRef OrganismRole ObservationDetail ObservationSource Observation@@

- `ref` is a Rust keyword, so the field is `knowledge_ref` with `#[serde(rename = "ref")]`.
- `KnowledgeRef` is adjacently tagged: `{ "table": "disease", "id": "tomato_early_blight" }`, with the slug typed by table.
- `confidence` is the flattened `Rated`.

### Measurement

@@items records/measurement.rs MeasureQuantity MeasureUnit Measurement@@

`measurable_from` encodes the table of quantities in contracts 7, which the knowledge loader also checks against an indicator's `image_kinds`.

### WeatherSummary

@@items records/weather.rs WeatherProviderName LocationPrecision CurrentWeather PastWeather ForecastDay SoilNow WeatherSummary CellWeather Coordinates WeatherCell@@

- `about_1_km` and `about_10_km` are renamed by hand: `rename_all = "snake_case"` would write `about1_km`.
- `CellWeather` is what the cache holds per cell and hour: a `WeatherSummary` without `location_precision`, which belongs to the garden, not the cell. `for_garden` adds it back.
- `WeatherCell` formats as `"43.65,-79.38"`, the stored `garden.weather_cell`.

### Diagnosis

@@items records/diagnosis.rs Weight EvidenceSource MeasuredValue IndicatorEvidence WeatherFit CandidateScore Citation DiagnosisStatus Prey DiagnosisOrganism RiskLevel Risk EvidenceBasis Diagnosis@@

- `Weight` only decodes 1, 2 or 3.
- `EvidenceSource` derives `Ord` in rank order (`vlm < measurement < user`): each indicator keeps the highest-ranked answer.
- `DiagnosisOrganism.preys_on` is computed from the `preys_on` graph when read and never stored.
- `Risk.risk` is an open vocabulary in the contract (a `string`); the known values are constants.

### ActionPlan

@@items records/plan.rs SAFETY_NOTE TriggerKind PlanWhen StepCategory PlanTarget PlanStep PlanConstraint ActionPlan PlanReason@@

`PlanReason` is the stored `action_plan.reason`, which adds `initial` to the four re-planning reasons ([database 3](../database/README.md#3-tables)).

The plan writer's output is the fixed shape whose JSON Schema the agent sends to the LLM API. It isn't a type in contracts 7; it is `ActionPlan`'s model-written fields, without `safety_note`, which the server adds:

@@items records/plan.rs PlanNeighbour PlanStepDraft PlanWriterOutput@@

The schema says `additionalProperties: false` (`#[schemars(extend(…))]`) so the model is told no extra keys are allowed, while serde still accepts and drops one, so a near-miss output reaches validation instead of failing to parse.

### Task

@@items records/task.rs Repeat TaskSource TaskTrigger DismissedReason Task TaskState AlertKey@@

`AlertKey` never leaves the server; its `Display` is the stored `task.alert_key`.

---

## 8. Chat and memory

[Contracts 8](../architecture/contracts.md#8-chat-and-memory).

### Conversations and messages

@@items chat.rs Anchor AnchorType CreateConversationRequest Conversation ConversationQuery MessageRole Attachment Message ConversationDetail ChatImageUploaded Rating MessageFeedbackRequest SendMessageRequest MessageReplay@@

- `anchor` is adjacently tagged: `{ "type": "plant", "id": "01JAX…" }`.
- An attachment's URL needs the conversation id, which `Message` doesn't carry, so `Sign for Message<Stored>` takes it as its scope.

### Chat SSE events

@@items chat.rs ChatRoute Usage MessageStarted TextDelta ToolStarted ToolFinished CitationEvent PendingActionEvent MemorySaved RescanRequested AnswerBlockEvent MessageCompleted ChatErrorEvent ChatEvent@@

The `data:` line has no `type` field; the event name is the SSE `event:` line. `ChatEvent` is therefore untagged and serialise-only: the API writes these and the app reads them, so the untagged decoding hazard doesn't arise.

### Pending actions

@@items chat.rs WriteTool PendingStatus Card Preview PendingAction ConfirmResult ConfirmResponse RejectResponse@@

`Preview::Task` boxes its `Task` to keep the two variants close in size.

### Memory

@@items chat.rs MemoryKind MemoryTopic MemorySource Memory MemoryQuery CreateMemoryRequest@@

### Beliefs

Shared memories as Agent Memory holds them ([agent-memory](../database/agent-memory.md)): `GET /plants/{plant_id}/beliefs` answers `PlantBeliefs`, and `search_memory` returns `MemorySearch`.

@@items chat.rs Belief PlantBeliefs BeliefHit MemorySearch@@

### Answer blocks

Native components the app draws with an answer, each built by `agent` from one tool result of the turn and the store ([contracts 8, answer blocks](../architecture/contracts.md#answer-blocks)).

@@items chat.rs AnswerBlockKind BlockPlant FrostPlant AnswerBlock BlockBody@@

- `AnswerBlock` is `{ call_id }` with `BlockBody` flattened into it, so the generated schema is `allOf` of the two and the app's type is `{ call_id } & (…)`. The body is internally tagged by `kind`; an unknown kind doesn't decode.
- `FrostPlant` flattens `BlockPlant` and adds `frost_tender`, which is omitted, never `false`, when the species is unknown or has no `frost`.
- `Message.blocks` has `#[serde(default)]`, so a message stored before blocks reads as having none, and `skip_serializing_if = "Vec::is_empty"`, so the app sees the field only when a block was sent.

### Tool return shapes

@@items chat.rs ToolName SimilarCase ProblemRef KnowledgeHit Lookalike SoilProfileView GardenMapView RescanRequest@@

These go to the model, never to the app, so they derive serde only; `Lookalike` also derives `ToSchema`, because a `candidates` block carries it to the app. `get_scan` returns `ScanDetail<Stored>`: "without image URLs", which is exactly the stored URL state.

---

## 9. Gardens, plants, tasks and account

[Contracts 9](../architecture/contracts.md#9-gardens-plants-tasks-and-account).

### Gardens

@@items garden/gardens.rs ConsentPrecision GardenLocationInput LocationQuery SoilProfileStatus TimeZone Garden CreateGardenRequest UpdateGardenRequest GardenWeather@@

`GardenLocationInput::query` checks "exactly one of `lat` and `lon`, `postal_code`, `city`" and the coordinate ranges, and gives the geocoder a `LocationQuery`.

### Members and invites

@@items garden/members.rs Member Invite CreateInviteRequest UpdateMemberRequest@@

### Plants, timeline and growth

@@items garden/plants.rs PlantsQuery CreatePlantRequest UpdatePlantRequest TimelineMeasurements TimelineEntry GrowthQuantity MeasurementPoint MeasurementsQuery@@

### Species

@@items garden/species.rs SpeciesProfileKind SpeciesSummary SpeciesPhoto SpeciesQuery@@

`SpeciesSummary` exists only on the wire: the knowledge record holds storage keys, and `signed_images` builds the signed photos from them.

### Garden map

@@items garden/map.rs BedInput PinInput GardenMapInput Bed Pin GardenMap MapResponse PinPlant ValidPin MapUpload@@

`GardenMapInput::validate` checks everything that needs no database (counts, simple polygons, 200 m, one pin per plant, exactly one of `plant_id` and `new_plant`, known bed ids, the north range) and computes each bed's area; the store checks the rest in the map transaction ([domain 9](domain.md#9-boundary-traits)).

### Tasks

@@items garden/tasks.rs TaskQuery CreateTaskRequest UpdateTaskRequest@@

### Account

@@items garden/account.rs Me UpdateMeRequest GardenTransfer AccountDeletion Platform PushTokenRequest@@

### Push notifications

@@items garden/push.rs PushScanReason PushPayload PushMessage@@

`PushPayload` is tagged by `kind` and every variant carries `v`.

### Auth webhook

@@items garden/webhook.rs AuthEventType AuthEventData AuthWebhookEvent WebhookHeaders WEBHOOK_TOLERANCE@@

### Ops

@@items garden/ops.rs WorkerStatus QueueCounts Status Healthz@@

### Farm connectors

[Connectors 4–7](../connectors/README.md#4-data); contracts 8–9 gain them with roadmap E1.5. Module `connectors`. The sealed tokens (`connector.secret`), a pending connect's `oauth` state and the sync cursors are on no wire type: they stay in the store's rows (`ConnectorRow` in [domain](domain.md)). The statuses are the schema's four; `endpoint` is `era` only for the demo garden's seeded connectors.

@@items connectors.rs ConnectorKind ConnectorStatus ConnectorEndpoint ConnectorAccount ConnectorSource Connector ConnectorList@@

The connect flow's requests. App requests deny unknown fields; the vendor's callback doesn't, because Slack and Google add their own parameters:

@@items connectors.rs CreateConnectorRequest CreateConnectorResponse OAuthCallbackQuery CompleteConnectorRequest AvailableSources SourceChoice SetSourcesRequest@@

A synced record's citation, which plans and messages keep as it was when the record is deleted. `PlanStep.farm_citations`, `ActionPlan.farm_records` and `Message.farm_citations` are omitted when empty and read as empty from rows stored before connectors:

@@items connectors.rs SyncErrorCode FarmRecordKind FarmCitation FarmRecordHit@@

@@test connectors.rs a_farm_citation_is_exactly_its_contract@@

---

## 10. Knowledge seed format

[Contracts 10](../architecture/contracts.md#10-knowledge-seed-format). Every seed type derives `schemars::JsonSchema` and denies unknown fields, so the JSON Schema says `additionalProperties: false` and a misspelt key in a reviewed YAML file fails validation.

### Problem profile

@@items jobs.rs IndicatorWhere@@

@@items knowledge.rs Prevalence MeasureStat IndicatorMeasure IndicatorSeed Range Conditions LooksLikeSeed ProblemSeed@@

The contract's example profile, as the JSON its YAML parses to, round-trips:

@@test knowledge.rs problem_seed_example_round_trips@@

### Other entity shapes

@@items knowledge.rs SourceSeed WaterNeed LightNeed Frost Care ReferenceImageSeed SpeciesSeed TaxonGroup SeedOrganismRole OrganismSeed TreatmentKind TreatmentSeed ProductSeed ProductsSeed ClassifierLabel ClassifierLabels ChunkTopic ChunkSeed SoilConditionSeed@@

- `SeedOrganismRole` has the four seeded roles; `unknown` exists only for findings reported as "other" (`OrganismRole`).
- `soil_conditions.yaml`'s shape isn't spelled out in contracts 10; `SoilConditionSeed` follows the `soil_condition` table of [schema.surql](../../backend/migrations/schema.surql).

### Soil profile

@@items knowledge.rs DepthBand SoilDepth SoilProfile@@

---

## 11. TypeScript to Rust map

`domain::` is omitted from the paths. `U` marks a type generic over the URL state (`X` is the signed form, `X<Stored>` the stored one).

| Contract (TypeScript) | Rust type | Module |
|---|---|---|
| `Id` | `Id<T>` and its aliases (`ScanId`, …) | `id` |
| `Slug` | `Slug<K>` (`SpeciesId`, `ProblemId`, `OrganismId`, `TreatmentId`, `SourceId`) | `id` |
| `Slug \| "healthy"` | `ProblemOrHealthy` | `id` |
| `"<problem_id>.<slug>"`, `"<image_id>.<n>"`, `"<source_id>.<n>"` | `IndicatorId`, `RegionId`, `ChunkId` | `id` |
| `"<connector_id>.<n>.<vendor_id>"` (a farm record) | `FarmRecordId` | `id` |
| `Connector`, `FarmCitation` and the connect requests | `Connector`, `FarmCitation`, … | `connectors` |
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
| `Memory` | `Memory`, `MemoryKind`, `MemoryTopic`, `MemorySource`, `MemoryQuery`, `CreateMemoryRequest` | `chat` |
| `Belief`, `PlantBeliefs`, `search_memory`'s result | `Belief`, `PlantBeliefs`, `BeliefHit`, `MemorySearch` | `chat` |
| tool return shapes | `SimilarCase`, `ProblemRef`, `KnowledgeHit`, `Lookalike`, `SoilProfileView`, `GardenMapView`, `RescanRequest`, `ToolName` | `chat` |
| `AnswerBlock`, `answer_block` | `AnswerBlock`, `BlockBody`, `AnswerBlockKind`, `BlockPlant`, `FrostPlant`, `AnswerBlockEvent` | `chat` |
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
