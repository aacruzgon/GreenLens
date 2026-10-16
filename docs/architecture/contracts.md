# PDM Data Contracts

> **Status:** Draft v0.8.3, 2026-10-16. Companion to the [architecture document](README.md).
> **v0.8.3** adds open questions 12–24 (section 12): behaviour the Rust backend needs that these contracts don't settle yet. No shape changes.
> **v0.8.2** applies the third audit:
>   - failing a scan supersedes its open question, and a guided check that finishes on a failed scan only stores its result, for a resubmit;
>   - `POST /tasks` refuses a `scan_id` that is missing or being deleted, in the insert's transaction;
>   - species photos and `preys_on` are computed when read or sent, and species photos are keyed and routed by a content hash;
>   - the knowledge loader's citation points at storage 2.
> **v0.8.1** applies the audit of v0.8:
>   - image URLs in events are signed each time an event is sent or replayed;
>   - the stream stays open while any guided check is analysing, including after a failure;
>   - `low_evidence` is narrowed to results where nothing points to a problem;
>   - species photo credits travel with the candidate, and timeline entries carry their failure code;
>   - push coverage is stated as it is;
>   - feedback waits for submit, and deleting a pending scan also deletes its jar-test task;
>   - reads skip scans being deleted, and the knowledge photos are uploaded on the control-plane host.
> **v0.8** adds what the app's screens need ([frontend](../frontend/README.md#20-contract-changes-the-screens-need)):
>   - thumbnails on plants and timeline entries, garden weather, and a species list with reference photos;
>   - the prey of beneficial organisms, and `low_evidence` on diagnoses;
>   - a scan link on tasks, a push payload, and stream closing while a guided check runs;
>   - the `image_quality` hints, a city as a garden location, and `retryable` for each `429`;
>   - deleting a `pending` scan, and Clerk with Svix as the auth provider.
> **v0.7.3** applies the third LiDAR audit: a step claim records its run and is released by new runs, failures and deletion; steps hand over to the next in their commit, every entry point claims or flags `rescore_pending`, and the sweep covers every live status; question steps end at `verifying`; the fallback's claim holds back photo-answer segmentation in SQL too (`job.question`); observations with no image are replaced by a run; the ground is the highest plane beneath the base; plan writes skip only a dead neighbour's task; map resends use a new key and the `400` lists every unknown plant; and `hole_count`.
> **v0.7.2** applies the LiDAR re-audit: `api`'s own steps between jobs are claimed by compare-and-set and recovered by the sweep (for runs and for photo answers), question-photo segmentation waits for the perception worker unless the rig is down, `min` and `max` are always sent so the healthy rule works for one leaf, the ground is the lowest supported plane, tasks are only ever created on live plants, observations are replaced per image, the `409` hint values are listed, and an upload that still pins a deleted plant drops the pin and resends.
> **v0.7.1** applies the LiDAR audit: the question-photo trigger counts only user and measured answers, the leaf distance range matches the hint, a map upload racing a plant deletion drops that pin, map uploads carry `base_version` and get `409` when stale, the growth series uses each scan's current measurements dated by the scan, height and width remove the ground first, the healthy rule needs no spot on any leaf, the segmentation-only job carries `question_id` and is spelled out, a stalled photo answer is recovered by the sweep, neighbour deletion leaves stored plans alone and is re-checked on confirm, face regions are zeroed in the confidence map too, and `storage_error`.
> **v0.7** adds LiDAR, on the phones that have it: depth maps uploaded with photos, a `measurement` stage and observation, measurable indicators answered before the VLM (on photos sent for a question too, after a segmentation-only `perception` job), a growth series per plant, and a garden map with neighbours, spacing and neighbour monitoring steps. All of it is optional: without depth, every contract behaves as in v0.6.
> **v0.6** adds batching: perception runs each model once per job over all of a scan's images or regions, a worker declares how many jobs it holds at once (`slots`, so the VLM worker can let vLLM batch two jobs when the day-2 benchmark allows it), and weather is fetched by cache cell, in multi-location requests for the scheduler.
> **v0.5** applies the fourth audit: plan-trigger task keys scoped to their plan, the dose check on every string the model writes into a tool call, a validated `revise_plan` preview that is stored exactly as shown, `healthy` only on positive evidence, a diagnosis for every guided check, one late-work rule, ratio checks that pass ordinary pruning text, and wording fixes.
> **v0.4** applies the third audit: guided checks outside runs and kind limits, serialised re-plans that keep chat revisions, `provisional` tied to `uncertain`, a readable-plant rule that keeps healthy plants out of `photos_unusable`, the dose check on every chat reply, one trigger vocabulary with deduplicated trigger tasks, a pipeline stage on every observation, and an `answering` state for questions.
> **v0.3** restores the features v0.2 had cut (guided soil checks, the full set of soil aspects, recurring and trigger-based plan steps, HEIC uploads, segmentation masks, partial worker results), routes photos to the VLM per call, and applies the second audit: a lease backstop, species-change runs that keep earlier stages, a full late-answer lifecycle, plan tasks that follow plan versions, stricter idempotency, one rule for cancelled jobs, a stronger dose check, signed image URLs, invite and removal rules, and memory sharing by consent.
> **v0.2** applied the first audit: existence-hiding 404s, a role matrix, scan runs, per-job VLM schemas, idempotency keys, lease extension, safety validators and the missing endpoint contracts.
> **Purpose:** agree on the shape of every message that crosses a boundary (app ↔ API, API ↔ workers, worker ↔ VLM, ingestion ↔ database) before build week starts, so the app, backend and ML work streams can build in parallel from Oct 16.

## Contents

1. [Conventions](#1-conventions)
2. [Errors](#2-errors)
3. [Scan REST API](#3-scan-rest-api)
4. [Scan events (SSE)](#4-scan-events-sse)
5. [Worker API and jobs](#5-worker-api-and-jobs)
6. [VLM structured output](#6-vlm-structured-output)
7. [Pipeline records](#7-pipeline-records)
8. [Chat and memory](#8-chat-and-memory)
9. [Gardens, plants, tasks and account](#9-gardens-plants-tasks-and-account)
10. [Knowledge seed format](#10-knowledge-seed-format)
11. [Changes to the architecture document](#11-changes-to-the-architecture-document)
12. [Open questions](#12-open-questions)

---

## 1. Conventions

### Source of truth

This document is the agreement for build week. Once the code exists, the source of truth becomes the Rust types in the `domain` crate:

- `utoipa` generates the OpenAPI spec from them, and `openapi-typescript` generates the app's TypeScript types from that spec.
- `schemars` generates JSON Schema from them for the fixed shapes: knowledge seed validation (section 10) and the plan writer's output. The VLM's schema is built per job by the worker (section 6), because its allowed values change with each payload.

A change to a contract changes the Rust type first, and this document is updated in the same pull request. The records are stored as defined in [docs/database/schema.surql](../database/schema.surql), which changes in the same pull request when a stored shape changes.

### Notation

Shapes are written as TypeScript types because they are compact and the app team reads them directly. `?` marks an optional field, which is omitted from the JSON when absent (never sent as `null`, unless the type says `| null`).

### Wire format

| Rule | Value |
|---|---|
| Encoding | JSON, UTF-8. Uploads are `multipart/form-data` |
| Field names | `snake_case`, matching the database and the Python ingestion scripts |
| IDs | Opaque strings: the ULID key of the SurrealDB record, without the table prefix (`"01JAX3K9T6R2B8V0QZ4M7D1E5F"`). The field name says what it points to. Knowledge-base IDs are readable slugs (section 10) |
| Timestamps | RFC 3339 in UTC with a `Z` suffix: `"2026-10-18T14:03:22Z"`. Calendar dates local to a garden are `YYYY-MM-DD` |
| Units | Metric. °C, mm, cm, m, cm², %, m³/m³, km/h. Unit suffixes in field names (`temp_c`, `precip_mm`, `x_m`) |
| Confidence | A number from 0 to 1, always sent with `confidence_band`: **high** ≥ 0.8, **medium** ≥ 0.5, **low** below that, computed by the API so every screen agrees |
| Enums | Lower-case `snake_case` strings |
| Regions | Normalised to the stored image: `[x, y, w, h]`, each from 0 to 1, origin top-left. Whoever produces a box converts it before sending |
| Images | Uploads may be JPEG, PNG, WebP or HEIC. On upload the API decodes the image, applies its EXIF orientation to the pixels, then strips all metadata, so `width`, `height` and every region refer to the upright image. Everything is stored as WebP |
| Depth | LiDAR depth maps are 16-bit grayscale PNGs in millimetres, 0 meaning no depth. They are uploaded with a photo (section 3) and stored on the server. No URL exists for one, and workers never receive them |
| Image URLs | Every `url` the API returns for an image or mask is **signed and short-lived** (15 minutes, `?exp=&sig=`) and needs no `Authorization` header, so plain `<img>` tags work in the web build. The app gets fresh URLs by re-reading the record |
| Lists | `?cursor=&limit=` (default 20, max 100), returning `{ items: T[], next_cursor?: string }`. Each list endpoint states its sort order |

```ts
type Id = string;            // ULID record key
type Slug = string;          // knowledge-base id, e.g. "tomato_early_blight"
type Timestamp = string;     // RFC 3339 UTC
type LocalDate = string;     // YYYY-MM-DD in the garden's time zone
type Confidence = number;    // 0..1
type ConfidenceBand = "high" | "medium" | "low";
type BBox = [number, number, number, number]; // normalised x, y, w, h

interface Region {
  region_id?: string;        // present for segmentation regions, referenced by later stages: "<image_id>.<n>", unique within the scan
  image_id: Id;
  bbox: BBox;
  mask_url?: string;         // signed PNG mask, when segmentation produced one
}

interface Page<T> {
  items: T[];
  next_cursor?: string;
}
```

### Versioning

- REST endpoints have no version prefix during the hackathon. Changes are additive only: new optional fields and new enum values. Clients ignore fields they don't know.
- Every SSE event and every worker job payload and result carries `"v": 1`. A worker that receives a `v` it doesn't support fails the job with the error code `unsupported_version` instead of guessing.
- Apps treat an unknown enum value as "other" and ignore unknown event types.

### Authentication headers

| Caller | Header |
|---|---|
| App | `Authorization: Bearer <Clerk session token>` (not needed for signed image URLs) |
| Worker | `Authorization: Bearer <worker token>`, plus `X-Lease-Token` on every call about a leased job (section 5) |
| Auth webhooks | Clerk's Svix headers (`svix-id`, `svix-timestamp`, `svix-signature`), verified before the body is parsed (section 9, auth webhook) |

### Idempotency

Every create call accepts: `POST /gardens`, `POST /gardens/{id}/invites`, `POST /plants`, `POST /scans`, `POST /scans/{id}/images`, `POST /scans/{id}/answers`, `POST /scans/{id}/feedback`, `POST /conversations`, `POST /conversations/{id}/images`, `POST /conversations/{id}/messages`, `POST /memories`, `POST /tasks` and `PUT /gardens/{id}/map` accept an `Idempotency-Key` header (a UUID the app generates per user action and reuses only when retrying that action).

| Rule | Behaviour |
|---|---|
| Scope | A key is unique per (user, method, path) and kept for 24 hours |
| Fingerprint | JSON bodies: a hash of the canonical JSON. Multipart: a hash of the form fields and the file bytes, never the raw body, whose boundary changes on each retry |
| Same key, same fingerprint, finished | The stored response is returned with its original status. For chat messages, which answer with a stream, the replay is `200 { message: Message }` |
| Same key, same fingerprint, still running | `409 request_in_progress`, `retryable: true`, with `retry_after_s` |
| Same key, different fingerprint | `422 idempotency_conflict` |
| What is stored | Only `2xx` responses and deterministic `4xx` responses (`400`, `403`, `404`, `413`, `415`, `422`). A `409`, `429` or `5xx` is not stored, so a retry runs again |
| Interrupted chat message | Not stored as the replay. The key is released; the app's "resend" action sends a new key |

### Roles and access

A user who is not a member of the garden, or not the owner of the conversation, gets **404 `not_found`**, never 403. A response never confirms that an ID exists to someone who may not see it. **403 `forbidden`** means the caller is a member whose role is too low.

| Action | viewer | reviewer | editor | owner |
|---|---|---|---|---|
| Read gardens, plants, scans, plans, tasks, timeline, measurements, the garden map | ✓ | ✓ | ✓ | ✓ |
| Chat in their own conversations, with the read tools and `remember` (which only writes the caller's private memory) | ✓ | ✓ | ✓ | ✓ |
| Scan feedback | | ✓ | ✓ | ✓ |
| Create scans, upload, delete a scan before it is submitted, answer questions, confirm species, add guided soil checks | | | ✓ | ✓ |
| Map the garden (from a LiDAR device) | | | ✓ | ✓ |
| Chat write tools (create and complete tasks, revise plans) and their confirmation cards | | | ✓ | ✓ |
| Create, complete and dismiss tasks | | | ✓ | ✓ |
| Create and edit plants | | | ✓ | ✓ |
| Share their own chat memories with the garden, add garden notes | | | ✓ | ✓ |
| Rename the garden, manage invites and members, change roles, delete plants or the garden, delete shared memories | | | | ✓ |

The agent's tool list is filtered by role before each turn: viewers and reviewers get the read tools and `remember`, never the write tools, so they never receive a confirmation card. A garden may have several owners. The last owner can't leave or be demoted.

---

## 2. Errors

Every non-2xx response has the same body:

```ts
interface ErrorBody {
  error: {
    code: ErrorCode;
    message: string;          // human-readable, safe to show; English for now
    retryable: boolean;       // the same request may succeed later
    retry_after_s?: number;   // with 409 request_in_progress, 429 rate_limited and 503
    details?: Record<string, unknown>;  // per code, below
    request_id: string;       // matches the server log line
  };
}
```

| HTTP | `code` | When | `details` |
|---|---|---|---|
| 400 | `invalid_request` | Malformed JSON, missing or wrong-typed field | `{ field, problem }` |
| 401 | `unauthenticated` | Missing, expired or invalid JWT | |
| 403 | `forbidden` | A member whose role is too low (section 1) | `{ required_role }` |
| 403 | `invalid_signature` | Expired or tampered signed image URL | |
| 404 | `not_found` | Unknown ID, or one the caller may not see | |
| 409 | `invalid_state` | The scan, question, plan, task, invite, pending action or garden map isn't in a state that allows this call | `{ status?, allowed?: string[], current_plan_id?, current_map_version?, hint?: "new_scan" \| "neighbour_removed" \| "plant_removed" }` |
| 409 | `request_in_progress` | Same `Idempotency-Key` while the first request is still running | |
| 409 | `lease_lost` | Worker call for a job whose lease was requeued, expired or cancelled, including jobs of an older run | `{ job_id }` |
| 413 | `upload_too_large` | Image over 12 MB | `{ max_bytes }` |
| 415 | `unsupported_media_type` | Not JPEG, PNG, WebP or HEIC, or a HEIC file that can't be decoded | `{ accepted: string[] }` |
| 422 | `image_quality` | The upload blur or exposure check failed. The image was not stored | `{ problem: "too_blurry" \| "too_dark" \| "too_bright", blur_score, exposure, hint: QualityHint }` |
| 422 | `missing_required_shot` | Submit without a `whole_plant` image | `{ missing: ImageKind[] }` |
| 422 | `too_many_images` | Kind limit reached (section 3) | `{ kind, max }` |
| 422 | `idempotency_conflict` | Reused `Idempotency-Key` with a different body | |
| 429 | `rate_limited` | Request rate limit, including invite-code attempts. `retryable: true`, with `retry_after_s` | |
| 429 | `budget_exceeded` | Daily chat token budget or daily scan limit used up. `retryable: false`: it lasts until `resets_at` | `{ budget: "chat_tokens" \| "scans", resets_at }` |
| 500 | `internal` | Bug. Logged with `request_id` | |
| 503 | `processing_unavailable` | Face blurring or another required upload step failed. The image was not stored (fails closed) | |
| 503 | `upstream_unavailable` | LLM API, weather or another dependency down | `{ upstream }` |

```ts
type ErrorCode =
  | "invalid_request" | "unauthenticated" | "forbidden" | "invalid_signature"
  | "not_found" | "invalid_state" | "request_in_progress" | "lease_lost"
  | "upload_too_large" | "unsupported_media_type" | "image_quality"
  | "missing_required_shot" | "too_many_images" | "idempotency_conflict"
  | "rate_limited" | "budget_exceeded" | "internal"
  | "processing_unavailable" | "upstream_unavailable";

// one hint per image_quality problem, for the app's retake prompt
type QualityHint =
  | "tap_to_focus"        // too_blurry: hold the phone still and tap the subject to focus
  | "more_light"          // too_dark: move into better light or turn the plant to face it
  | "avoid_glare";        // too_bright: shade the plant or step out of direct sun
```

Example:

```json
{
  "error": {
    "code": "image_quality",
    "message": "This photo is too blurry. Hold the phone still and tap the leaf to focus.",
    "retryable": true,
    "details": { "problem": "too_blurry", "blur_score": 41.2, "exposure": 0.52, "hint": "tap_to_focus" },
    "request_id": "req_01JAX4B2Q8"
  }
}
```

A scan that fails during analysis is not an HTTP error: it ends in the `failed` status with a `ScanFailure` (section 3). Worker job errors (`unsupported_version`, model errors) travel in the job's `complete` body (section 5), never as HTTP status codes.

---

## 3. Scan REST API

### Scan lifecycle

```
pending ──submit──▶ queued ──▶ analysing ──▶ [awaiting_answer] ──▶ diagnosed ──▶ planned
                                   ▲          (at most 2 per run)
                                   └── species changed: new run, from analysing,
                                       awaiting_answer, diagnosed or planned
failed ──resubmit (retryable failures only)──▶ queued (new run)
```

- A **run** is one pass of the pipeline over the scan's images. `run` becomes 1 on submit and increases on a resubmit and on a species change. Jobs, events, observations and diagnoses carry the run; jobs and late results of an older run are rejected (section 5).
- A run asks the user **at most 2 questions**, all before the scan is `planned`. No new question is asked once the scan is `planned`. If the answers still leave it ambiguous, the pipeline continues with the best candidate marked `uncertain`.
- A **healthy** plant gets a short care plan and ends in `planned` like any other scan.
- A scan where no plant is found ends in `failed` with `no_plant_found` (perception's `plant_present` is false). A scan where a plant is present but nothing can be read from it ends with `photos_unusable`. A plant counts as **readable** when any of these holds: a leaf region was found; the disease stage produced any result of medium confidence or more, `healthy` included; BioCLIP or the VLM named a species with medium confidence or more; or the VLM findings contain any finding of medium confidence or more. Only a plant with none of these is unusable, so a healthy plant always reaches scoring, on the GPU or on the fallback worker. Being readable doesn't make a plant healthy: scoring calls it `healthy` only on positive evidence (section 7), and otherwise the diagnosis is `uncertain`. Those failures are not retryable: the app starts a **new scan** for new photos. Only `processing_error` and `upstream_unavailable` can be resubmitted.
- **Late work** is a late answer, a photo for a question the scheduler has already continued past, and every guided soil check. Late answers and late photos happen while the scan is `diagnosed` or `planned`. Late work never changes the scan's status and never fails the scan: a failure in it is reported as an event (section 5), and the current diagnosis and plan stay.

**Steps between jobs, and recovery.** Between jobs, `api` does work of its own in three steps: `retrieve` (after the perception result: measurement, context, retrieval and queueing the `vlm` job), `score` (scoring, fusion and the diagnosis, then a question or the plan) and `plan` (the plan writer). A restart inside one would leave the scan with nothing in flight and nobody to finish it. So a scan holds at most one **step claim**: `pipeline_step`, the run it belongs to (`pipeline_step_run`) and when it was claimed (`pipeline_step_at`).

| Rule | What happens |
|---|---|
| Claiming | Every entry point claims before it works, with a compare-and-set on `pipeline_step = NONE` that records the scan's current run: the handler that stores a `perception` result (claims `retrieve`), a `vlm` or run `verify` result (`score`), in the same transaction as the result; an indicator answer; a photo answer once it is measured or verified; the 10-minute continuation of an unanswered question; a late answer; a guided check's result on a `diagnosed` or `planned` scan (all `score`). A check's result on any other status claims nothing (section 7) |
| A busy scan | If another step holds the claim, the entry point's input is already stored (the answer, the job result, the check), so it only sets `scan.rescore_pending`. Claiming `score` clears the flag, because the step reads every input when it starts. A step that commits with the flag set hands over to `score` instead of its own next step, so input that arrived while it ran is scored, and nothing waits in memory |
| Committing | A step commits its output (a queued job, the diagnosis, a question, the plan) in one transaction that first re-checks its claim: the same step, run and stamp, the scan's `run` unchanged, and neither the plant nor the garden being deleted. If anything changed, the transaction rolls back and the step stops |
| Handing over | The same commit sets the next step: `retrieve` queues the `vlm` job and releases the claim, since a job is now in flight; `score` asks a question (the scan goes to `awaiting_answer` and the claim is released), or sets the claim to `plan` with a new stamp when a plan must be written (section 7, re-planning), or releases it; `plan` stores the plan and releases it |
| Runs, failures and deletion | Starting a run (submit, resubmit, species change), failing the scan, and starting the deletion of its plant or garden release the claim in the same transaction, and superseding a question clears its `step`. Failing the scan also resolves its open or `answering` question as `superseded` in that transaction, so a `failed` scan never holds a question. A step of an older run can therefore never commit, and a leftover claim never blocks the next run |
| The sweep | Re-claims a step whose stamp is older than its limit (2 minutes; 5 for `plan`, which waits on the LLM API) on any scan that isn't `pending`, `failed` or being deleted, with a compare-and-set on the old stamp and run, and re-runs it. The original commits only if its stamp still holds, so only one of them ever commits |

Every step is idempotent and re-reads its inputs when it starts: queueing checks for an existing job of the run, and the diagnosis and plan are written with a compare-and-set like any other (section 7). Chat plan revisions are not a step: they are stored synchronously by the per-scan plan writer (section 7).

### Shared types

```ts
type ScanStatus =
  | "pending" | "queued" | "analysing" | "awaiting_answer"
  | "diagnosed" | "planned" | "failed";

type ImageKind =
  | "whole_plant"      // required, max 1
  | "leaf_closeup"     // max 2
  | "leaf_underside"   // max 1
  | "soil_surface"     // max 1
  | "flower_fruit"     // max 1
  | "squeeze_test"     // guided soil check: max 1 before submit, any number after
  | "jar_test";        // guided soil check: max 1 before submit, any number after

interface ScanImage {
  id: Id;
  kind: ImageKind;
  answers_question_id?: Id;  // a photo sent to answer a question; doesn't count toward kind limits
  width: number;
  height: number;
  url: string;               // signed, short-lived
  uploaded_at: Timestamp;
  depth?: {                  // LiDAR phones only; the depth map itself is never served
    width: number;
    height: number;
    has_gravity: boolean;    // plant height and canopy width need it
  };
}

interface SpeciesRef {
  id: Slug;
  scientific_name: string;
  common_name: string;
}

interface SpeciesCandidate {
  species_id: Slug | null;   // null when the name isn't in the knowledge base
  name: string;
  score: Confidence;
  source: "bioclip" | "vlm" | "user";
  reference_image?: {           // the species' first reference photo (section 9, species); computed from species_id when read or sent, never stored
    url: string;                // signed
    credit: string;             // shown with the photo
  };
}

interface Plant {
  id: Id;
  garden_id: Id;
  name: string;                       // "Tomato by the fence"
  species?: SpeciesRef;
  custom_species_name?: string;       // "something else" in the confirmation card
  species_status: "unknown" | "suggested" | "confirmed";
  species_candidates: SpeciesCandidate[];
  status: "healthy" | "watch" | "sick" | "dead";
  position?: { map_version: number; x_m: number; y_m: number };  // from the garden map (section 9)
  latest_scan?: {                     // the plant's most recent scan, in any status
    scan_id: Id;
    created_at: Timestamp;
    status: ScanStatus;
    thumbnail_image_id?: Id;          // its whole_plant shot, when it has one
    thumbnail_url?: string;           // signed URL of that shot
  };
  created_at: Timestamp;
}

type ScanFailureCode =
  | "no_plant_found"          // retryable: false; start a new scan with new photos
  | "photos_unusable"         // retryable: false; start a new scan with new photos
  | "processing_error"        // retryable: true; job attempts exhausted
  | "upstream_unavailable";   // retryable: true; LLM API or weather down

interface ScanFailure {
  code: ScanFailureCode;
  message: string;
  retryable: boolean;
}

interface Question {
  id: Id;
  run: number;
  kind: "indicator" | "photo_request";
  text: string;                       // "Do the spots have rings, like a target?"
  options?: { id: "yes" | "no" | "unsure"; label: string }[];  // indicator questions
  photo_kind?: ImageKind;             // photo_request: which shot to take
  photo_hint?: string;                // "Turn the leaf over and photograph the underside"
  candidates: Slug[];                 // the problems this question separates
  asked_at: Timestamp;
  continue_at: Timestamp;             // when the scheduler continues without an answer
  status: "open" | "answering";      // answering: a photo for it is being analysed; further answers and photos get 409
}

interface GuidedCheck {
  image_id: Id;
  kind: "squeeze_test" | "jar_test";
  status: "analysing" | "done" | "failed";
  result?: Observation;               // a soil observation with aspect "texture" (section 7)
}

interface Scan {
  id: Id;
  garden_id: Id;
  plant_id: Id;
  status: ScanStatus;
  run: number;                        // 0 until submitted
  images: ScanImage[];
  guided_checks: GuidedCheck[];
  created_at: Timestamp;
  created_by?: { user_id: Id; display_name: string };  // absent once that user is deleted
  submitted_at?: Timestamp;
  open_question?: Question;           // present while a question is unanswered, in any status
  diagnosis_id?: Id;                  // the current diagnosis
  plan_id?: Id;                       // the current plan version
  failure?: ScanFailure;              // with status failed
  last_event_seq: number;             // highest scan_event.seq so far
}

interface ScanDetail extends Scan {
  plant: Plant;
  observations: Observation[];        // the latest observations of each stage, across runs (below)
  weather?: WeatherSummary;           // section 7
  diagnosis?: Diagnosis;              // section 7
  plan?: ActionPlan;                  // section 7
}
```

**Observations across runs.** Every observation records the `pipeline_stage` that produced it (section 7). A pipeline stage's observations are replaced only when a later run re-runs that pipeline stage, and only for the images that run covers, together with that stage's observations tied to no image (the species candidates, the plant check, the VLM's species guess). A re-measure of one photo replaces that photo's measurements alone, and a question photo's observations replace nothing. Guided-check observations (`pipeline_stage: "guided"`) are never replaced by a run; the diagnosis uses the latest check of each kind. A species-change run re-runs species, disease, the VLM stages and scoring, so `ScanDetail.observations` keeps the plant check, segmentation regions, measurements and image embeddings of the earlier run (measuring again only if the earlier run hadn't finished it), and shows the new run's results for the rest. The regions and measurements of a photo sent for a question are added to its run's observations and replace nothing. Every observation carries the `run` that produced it.

**Open questions.** `open_question` stays on the scan after the scheduler has continued without an answer, until the user answers it or a new run supersedes it. The current plan shows it at the top while it is open.

### `POST /scans`

Creates a scan in `pending`. Exactly one of `plant_id` and `new_plant` is required. Role: editor. Counts toward the daily scan limit (30 per user).

```ts
interface CreateScanRequest {
  garden_id: Id;
  plant_id?: Id;
  new_plant?: { name: string; species_id?: Slug };
}
// 201 → Scan
```

### `POST /scans/{id}/images`

`multipart/form-data` with the fields `kind` (an `ImageKind`), `file`, optionally `question_id` when the photo answers a `photo_request` question, and, from a LiDAR phone, `depth`, `depth_confidence` and `depth_meta` (below). Role: editor.

| Upload | Allowed when | What happens |
|---|---|---|
| Any plant or soil kind, no `question_id` | The scan is `pending` | Stored for the run. A kind already at its limit returns `too_many_images`; the app deletes the shot first to replace it |
| `squeeze_test` or `jar_test` | Any status except `failed` | Stored as a new `GuidedCheck`, and its own `guided` job analyses it, whether it was uploaded before or after submit (section 5). A check never blocks or fails the run. After submit there is no limit: a jar that hadn't settled, or a check that failed, is simply photographed again as a new check. The jar test is usually uploaded hours later, from the task the app created when the user started it |
| With `question_id` | That question's `status` is `open`, whatever the scan's status | The question becomes `answering` and the photo goes to a `verify` job; no submit is needed. A photo with depth may first be segmented and measured (section 7, measurement). Doesn't count toward kind limits |

A question that is already answered, superseded or `answering` returns `409 invalid_state`.

**Depth (LiDAR phones only).** Three optional parts, sent with any plant or soil kind, including a photo with `question_id`:

| Part | Content |
|---|---|
| `depth` | 16-bit grayscale PNG of depth (not disparity) in millimetres, 0 for no depth, covering the same field of view as the photo, with its aspect ratio (within 1%). It is in the frame of the photo's pixels **before** EXIF orientation, the sensor frame in which the camera delivers both, so the photo must be the file the LiDAR module wrote, uploaded unchanged. At most 4 MB |
| `depth_confidence` | Optional 8-bit PNG of the same size: 0 low, 1 medium, 2 high. Sent only when the capture path has one: ARKit's scene depth does, AVFoundation's `AVDepthData` doesn't |
| `depth_meta` | JSON, a `DepthMeta` |

```ts
interface DepthMeta {
  source: "lidar";
  intrinsics: {                        // pixels of a reference size, in the sensor frame
    fx: number; fy: number; cx: number; cy: number;
    ref_width: number; ref_height: number;
  };
  distortion?: {                       // only if the day-1 check finds the photo and depth distorted
    center: [number, number];          // reference-size pixels, sensor frame
    inverse_lookup: number[];          // the calibration's inverse lens distortion table
  };
  gravity?: [number, number, number];  // unit vector towards the ground, in camera coordinates
                                       // (x right, y down, z forward), sensor frame
  device_model: string;                // "iPhone16,1"
  filtered: boolean;                   // the phone filled holes in the depth map
}
```

The API applies the photo's EXIF orientation to the depth and confidence maps and transforms the intrinsics and gravity to match, scales the intrinsics to the stored image's size, sets the depth, and the confidence map when there is one, to 0 inside every face box found for blurring, with each box grown by 10% and rounded outwards when it is scaled down to the depth map's resolution so no edge of a face survives, and stores the result (architecture 6.7). Orientations 1, 3, 6 and 8 are rotations: a quarter turn swaps `fx` with `fy` and `ref_width` with `ref_height`, and the principal point, the distortion centre and gravity's x and y turn with the pixels. The mirrored orientations (2, 4, 5 and 7) can't come from the rear camera, so their depth is dropped with `invalid_meta`. A photo uploaded with depth is stored with a long side of at least 2048 pixels, or at its own size when smaller, because spot sizes are measured on the stored image (section 7). **Depth problems never fail the upload:** the photo is stored, the depth is dropped, and the response says why. Depth sent with a `squeeze_test` or `jar_test` is dropped in the same way. The idempotency fingerprint covers every part.

```ts
// 201 →
interface UploadImageResponse {
  image: ScanImage;
  quality: { blur_score: number; exposure: number; passed: true };
  depth?:                              // present when depth parts were sent
    | { stored: true }
    | {
        stored: false;
        problem: "aspect_mismatch" | "too_sparse" | "invalid_meta"
               | "unsupported_format" | "too_large" | "not_for_this_kind" | "storage_error";
      };
}
```

`too_sparse` means fewer than 20% of the pixels have depth. `storage_error` means writing the depth or confidence map to storage failed after the photo was stored; the photo is kept, and nothing of the depth is.

Errors: `image_quality` (not stored: show `message` and retake), `upload_too_large`, `unsupported_media_type`, `too_many_images`, `invalid_state`, `processing_unavailable`.

### `DELETE /scans/{id}/images/{image_id}`

Removes a shot, with its depth and confidence maps, while the scan is `pending`. Role: editor. `204`.

### `DELETE /scans/{id}`

Deletes a scan that was never submitted, so an abandoned capture leaves nothing behind. Role: editor. `204`.

- Only a `pending` scan can be deleted. Any other status gets `409 invalid_state` with `status`; a submitted scan is history, deleted only with its plant or garden.
- The scan is marked as being deleted first (`deleting_at`), so an upload or submit racing the delete gets `404`. Its guided jobs are cancelled, then its images with their depth maps, its guided checks with their observations and events, and the conversations anchored to it are deleted, in the order of [database 4](../database/README.md#deletion-). Tasks that name the scan (`Task.scan_id`: the jar-test task) are deleted with it, since they can only be done on that scan.
- From the moment deletion starts, the scan reads as missing: `GET /scans/{id}` gets `404`, `Plant.latest_scan` and the timeline skip it, and it is never the "most recent scan" a species change follows.
- The scan still counts toward the daily scan limit. A plant created through `new_plant` stays; an owner can delete it.

### `GET /scans/{id}/images/{image_id}` and `GET /scans/{id}/masks/{mask_id}`

Return the stored, face-blurred image (`image/webp`) or a segmentation mask (`image/png`). Reached through the signed URLs in the records; a request without a valid signature gets `403 invalid_signature`.

### `POST /scans/{id}/submit`

No body. Role: editor.

| Scan status | Effect | Response |
|---|---|---|
| `pending` | Run 1 starts: `queued`, `perception` job created | `202 Scan` |
| `failed` with a retryable failure | A new run starts: `queued` | `202 Scan` |
| `failed` with `no_plant_found` or `photos_unusable` | Nothing | `409 invalid_state` with `hint: "new_scan"` |
| anything else | Nothing; submit is idempotent | `200 Scan` |

Errors: `missing_required_shot`.

### `GET /scans/{id}`

```ts
// 200 → ScanDetail
```

The app calls this after `plan_ready`, after `plan_superseded`, when opening a scan from the timeline, and to refresh signed image URLs.

### `POST /scans/{id}/answers`

Answers an `indicator` question. Photo requests are answered by uploading with `question_id`. Role: editor.

```ts
interface AnswerRequest {
  question_id: Id;
  option_id: "yes" | "no" | "unsure";
}
// 202 → Scan
```

- Before the scan is `planned`: the answer is rescored and the run continues.
- After the scheduler has continued without it (the scan is `diagnosed` or `planned`): the answer is a **late answer**. It always produces a new diagnosis and a new plan version with reason `late_answer`, whether or not the winner changes. The plan is provisional exactly when that diagnosis is `uncertain` (section 7), so an answer that settles the question ends the provisional plan, and an `unsure` that leaves it ambiguous keeps it provisional.
- A question that is already answered, superseded or `answering` returns `409 invalid_state`.
- While a photo answer is analysed (its segmentation job when it has depth, then its `verify` job), the scan keeps its current status, progress shows as stage events, and the scheduler's sweep waits for those jobs before continuing the scan.
- **A photo answer that stalls is recovered.** Between its jobs, `api` measures the photo and then queues `verify`, so a restart at that moment would leave the question `answering` with nothing in flight. The question therefore records its step (`segmenting`, `measuring` or `verifying`) and when that step started, and belongs to its `run`:
  - the handler that stores the segmentation result moves it to `measuring` in the same transaction;
  - `measuring` is `api`'s own work: it measures the photo, answers what the measurements settle, and then either queues `verify` and moves to `verifying`, or, when nothing is left, claims the scan's `score` step (above). It commits only if the question is still `answering` with the same step and stamp, its run is still the scan's, and the plant isn't being deleted;
  - a `verify` result is an entry point for the scan's `score` step, which also resolves the question;
  - the sweep re-claims a `measuring` step older than 2 minutes, with a compare-and-set on its stamp, and re-runs it; the original commits only if its stamp still holds. Superseding the question clears its step, so an old run's photo never queues or scores anything. Job steps belong to the broker's leases and attempts (section 5).
  Re-measuring replaces that photo's measurements, and queueing checks for an existing `verify` job naming the question.

### `POST /plants/{id}/species`

Confirms or corrects the species. Exactly one of the two fields. Role: editor.

```ts
interface ConfirmSpeciesRequest {
  species_id?: Slug;
  custom_name?: string;   // "something else", at most 80 characters: the plant is treated as unsupported
}
// 200 → Plant
```

The plant is updated. Then the plant's **most recent scan**, if it was created in the last 24 hours and isn't `failed`, follows the species:

| That scan's status | Effect |
|---|---|
| `pending` or `queued` | Nothing more: the run uses the new species when it starts |
| `analysing`, `awaiting_answer`, `diagnosed` or `planned`, and the new species differs from the run's working species | A new run starts, in every case: the status becomes `analysing`; the old run's jobs are cancelled; its open question is resolved as `superseded`; and if the scan had a plan, the new run's plan supersedes it with reason `species_changed`. Which stages the new run repeats depends on the earlier run: if it had finished plant check and segmentation, the new run re-runs species, disease, the VLM stages and scoring, and keeps the earlier observations (see above), measuring again only if the earlier run hadn't finished it; otherwise it runs every perception stage and measures again. With `custom_name` ("something else"), the new run skips species identification |

### `POST /scans/{id}/feedback`

Role: reviewer or above. A scan that is still `pending` gets `409 invalid_state`: there is nothing to judge yet, and feedback would block deleting it.

```ts
interface FeedbackRequest {   // stored with the author's user id; deleted with their account
  verdict: "correct" | "wrong" | "unsure";
  corrected_label?: string;   // free text, or a Slug when picked from a list
  notes?: string;
}
// 201 → { id: Id }
```

---

## 4. Scan events (SSE)

### Transport

`GET /scans/{id}/events`, consumed with a streaming `fetch` (architecture section 9). Each event is written as:

```
id: 17
event: finding
data: {"v":1,"seq":17,"run":1,"scan_id":"01JAX...","ts":"2026-10-18T14:03:22Z","type":"finding","observation":{...}}

```

- `id` equals `seq`: an integer per scan, starting at 1, strictly increasing, with no gaps, across all runs.
- **Image URLs in events are signed as the event is sent**, live or replayed. The stored payload keeps image and mask IDs, never URLs, so a replay hours later carries fresh 15-minute URLs (`Region.mask_url`). A species photo has no image ID: `SpeciesCandidate.reference_image` isn't stored in the payload at all, and is filled in from `species_id` as the event is sent.
- Every event is stored in `scan_event` before it is sent. Events for a scan come from several places (worker events, answers, the scheduler's sweep, species changes), so **one serialised writer per scan** assigns `seq`, stores the event and broadcasts it, in that order. A live subscriber that falls behind the broadcast (`Lagged`) switches to replaying from the database.
- **Resume:** on reconnect the app sends `Last-Event-ID`. The server first subscribes to the scan's live broadcast, then replays every stored event with a higher `seq`, then forwards live events, dropping any whose `seq` it has already sent. The app also ignores any `seq` it has already applied. Without `Last-Event-ID` the server replays the whole history, which is how the app rebuilds stage progress for a scan it opens partway through a run.
- The server sends a `: keepalive` comment every 15 seconds.

### When the stream closes

The server closes the stream when the scan reaches a resting point:

| Resting point | Last events before close |
|---|---|
| A run ends with a plan, no open question and no guided check `analysing` | `status_changed → planned`, then `plan_ready` |
| A run ends with a plan while a guided check is `analysing` | Stays open until the check's `guided_check_done`, and any `diagnosis` and plan events it causes |
| A run ends with a plan and an open question | Stays open until the question resolves |
| A question resolves after `planned`, and no guided check is `analysing` | `question_resolved`, then `diagnosis` and `plan_superseded` if a new plan was made |
| A guided check or late `verify` job finishes after `planned`, with no open question and no other guided check `analysing` | `guided_check_done` or `question_resolved`, then `diagnosis` and any new plan events |
| The scan fails, and no guided check is `analysing` | `question_resolved` with `superseded` if a question was open or `answering`, then `failed` |
| The scan fails while a guided check is `analysing` | Stays open until the last check's `guided_check_done` |

The app may close the stream at any time and reopen it later with `Last-Event-ID`. The most important changes while it is closed are also sent as a push notification; section 9 (push notifications) lists which, and to whom. Everything else is on the scan when the app reads it again.

**Opening the stream of a scan at rest** (no question open or `answering`, no guided check `analysing`, and `planned` or `failed`) replays the stored events after `Last-Event-ID` and then closes.

### Envelope

```ts
interface ScanEventBase {
  v: 1;
  seq: number;
  run: number;
  scan_id: Id;
  ts: Timestamp;
  type: ScanEventType;
}
```

### Event catalogue

| `type` | Extra fields | App behaviour |
|---|---|---|
| `run_started` | `reason: "submit" \| "resubmit" \| "species_changed", rerun_stages: Stage[]` | Reset the listed stages; clear their findings. Findings of other stages stay |
| `status_changed` | `from: ScanStatus, to: ScanStatus` | Update the status chip |
| `stage_started` | `stage: Stage` | Show the stage as in progress |
| `stage_completed` | `stage: Stage, duration_ms: number, skipped?: { reason: string }` | Tick the stage. `skipped` covers, for example, disease classification for an unsupported species |
| `finding` | `observation: Observation` | Add the finding; draw its region and mask on the photo. Findings arrive when their stage completes, before the job ends |
| `finding_retracted` | `observation_id: Id` | Remove a finding the job's final result didn't keep (section 5, partial results) |
| `weather_loaded` | `weather: WeatherSummary` | Show the weather strip |
| `candidates_selected` | `candidates: { problem_id: Slug; name: string; kind: ProblemKind; source: "classifier" \| "species" \| "lookalike" \| "organism" \| "damage" }[]` | "Checking 9 possible problems". Sent before the VLM stage, and again when a `verify` job adds a candidate the VLM found (source `organism` or `damage`) |
| `species_suggested` | `candidates: SpeciesCandidate[], working_species_id: Slug \| null, needs_confirmation: boolean` | Sent after `vlm_findings`, once BioCLIP and the VLM have both been compared. Show the confirmation card when `needs_confirmation` |
| `species_confirmed` | `species_id: Slug \| null, custom_name?: string, rerun: boolean` | Hide the card. A rerun is announced by `run_started` |
| `question` | `question: Question` | Show the question card |
| `question_resolved` | `question_id: Id, by: "user" \| "photo" \| "superseded" \| "dropped"` | Hide the card. `dropped`: the photo sent for it couldn't be analysed; the plan stays |
| `guided_check_done` | `check: GuidedCheck` | Show the texture result, or the failure |
| `diagnosis` | `diagnosis: Diagnosis` | Show the result |
| `plan_ready` | `plan_id: Id, provisional: boolean` | Fetch `GET /scans/{id}` and show the plan |
| `plan_superseded` | `old_plan_id: Id, new_plan_id: Id, reason: "late_answer" \| "species_changed" \| "chat_revision" \| "guided_check"` | Replace the plan, show a notice. The old plan's open tasks have been dismissed |
| `failed` | `failure: ScanFailure` | Show the error; offer "retry" when `retryable`, otherwise "new scan" |

When the scheduler continues a scan without an answer, there is no special event: `status_changed` (`awaiting_answer → diagnosed`) and a `diagnosis` with status `uncertain` say it, and the question stays open on the scan.

```ts
type Stage =
  | "plant_check" | "segmentation" | "species" | "disease" | "image_embedding"
  | "measurement" | "context" | "retrieve" | "vlm_findings" | "vlm_indicators" | "guided"
  | "verify" | "scoring" | "fusion" | "plan";

type ProblemKind = "fungal" | "bacterial" | "viral" | "pest" | "wildlife" | "abiotic";
```

The `measurement` stage appears only when a photo has depth: in a run with at least one such photo, and for a photo with depth sent for a question (section 7). For other scans it is never started and never listed in `rerun_stages`. `api` sends its stage events itself, on the GPU and the fallback path alike; workers never report it. Its `finding` events carry `measurement` observations (section 7). When there is depth but nothing could be measured (no gravity, no mask, no confident depth), it completes with `skipped: { reason: "nothing_measurable" }`; when measuring fails, with `skipped: { reason: "error" }`, and the run continues without measurements. Organism lengths are added to organism findings before their `finding` events are sent, so they need no stage of their own. When measurements answered every indicator, `vlm_indicators` completes with `skipped: { reason: "all_measured" }`.

Example of a typical run, where the classifier suggests early blight and the indicators confirm it:

```
1  run_started         submit, all stages
2  status_changed      pending → queued
3  status_changed      queued → analysing
4  stage_started       plant_check          5  stage_completed  plant_check
6  stage_started       segmentation         7  stage_completed  segmentation
8  stage_started       species              9  stage_completed  species
10 stage_started       disease              11 stage_completed  disease
12 finding             disease: early blight 0.71, leaf_closeup region with mask
13 stage_started       image_embedding      14 stage_completed  image_embedding
15 stage_started       context
16 weather_loaded
17 stage_completed     context
18 candidates_selected 9 problems
19 stage_started       vlm_findings         20 stage_completed  vlm_findings
21 finding             organism: ladybug larva (beneficial)
22 finding             soil: moisture dry
23 species_suggested   tomato, needs_confirmation: true
24 stage_started       vlm_indicators       25 stage_completed  vlm_indicators
26 stage_started       scoring              27 stage_completed  scoring
28 diagnosis           tomato_early_blight, confirmed
29 status_changed      analysing → diagnosed
30 stage_started       plan                 31 stage_completed  plan
32 status_changed      diagnosed → planned
33 plan_ready          provisional: false   (stream closes)
```

---

## 5. Worker API and jobs

### Leases, runs and attempts

- A worker holds at most **`slots`** jobs at once, declared in its claim and heartbeat. The broker never leases a worker more jobs than its `slots`:

  | Worker | `slots` |
  |---|---|
  | perception-worker (RTX 3070) | 1. It batches inside a job instead (section 5, results) |
  | vlm-worker (RTX 5070 Ti) | `VLM_SLOTS`: 1 or 2, deployment configuration set from the day-2 benchmark (open question 9). With 2, vLLM batches the two jobs' calls together; vLLM's `max_num_seqs` is set to the same value |
  | fallback-worker | Several (default 4), bounded by the LLM API's rate limit |

- `POST /internal/jobs/claim` returns a `lease_token` with each job. Every later call about that job sends it as `X-Lease-Token`.
- The lease lasts 2 minutes. A heartbeat that lists the job, with its lease token, in `current_jobs` extends it to 2 minutes from the heartbeat, but **only if the token matches, the job is leased to that worker and isn't cancelled**, and **never beyond `claimed_at + max_runtime`** for the job's kind:

  | Kind | `max_runtime` |
  |---|---|
  | `perception` | 2 minutes |
  | `vlm` | 5 minutes |
  | `verify`, `guided` | 3 minutes |
  | any kind on the fallback worker | twice the above |

  A worker whose heartbeat thread keeps running while its model hangs still loses the job at `max_runtime`. These limits hold at the configured `slots`: the day-2 benchmark checks that a `vlm` job's p95 time with `VLM_SLOTS` jobs running together stays under half its `max_runtime`.
- The broker requeues a job when its worker's heartbeat is older than 30 seconds, or when its lease expires. Requeueing invalidates the lease token.
- `attempts` counts `failed` completions with `retryable: true`, **plus every requeue after the 2nd**. A job that keeps crashing its worker therefore runs out of attempts. After the 3rd attempt, the job fails for good: a run's job sends the scan to `failed` with `processing_error`, resolving its open or `answering` question as `superseded` (section 3); a `verify` job for late work (section 3: the scan is `diagnosed` or `planned`) and every `guided` job report their failure as an event (`question_resolved` with `dropped`, or `guided_check_done` with a failed check) and leave the scan as it is. The segmentation-only `perception` job for a photo sent to a question (section 7, measurement) never fails the scan or the question: when it fails, the photo goes on to its `verify` job unmeasured.
- Every `perception`, `vlm` and `verify` job belongs to a scan **run**. A `guided` job belongs to the scan, not to a run: a new run never cancels it, and it never fails the scan. When a new run starts (a resubmit or a species change), the broker cancels the old run's queued and leased run jobs, and the old run's open question is resolved as `superseded`. A worker learns this from its next heartbeat response (`cancel_job_ids`) and stops.
- **Any call about a requeued, expired or cancelled job, including a job of an older run, gets `409 lease_lost`.** This holds for `events` and `complete` alike. The worker drops the job.
- Repeating `complete` with the same lease token for a job that already completed (a retry after a network timeout) returns `204` and changes nothing.
- `queued_at` is set when a job is queued and reset when it is requeued. A job still unclaimed 15 seconds after `queued_at` becomes claimable by the fallback worker, with one exception: a `perception` job with `question_id` (section 7, photos sent for a question) is claimable by the fallback worker only when no worker with the `perception` capability has sent a heartbeat in the last 30 seconds. Its only purpose is masks, which the fallback can't produce, so while the rig is up it waits for the perception worker even when that worker is busy.
- The `vlm` capability claims `vlm`, `verify` and `guided` jobs. `fallback` claims every kind.

### `POST /internal/workers/heartbeat`

Sent every 10 seconds.

```ts
type Capability = "perception" | "vlm" | "fallback";

interface Heartbeat {
  worker_id: string;              // stable per process, e.g. "rig-3070-perception-1"
  caps: Capability[];
  slots: number;                  // the most jobs this worker holds at once
  gpu?: { name: string; vram_used_mb: number; vram_total_mb: number };
  current_jobs: { job_id: Id; lease_token: string }[];  // extends these leases, within max_runtime
  versions: Record<string, string>; // model name → version, shown on /status
}

// 200 →
interface HeartbeatResponse {
  cancel_job_ids: Id[];           // stop these jobs now
}
```

### `POST /internal/jobs/claim`

Long poll.

```ts
interface ClaimRequest {
  worker_id: string;
  caps: Capability[];
  slots: number;                  // as in the heartbeat; a worker already holding `slots` jobs gets 204
  wait_s?: number;                // default 25, max 30
}
// 200 → Job, or 204 when nothing arrived within wait_s

interface Job {
  v: 1;
  job_id: Id;
  kind: "perception" | "vlm" | "verify" | "guided";
  scan_id: Id;
  run?: number;                   // absent for guided jobs, which don't belong to a run
  attempt: number;                // 1 on the first try
  lease_token: string;
  lease_until: Timestamp;
  max_runtime_s: number;
  payload: PerceptionPayload | VlmPayload | VerifyPayload | GuidedPayload;
}

interface JobImage {
  image_id: Id;
  kind: ImageKind;
  width: number;
  height: number;
  url: string;                    // GET /internal/jobs/{job_id}/images/{image_id}
}
```

### Payloads

```ts
interface PerceptionPayload {
  images: JobImage[];             // every plant and soil image of the scan, or only a question's photo (section 7);
                                  // no guided-check images; never depth
  stages: ("plant_check" | "segmentation" | "species" | "disease" | "image_embedding")[];
  species_id?: Slug;              // confirmed or user-chosen species: skip identification, use this
  skip_species_identification?: boolean;  // true when the user chose "something else" (no species_id)
  regions?: (Region & { region_id: string; class: RegionClass })[];  // from an earlier run, when segmentation isn't re-run
  label_set_version: string;      // BioCLIP label list generated from the knowledge base
  classifier_labels_version: string; // classifier class → problem mapping (section 10)
  question_id?: Id;               // set on a question photo's segmentation-only job (section 7): stages is ["segmentation"],
                                  // images holds that photo alone, and nothing it returns can fail the scan
}

interface Indicator {
  indicator_id: string;           // "tomato_early_blight.concentric_rings"
  question: string;               // asked to the VLM verbatim
  where: IndicatorWhere;
  image_kinds: ImageKind[];       // which shots can show it
}

interface CandidatePrompt {
  problem_id: Slug;
  name: string;
  indicators: Indicator[];        // its 3 discriminating ones, minus those a measurement settled (section 7);
                                  // a candidate with none left is not sent
}

interface VlmPayload {
  findings_images: JobImage[];                   // at most 4, fixed order (below)
  indicator_images: JobImage[];                  // at most 4, chosen from the indicators (below)
  working_species?: { id: Slug | null; name: string };
  species_options: { id: Slug; name: string }[];   // allowed species_guess values
  organism_options: { id: Slug; name: string }[];  // allowed organism labels
  candidates: CandidatePrompt[];                   // at most 9
}

interface VerifyPayload {
  reason: "outside_candidate_set" | "new_photo";
  question_id?: Id;               // with new_photo: the question the photo answers
  images: JobImage[];             // at most 4; for new_photo, the new shot first
  candidates: CandidatePrompt[];
}

interface GuidedPayload {
  image: JobImage;                // one squeeze_test or jar_test image, alone
}
```

**Photo routing to the VLM.** Every VLM call gets at most 4 images. Perception gets every plant and soil image.

| Call | Images |
|---|---|
| Findings | The first present, in this order: `whole_plant`, first `leaf_closeup`, `leaf_underside`, `soil_surface`, second `leaf_closeup`, `flower_fruit` |
| Indicators | Chosen by the questions being asked: each image kind present scores the number of the payload's indicators that list it in `image_kinds`. The 4 highest-scoring images are sent, ties broken by the findings order. `whole_plant` is included whenever any indicator lists it |
| Guided | The single `squeeze_test` or `jar_test` image, in its own `guided` job |
| Verify | The answering photo first, then the indicator-call rule for the rest |

The images go first in both prompts of the `vlm` job, so vLLM's prefix caching can reuse the image tokens when the two calls share images.

When measurements have answered every indicator (section 7), `candidates` and `indicator_images` are empty: the worker skips the indicators call, returns no answers, and reports `vlm_indicators` as skipped with reason `all_measured`.

### `POST /internal/jobs/{id}/events`

Progress while the job runs. The API turns these into scan events.

```ts
interface WorkerEvents {
  v: 1;
  events: (
    | { type: "stage_started"; stage: Stage }
    | { type: "stage_completed"; stage: Stage; duration_ms: number; skipped?: { reason: string } }
    | { type: "partial_result"; stage: Stage; result: PartialResult }
  )[];
}
// 204

type PartialResult =
  | { stage: "plant_check"; plant_present: boolean }
  | { stage: "segmentation"; regions: PerceptionResult["regions"] }
  | { stage: "species"; species: NonNullable<PerceptionResult["species"]> }
  | { stage: "disease"; disease: NonNullable<PerceptionResult["disease"]> }
  | { stage: "vlm_findings"; findings: VlmFindings };
```

**Partial results.** A `partial_result` carries the complete output of **one finished stage**, so findings reach the app before the job ends. The `complete` body repeats every stage. The API stores each stage's output once: if `complete` matches the partial result, nothing changes; if it differs, the `complete` version wins, and the API sends `finding` events for new findings and `finding_retracted` for dropped ones.

### `POST /internal/jobs/{id}/complete`

```ts
interface CompleteJob {
  v: 1;
  status: "done" | "failed";
  result?: PerceptionResult | VlmResult | VerifyResult | GuidedResult;   // with done
  error?: { code: string; message: string; retryable: boolean };          // with failed
  model_versions: Record<string, string>;
  timings_ms: Record<string, number>;
}
// 204
```

### Results

```ts
type RegionClass = "plant" | "leaf" | "soil" | "surroundings";

interface RleMask {
  size: [number, number];         // height, width of the stored image
  counts: string;                 // COCO-style compressed run-length encoding
}

interface PerceptionResult {
  plant_present?: boolean;        // present when plant_check was requested; false → the scan fails with no_plant_found
  regions: {
    region_id: string;
    image_id: Id;
    class: RegionClass;
    bbox: BBox;
    mask?: RleMask;               // the API stores it as a PNG and serves it through a signed URL
    confidence: Confidence;
  }[];
  species?: { candidates: { species_id: Slug | null; label: string; score: Confidence }[] };
  disease?: {
    image_id: Id;
    region_id?: string;           // absent when the classifier ran on the whole close-up
    top: { problem_id: Slug | "healthy"; confidence: Confidence }[];  // top 3, mapped through classifier_labels
  }[];
  disease_skipped?: { reason: "species_not_supported" | "no_leaves_found" };
  image_embeddings?: { image_id: Id; model: string; vector: number[] }[];
}

interface VlmResult {
  findings: VlmFindings;              // section 6, aliases already mapped back to IDs
  indicators: VlmIndicatorAnswers;    // section 6
}

interface VerifyResult {
  indicators: VlmIndicatorAnswers;
}

interface GuidedResult {
  texture: GuidedTexture;             // section 6
}
```

**Segmentation.** The leaf detector is fine-tuned on PlantDoc's leaf boxes, so `leaf` regions are reliable. PlantDoc has boxes but no mask outlines, so masks come from a SAM-class model prompted with those boxes, when it runs; masks are always optional. `plant`, `soil` and `surroundings` regions are produced when the model has them and are otherwise left out. Every later stage works without them: disease classification falls back to the whole `leaf_closeup` image (`region_id` absent).

**Batching inside a job.** The perception-worker runs **each model once per job** over a batch, not once per image or region: the plant check and segmentation over every image, the SAM-class model over every leaf box of an image in one prompt, BioCLIP and the image-embedding model over every plant image, and the disease classifier over every leaf crop. The ONNX models are exported with a dynamic batch dimension. A batch larger than `PERCEPTION_MAX_BATCH` (deployment configuration, default 16, set on day 2 so the 3070 keeps its VRAM headroom) is split into several. Partial results are still sent per stage, after the stage's last batch.

**Classifier candidates.** The worker restricts the classifier's output to the labels of the working species before taking each region's `top` 3, so a tomato leaf is never labelled with a grape disease. The API turns the per-region `top` lists into the scan's classifier candidates: the 3 distinct problems with the highest confidence across all regions, ignoring `healthy`.

The fallback worker returns the same shapes. For each `leaf_closeup` it judges to show leaves, it returns one `leaf` region covering the whole image. It leaves out the other regions, masks and `image_embeddings` it can't produce, and reports species and disease from the LLM API, with `model_versions` saying so. A job with `question_id` is completed at once with no regions and no LLM call, because whole-image regions have no mask to measure (section 7).

**Region IDs** are `<image_id>.<n>`, numbered per image by the worker that segments it. A question photo is a new image, so its regions can never collide with the run's existing ones.

---

## 6. VLM structured output

### Image preparation

Qwen2.5-VL-class models resize every image internally ("smart resize": each side rounded to a multiple of 28, total pixels kept between `min_pixels` and `max_pixels`) and return box coordinates in pixels of **that resized image**. To make "the image as sent" and "the image as seen" the same:

1. The worker resizes each image itself with the same rule, so both sides are multiples of 28 and the pixel count is at most `VLM_MAX_PIXELS`. This is deployment configuration shared by the worker and the vLLM server, not a payload field, because vLLM fixes it when the server starts (chosen on day 2, around 1 megapixel).
2. vLLM is started with `mm_processor_kwargs` setting the processor's `min_pixels` **low** (for example 256 × 28 × 28) and its `max_pixels` **equal to** `VLM_MAX_PIXELS`. Every prepared image then falls inside the processor's range, so it is not resized again.
3. The worker divides each returned box by the prepared image's width and height to get the normalised `BBox`.

### Box formats per model adapter

Models disagree on box coordinates. Each model adapter declares its format, and the worker converts:

| `box_format` | Used by | Conversion to `BBox` |
|---|---|---|
| `abs_px_prepared` | Qwen2.5-VL on vLLM with the preparation above | Divide by the prepared image's size |
| `norm_1000` | Hosted LLMs on the fallback worker, and VLMs trained on 0–1000 coordinates | Divide by 1000 |
| `norm_1` | Models that return 0–1 coordinates | None |

If the VLM runs on llama.cpp instead of vLLM, `mm_processor_kwargs` doesn't apply; its adapter declares the format that server produces. The day-2 benchmark of two VLMs records each one's `box_format`.

### Per-job schema

The worker builds the JSON Schema for each call from the payload and passes it to vLLM's structured-output (guided decoding) option, so the model can only produce valid values:

| In the schema | Alias | Mapped back to |
|---|---|---|
| Images | `img1` … `img4` | `image_id` |
| Indicator answers | object keys `q1_<slug>` … `qN_<slug>`, where `<slug>` is the indicator's own slug cut to 24 characters (`q3_concentric_rings`) | `indicator_id` |
| Species guess | an enum of `species_options` slugs plus `other` | `Slug` or `null` |
| Organism labels | an enum of `organism_options` slugs plus `other` | `Slug` or `null` |

Keys that carry the indicator's meaning make it much harder for the model to answer one question under another's number.

Text limits are `maxLength` in characters and `maxItems` for lists. Whether vLLM's structured-output backend enforces `maxLength` and `maxItems` is checked on day 2; if it doesn't, `max_tokens` bounds the output and the worker truncates over-long values.

### Findings call

The shape for a job with 3 findings images; the enums are filled from the payload.

```ts
type Certainty = "low" | "medium" | "high";
type ImageAlias = "img1" | "img2" | "img3";       // per call

type SoilReading<V> = { value: V; image: ImageAlias; certainty: Certainty } | null;  // null when not visible

interface VlmFindingsRaw {
  species_guess: {
    species: string;            // a species_options slug, or "other"
    other_name: string | null;  // with "other"
    certainty: Certainty;
  };
  soil: {
    moisture:  SoilReading<"dry" | "moist" | "wet" | "waterlogged">;
    surface:   SoilReading<"normal" | "crusted" | "cracked">;
    structure: SoilReading<"crumbly" | "cloddy" | "compacted" | "unclear">;
    colour:    SoilReading<"dark" | "medium" | "pale" | "grey_mottled">;
    cover:     SoilReading<"bare" | "mulched" | "residue" | "ground_cover">;
    deposits:  SoilReading<"none" | "salt_crust" | "moss" | "algae" | "mould">;
    erosion:   SoilReading<"none" | "rills" | "exposed_roots">;
    weeds:     SoilReading<"none" | "few" | "many">;
  };
  organisms: {                  // maxItems 10
    label: string;              // an organism_options slug, or "other"
    other_name: string | null;
    count: "one" | "few" | "many";
    image: ImageAlias;
    bbox_2d: [number, number, number, number];  // integers, in the adapter's box_format
    certainty: Certainty;
  }[];
  damage: {                     // maxItems 10
    pattern: DamagePattern;
    other_description: string | null;  // with "other", maxLength 100
    image: ImageAlias;
    bbox_2d: [number, number, number, number] | null;
    certainty: Certainty;
  }[];
  notes: string;                // maxLength 200
}
```

`DamagePattern`: `shot_holes`, `ragged_holes`, `chewed_margins`, `leaf_mines`, `sticky_residue`, `sooty_mould`, `webbing`, `slime_trails`, `frass`, `skeletonised_leaves`, `cut_stems`, `torn_stems`, `digging`, `tunnels_or_mounds`, `tracks`, `droppings`, `pecked_fruit`, `other`.

Earthworms are reported as organisms, not as a soil aspect.

### Indicators call

```ts
// one required key per indicator in the payload, e.g. "q1_concentric_rings"
type VlmIndicatorsRaw = Record<string, {
  reason: string;               // maxLength 60; written first, so the answer follows from it
  answer: "yes" | "no" | "unsure";
  image: ImageAlias | null;     // the shot that decided it
}>;
```

Every key is required, so guided decoding guarantees exactly one answer per indicator. When none of an indicator's `image_kinds` is among the call's images, that key's `answer` enum is `"unsure"` only.

### Guided-check call

```ts
type VlmGuidedRaw =
  | { kind: "squeeze_test"; result: "crumbles" | "holds_ball" | "sticky_ribbon" | "unclear"; certainty: Certainty }
  | { kind: "jar_test"; settled: boolean; texture: "sandy" | "loamy" | "silty" | "clayey" | "unclear"; certainty: Certainty };
```

The schema for a job contains only the branch for its image's kind.

### After decoding

The worker maps the aliases back and converts the result to the shapes the API stores:

```ts
type SoilAspect =
  | "moisture" | "surface" | "structure" | "colour" | "cover"
  | "deposits" | "erosion" | "weeds" | "texture";   // texture comes from guided checks

interface VlmFindings {
  species_guess: { species_id: Slug | null; other_name?: string; confidence: Confidence };
  soil: { aspect: SoilAspect; value: string; image_id: Id; confidence: Confidence }[];
  organisms: {
    organism_id: Slug | null;   // null for "other"
    other_name?: string;
    count: "one" | "few" | "many";
    region: Region;
    confidence: Confidence;
  }[];
  damage: {
    pattern: DamagePattern;
    other_description?: string;
    region?: Region;
    image_id: Id;
    confidence: Confidence;
  }[];
  notes: string;
}

interface VlmIndicatorAnswers {
  answers: {
    indicator_id: string;
    answer: "yes" | "no" | "unsure";
    reason: string;
    image_id?: Id;
  }[];
}

type GuidedTexture =
  | { kind: "squeeze_test"; result: "crumbles" | "holds_ball" | "sticky_ribbon" | "unclear"; confidence: Confidence }
  | { kind: "jar_test"; settled: boolean; texture: "sandy" | "loamy" | "silty" | "clayey" | "unclear"; confidence: Confidence };
```

- **Certainty as a word, not a number.** A quantized 7–8B model's self-reported probabilities are poorly calibrated. The worker maps `low`, `medium` and `high` to 0.4, 0.65 and 0.85, so a VLM finding can never show more than "high (0.85)".
- Measurements (section 7) are never put into a VLM prompt, so the VLM's answers stay independent of them.
- Guided decoding guarantees the shape; the worker still validates it. On a validation failure it retries the call once, then fails the job with `retryable: true`.
- A jar test with `settled: false` produces no texture observation; the app keeps the task to photograph the jar again later.
- The fallback worker sends the same per-call schema to the LLM API's structured-output feature, requesting `norm_1000` boxes, and relies on `max_tokens` and truncation where that feature ignores `maxLength` or `maxItems`.

---

## 7. Pipeline records

These are the records the app reads (through `GET /scans/{id}` and scan events) and that the pipeline stores.

### Observation

```ts
interface Observation {
  id: Id;
  scan_id: Id;
  run: number;
  stage: "plant_check" | "segmentation" | "species" | "disease" | "soil" | "organism" | "damage" | "measurement";
  pipeline_stage: Stage;             // the pipeline stage that produced it; runs replace observations by this
  label: string;                     // display text: "Early blight", "Ladybug larva", "Soil: dry", "Leaf"
  ref?: {                            // the knowledge-base record, when there is one
    table: "species" | "disease" | "organism";
    id: Slug;
  };
  role?: "pest" | "beneficial" | "neutral" | "invasive" | "unknown";  // organisms only; "unknown" for "other"
  detail?: {
    aspect?: SoilAspect;
    value?: string;
    pattern?: DamagePattern;
    count?: "one" | "few" | "many";
    region_class?: RegionClass;      // segmentation
    guided_kind?: "squeeze_test" | "jar_test";
    measurement?: Measurement;       // measurement observations
    length_mm?: number;              // organisms on a photo with depth; shown, not scored
  };
  confidence: Confidence;
  confidence_band: ConfidenceBand;
  region?: Region;
  source: "segmentation" | "bioclip" | "classifier" | "vlm" | "llm_fallback" | "user" | "measurement";
  model_version: string;             // for measurements: the algorithm version and depth_meta.device_model
  created_at: Timestamp;
}
```

### Measurement

Only for photos uploaded with depth (architecture 6.7). It runs in `api` after the perception result is stored, when a plant was found, before retrieval; and for a photo sent to a question, after that photo's segmentation (below). An error while measuring never fails a scan (section 4).

```ts
type MeasureQuantity =
  | "plant_height_cm" | "canopy_width_cm"
  | "leaf_length_cm" | "leaf_area_cm2"
  | "lesion_diameter_mm" | "lesion_count" | "affected_area_pct"
  | "hole_diameter_mm" | "hole_count";

interface Measurement {
  quantity: MeasureQuantity;
  value: number;                     // the median over the items (table below), or the single value
  min: number;                       // over the items; equal to value when n = 1
  max: number;
  n: number;                         // the number of items; 1 for height and canopy width
  distance_m: number;                // median camera distance over what was measured
}
```

| Quantity | From | Items | Needs |
|---|---|---|---|
| `plant_height_cm` | `whole_plant` | One value | Gravity and a ground plane. The plant clear of the top and bottom edges of the frame |
| `canopy_width_cm` | `whole_plant` | One value | Gravity. The plant clear of the side edges of the frame |
| `leaf_length_cm`, `leaf_area_cm2` | `leaf_closeup`, `leaf_underside` | Leaves with a mask | A leaf mask |
| `lesion_count`, `affected_area_pct` | `leaf_closeup`, `leaf_underside` | Leaves with a mask: each leaf's spot count, or the share of its area covered by spots and their yellowing | A leaf mask |
| `lesion_diameter_mm` | `leaf_closeup`, `leaf_underside` | Spots at least 4 pixels across | A leaf mask and one such spot |
| `hole_diameter_mm` | `leaf_closeup`, `leaf_underside` | Holes at least 4 pixels across | A leaf mask and one such hole |
| `hole_count` | `leaf_closeup`, `leaf_underside` | Leaves with a mask: each leaf's number of holes, of any size | A leaf mask |

There is one observation per image and quantity. Its `region` is always present, so it names its image: the plant's box for height and canopy width, otherwise the box around the measured leaves. A quantity whose needs aren't met is not reported. The fallback worker's whole-image regions have no mask, so on the fallback path only height and canopy width can be measured.

**Geometry.** These definitions are part of the contract, so measurements stay comparable between versions of the code:

- **Pixels to a plane.** A pixel (u, v) of the stored image is the ray K⁻¹ · (u, v, 1), with K the intrinsics scaled to the stored image, after the pixel is undistorted when `depth_meta.distortion` was sent. A point on a leaf is where that ray meets the leaf's plane.
- **Depth inside a mask** is read only after the mask is shrunk by two depth pixels, because depth along an outline mixes the leaf with what lies behind it.
- **Leaf plane:** fitted by RANSAC to the depth inside the shrunken mask, from pixels with medium or high depth confidence. Its residual is the RMS distance of the inliers to the plane.
- **Leaf length** is the longest distance between two points of the mask's outline projected onto the plane. **Leaf area** is the projected area of the mask.
- **Spots** are connected groups of pixels inside a leaf mask that the `SpotSegmenter` marks as necrotic: brown, black or pale against the leaf's own green. Yellow pixels around them count toward `affected_area_pct`, never toward a spot's size, because sources give lesion sizes without the halo. A spot's diameter is that of the circle with its projected area. Spots under 4 pixels across are counted but not sized.
- **Holes** are gaps enclosed by a leaf mask's outline, sized like spots. Holes under 4 pixels across are counted in `hole_count` but not sized.
- **Ground plane.** Found first, among the confident depth points whose surface normal is within 15° of gravity. RANSAC is run repeatedly, removing each plane's inliers, and every plane with at least 5% of those points is a candidate. The **base** is estimated without the plant's points, so the rule isn't circular: the lowest 5% along gravity of the confident points inside the `plant` mask or box when segmentation gave one, otherwise inside the central third of the frame. The ground is the **highest candidate at or below the base**, allowing 5 cm above it where leaves touch the soil. So a raised bed's soil wins over the path beside it, and flat leaves (squash, lettuce) or a bench above the base are never ground. Points within 3 cm of it are ground, not plant. A whole-plant shot with no candidate at or below the base (the base is out of frame, or the plant is held up close) gets no height; its width can still be measured.
- **Plant points** are the `plant` region's mask when it has one. Otherwise they are the connected depth layer around the centre of the frame (neighbouring depth pixels within 5 cm of each other), grown only through points that aren't ground, and kept inside the `plant` box when there is one. Without that step the layer would run into the ground at the base and measure the visible soil. **Height** runs along gravity from the ground plane to their 98th percentile; when a `plant` mask's lowest points sit more than 5 cm above the ground (a pot or a planter on a table), it runs from those points instead, so the pot isn't counted. Without a mask a pot counts as plant, which the confidence rules below cap at medium. **Canopy width** is their spread along the horizontal direction perpendicular to the line of sight, from the 2nd to the 98th percentile. The edge rules in the table above apply to the plant points after the ground is removed.
- **Organism length** (`detail.length_mm`) is the longer side of the organism's box, at the median depth inside the box. It isn't corrected for tilt, and it is shown, never scored. `api` adds it when it stores the VLM findings, before their `finding` events are sent.

**Confidence.** Each measurement is rated on the criteria that apply to it. Its band is the lowest that any criterion reaches, mapped to the VLM's values:

| Criterion | high (0.85) | medium (0.65) | Applies to |
|---|---|---|---|
| Coverage: the share of the shrunken region's pixels with medium or high depth confidence | 60% or more | 30% or more | All |
| Plane residual (RMS) | 5 mm or less | 10 mm or less | Leaf, spot and hole quantities |
| Median distance against the kind's valid range: 0.4–4 m for `whole_plant`, 0.25–0.6 m for the leaf kinds. The leaf kinds' lower bound is LiDAR's shortest reliable distance, the same value as the close-up distance hint (architecture 6.7), and moves with it after the day-1 check | Inside | Up to 20% beyond the upper end. Below the lower end, depth isn't reliable, so the measurement is low | All |
| Plant region | A `plant` mask | The connected depth layer | Height and canopy width |
| Depth map | Unfiltered, or with a confidence map | `filtered: true` without a confidence map | All |

A measurement below medium on any criterion is low. Low measurements are shown but never answer an indicator and never enter the growth series. Without a `depth_confidence` map, every non-zero depth pixel counts as medium.

**Measurable indicators.** Only a profile's discriminating indicators are asked, so only their `measure` (section 10) is used. Before a `vlm` or `verify` job is queued, `api` answers each measurable indicator of the job's candidates from the best measurement of its quantity (highest confidence, then largest `n`) that has medium confidence or more and comes from an image whose kind is in the indicator's `image_kinds`. The indicator's `stat` picks `value` (`median`), `max` or `min`. The margin m at each bound is 10% of the bound, but at least two stored-image pixels at the measured distance for lengths and diameters (2 × `distance_m` / `fx` metres, with `fx` in stored-image pixels: × 1000 for a quantity in mm, × 100 for one in cm), and at least 1 for counts:

| Measured value | Answer |
|---|---|
| Inside the range by at least m from each bound it has | `yes` |
| Outside the range by more than m | `no` |
| Within m of a bound, or no usable measurement | Not answered: the indicator goes to the VLM as before |

The answer is stored as `IndicatorEvidence` with `source: "measurement"`, `measured`, and a generated `reason` ("Median spot 2.1 mm; range 1.5–3 mm").

- Each indicator keeps one answer, ranked `user`, then `measurement`, then `vlm`. A user's answer to the same indicator replaces a measured one.
- A measured indicator leaves the job's `CandidatePrompt`, and a candidate with none left is not sent. When no candidate is left, a `vlm` job skips its indicators call (section 5), a `verify` job isn't queued, and the scan is rescored straight away.
- A question is chosen only from indicators that neither the user nor a measurement has answered.
- For the `healthy` status (below), a measured `no` counts as evidence of health only when the image it came from shows nothing on any leaf: its `lesion_count` and `hole_count` observations both have a `max` of 0, so no measured leaf has a spot or a hole of any size. A median can be 0 while some leaves are spotted, and a size `no` means something of the wrong size was found, so neither counts on its own. A measured `no` always counts against its candidate.

**Photos sent for a question.** A photo uploaded with `question_id` and depth, when one of the question's candidates has a measurable indicator that neither the user nor a measurement has answered yet (a VLM answer, `unsure` included, doesn't count) and whose `image_kinds` include the photo's kind, first gets a `perception` job of the same run with `stages: ["segmentation"]`, `question_id` set, and that photo alone (section 5). The fallback worker takes such a job only when the rig is offline (section 5), and then completes it at once with no regions and makes no LLM call. `api` then measures the photo, answers what the measurements settle, and queues the `verify` job for the rest, or rescores straight away when nothing is left. The photo's regions and measurements are added to the run's observations and replace nothing. This segmentation job never fails the scan or drops the question: if it fails, or its regions have no mask, only what needs no mask is measured (the height and canopy width of a `whole_plant` photo) before the photo goes to its `verify` job. A photo without depth, or for a question with nothing measurable, goes straight to `verify` as before.

**Growth.** Measurements of `plant_height_cm` and `canopy_width_cm` with medium confidence or more form the plant's growth series (`GET /plants/{id}/measurements`, section 9): one point per scan and quantity: the highest-confidence measurement among the scan's **current** observations, the set `ScanDetail.observations` shows, whichever run produced it. A species-change run keeps the earlier run's measurements with their earlier `run`, so filtering on the scan's current `run` would drop the scan from the series. Each point is dated by the scan's `created_at`, when the photos were taken, so re-measuring a scan never moves its point. Fusion adds the risk `growth_stalled` (level `low`) when all of these hold: the plant's species has a `mature_height_cm`; the latest height in the series is from the past 7 days and below `mature_height_cm`; and the most recent height taken at least 14 days before it differs from it by less than 2 cm or 5% of the earlier height, whichever is larger.

### WeatherSummary

```ts
interface WeatherSummary {
  provider: "open_meteo";
  fetched_at: Timestamp;
  location_precision: "exact" | "about_1_km" | "about_10_km" | "postal_code" | "city";
  current: { temp_c: number; rh_pct: number; wind_kmh: number; precip_mm_last_24h: number };
  past_7_days: { precip_mm: number; mean_rh_pct: number; hours_rh_over_90: number };
  forecast: {
    date: LocalDate;
    t_min_c: number;
    t_max_c: number;
    precip_mm: number;
    mean_rh_pct: number;
    hours_rh_over_90: number;
    frost_risk: boolean;
  }[];
  soil: {
    moisture_0_9cm_m3m3: number;
    moisture_9_27cm_m3m3: number;
    temp_6cm_c: number;
    et0_mm_last_24h: number;
  };
}
```

`hours_rh_over_90` is the stand-in for leaf wetness, which Open-Meteo doesn't provide. Disease profiles state their wetness condition in the same unit (section 10).

**Fetching and caching.** Scans and the scheduler use the same `WeatherSummary`, fetched and cached the same way:

- The cache key is a **cache cell**: the garden's stored coordinates rounded to 0.01° (about 1 km, the resolution of the weather models), plus the hour. Gardens stored at about 10 km, by postal code or by city already share their coordinates.
- A scan reads its garden's cell and fetches only on a miss.
- The scheduler groups all gardens by cell, fetches each cell **once**, and sends the missing cells to Open-Meteo in multi-location requests (comma-separated `latitude` and `longitude`) of at most 50 cells. Open-Meteo's fair-use limit counts each location in such a request, so the quota saving comes from fetching each cell once; the batching saves round trips.
- The scheduler then evaluates every garden's trigger rules from its cell's summary, so scans in the same hour reuse what the scheduler fetched.
- A failed multi-location request is retried once, then its cells are fetched one by one; a cell that still fails skips its gardens until the next run.

### Diagnosis

```ts
interface IndicatorEvidence {
  indicator_id: string;
  text: string;                      // "Concentric rings in the spots"
  weight: 1 | 2 | 3;
  answer: "yes" | "no" | "unsure";
  reason: string;
  source: "vlm" | "user" | "measurement";
  measured?: { quantity: MeasureQuantity; value: number; image_id: Id };  // with source "measurement"
}

interface CandidateScore {
  problem_id: Slug;
  name: string;
  kind: ProblemKind;
  score: number;                     // sum of matched weights minus contradicted weights
  matched: IndicatorEvidence[];
  contradicted: IndicatorEvidence[];
  unsure: IndicatorEvidence[];
  weather_fit: "favourable" | "neutral" | "unfavourable";
}

interface Citation {
  source_id: Slug;
  title: string;
  publisher: string;
  url: string;
}

interface Diagnosis {
  id: Id;
  scan_id: Id;
  run: number;
  supersedes?: Id;                   // the earlier diagnosis of the same scan, after a late answer, a rerun or a guided check
  status: "confirmed" | "uncertain" | "healthy";
  low_evidence: boolean;             // uncertain, and nothing points to any problem (below)
  primary?: CandidateScore;          // absent when healthy
  candidates: CandidateScore[];      // ranked, at most 9
  organisms: {
    observation_id: Id;
    organism_id: Slug | null;
    name: string;
    role: "pest" | "beneficial" | "neutral" | "invasive" | "unknown";
    reporting_url?: string;          // invasive only: the app shows "How to report"
    preys_on?: { organism_id: Slug; name: string }[];  // beneficial only: the pests it eats, from the preys_on graph when read; never stored
  }[];
  soil: Observation[];               // the soil observations the diagnosis used, guided checks included
  risks: {
    risk: string;                    // "fungal_spread", "root_rot", "frost_damage", "growth_stalled"
    level: "low" | "medium" | "high";
    drivers: string[];               // "4 days over 90% humidity forecast", "Crowded: nearest plant 30 cm away"
  }[];
  evidence_basis: {                  // architecture 6.4: user input, then photo, then modelled data
    user_input: string[];            // from shared memories only (section 8)
    photo: string[];                 // measurements included: "Median spot 2.1 mm"
    modelled: string[];
  };
  citations: Citation[];
  model_versions: Record<string, string>;
  created_at: Timestamp;
}
```

The status is decided in this order, and the first row that holds wins:

| `status` | When |
|---|---|
| `healthy` | **All** of these hold: no candidate scored above zero; no pest organism and no damage pattern was found with medium confidence or more; no classifier candidate other than `healthy` has medium confidence or more; and there is **positive evidence of health**: the classifier's highest-confidence result across the leaf regions is `healthy` with medium confidence or more, or every candidate has at least 2 of its 3 discriminating indicators answered `no` (a measured `no` counts only when its image shows no spot on any measured leaf and no hole: see measurement above). The plan is a short care plan |
| `confirmed` | The top candidate scored above zero, leads the second by at least the fusion margin (open question 3), and no question is open |
| `uncertain` | Everything else: the lead is too small, every score is zero without positive evidence of health, or the scan was continued without an answer. The plan is provisional |

**`low_evidence`** is true exactly when `status` is `uncertain` and **nothing points to any problem**:
- no candidate scored above zero;
- no pest organism and no damage pattern was found with medium confidence or more;
- no classifier candidate other than `healthy` has medium confidence or more.

Those are the first three conditions of `healthy`, without its positive evidence. A classifier result of early blight at 0.71 with every indicator `unsure` (the example below) is therefore `uncertain` but not `low_evidence`: the app shows the possible problem and its question.

With `low_evidence` the app names no problem (architecture 6.1, low confidence). It asks for a closer photo through the open `photo_request` question when there is one, and through a new scan otherwise, and it suggests a local extension office. A guided check's new diagnosis copies it.

A result that is neither `healthy` nor `confirmed` is **ambiguous**: the run asks a question if it is still `analysing` or `awaiting_answer` and has asked fewer than 2, and is otherwise diagnosed `uncertain` (a late answer or a guided check never asks one). When every score is zero because the indicators came back `unsure`, the question is a `photo_request` for the shot those indicators need, usually a `leaf_closeup`. Two examples: a distant whole-plant photo where only the species is recognised and every indicator is `unsure` is not called healthy; and a classifier result of early blight at 0.71 with the VLM's indicators `unsure` is ambiguous, never `healthy` next to an early-blight finding.

### ActionPlan

```ts
type PlanWhen =
  | { kind: "now" }
  | { kind: "by_date"; due: LocalDate }
  | { kind: "recurring"; every_days: number; until?: LocalDate }          // "check for new spots every 3 days"
  | { kind: "on_trigger"; trigger: TriggerKind; until?: LocalDate };     // "after the next rain, remove splashed leaves"

// one vocabulary for forecast alerts, plan triggers and Task.trigger
type TriggerKind = "frost" | "heat" | "humid_stretch" | "heavy_rain" | "after_rain" | "dry_soil";

interface PlanStep {
  id: string;
  order: number;
  title: string;                     // "Remove the lowest infected leaves"
  detail: string;
  treatment_id?: Slug;               // the knowledge-base treatment the step applies, when there is one
  category: "cultural" | "biological" | "mechanical" | "chemical" | "monitoring" | "wildlife_deterrent";
  when: PlanWhen;
  targets: { table: "disease" | "organism"; id: Slug }[];  // what the step acts on; may be empty
  plant_id?: Id;                     // a neighbour this monitoring step is about (garden map); its task goes to that plant
  citations: Slug[];                 // source ids; at least one
  safety_note?: string;              // added by the server to every chemical step (below)
}

interface ActionPlan {
  id: Id;
  scan_id: Id;
  plant_id: Id;
  version: number;
  supersedes?: Id;
  provisional: boolean;              // true exactly when the diagnosis it was written from is "uncertain"
  diagnosis_id: Id;                  // the diagnosis this version was written from
  open_question?: Question;          // the question still open when this version was written
  constraints: {                     // from confirmed chat revisions; carried into every later version
    text: string;                    // "No copper products"
    from_pending_action_id: Id;
    created_at: Timestamp;
  }[];
  summary: string;
  steps: PlanStep[];
  watch_for: string[];               // "New spots on upper leaves within 5 days"
  citations: Citation[];             // every source the steps cite, resolved
  task_ids: Id[];
  llm_model: string;
  prompt_version: string;
  created_at: Timestamp;
}
```

**Steps to tasks.**

| `when` | Task |
|---|---|
| `now` | A task due today |
| `by_date` | A task due on that date |
| `recurring` | A task with `repeat`; completing it creates the next occurrence until `until` |
| `on_trigger` | No task yet. A plan trigger rule is registered with the scheduler, which creates the task when its rule fires (architecture 8.3), until `until` or until the plan is superseded. The task's `alert_key` is `plan:<plan_id>:<plan_step_id>:<date>`, under the same unique index as forecast alerts, so it is created at most once a day per step of that plan version even when scheduler runs overlap. Step ids are unique only within a plan, so the plan id keeps two plants' `s3` steps, or `s3` in two versions of one plan, apart |

A step's task belongs to the step's `plant_id` when it has one, otherwise to the plan's plant. An `on_trigger` step's trigger rule records that same plant, the one its tasks will be created on.

**When a neighbour is deleted.** Stored plans are never edited outside the plan writes below. Deleting plant B deletes the tasks and trigger rules that other plants' plans made for B (section 9, deletion cascades), but those plans keep their steps with `plant_id` = B, and the app shows such a step as "plant removed", with no task.

**Neighbours for the plan writer.** When the plant is pinned on the current garden map (section 9) and the diagnosis's primary problem, or for an `uncertain` diagnosis any remaining candidate, is of a kind that spreads (`fungal`, `bacterial`, `viral` or `pest`), the plan writer is given up to 5 neighbours, nearest first, as `{ plant_id, name, species_name, distance_m, problems: Slug[] }`. `problems` are the spreading problems among those that the neighbour's species, confirmed or suggested, is `susceptible_to`. A neighbour with no species, or susceptible to none of them, is left out. Healthy diagnoses get no neighbours. A new map version doesn't re-plan (the reasons below are the only ones): it is used the next time a plan is written.

**Re-planning.** A new plan version is written for one of these reasons, and only these:

| Reason | Trigger |
|---|---|
| `late_answer` | An answer or photo for a question after the scheduler continued without it |
| `species_changed` | A species-change run reaches `planned` again |
| `guided_check` | A guided check's new diagnosis (below) differs in texture from the diagnosis the current plan was written from, or that diagnosis had no texture |
| `chat_revision` | A confirmed `revise_plan` card |

**Every plan write of a scan runs one at a time**, queued through the same per-scan writer as its events: a run's first plan, automatic re-plans and confirmed chat revisions alike. Each write reads the current diagnosis and the current plan when it starts, and before storing checks that the plan it replaces is still the scan's `plan_id` (or that the scan still has no plan, for a first plan).

- **Automatic re-plans** (`late_answer`, `species_changed`, `guided_check`) that lose this race start again from the new current plan.
- **A chat revision** never calls the plan writer and is never retried. Confirming stores exactly the validated preview the card showed (section 8): the plan at `base_plan_id` with `changed_steps` applied, `removed_step_ids` dropped and `constraints_added` appended. If `base_plan_id` is no longer the current plan when its turn comes, nothing is stored and `confirm` returns `409 invalid_state` with `current_plan_id`.
- A chat revision's constraints are stored on the plan and given to the plan writer for every later version, so a later automatic re-plan keeps "no copper".

**Guided checks and the diagnosis.** A guided check that finishes while the scan is `diagnosed` or `planned` writes a **new diagnosis** for the current run: a copy of the current one that `supersedes` it, with `soil` updated to the latest check of each kind and fusion's `risks` recomputed. Candidate scores and `status` don't change. It then queues a `guided_check` re-plan, which does nothing if the scan has no plan yet (the run's first plan write reads the new diagnosis when it starts) or if the current plan was already written from a diagnosis with the same texture. A check that finishes while the scan is `analysing` or `awaiting_answer` writes no diagnosis: the run's diagnosis uses it. A check that finishes while the scan is `pending` or `queued` writes no diagnosis and claims no step: the run reads it when it starts. A check that finishes while the scan is `failed` writes no diagnosis and claims no step either: its result stays on the scan, and a resubmitted run reads it.

When a plan is **superseded**, the system dismisses its open tasks (`dismissed_reason: "plan_superseded"`) and removes its trigger rules, so the user never sees steps from an outdated diagnosis. The new plan creates its own tasks.

**Plan validation.** The plan writer's output is checked before it is stored. A plan that fails is sent back to the plan writer once with the reasons, then stored without the failing steps:

| Rule | Check |
|---|---|
| No doses | The shape has no dose, rate or mixing-ratio field. In addition, the dose check below runs on `summary`, `watch_for`, and the `title` and `detail` of **every** step, whatever its category |
| The category comes from the knowledge base | When a step has a `treatment_id`, its `category` is taken from that treatment's `kind`, never from the plan writer |
| Products make a step chemical | A step that mentions a pest-control product (pesticide, fungicide, insecticide, herbicide, copper, sulfur, neem, insecticidal soap, horticultural oil, Bt, spinosad, pyrethrin, and the list in the knowledge base) is re-categorised as `chemical` |
| Chemical steps carry the safety note | The server adds a fixed `safety_note` to every `chemical` step: "Use only a product registered for this use where you live, and follow its label exactly." The plan writer never writes it |
| Beneficials are never targeted | A step whose `targets` include an organism with role `beneficial` is rejected. Organisms reported as `other` have role `unknown` and cannot be targeted |
| Beneficials are not harmed by the way | When the scan found a beneficial organism, a `chemical` step naming a product marked `broad_spectrum` in `products.yaml` is rejected, and the plan writer is asked for a targeted option instead |
| Every step is cited | `citations` has at least one source, and every source appears in `ActionPlan.citations` |
| Neighbour steps | A step with `plant_id` must be `monitoring`, that plant must be one of the neighbours given to the plan writer (above), and the step's `targets` must include one of that neighbour's `problems`, or an organism one of them is `caused_by`. A `revise_plan` preview is checked against the neighbour list computed from the current map when the preview is made, not the list the base plan was written with; confirming re-checks only that each named plant still exists (section 8) |

**The dose check.** Case-insensitive, with word boundaries (`\b`) around every unit, so "2 lowest leaves" or "3 large leaves" don't match. A text fails when it contains:

- a quantity followed by a measure: the quantity is a number (`2`, `0.5`, `1/2`, `½`, `¼`) or a number word (`one` … `twenty`, `half`, `a`, `an`), and the measure is one of `ml`, `millilitre(s)`, `milliliter(s)`, `l`, `litre(s)`, `liter(s)`, `fl oz`, `oz`, `ounce(s)`, `pint(s)`, `quart(s)`, `gallon(s)`, `gal`, `g`, `gram(s)`, `kg`, `mg`, `lb(s)`, `pound(s)`, `tbsp`, `tablespoon(s)`, `tsp`, `teaspoon(s)`, `spoonful(s)`, `cup(s)`, `drop(s)`, `capful(s)`, `pinch(es)`, `ppm`. The everyday words `drop`, `cup`, `spoonful`, `capful` and `pinch` count only when `of` follows within three words ("a drop of soap", "two cups of water"), so "a drop in temperature" and "leaves curl into a cup" pass;
- "a few drops", "a splash" or "a squirt" followed by `of` within three words;
- a rate: `per litre`, `per liter`, `per gallon`, `/l`, `/gal`, `ml/l`, `g/l`;
- a ratio: `\d+\s*:\s*\d+`; "<quantity> part(s) … to <quantity> part(s)" ("one part soap to ten parts water"); or "<quantity> part(s) … to <quantity>" and "<quantity> in <quantity>" in a sentence that also contains a product name or a mixture word ("mix 1 in 10 with water"). In ratios a quantity is a number or `one` … `twenty`, never `a`, `an` or `half`, so "cut a part of the stem back to a healthy bud", "plant one in a pot" and "remove one in three canes" pass;
- a percentage in the same sentence as a product name or a mixture word: "a 2% soap spray" fails; "more than 50% of the leaves" and "above 90% humidity" pass.

The mixture words are `solution`, `mix`, `mixture`, `spray`, `dilute`, `concentration`.

Time, spacing and measured sizes ("2 weeks", "every 3 days", "10 cm apart", "30 cm deep", "spots about 2 mm across") don't match any measure and pass. Three known false positives: "1 gallon sprayer"; clock times such as "10:30", which match the ratio form; and a measured share in a sentence with a mixture word ("spots cover 12% of the leaf, so spray…"). The plan writer and the chat prompt are told to describe equipment without sizes, to write times of day in words ("early morning"), and to give measured shares in a sentence of their own.

The same check runs on **everything the chat model writes**, whatever the question's route (section 8), because a routine care question can still get a dose in its answer:

- **Reply text** is checked one sentence at a time before each sentence is streamed. A failing sentence is replaced by the fixed safety note.
- **Tool-call arguments**: every string the model writes into a call to `create_task`, `complete_task`, `revise_plan`, `request_rescan` or `remember` (task titles and details, card text, plan summaries, steps and constraints, rescan instructions, memory text) is checked before the tool runs, so before any pending action, task, memory or event exists. A failing call is rejected and the model gets the reason as the tool result, so it can rewrite the call without the amount. The card's fixed labels are written by the server.
- **A `revise_plan` preview** also passes the full plan validation above, on the plan it would produce, before the card is created. A failing step is sent back to the model with the reason, as for a tool call, never dropped silently, so the card shows exactly what confirming stores.

### Task

```ts
interface Task {
  id: Id;
  garden_id: Id;
  plant_id?: Id;
  plan_id?: Id;
  plan_step_id?: string;
  scan_id?: Id;                      // a guided-check task's scan: the jar test photographed later
  title: string;
  detail?: string;
  due?: LocalDate;
  repeat?: { every_days: number; until?: LocalDate };
  source: "plan" | "forecast_alert" | "chat" | "user";
  trigger?: { kind: TriggerKind; date: LocalDate };
  done_at?: Timestamp;
  dismissed_at?: Timestamp;
  dismissed_reason?: "user" | "plan_superseded";
  created_at: Timestamp;
}
```

`alert_key` stays server-side: `<garden_id>:<trigger>:<date>` for forecast alerts, `plan:<plan_id>:<plan_step_id>:<date>` for plan triggers. Forecast-alert and plan tasks can be completed or dismissed but not deleted, so the scheduler never recreates a task the user removed.

**Tasks only on live plants.** Every write that creates a task with a `plant_id` (`POST /tasks`, a confirmed `create_task`, a plan, a plan trigger rule firing, a repeating task's next occurrence) checks, in the same transaction, that the plant exists in the garden and has no `deleting_at`. The schema accepts a link to a missing record, so this check is what keeps a task from pointing at a deleted plant. A write that loses the check creates nothing: the scheduler skips the rule, and the endpoints answer as below. A plan write is not aborted by it: only that step's task, or its trigger rule (which the same check guards when it is registered), is skipped, and the stored step shows "plant removed", as when a neighbour is deleted later (section 7).

---

## 8. Chat and memory

### Access

Conversations are private to their owner. Every chat endpoint checks the conversation's owner **and** the garden membership; anyone else gets `404`. Write tools are offered only to editors and owners (section 1).

### Endpoints

```ts
interface CreateConversationRequest {
  garden_id: Id;
  anchor: { type: "scan" | "plant" | "garden"; id: Id };
}

interface Conversation {
  id: Id;
  garden_id: Id;
  anchor: { type: "scan" | "plant" | "garden"; id: Id };
  title?: string;
  created_at: Timestamp;
  last_message_at?: Timestamp;
}

interface Message {
  id: Id;
  role: "user" | "assistant";
  text: string;
  attachments: { image_id: Id; url: string }[];
  citations: Citation[];
  pending_actions: PendingAction[];
  interrupted: boolean;              // the stream dropped before message_completed
  created_at: Timestamp;
}

interface ConversationDetail extends Conversation {
  messages: Page<Message>;           // newest first; ?cursor= for older
}
```

| Endpoint | Body | Response |
|---|---|---|
| `POST /conversations` | `CreateConversationRequest` | `201 Conversation` |
| `GET /conversations?garden_id=&anchor_type=&anchor_id=` | | `200 Page<Conversation>`, the caller's own, newest `last_message_at` first |
| `GET /conversations/{id}` | | `200 ConversationDetail` |
| `POST /conversations/{id}/images` | `multipart/form-data` with `file` | `201 { image_id: Id, url: string }`. Same checks as scan uploads; depth parts are ignored |
| `GET /conversations/{id}/images/{image_id}` | | The image, through its signed URL |
| `POST /conversations/{id}/messages` | `SendMessageRequest` | An SSE stream (below) |
| `POST /messages/{id}/feedback` | `{ rating: "up" \| "down", notes?: string }` | `204`, owner only, assistant messages only |

Tool calls and tool results are stored but never sent to the app.

### Sending a message

```ts
interface SendMessageRequest {
  text: string;                      // at most 2,000 characters
  image_ids?: Id[];                  // from POST /conversations/{id}/images, at most 3
}
```

- Errors **before** the stream starts (auth, budget, validation) are ordinary HTTP errors with the section 2 body.
- Errors **after** it starts arrive as an `error` event, and the stream ends.
- Every assistant sentence passes the dose check (section 7) before it is streamed, and every string the model writes into a tool call passes it before the tool runs.
- The stream is **not resumable**: events have no `id:` line. If the client disconnects, the server stops the LLM call and stores the message with `interrupted: true`; the app shows it with a "resend" action, which sends a new `Idempotency-Key`.

| `event` | `data` |
|---|---|
| `message_started` | `{ v: 1, message_id: Id, route: "diagnosis_question" \| "care_question" \| "task_change" \| "safety_sensitive" \| "off_topic" }` |
| `text_delta` | `{ v: 1, text: string }` |
| `tool_started` | `{ v: 1, call_id: string, tool: string, label: string }`. `label` is for the UI: "Checking the forecast" |
| `tool_finished` | `{ v: 1, call_id: string, ok: boolean }` |
| `citation` | `{ v: 1, citation: Citation }` |
| `pending_action` | `{ v: 1, pending_action: PendingAction }` |
| `memory_saved` | `{ v: 1, memory: Memory }`. Shown as "Remembered: …" with **Undo** and **Share with garden** |
| `rescan_requested` | `{ v: 1, plant_id: Id, photo_kind: ImageKind, instructions: string }`. The app offers to open the capture flow for a new scan of that plant |
| `message_completed` | `{ v: 1, message_id: Id, usage: { tokens_in: number; tokens_out: number } }` |
| `error` | `{ v: 1, code: ErrorCode, message: string, retryable: boolean }` |

### Pending actions (confirmation cards)

```ts
interface PendingAction {
  id: Id;
  conversation_id: Id;
  tool: "create_task" | "complete_task" | "revise_plan";
  status: "proposed" | "confirmed" | "rejected" | "expired";
  card: {
    title: string;                   // "Add a task?"
    body: string;                    // "Spray the tomato with water to knock off aphids"
    confirm_label: string;           // "Add task"
    reject_label: string;            // "Not now"
  };
  preview:
    | { kind: "task"; task: Task }
    | {
        kind: "plan";
        base_plan_id: Id;
        plan_summary: string;
        changed_steps: PlanStep[];       // replace the base plan's step with the same id, or add a new one
        removed_step_ids: string[];
        constraints_added: string[];     // "No copper products"
      };
  expires_at: Timestamp;             // 24 hours
}
```

| Call | Result |
|---|---|
| `POST /pending-actions/{id}/confirm` on `proposed` | `200 { pending_action, result: { task?: Task; plan?: ActionPlan } }`. A confirmed plan revision stores the preview exactly as shown, as a new version with reason `chat_revision` (section 7, re-planning). Its card says that the plan is shared: "This changes the plan for everyone in the garden" |
| `confirm` on `confirmed` (a retry) | `200` with the same result as the first time |
| `confirm` on `rejected` or `expired` | `409 invalid_state` |
| `confirm` of a plan revision whose `base_plan_id` is no longer the current plan, including one that loses the race to an automatic re-plan | `409 invalid_state` with `current_plan_id`; nothing is stored, nothing is retried automatically, and the card offers to ask again |
| `confirm` of a plan revision with a step whose `plant_id` names a plant deleted, or being deleted, since the preview was made | `409 invalid_state` with `hint: "neighbour_removed"`; nothing is stored, and the card offers to ask again |
| `confirm` of a `create_task` whose plant was deleted, or is being deleted, since the card was made | `409 invalid_state` with `hint: "plant_removed"`; nothing is stored |
| `POST /pending-actions/{id}/reject` | `200 { pending_action }` |

All pending-action calls check the conversation's owner, the garden membership and the editor role. Every model-written string in a pending action's `card` and `preview` passed the dose check (section 7) when the tool call was made, and a plan preview passed plan validation.

### Memory

```ts
interface Memory {
  id: Id;
  garden_id: Id;
  plant_id?: Id;
  kind: "fact" | "event" | "summary";
  text: string;
  source: "pipeline" | "chat" | "user";
  shared: boolean;                   // false for chat memories until their author shares them
  created_at: Timestamp;
}
```

| `source` | Written by | Visible to | Used by the plan writer |
|---|---|---|---|
| `pipeline` | The pipeline after each scan | Every member | Yes |
| `chat` | Automatic extraction and the `remember` tool, from one member's conversation | Only its author's conversations | No, until shared |
| `user` | A chat memory its author chose to share, or a garden note an editor added | Every member | Yes |

| Endpoint | Who | Effect |
|---|---|---|
| `GET /memories?garden_id=&plant_id=` | any member | `200 Page<Memory>`: shared memories, plus the caller's own chat memories; newest first |
| `POST /memories` | editor | `{ garden_id, plant_id?, text }` → `201 Memory` with `source: "user"` (a garden note) |
| `POST /memories/{id}/share` | the chat memory's author, if they are an editor or owner | `200 Memory` with `source: "user"`, `shared: true` |
| `DELETE /memories/{id}` | the author of a chat memory; an owner for shared memories | `204` (the "Undo" of `memory_saved`) |

Because the plan writer reads only shared memories, a plan that every member sees never quotes something a member said in a private chat. `evidence_basis.user_input` lists only shared memories.

### Tool return shapes and scope

The agent's tools return the same records the app sees. Every tool runs as the session user, inside the conversation's garden. Viewers and reviewers get only the read tools.

| Tool | Scope | Returns |
|---|---|---|
| `get_scan` | Scans of the garden | `ScanDetail` without image URLs |
| `view_image` | The garden's scan images and this conversation's own images, nothing else | The image to the model; `{ ok: true }` to the log |
| `get_forecast` | The garden | `WeatherSummary["forecast"]` |
| `search_memory` | Shared memories of the garden, plus the caller's own chat memories | `Memory[]` |
| `find_similar_cases` | Scans of the garden | `{ scan_id: Id; created_at: Timestamp; primary?: { problem_id: Slug; name: string }; similarity: number; thumbnail_image_id?: Id }[]`, at most 5 |
| `search_knowledge` | Knowledge base | `{ text: string; citation: Citation }[]` |
| `get_species_profile`, `get_disease_profile`, `get_organism_profile` | Knowledge base | The seed records of section 10 |
| `compare_lookalikes` | Knowledge base | `{ problem_id: Slug; name: string; how_to_tell: string }[]` |
| `get_soil_profile` | The garden | `{ soil_profile: SoilProfile \| null; latest: WeatherSummary["soil"]; texture?: Observation }` |
| `get_plant_measurements` | Plants of the garden | `MeasurementPoint[]` (section 9): the latest 50, oldest first |
| `get_garden_map` | The garden | `{ map: GardenMap \| null; neighbours: Record<Id, Id[]> }` (section 9) |
| `list_tasks` | The garden | `Task[]` |
| `remember` | The garden | `Memory` with `source: "chat"`, and a `memory_saved` event |
| `request_rescan` | Plants of the garden | `{ plant_id: Id; photo_kind: ImageKind; instructions: string }`, and a `rescan_requested` event |
| `create_task`, `complete_task`, `revise_plan` | The garden; editors and owners only | `PendingAction` |

---

## 9. Gardens, plants, tasks and account

### Gardens

```ts
interface GardenLocationInput {
  lat?: number;                      // with location consent
  lon?: number;
  precision?: "exact" | "about_1_km";
  postal_code?: string;              // without consent; geocoded once
  city?: string;                     // without consent, instead of a postal code: "Guelph, ON"; geocoded once
}
// exactly one of: lat and lon, postal_code, city

interface Garden {
  id: Id;
  name: string;
  my_role: "owner" | "editor" | "viewer" | "reviewer";
  location_label: string;            // "Toronto, ON"; raw coordinates are never returned
  location_precision: "exact" | "about_1_km" | "about_10_km" | "postal_code" | "city";
  time_zone: string;                 // IANA, e.g. "America/Toronto"; derived from the location; sets every LocalDate
  soil_profile_status: "pending" | "ready" | "unavailable";
  member_count: number;
  created_at: Timestamp;
}
```

| Endpoint | Role | Body | Response |
|---|---|---|---|
| `GET /gardens` | any | | `200 Page<Garden>`, the caller's, by name |
| `POST /gardens` | signed in | `{ name, location: GardenLocationInput }` | `201 Garden`; the caller becomes owner; SoilGrids is fetched in the background |
| `GET /gardens/{id}` | any | | `200 Garden` |
| `PATCH /gardens/{id}` | owner | `{ name?, location? }` | `200 Garden` |
| `DELETE /gardens/{id}` | owner | | `202`; the cascade below runs in the background. The other members get a `garden_deleted` push (push notifications, below) |
| `GET /gardens/{id}/weather` | any | | `200 { weather: WeatherSummary }`: the garden's weather cache cell for the current hour (section 7, fetching and caching), fetched on a miss exactly as a scan does. `503 upstream_unavailable` when the fetch fails |

### Members and invites

Members join with **invite codes**, so the system never stores or looks up email addresses.

| Endpoint | Role | Body | Response |
|---|---|---|---|
| `GET /gardens/{id}/members` | any | | `200 Page<{ user_id, display_name, role, since }>`, by `since` |
| `GET /gardens/{id}/invites` | owner | | `200 Page<Invite>`, unused and unexpired, newest first |
| `POST /gardens/{id}/invites` | owner | `{ role: "owner" \| "editor" \| "viewer" \| "reviewer" }` | `201 Invite` |
| `DELETE /gardens/{id}/invites/{invite_id}` | owner | | `204`; the code stops working |
| `POST /invites/{code}/accept` | signed in | | `200 Garden`. An existing member gets `200` with no change to their role. Unknown, used, revoked or expired codes get `404`. Rate-limited to 10 attempts per user per minute (`429`) |
| `PATCH /gardens/{id}/members/{user_id}` | owner | `{ role }` | `200` member. Making someone `owner` adds an owner. Demoting the last owner: `409 invalid_state` |
| `DELETE /gardens/{id}/members/{user_id}` | owner, or the member themselves | | `204`. The last owner can't leave: `409 invalid_state` |

```ts
interface Invite {
  id: Id;
  code: string;                      // 26 random base32 characters (128 bits); shared as a link
  role: "owner" | "editor" | "viewer" | "reviewer";
  created_by: Id;
  expires_at: Timestamp;             // 7 days
}
```

**Removing a member** (by an owner, or by leaving) runs the shared-garden step of the account deletion cascade for that garden only: their conversations in the garden with their messages, chat images, pending actions, message feedback and unshared chat memories are deleted. Their scans and shared memories stay with the garden, with `created_by` kept. If they are invited again, they start with no chats.

### Plants

| Endpoint | Role | Body | Response |
|---|---|---|---|
| `GET /plants?garden_id=` | any | | `200 Page<Plant>`, by name |
| `POST /plants` | editor | `{ garden_id, name, species_id? }` | `201 Plant` |
| `GET /plants/{id}` | any | | `200 Plant` |
| `PATCH /plants/{id}` | editor | `{ name?, status? }` | `200 Plant` |
| `DELETE /plants/{id}` | owner | | `202`; the cascade below runs in the background |
| `GET /plants/{id}/timeline` | any | | `200 Page<TimelineEntry>`, newest first |
| `GET /plants/{id}/measurements?quantity=` | any | | `200 Page<MeasurementPoint>`, oldest first: the growth series (section 7) |

```ts
interface TimelineEntry {
  scan_id: Id;
  created_at: Timestamp;
  status: ScanStatus;
  diagnosis_status?: "confirmed" | "uncertain" | "healthy";
  primary?: { problem_id: Slug; name: string };
  thumbnail_image_id?: Id;           // the whole_plant shot, when there is one
  thumbnail_url?: string;            // signed URL of that shot
  failure_code?: ScanFailureCode;    // with status failed: "Failed: no plant found"
  plan_id?: Id;
  measurements?: { plant_height_cm?: number; canopy_width_cm?: number };  // medium confidence or more
}

interface MeasurementPoint {
  scan_id: Id;
  created_at: Timestamp;          // the scan's created_at, when the photos were taken
  quantity: "plant_height_cm" | "canopy_width_cm";
  value: number;
  confidence_band: ConfidenceBand;
}
```

The growth series has one point per scan and quantity, with medium confidence or more (section 7, growth).

`Plant.latest_scan` and `TimelineEntry.thumbnail_url` carry signed URLs like every other image (section 1), refreshed by reading the record again.

### Species

The knowledge base's species, for the species picker and the confirmation card. Read by any signed-in user; species aren't garden data.

| Endpoint | Response |
|---|---|
| `GET /species?q=` | `200 Page<SpeciesSummary>`, by common name. `q` matches common and scientific names, case-insensitively, anywhere in the name; without `q`, every species |
| `GET /species/{id}` | `200 SpeciesSummary`; an unknown slug gets `404` |
| `GET /species/{id}/images/{hash}` | The reference photo (`image/webp`), through its signed URL. `{hash}` is the photo's content hash (section 10) |

```ts
interface SpeciesSummary extends SpeciesRef {
  profile: "full" | "label_only";
  classifier_supported: boolean;     // the disease classifier covers it (architecture 6.2)
  reference_images: {                // at most 3, in seed order
    url: string;                     // signed
    credit: string;                  // "Photo: A. Author, CC BY-SA 4.0"; the app shows it under the photo
  }[];
}
```

`SpeciesCandidate.reference_image` is the first of these, with its credit, so the confirmation card needs no second call. It is computed from `species_id` each time a candidate is read or sent in an event, never stored with the candidate, so a reseed shows the new photo everywhere. Each photo's storage key carries a hash of its content (section 10), so its URL changes when the photo does.

### Garden map

Made on a LiDAR device (architecture 6.8). Viewing it needs no LiDAR.

| Endpoint | Role | Body | Response |
|---|---|---|---|
| `GET /gardens/{id}/map` | any | | `200 { map: GardenMap \| null }` |
| `PUT /gardens/{id}/map` | editor | `GardenMapInput` | `200 GardenMap`, a new version. `409 invalid_state` with `current_map_version` when `base_version` isn't the current version |

```ts
interface GardenMapInput {
  base_version: number;              // the version current when this mapping session started; 0 when the garden had no map
  beds: {                            // at most 20
    id: string;                      // the app's own id, unique within the map
    name?: string;
    polygon_m: [number, number][];   // ground plane, metres; 3–50 points, a simple polygon
  }[];
  pins: {                            // at most 200; a plant appears at most once
    plant_id?: Id;                   // exactly one of plant_id and new_plant
    new_plant?: { name: string; species_id?: Slug };
    x_m: number;                     // within 200 m of the origin
    y_m: number;
    canopy_radius_m?: number;
    bed_id?: string;
  }[];
  north_deg?: number;                // 0–360, clockwise from the map's +y axis to north; only with a compass accuracy of 20° or better
}

interface GardenMap {
  garden_id: Id;
  version: number;
  beds: { id: string; name?: string; polygon_m: [number, number][]; area_m2: number }[];
  pins: { plant_id: Id; x_m: number; y_m: number; canopy_radius_m?: number; bed_id?: string }[];
  north_deg?: number;
  created_by?: { user_id: Id; display_name: string };  // absent once that user is deleted
  created_at: Timestamp;
}
```

- The frame is local, in metres on the ground plane: the origin is where the AR session started, `x` is ARKit's world x axis and `y` its world −z axis, so the plan reads normally from above. Nothing in the map is tied to the garden's location.
- `north_deg` comes from the phone's compass during mapping, and is sent only when the reported heading accuracy is 20° or better. It and `canopy_radius_m` are shown on the map; no rule uses them.
- **Concurrent mapping.** The app reads `GET /gardens/{id}/map` when a mapping session starts, and sends that version as `base_version`. A `PUT` whose `base_version` isn't the current version gets `409 invalid_state` with `current_map_version`, so an edit started from an older map never silently replaces a newer one. The app then says the map changed while the user was mapping, and offers to reload it (and map again) or to replace it anyway, which resends with the new `base_version` only after the user confirms. The `ARWorldMap` the app saves for later edits is tagged with the version it produced; when that tag isn't the current version, the app doesn't offer an edit and maps from the start.
- A `PUT` is atomic. New plants are created, every pinned plant's `position` moves to the new version, and plants without a pin lose their position. The version is checked and incremented in the same transaction, so of two uploads from the same base, one succeeds and the other gets `409`. A `plant_id` that isn't a plant of this garden gets `400 invalid_request` with `{ field: "pins", problem: "unknown_plant", plant_ids }` listing every such plant, the same whether or not the ID exists elsewhere. A pin for a plant of this garden whose deletion has started is dropped from the upload without an error, and left out of the response, so a map uploaded while a plant is being deleted never pins it. Reads leave out pins of plants being deleted too.
- **Neighbours** of a plant are the plants pinned on the current map within `NEIGHBOUR_RADIUS_M` (1.5 m), centre to centre.
- **Crowding:** when a plant's species has a `care.spacing_cm`, the nearest other pin on the current map is closer than 0.75 × that spacing, and the forecast or the past 7 days hold a humid stretch, fusion adds the driver "Crowded: nearest plant 30 cm away, recommended spacing 60 cm" to `fungal_spread`, raising its level one step (at most `high`), or adding it at `low` when the weather alone didn't. Plants without a species, or whose species has no `spacing_cm`, are never called crowded.
- Deleting a plant removes its pin from the current map in place, without a new version, so a mapping session that started before still holds a valid `base_version`. If that session's upload still pins deleted plants, it gets `400 invalid_request` with `{ field: "pins", problem: "unknown_plant", plant_ids: [...] }`, listing every unknown plant at once. The app removes those pins, since the plants are gone, and resends with the same `base_version` and a **new** `Idempotency-Key`, because the `400` is stored under the old key and a different body would get `422 idempotency_conflict`.

### Deletion cascades

| Deleted | Also deleted | Jobs |
|---|---|---|
| A plant | Its scans with their images, depth maps, masks, observations, events, diagnoses, plans and questions; its tasks and plan trigger rules, including the neighbour tasks and trigger rules its plans made for other plants; the tasks and trigger rules other plants' plans made for it (those plans are not changed: section 7, when a neighbour is deleted); conversations anchored to the plant or its scans, with their messages, images, pending actions and feedback; memories about the plant; its pin on the garden map | Queued and leased jobs of its scans are cancelled, and their step claims and question steps cleared, so no running step can commit afterwards |
| A `pending` scan (`DELETE /scans/{id}`, section 3) | Its images with their depth maps, its guided checks with their observations and events, conversations anchored to it, and the tasks naming it (`Task.scan_id`) | Its guided jobs are cancelled |
| A garden | Everything under `gardens/{garden_id}/` in RustFS, and every record of the garden: plants (as above), tasks, memories, conversations, invites, memberships, the garden map | All its jobs are cancelled, and every step claim and question step cleared |

### Tasks

| Endpoint | Role | Body | Response |
|---|---|---|---|
| `GET /tasks?garden_id=&source=&state=open\|done\|dismissed` | any | | `200 Page<Task>`, by `due` (tasks without one last), then `created_at`. `?source=forecast_alert` is the alert inbox |
| `POST /tasks` | editor | `{ garden_id, plant_id?, scan_id?, title, detail?, due?, repeat? }` | `201 Task` with `source: "user"`. A `plant_id` that isn't a live plant of the garden, unknown or being deleted, gets `404 not_found`. A `scan_id` must be a live scan of the garden, and of that plant when `plant_id` is given: a scan that is missing, of another garden or plant, or has `deleting_at` gets `404 not_found`. Like the plant, it is checked in the same transaction as the insert, since the schema accepts a link to a missing scan. The app sets it on the task it creates when a jar test starts (section 3), so the jar photo can be uploaded to the same scan later from any device |
| `PATCH /tasks/{id}` | editor | `{ done?: boolean, dismissed?: boolean, title?, detail?, due?, repeat? }` | `200 Task`. Only `user` and `chat` tasks can change `title`, `detail`, `due` and `repeat`. Completing a repeating task creates the next one |
| `DELETE /tasks/{id}` | editor | | `204` for `user` and `chat` tasks; `409 invalid_state` for `plan` and `forecast_alert` tasks, which are dismissed instead |

### Account

| Endpoint | Body | Response |
|---|---|---|
| `GET /me` | | `200 { id, display_name, location_consent, created_at }` |
| `PATCH /me` | `{ display_name?, location_consent? }` | `200` the same shape. Withdrawing consent rounds the location of every garden the user owns to about 10 km (`about_10_km`); no reverse geocoding is needed |
| `DELETE /me` | | `202 { deleted_gardens: Id[], transferred_gardens: { garden_id: Id; new_owner_user_id: Id }[] }`. The cascade (architecture section 5) runs in the background, then the account is deleted at the auth provider too, retried until it succeeds, so signing in again doesn't recreate the user |
| `POST /me/push-tokens` | `{ token: string, platform: "ios" \| "android" \| "web" }` | `204`; registering the same token again is a no-op |
| `DELETE /me/push-tokens/{token}` | | `204`. The token is URL-encoded in the path (Expo tokens contain `[` and `]`) |

### Push notifications

Sent through the Expo Push API to every token the recipient registered with `platform` `ios` or `android`. Expo Push doesn't deliver to the web, so `web` tokens are stored and not yet used. The message has a short English title and body, and this payload as its `data`:

```ts
type PushPayload =
  | { v: 1; kind: "scan"; reason: "plan_ready" | "plan_superseded" | "question" | "failed";
      garden_id: Id; plant_id: Id; scan_id: Id }
  | { v: 1; kind: "alert"; garden_id: Id; task_id: Id; trigger: TriggerKind }
  | { v: 1; kind: "garden_deleted"; garden_name: string };
```

| `kind` | Sent when | To |
|---|---|---|
| `scan`, `plan_ready` | A run reaches `planned` (architecture 8.1) | The scan's creator, while still a member |
| `scan`, `plan_superseded` | A late answer, a guided check or a species change produces a new plan version. Not for `chat_revision`: the member who confirmed it is already looking at it | The scan's creator, while still a member |
| `scan`, `question` | A run asks a question | The scan's creator, while still a member |
| `scan`, `failed` | A scan fails | The scan's creator, while still a member |
| `alert` | A forecast alert creates its task (architecture 8.3) | Every member of the garden |
| `garden_deleted` | A garden is deleted by its owner, or by the account-deletion cascade (architecture 5) | Every other member it had |

Nothing else is pushed: a guided check that doesn't change the plan, a dropped photo answer and other late events are seen when the scan is next opened. The app ignores a payload with an unknown `v` or `kind`, and suppresses a `scan` notification for the scan already on screen. A push is a hint, never the only copy: the alert inbox, the timeline and the scan itself hold the same information.

### Auth webhook

`POST /webhooks/auth`, from Clerk, delivered through Svix. The adapter verifies the `svix-id`, `svix-timestamp` and `svix-signature` headers against the endpoint's signing secret before parsing the body, rejects a timestamp more than 5 minutes old, and maps Clerk's `user.created` and `user.deleted` events to:

```ts
interface AuthWebhookEvent {
  type: "user.created" | "user.deleted";
  data: { id: string };              // the provider's user id, stored as user.auth_id
}
// 204, idempotent: a repeated event changes nothing
```

### Ops

`GET /status` needs no sign-in and returns no user data:

```ts
interface Status {
  workers: { worker_id: string; caps: Capability[]; healthy: boolean; last_heartbeat: Timestamp; gpu?: string }[];
  queue: { queued: number; leased: number };
}
```

`GET /healthz` returns `200 { ok: true }` when the API can reach SurrealDB and RustFS.

---

## 10. Knowledge seed format

### Files

```
ml/knowledge/seed/
├─ sources.yaml
├─ species/<species_id>.yaml
├─ problems/<problem_id>.yaml        (the `disease` table: every kind of problem)
├─ organisms/<organism_id>.yaml
├─ treatments/<treatment_id>.yaml
├─ products.yaml                     (pest-control product names for the plan validator)
├─ soil_conditions.yaml
├─ classifier_labels.yaml
└─ chunks/<source_id>.jsonl
```

- YAML, because a teammate reviews every profile by hand and YAML diffs read well in pull requests. Chunks are JSON Lines, because they are generated, not hand-written.
- The ingestion scripts validate every file against the JSON Schema generated from the `domain` types, then generate the `.surql` migration. A file that fails validation blocks the migration.
- **IDs are slugs** used directly as SurrealDB record keys, **unique across all tables**, and stable across environments, because indicator IDs appear in jobs, diagnoses and the evaluation set. **An ID is never reused for a different thing.**
- Indicator IDs are `<problem_id>.<indicator_slug>`.
- `looks_like` is written once, in one of the two problem files. The loader writes both directions.
- Treatments are linked **only** from the problem's `treatments` list. The loader writes the `manages` edges from it.

### Problem profile

```ts
type IndicatorWhere =
  | "lower_leaves" | "upper_leaves" | "whole_plant" | "leaf_underside"
  | "stem" | "fruit" | "soil";

interface ProblemSeed {
  id: Slug;
  name: string;
  kind: ProblemKind;
  affects: Slug[];                   // species → susceptible_to edges
  caused_by: Slug[];                 // organisms, for pest and wildlife problems
  damage_patterns: DamagePattern[];  // patterns that point to this problem (section 6)
  prevalence: "common" | "occasional" | "rare";  // tie-breaker in the candidate ranking
  indicators: {
    id: string;                      // "<problem_id>.<slug>"
    text: string;
    question: string;                // asked to the VLM verbatim
    weight: 1 | 2 | 3;
    discriminating: boolean;         // exactly 3 per problem
    where: IndicatorWhere;
    image_kinds: ImageKind[];
    measure?: IndicatorMeasure;      // answered from LiDAR measurements when possible (section 7)
  }[];
  conditions: {
    temp_c?: { min: number; max: number };
    rh_pct_min?: number;
    humid_hours_min?: number;        // hours with RH over 90% in the past 7 days (leaf-wetness proxy)
    notes?: string;
  };
  looks_like: { problem: Slug; how_to_tell: string }[];
  treatments: Slug[];
  sources: Slug[];
  reviewed_by: string;
  reviewed_at: LocalDate;
}

interface IndicatorMeasure {
  quantity: MeasureQuantity;
  stat: "median" | "max" | "min";
  min?: number;                      // at least one of min and max, in the quantity's unit
  max?: number;
}
```

An indicator with a `measure` keeps its `question`, because photos without depth are still asked it. It states the size and nothing else, so a measurement answers the whole indicator; a size that comes with other signs ("small spots with dark centres") is split into two indicators. Only discriminating indicators are asked, so a `measure` on any other indicator is never used. Ranges come from the sources, like the rest of the profile.

Example. The content illustrates the format; the real profiles are written from the curated sources and reviewed.

```yaml
id: tomato_early_blight
name: Early blight
kind: fungal
affects: [tomato, potato]
caused_by: []
damage_patterns: []
prevalence: common
indicators:
  - id: tomato_early_blight.concentric_rings
    text: Brown spots with concentric rings, like a target
    question: Do the brown spots have rings inside them, like a target?
    weight: 3
    discriminating: true
    where: lower_leaves
    image_kinds: [leaf_closeup]
  - id: tomato_early_blight.lower_leaves_first
    text: Spots started on the oldest, lowest leaves
    question: Are the spots mostly on the lowest leaves of the plant?
    weight: 2
    discriminating: true
    where: whole_plant
    image_kinds: [whole_plant]
  - id: tomato_early_blight.yellow_halo
    text: Yellowing around each spot
    question: Is there a yellow ring around the brown spots?
    weight: 1
    discriminating: true
    where: lower_leaves
    image_kinds: [leaf_closeup]
conditions:
  temp_c: { min: 24, max: 29 }
  rh_pct_min: 90
  humid_hours_min: 24
  notes: Spreads in warm, humid weather with long periods of wet leaves.
looks_like:
  - problem: tomato_septoria_leaf_spot
    how_to_tell: Septoria spots are smaller, with no rings, and have tiny black dots in a pale centre.
treatments: [remove_lower_infected_leaves, mulch_soil_surface, avoid_overhead_watering]
sources: [example_extension_tomato_diseases]
reviewed_by: <teammate>
reviewed_at: 2026-10-20
```

A measurable indicator, from the look-alike's profile:

```yaml
# problems/tomato_septoria_leaf_spot.yaml (excerpt)
  - id: tomato_septoria_leaf_spot.small_spots
    text: Spots about 1.5–3 mm across
    question: Are the spots small, about the size of a pinhead or a little bigger?
    weight: 3
    discriminating: true
    where: lower_leaves
    image_kinds: [leaf_closeup]
    measure: { quantity: lesion_diameter_mm, stat: median, min: 1.5, max: 3 }
```

### Other entity shapes

```ts
interface SourceSeed {
  id: Slug;
  title: string;
  publisher: string;
  url: string;
  licence?: string;
  retrieved_at: LocalDate;
}

interface SpeciesSeed {
  id: Slug;
  scientific_name: string;
  common_names: string[];
  profile: "full" | "label_only";    // label_only: recognised by BioCLIP, no care profile or problems yet
  care?: {                           // required when profile is "full"
    water: "low" | "medium" | "high";
    light: "full_sun" | "part_shade" | "shade";
    soil_ph: { min: number; max: number };
    hardiness_zones?: string;        // Canadian plant hardiness zones, e.g. "annual" or "4-8"
    spacing_cm?: number;             // recommended distance between plants; the garden map's crowding rule
    mature_height_cm?: number;       // the growth_stalled rule stops here
    notes?: string;
  };
  classifier_supported: boolean;     // checked against classifier_labels.yaml at ingestion
  bioclip_label: string;             // the text prompt BioCLIP scores against
  sources: Slug[];                   // may be empty for label_only
  reference_images?: {               // at most 3, for the species confirmation card
    file: string;                    // under ml/knowledge/seed/images/, e.g. "species/tomato-1.jpg"
    source_id: Slug;                 // a source whose licence allows reuse with credit
    credit: string;                  // "Photo: A. Author, CC BY-SA 4.0", shown with the photo
  }[];
  reviewed_by: string;
}

interface OrganismSeed {
  id: Slug;
  name: string;
  scientific_name: string;
  taxon_group: "insect" | "mite" | "mollusc" | "worm" | "mammal" | "bird";
  role: "pest" | "beneficial" | "neutral" | "invasive";
  recognition: string;               // how to recognise it in a photo
  preys_on?: Slug[];                 // beneficial → pest edges
  reporting_url?: string;            // required when role is "invasive"
  bioclip_label: string;
  sources: Slug[];
  reviewed_by: string;
}

interface TreatmentSeed {
  id: Slug;
  kind: "cultural" | "biological" | "mechanical" | "chemical";
  text: string;                      // no doses, ever; checked like plan steps (section 7)
  sources: Slug[];
}

interface ProductsSeed {             // products.yaml
  products: {
    name: string;                    // matched case-insensitively; a step naming one becomes chemical
    broad_spectrum: boolean;         // harms beneficial insects too (section 7)
  }[];
}

interface ClassifierLabels {         // classifier_labels.yaml
  model_version: string;
  labels: {
    index: number;                   // the classifier's output index
    label: string;                   // the training dataset's class name
    species_id: Slug;
    problem_id: Slug | "healthy";
  }[];
}

interface ChunkSeed {                // one line of chunks/<source_id>.jsonl
  id: string;                        // "<source_id>.<n>"
  source_id: Slug;
  crop?: Slug;
  problem?: Slug;
  organism?: Slug;
  topic: "symptoms" | "conditions" | "management" | "care" | "recognition";
  text: string;                      // a short excerpt, at most about 200 words
}

interface SoilProfile {              // garden.soil_profile, from SoilGrids
  fetched_at: Timestamp;
  depths: {
    depth_cm: "0-5" | "5-15" | "15-30" | "30-60" | "60-100" | "100-200";
    ph_h2o?: number;
    clay_pct?: number;
    sand_pct?: number;
    silt_pct?: number;
    organic_carbon_g_kg?: number;
    nitrogen_g_kg?: number;
    cec_cmol_kg?: number;
    bulk_density_kg_dm3?: number;
  }[];
}
```

A treatment's safety note is not seeded: the server adds the same fixed note to every chemical step (section 7).

Chunk embeddings are not stored in the seed. They are computed at load time with the pinned text-embedding model (architecture section 10), so changing the model re-embeds every chunk from the same text.

`soil_conditions.yaml` holds a description for each `SoilAspect` and value (section 6), with what to do and its sources.

### Checks and generated files

The loader fails the build when:

- an ID is used twice, in any table;
- a problem has fewer or more than 3 `discriminating` indicators;
- a `looks_like`, `treatments`, `caused_by`, `affects`, `preys_on` or `sources` entry points to an unknown ID;
- a classifier label's `species_id` or `problem_id` is unknown, or `classifier_supported` disagrees with `classifier_labels.yaml`;
- a `full` species has no `care`;
- any treatment text fails the dose check of section 7;
- an invasive organism has no `reporting_url`;
- a species has more than 3 `reference_images`, a file is missing, or its source has no `licence`;
- a `measure` names a quantity that none of the indicator's `image_kinds` can measure (section 7), has neither `min` nor `max`, or has `min` ≥ `max`.

It generates, alongside the migration:

- the BioCLIP label list (`bioclip_label` of every species, `full` and `label_only`, and every organism) with its `label_set_version`;
- the `species_options` and `organism_options` lists for VLM payloads;
- the `classifier_labels_version` for perception payloads.

The loader also converts each reference image to WebP (long side 1024 pixels, metadata stripped), writes it to RustFS under `knowledge/species/{species_id}/{hash}.webp`, where `{hash}` is the first 16 hex digits of the WebP file's SHA-256, with its own storage identity ([storage 2](../storage/README.md#2-bucket-keys-and-access)), and deletes the objects of images no longer in the seed. RustFS is reachable only on the control plane's Docker network, so this upload runs there as a one-shot container, **before** the API starts with the migration that references the photos ([storage 2](../storage/README.md#2-bucket-keys-and-access)). A changed photo therefore gets a new key and a new URL, and an app never shows a cached old photo next to a new credit.

---

## 11. Changes to the architecture document

The contracts changed these parts of the architecture document, which has been updated to match:

| Change | Where in the architecture document |
|---|---|
| Scan runs; species-change runs that keep earlier stages; questions limited per run and never after `planned`; late answers always re-plan; resubmit only for retryable failures; post-`planned` work never fails the scan | 8.1, 6.2, 6.3 |
| Lease backstop (`max_runtime`), requeue counting, worker `slots` (1 for perception, `VLM_SLOTS` for the VLM), `409 lease_lost` for cancelled and old-run jobs, `guided` jobs | 8.4, 10, 5 (job schema) |
| Batching: each perception model once per job, weather by cache cell with multi-location requests for the scheduler, ingestion embeddings in batches | 5 (ingestion), 6.1, 8.3, 10 |
| Photo routing per VLM call; guided soil checks in their own call; full soil aspects; masks from a SAM-class model | 6.1, 6.4, 6.6, 10 |
| Plan steps: recurring and trigger-based, tasks dismissed when a plan is superseded, server-added safety note, the dose check on every step, every chat reply and every model-written tool argument; plan-trigger keys scoped to their plan; serialised plan writes that keep chat constraints, with chat revisions stored as previewed; `provisional` exactly when `uncertain` | 6.3, 8.2, 8.3, 11 |
| Guided checks outside runs and kind limits, each with its own diagnosis; readable-plant rule for `photos_unusable`; `healthy` only on positive evidence; one late-work rule; `answering` questions | 6.3, 6.4, 8.1 |
| Memory: chat memories private until shared; plan writer reads only shared memories | 7 |
| Role matrix; 404 for non-members; invites; member removal; deletion cascades; signed image URLs; daily scan limit; provider account deletion | 4, 5, 9, 11 |
| LiDAR: depth uploads, the `measurement` stage, measurable indicators (also on photos sent for a question), the growth series, the garden map with neighbours, spacing and neighbour plan steps | 1, 2, 3, 5, 6.1, 6.2, 6.3, 6.6, 6.7, 6.8, 7, 8.1, 8.3, 8.4, 8.5, 9, 10, 11, 12, 13, 14, 15 |
| What the screens need: thumbnails, garden weather, the species list with reference photos, prey of beneficials, `low_evidence`, a task's scan, the push payload, stream closing with a guided check running, quality hints, a city as location, `retryable` on `429`s, deleting a pending scan, Clerk and Svix | 3, 4, 5, 6.1, 6.2, 6.5, 8.1, 8.3, 8.5, 9, 12, 13, 15 |
| The v0.8 audit: event URLs signed when sent, the stream kept open for any running guided check, the narrower `low_evidence`, photo credits on candidates, `TimelineEntry.failure_code`, push coverage, feedback after submit, jar-test tasks deleted with a pending scan, and the knowledge photo upload on the control plane | 3, 5, 6.1, 8.1, 12 |
| The third audit: a failed scan supersedes its question and keeps a late guided check for a resubmit, `POST /tasks` checks its scan, species photos computed when read and keyed by a content hash, the loader's `seed-upload` container before `api`, and `low_evidence`'s threshold | 3, 5, 6.1, 8.4, 9, 12 |

---

## 12. Open questions

1. **`VLM_MAX_PIXELS`** for the VLM: around 1 megapixel per image keeps 4 images within context and VRAM on the 5070 Ti. Measure on day 2, with whether vLLM's structured-output backend enforces `maxLength` and `maxItems`.
2. **Certainty mapping** for VLM answers (0.4 / 0.65 / 0.85): calibrate against the evaluation set on Oct 24.
3. **Diagnosis margin:** how far the top candidate must lead the second to count as `confirmed`. Set from the evaluation set.
4. **Indicator weights** of 1–3: enough resolution, or does scoring need finer weights?
5. **Expiry times:** pending actions 24 hours, invites 7 days, signed URLs 15 minutes.
6. **Species-change window:** re-running only the plant's most recent scan from the last 24 hours.
7. **Daily scan limit** of 30 per user, and whether the fallback worker needs its own spending cap.
8. **HEIC decoding** in the Rust image pipeline (libheif bindings): confirm on day 1; until then, the app converts HEIC to JPEG before upload.
9. **`VLM_SLOTS`**: 2 if the day-2 benchmark shows that two `vlm` jobs of 4 images each fit the 5070 Ti's KV cache next to the model at `VLM_MAX_PIXELS`, and that a job's p95 time with two running stays under half its `max_runtime` (2.5 minutes); otherwise 1. Measure alongside open question 1, with `PERCEPTION_MAX_BATCH` on the 3070.
10. **Measurement thresholds:** the indicator margin (10% of the bound, at least two pixels), the confidence rules (60% coverage, 5 mm residual), the spot colour thresholds, the smallest sized spot (4 pixels) and the stored size of photos with depth (a long side of at least 2048 pixels), the growth noise floor (2 cm or 5%), the crowding factor (0.75) and `NEIGHBOUR_RADIUS_M` (1.5 m). Set from LiDAR photos in the evaluation set.
11. **LiDAR capture:** the shortest distance at which LiDAR depth is reliable, the depth map's size from the LiDAR depth camera, whether unfiltered depth keeps its holes, and whether the photo and its depth carry lens distortion (which decides whether `DepthMeta.distortion` is sent). Day-1 check; the distance sets the leaf kinds' valid range and the close-up distance hint. If AVFoundation's path fails the check, capture moves to ARKit's scene depth, which also sends `depth_confidence`.
12. **Timestamp precision:** section 1 fixes RFC 3339 in UTC but not the precision. The backend skills send whole seconds (`2026-10-18T14:03:22Z`, as in every example here) and keep full precision in the database. Confirm whole seconds, or allow fractions.
13. **Unknown fields in app requests:** the backend rejects a request body with a field it doesn't know (`400 invalid_request` naming the field), so a misspelt optional field isn't silently ignored. Section 2 lists malformed JSON and missing or wrong-typed fields only. Confirm, and say that the API must be deployed before an app that sends a new field. Worker requests keep ignoring unknown fields.
14. **A malformed ID:** a path or body ID that isn't a ULID can name no record. The backend answers `400 invalid_request` naming the field; section 2 says only that an unknown ID is `404`. Choose `400` or `404`.
15. **Oversized non-image bodies:** `413 upload_too_large` is defined for an image over 12 MB, and the app re-encodes the photo when it gets one. The backend answers a JSON body over its 1 MiB limit with `400 invalid_request` (`field: "body"`, the limit in the message). Confirm, or add a code.
16. **`405`, `408` and `504`:** section 2 has no code for a method a route doesn't have, or for a request the API timed out. The backend answers a missing method with `404 not_found` (without `Allow`), and a timeout with `500 internal`. Confirm, or add codes.
17. **Timeouts and `retryable`:** a request that times out answers `internal`, which is `retryable: false`, so the app doesn't retry it, even when a dependency was only slow. Decide whether a timeout is retryable, and with which code.
18. **Job error codes:** section 5 names only `unsupported_version` for a failed job's `error.code`. The workers also need a code for a job they stopped at `max_runtime_s` (`max_runtime_exceeded` in the backend skills) and for model failures (`model_error` as a placeholder). List the codes and whether each is retryable.
19. **Is `unsupported_version` retryable?** Section 1 says a worker fails a job with an unknown `v` but not with which `retryable`. The backend sends `false`: the same worker can't succeed on a retry, and another worker may claim the job only if it is requeued. Confirm, or say that the job should go back to the queue for a newer worker.
20. **Lease hand-back at shutdown:** a worker that stops while a claim is in flight may hold a lease it never learns about. It is requeued when the worker's heartbeat goes stale (30 seconds) or the lease expires. Add a call to hand leases back, or confirm the wait.
21. **An unknown job `kind`:** a worker that claims a job whose `kind` it doesn't know (an older worker against a newer API) can't read the payload. The backend treats it as a bug and fails the job. Decide whether the claim should filter by the kinds a worker declares (it does today, through `kinds`) and what `complete` reports if one slips through.
22. **The per-scan event writer's lifecycle:** section 4 requires one serialised writer per scan inside `api`, but not when it starts or stops. The backend skills start it on the first event and retire it only when no SSE subscriber is listening. Confirm, or fix when a writer is retired.
23. **A missing `X-Lease-Token`:** section 2 defines `409 lease_lost` for a wrong or stale lease token, but not for a worker call about a leased job that sends none. The backend answers `400 invalid_request` with `field: "X-Lease-Token"`. Confirm, or answer `lease_lost`.
24. **Auth webhook failures:** the auth webhook section says the adapter rejects a timestamp more than 5 minutes old, but not with which status, and not what a timestamp more than 5 minutes in the future gets. The backend answers a bad signature or a timestamp more than 5 minutes away in either direction with `401 unauthenticated`, as Svix recommends checking both directions, and a body it can't read after a good signature with `400 invalid_request`. Confirm the codes.
