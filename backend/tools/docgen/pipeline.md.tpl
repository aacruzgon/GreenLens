# PDM Backend: `pipeline` crate

> **Status:** Draft v0.5.4, 2026-10-23. v0.5.4: a perception job stores segmentation when its own stages include it, read from the job row that `start_run` commits with the run rather than from the run's `run_started`, which may be missing (section 4). v0.5.3: a species-change run's completion leaves the earlier run's segmentation and masks alone, a question job's partial result covers the question's photos alone and replaces nothing tied to no image, and masks put after the lease is lost are deleted (section 4). v0.5.2: `job_completed` stores a perception result's masks: each RLE is written to object storage as an 8-bit PNG before `complete_job` records it with the segmentation observations and sets its region's `mask_id`, and measurement finds a region's mask by that id (section 4). v0.5.1: a `guided` job's completion sends no catch-up stage events, since the `guided` stage is shared by the run's checks and the fold can't tell them apart; the catch-up wiring is tested through `api`'s fake store (`tests/pipeline.rs`); a failed read of the run's stage events is logged with its error chain; open questions 4 to 6 record the limits and the bug the audit found. v0.5: `job_completed` sends the stage events the stream missed before a job's findings (`catch_up::ResultStages`, read through `EventRepo::run_stage_events`): a stage left started gets its `stage_completed`, a stage of the result with no event in the run gets both, with `duration_ms: 0`, so a stage whose worker events were lost while `api` was down doesn't stay open (backend roadmap gap 9). v0.4: `PipelineDeps` gains `beliefs` (Agent Memory) and `PipelineConfig` its `beliefs_read_timeout`; the plan step reads the plant's current beliefs beside its shared memories and passes them to the plan writer, and the scan summary memory has topic `health`, so each new summary supersedes the last in Agent Memory. v0.3.1: open question 3 is resolved: the run's evidence fields are in the schema. v0.3: the cross-crate fixes: `ScanWriter` lost `job_queued` (the store's `Notify` is the only wake-up), `plan` releases its claim after 3 lost compare-and-sets, `worker_events` forwards a `guided` job's stage events on the scan's current run, `labels_loaded` splits the heartbeat's comma-separated lists, organism findings get `detail.length_mm` from the depth map before their `finding` events, and fusion counts a humid stretch in the past week (`past_7_days.longest_humid_run_days`). v0.2: fusion reads the shared `WeatherThresholds` (`PipelineConfig::weather`): a humid stretch is two days in a row with ≥ 10 hours over 90% each (the mean-humidity and past-week variants are gone), frost is a 2 m minimum ≤ 0.5 °C or the day's `frost_risk`, hard frost ≤ −2 °C; the memory summary is embedded with `EmbedMode::Document`; `PhotoAnswerStart::Segment` boxes its job. v0.1: `api`'s own work between a scan's jobs: the `retrieve`, `score` and `plan` steps under the scan's step claim, every entry point that claims them, measurable indicators, fusion, re-planning, runs and recovery, compiled on Rust 1.99.1 against the `domain`, `knowledge`, `agent` and `vision` crates.

Companion to the [backend architecture](README.md). It implements [contracts 3, steps between jobs](../architecture/contracts.md#scan-lifecycle), [contracts 4](../architecture/contracts.md#4-scan-events-sse) (event order), [contracts 7](../architecture/contracts.md#7-pipeline-records), [architecture 6](../architecture/README.md#6-analysis-pipeline), [8.1](../architecture/README.md#81-scan--diagnosis--plan) and [8.3](../architecture/README.md#83-forecast-alerts), and [database 4, steps between jobs](../database/README.md#steps-between-jobs-) and [plan writes](../database/README.md#plan-writes-compare-and-set-). The rules of retrieve-then-verify are in [knowledge](knowledge.md). Every listing is copied from `crates/pipeline` by the docgen tool. `cargo check --workspace`, `cargo clippy -p pipeline --all-targets` and `cargo test -p pipeline` pass.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [Ports and construction](#4-ports-and-construction)
5. [The step machine](#5-the-step-machine)
6. [Retrieve](#6-retrieve)
7. [Score](#7-score)
8. [Plan](#8-plan)
9. [Entry points](#9-entry-points)
10. [Events](#10-events)
11. [Runs](#11-runs)
12. [Idempotency and recovery](#12-idempotency-and-recovery)
13. [Concurrency and lifetimes](#13-concurrency-and-lifetimes)
14. [Errors](#14-errors)
15. [Testing](#15-testing)
16. [Domain changes](#16-domain-changes)
17. [Open questions and decisions](#17-open-questions-and-decisions)
18. [Sources](#18-sources)

---

## 1. Purpose and boundaries

**What it owns.** Everything `api` does to a scan between jobs ([contracts 3](../architecture/contracts.md#scan-lifecycle)):

- **The three steps.** `retrieve` (measurement, context, the candidate set, the measured answers, the `vlm` job), `score` (the outside-set `verify` job, scoring, a question or the diagnosis, fusion) and `plan` (the plan writer, the plan, its tasks and trigger rules).
- **The entry points.** Every input that continues a scan: a job's events and result, an indicator answer, a photo answer and its `measuring` step, the 10-minute continuation, a late answer, a guided check, the sweep, a run start (submit, resubmit, species change), a confirmed chat revision.
- **The events** these send, in the order of contracts 4, through the per-scan [`ScanWriter`](#4-ports-and-construction).

**What it never does.**

- It holds no HTTP, no SSE and no database client. It talks to the world only through the ports in [`PipelineDeps`](#4-ports-and-construction).
- It never spawns tasks: work that outlives a request comes back to `api` as a [`FollowUp`](#4-ports-and-construction), and `api` runs it on its task tracker.
- It doesn't check dose limits or validate plans; the agent crate does both (`agent::safety` and `agent::plan_writer`).
- It sends no push notifications. `api`'s writer pushes by the contracts 9 rule, because it sees every event, including the ones `api` emits itself.

**Who calls it.**

| Caller | Calls |
|---|---|
| `api`'s handlers | `submit`, `species_confirmed`, `answer`, `photo_answer_start`, `photo_answer_added`, `store_revision` |
| `api`'s job broker | `worker_events`, `job_completed`, `job_failed_for_good` |
| `api`'s scheduler | `sweep`, the one owner of recovery and of the 10-minute continuation (it calls `continue_without_answer` for every due question) |
| `api`'s task tracker | `follow_up` for everything the calls above return |

---

## 2. Dependencies

@@file ../Cargo.toml@@

| Crate | Why |
|---|---|
| `domain` | Shapes, rules (`steps`, `questions`, `stages`, `scan_state`, `routing`, `limits`) and every port |
| `knowledge` | Candidate ranking, scoring, status, the question choice ([knowledge](knowledge.md)) |
| `agent` | `plan_writer::{PlanWriter, PlanInput, SharedMemories, WrittenPlan}`, `PlanWriterError` and `beliefs::current_beliefs` |
| `vision` | `measure::Measurer`, behind [`ImageMeasurer`](#4-ports-and-construction) |
| `tokio` | `spawn_blocking` for measurement, the CPU `Semaphore`, `OwnedMutexGuard` in `PlanGuard` |
| `chrono`, `chrono-tz` | A garden's local date for task due dates (`chrono-tz` 0.10.4, added to the workspace) |
| `bytes` | Photo and depth bytes moved into the blocking task |
| `serde`, `serde_json` | Raw job results (`serde_json::Value`) and enum wire names in labels |
| `async-trait`, `thiserror`, `tracing` | The `ScanWriter` and `PlanAuthor` traits, `PipelineError`, logs |
| `domain` with `mock`, `test-util`; `proptest` (dev) | Mocks, `SequentialIds`, property tests |

---

## 3. Module layout

```
crates/pipeline/src/
├─ lib.rs            Pipeline, PipelineDeps, PipelineConfig, FollowUp, Handled, local_today
├─ error.rs          PipelineError
├─ writer.rs         ScanWriter, PlanGuard: the per-scan writer port api implements
├─ plan_author.rs    PlanAuthor, implemented for agent::plan_writer::PlanWriter
├─ measure.rs        ImageMeasurer, MeasureRequest; the margin rule and measured answers
│  └─ vision_adapter.rs   VisionMeasurer over vision::measure::Measurer
├─ findings.rs       reading observations: readable, plant check, classifier hits, soil, texture
├─ payloads.rs       perception, vlm and verify payloads with photo routing
├─ fusion.rs         risks: fungal spread, crowding, frost, root rot, growth_stalled
├─ questions.rs      a question's wording from the knowledge crate's choice
├─ replan.rs         PlanNeed, plan_problems, neighbours, steps to tasks
├─ results.rs        worker results to observations and evidence; the species suggestion
├─ runs.rs           submit, species change, the label-version rule
├─ entry.rs          the other entry points: answers, photo answers, broker, sweep, revisions
├─ catch_up.rs       ResultStages: the stage events a job's complete stands in for
├─ steps.rs          the step loop, commit, fail_run, event helpers
│  ├─ retrieve.rs    retrieve, measurement, the weather cache
│  ├─ score.rs       score, verify_outside, diagnose
│  ├─ plan.rs        plan, plan_context, the pipeline's memory
│  └─ question.rs    a photo answer's measuring step
└─ testkit.rs        (tests) record builders
```

---

## 4. Ports and construction

Every external dependency is a trait object: runtime-swappable, dyn compatible and `Send + Sync`, as [README 7](README.md#7-rust-conventions) requires. `api` builds one `PipelineDeps` in its wiring and holds the `Pipeline` in an `Arc`. Its `memory` is `agent::MirroredMemory`, so the pipeline's summary reaches Agent Memory without the pipeline knowing; `beliefs` is the `BeliefStore` itself (`NoBeliefs` when Agent Memory isn't configured), read only by the plan step.

@@items lib.rs PipelineConfig PipelineDeps FollowUp Handled Pipeline@@

**The per-scan writer.** Contracts 4 requires one serialised writer per scan that assigns `seq`, stores, then broadcasts. Contracts 7 requires every plan write of a scan to run one at a time. The pipeline defines the port and `api` implements it ([api](api.md)): a per-scan `mpsc` actor for events, and a per-scan mutex for plan writes, so a 10-second plan-writer call never delays event delivery. It lives here, not in `domain`, because `PlanGuard` is tokio's `OwnedMutexGuard` and `domain` has no runtime. Its lifecycle answers contracts open question 22 ([section 17](#17-open-questions-and-decisions)).

@@items writer.rs PlanGuard ScanWriter@@

**The plan writer.** The agent crate owns the LLM call and the validation round trip ([contracts 7, plan validation](../architecture/contracts.md#actionplan)). A pipeline-local trait lets tests replace it:

@@file plan_author.rs@@

**The wake-up.** `ScanWriter` has no `job_queued`: every store write that queues or requeues a job signals the claim long poll's `Notify` after its commit ([store 14](store.md#14-jobs-and-workers)), so the pipeline never wakes it by hand.

**Measurement.** `vision::measure::Measurer` is synchronous CPU work over borrowed buffers. The pipeline's own `ImageMeasurer` takes an owned `MeasureRequest`, because the request must move into `spawn_blocking`, which requires `'static`. The adapter borrows it back into vision's `MeasureInput<'_>` inside the closure:

@@items measure.rs MeasureRegion MeasureRequest MeasuredQuantity ImageMeasurer@@

@@impl measure/vision_adapter.rs impl ImageMeasurer for VisionMeasurer@@

**Organism length** ([contracts 7](../architecture/contracts.md#measurement), organism length). `ImageMeasurer::organism_length_mm` sizes an organism box from the photo's depth PNG (`vision::measure::Measurer::organism_length_mm`: the box's longer side at the median confident depth inside it). The pipeline adds it when it stores the VLM findings, in `job_completed` and for a `vlm_findings` partial result alike, before their `finding` events are sent: each organism observation on a photo with depth gets `detail.length_mm`, rounded to 0.1 mm. One depth map is read per photo, and the geometry runs on the blocking pool under a CPU permit. It is best effort, since the length is shown and never scored: a depth map that can't be read leaves it out and is logged.

@@items measure.rs organism_boxes set_organism_length@@

**Segmentation masks** ([contracts 5](../architecture/contracts.md#results), `PerceptionRegion.mask`; [storage 4](../storage/README.md#4-writes-and-deletes)). When a perception job completes, `job_completed` stores each region's mask before the store records the result: the RLE must fit its photo (`size` is the stored `[height, width]`), `vision::rle::to_png` turns it into an 8-bit grayscale PNG on the blocking pool under a CPU permit, and `BlobStore::put` writes it under `gardens/{garden}/scans/{scan}/masks/{mask}.png` (`ObjectKey::mask`). `results::attach_masks` then adds each `NewMask` to the segmentation write and sets its region's `mask_id`, so `complete_job` writes the records with the observations, and `GET /scans/{id}` and the `finding` events sign a `mask_url`. The object comes first, so a crash leaves an object without a record, which the orphan sweep removes, never a record without its object. A mask that doesn't fit its photo or doesn't decode is left out and logged, and its region is kept without it, since masks are optional. A partial result stores no masks: the final result replaces its observations. A result covers the images `covered_images` gives, for its partial results too: a question job's covers the question's photos alone and, like its completion, replaces nothing tied to no image, so a question photo's partial segmentation leaves the run's regions and masks alone ([contracts 3](../architecture/contracts.md#3-scan-rest-api), observations across runs).

A job stores segmentation only when its own stages include it (`LeasedJob::segmentation`, which the store reads from the job's `payload.stages`): a species-change run's job leaves segmentation out and is sent the earlier run's regions, which the worker passes through without masks and at confidence 1, so its completion drops the segmentation write and stores no masks, and the earlier observations keep their `mask_id`s and the masks their records. The rule reads the job row because `start_run` commits it with the run in one transaction, while the run's `run_started` is emitted afterwards and may be missing; a question photo's job is segmentation alone. After the puts the lease is checked again: a garden or account deletion that started meanwhile cancelled the job and may have listed the garden's prefix before the objects landed, and the sweep never lists a deleted garden, so without a live lease the new objects are deleted through `deps.blobs`, best effort, and nothing is recorded. `api` gives the pipeline its delete queue's store (`blob::DeleteQueue::queued_store`), so that delete is never cancelled with the request. Measurement finds a region's mask by that `mask_id` (`regions_of`), since region ids repeat across runs.

@@items entry.rs covered_images@@

@@items entry.rs store_masks@@

@@items results.rs attach_masks@@

---

## 5. The step machine

A scan holds at most one step claim (`pipeline_step`, `pipeline_step_run`, `pipeline_step_at`) ([contracts 3](../architecture/contracts.md#scan-lifecycle), [database 4](../database/README.md#steps-between-jobs-)). Every step:

1. takes its [`StepClaim`](domain.md#9-boundary-traits);
2. re-reads everything through `ScanRepo::snapshot`;
3. commits one `StepOutput` through `StepRepo::commit_step`. The commit re-checks the claim, the stamp, the run and deletion, writes the output, and sets the next claim in the same transaction.

```mermaid
stateDiagram-v2
    [*] --> retrieve: perception result (claim in its transaction)
    retrieve --> released: commit Retrieve + vlm job
    retrieve --> score: commit with rescore_pending
    released --> score: vlm or run verify result, answer, continuation, late answer, guided check on diagnosed/planned
    score --> released: commit Ask (question, awaiting_answer)
    score --> released: commit Verify + job (outside the set)
    score --> score: commit Verify without job, or rescore_pending
    score --> plan: commit Score with needs_plan
    score --> released: commit Score without needs_plan, or Release
    plan --> released: commit Plan (compare-and-set) or Release
    plan --> score: commit with rescore_pending
    released --> [*]
    note right of released
        Busy entry points set rescore_pending.
        The sweep re-claims a stamp older than
        2 minutes (5 for plan) and re-runs the step.
    end note
```

| Step | Reads | Commits | Own hand-over | With `rescore_pending` |
|---|---|---|---|---|
| `retrieve` | Snapshot, the weather cache or Open-Meteo, profiles | `Retrieve { observations, evidence, candidates, weather, job }` | Release (the `vlm` job is in flight) | `score` |
| `score`, VLM stage not done, or idle status | Snapshot | `Release` | Release | `score` |
| `score`, organisms or damage outside the set | Snapshot, profiles | `Verify { job, evidence, added_candidates }` | Release with a job; `score` without | `score` |
| `score`, ambiguous and may ask | Snapshot, profiles | `Ask { question, resolved_question }` | Release (`awaiting_answer`) | `score` |
| `score`, otherwise | Snapshot, profiles, heights, map, memories | `Score { diagnosis, status, resolved_question, needs_plan }` | `plan` when `needs_plan`, else release | `score` |
| `plan` | Snapshot, profiles, memories, map, plants | `Plan(PlanWrite)` or `Release` | Release | `score` |

The loop follows hand-overs until a commit releases. A lost claim (`Conflict::ClaimLost` or `QuestionStepLost`) ends it quietly, because another owner has the work:

@@impl steps.rs impl Pipeline@@

---

## 6. Retrieve

[Contracts 3](../architecture/contracts.md#scan-lifecycle): measurement, context, retrieval and queueing the `vlm` job, after the perception result.

| Order | Work | Events | Failure |
|---|---|---|---|
| 1 | Plant check: `plant_present: false` | `question_resolved` (superseded) if one was open, then `failed` | `no_plant_found` |
| 2 | Measurement, when a run photo has depth and the run must measure (below) | `stage_started measurement`, `stage_completed measurement` (`skipped: nothing_measurable` or `error`) | Never fails the scan |
| 3 | Context: the garden's weather cache cell for this hour, fetched from Open-Meteo on a miss and cached | `stage_started context`, `weather_loaded`, `stage_completed context` | `upstream_unavailable` |
| 4 | Retrieve: rank the candidates, answer measurable indicators, build the `vlm` payload | `stage_started retrieve`, `stage_completed retrieve` | |
| 5 | Commit (the store wakes the claims for the `vlm` job) | `finding` per measurement, then `candidates_selected` | |

**Which runs measure** ([architecture 6.7, runs](../architecture/README.md#67-depth-and-measurement-lidar)): every photo with depth when the run has no measurement yet, or when segmentation re-ran after the last measurement. A species-change run that kept segmentation and measurements measures nothing.

@@items steps/retrieve.rs images_to_measure regions_of@@

Measurement reads each photo, its depth PNG and its confidence PNG through `BlobStore`. It then runs on the blocking pool behind a semaphore of one permit per core, as the `rust-async` skill (§2, `.claude/skills/`) says. Each quantity becomes an observation with `model_version` = the algorithm version plus `depth_meta.device_model` ([contracts 7, observation](../architecture/contracts.md#observation)).

**Measurable indicators** ([contracts 7, measurable indicators](../architecture/contracts.md#measurement)):

| Rule | Here |
|---|---|
| Which measurement | The indicator's quantity, from an image whose kind it lists, medium confidence or more; highest confidence, then largest `n` |
| Which value | `stat`: `median` is `value`, `max`, `min` |
| Margin | 10% of the bound; at least 2 stored-image pixels at the measured distance for lengths (2 × `distance_m` / `fx`, × 1000 in mm, × 100 in cm); at least 1 for counts |
| `yes` | Inside by at least the margin from each bound it has |
| `no` | Outside by more than the margin |
| Not answered | Within the margin, or no usable measurement: the indicator goes to the VLM |
| Evidence | `source: measurement`, `measured { quantity, value, image_id }`, a reason like "Median spot 2.1 mm; range 1.5–3 mm" |
| `fx` | From the photo's stored `DepthMeta`, rescaled when its reference width isn't the stored width |

@@items measure.rs margin pick best_measurement answer_indicator clean_images@@

**Payloads and photo routing** ([contracts 5, payloads](../architecture/contracts.md#payloads)). A measured indicator leaves the job; a candidate with none left is not sent. When none is left, `candidates` and `indicator_images` are empty and the worker skips the indicators call:

@@items payloads.rs run_images prompts vlm_payload verify_payload@@

---

## 7. Score

[Contracts 3](../architecture/contracts.md#scan-lifecycle): scoring, fusion and the diagnosis, then a question or the plan. Where the scan is decides what `score` may do:

@@items steps/score.rs ScoreMode mode@@

| Order | Check | Result |
|---|---|---|
| 1 | Status claims nothing, or in a run the VLM stage hasn't stored findings | `Release` |
| 2 | In a run, the plant isn't readable ([contracts 3](../architecture/contracts.md#scan-lifecycle): no leaf region, no medium disease result, no medium species, no medium VLM finding) | `failed` with `photos_unusable` |
| 3 | In a run, the VLM's organisms or damage (medium or more) point to problems outside the set | `Verify`: the added candidates, their measured answers and one `verify` job; `candidates_selected` again |
| 4 | Score the stored set; decide the status ([knowledge 8](knowledge.md#8-diagnosis-status-and-the-question)) | |
| 5 | Ambiguous, in a run, not a continuation, fewer than 2 asked, and a question exists | `Ask`, then `question` and `status_changed → awaiting_answer` |
| 6 | Otherwise: fusion and the diagnosis | |
| 7 | Late work whose result didn't change and resolved no question | `Release` |
| 8 | `Score`, `needs_plan` by the [re-planning rules](#8-plan) | `question_resolved` (photo) if the score resolved one, `diagnosis`, `status_changed → diagnosed` in a run |

The open question tells the cases apart. `answering` with no step means its `verify` job finished, so this score resolves it by photo. `open` means the scheduler continued without an answer: the diagnosis is `uncertain`, and no new question is asked.

@@items findings.rs readable finding_summary@@

**Fusion** ([architecture 6.1, fusion](../architecture/README.md#61-stages); [contracts 7](../architecture/contracts.md#diagnosis), [contracts 9, garden map](../architecture/contracts.md#garden-map)). Disease thresholds come from the profiles; weather thresholds from `domain::rules::weather::WeatherThresholds` ([domain 7, weather thresholds](domain.md#weather-thresholds)), the instance `api` also gives Open-Meteo and its triggers (`PipelineConfig::weather`); the other constants are the contracts' own:

| Risk | Rule |
|---|---|
| `fungal_spread` | For each fungal or bacterial problem the diagnosis points to, the forecast days that meet its `conditions`: 1 day `low`, 2–3 `medium`, 4 or more `high` |
| Crowding | The plant's nearest pin closer than 0.75 × its species' `spacing_cm`, plus a humid stretch forecast or in the past 7 days, raises `fungal_spread` one step, or adds it at `low` |
| `frost_damage` | A frost day in the next 3 days (a 2 m minimum ≤ 0.5 °C, or the day's `frost_risk`): `medium`, `high` at hard frost (≤ −2 °C) |
| `root_rot` | The photo shows waterlogged soil at medium or more: `low`, raised a step when the modelled top-9-cm moisture is at least 0.45 m³/m³ |
| `growth_stalled` | The species has `mature_height_cm`; the latest height is from the past 7 days and below it; the latest height 14 or more days earlier is within 2 cm or 5%. Medium and high points only |

A **humid stretch** is two humid days in a row, a humid day having 10 or more hours over 90%, forecast in the next 7 days or in the past 7 ([contracts 9](../architecture/contracts.md#garden-map), crowding; [architecture 6.8](../architecture/README.md#68-garden-map-lidar)): `WeatherThresholds::humid_stretch_now_or_past`. The past week comes from `past_7_days.longest_humid_run_days`, which Open-Meteo's adapter computes from the hourly history with the same thresholds ([integrations 7](integrations.md#7-open-meteo-weather)). The driver text names the forecast when it holds the stretch, else the past week. The scheduler's `humid_stretch` trigger reads the forecast rule alone, over its 3-day horizon.

@@items fusion.rs humid_stretch favourable_days growth_stalled risks@@

`evidence_basis` lists the user's shared memories (`source: user`), then the photo (measured reasons and soil findings), then modelled data (weather, soil moisture, SoilGrids pH) ([architecture 6.4](../architecture/README.md#64-soil)). `citations` are the sources of the problems the diagnosis points to, or the species' sources for a healthy plant.

---

## 8. Plan

[Contracts 7, re-planning](../architecture/contracts.md#actionplan). `plan` holds the scan's plan lock from reading the current plan to storing the new one, and decides from the snapshot what to write, so the sweep's re-run decides the same way:

@@items replan.rs PlanNeed plan_need plan_problems@@

| Case | Reason |
|---|---|
| No plan yet | `initial` (stored as `action_plan.reason`; no `plan_superseded`) |
| The plan was written from this diagnosis | Nothing to write |
| The plan's diagnosis is of an earlier run | `species_changed` |
| The question open when the plan was written isn't open now, or the scores or status changed | `late_answer` |
| The texture differs, or the plan's diagnosis had none | `guided_check` |
| Otherwise | Nothing to write |

**What the plan writer reads** (`PlanInput<'_>`, agreed with the agent crate): the plan's problems (the primary, or every candidate scored zero or more for `uncertain`, none for `healthy`), their treatments, the organisms seen and the ones the problems are `caused_by`, the species, the shared memories (`SharedMemories::new` refuses a private one), the plant's and its garden's current beliefs from Agent Memory (`agent::beliefs::current_beliefs`, read with `tokio::join!` beside `shared_for_plan` and bounded by `PipelineConfig::beliefs_read_timeout`, 1.5 s; empty when Agent Memory is off, failing or slow, and the plan is written without them), the constraints carried from the current plan, the neighbours, the open question and the weather. `known_sources` is the union of their sources.

**Neighbours** ([contracts 7](../architecture/contracts.md#actionplan)): when the plant is pinned and a plan problem spreads, up to 5 pins within 1.5 m, nearest first, whose species is `susceptible_to` a spreading plan problem:

@@items replan.rs NeighbourPlant neighbours@@

**Assembly and storage.** The pipeline assembles the `ActionPlan`:

- `version` is the current version plus one, and `supersedes` the current plan.
- `provisional` is true exactly when the diagnosis is `uncertain`.
- `open_question` is the scan's open question, and the constraints are the current plan's.
- Citations are resolved through `KnowledgeStore::citations`.

It then commits `StepOutput::Plan(PlanWrite { expected: current plan })`. The store's compare-and-set ([database 4](../database/README.md#plan-writes-compare-and-set-)) dismisses the superseded plan's open tasks, deactivates its trigger rules, and skips tasks for plants that are gone. A `Conflict::StalePlan` starts again from the new current plan, up to 3 times (`MAX_PLAN_ATTEMPTS`).

**After 3 lost compare-and-sets** the step commits `StepOutput::Release`: the claim is released, or handed over to `score` when an input set `rescore_pending` meanwhile. It doesn't return with the claim held, which would block the scan's steps and keep its stream open (`step_claimed`) until the 5-minute sweep. Every plan write of a scan in this process holds the plan lock, so a loss means a writer in another process stored a newer plan; that plan stays current, and the next diagnosis change re-plans from it. The step logs a warning.

@@items steps/plan.rs MAX_PLAN_ATTEMPTS@@

| `when` | Task |
|---|---|
| `now` | Due today in the garden's time zone |
| `by_date` | Due on its date |
| `recurring` | Due today, with `repeat` |
| `on_trigger` | No task: a plan trigger rule on the step's plant |

@@items replan.rs tasks_for@@

If the plan writer fails on a run's first plan (status `diagnosed`, no plan), the scan fails with `upstream_unavailable`. A failing re-plan keeps the current plan, and the sweep retries the step. After a first plan or a species-change plan, the pipeline stores a short memory with `source: pipeline`, kind `summary` and topic `health` (best effort); its belief key is `scan_summary`, so each new summary supersedes the last one in Agent Memory.

---

## 9. Entry points

Every entry point stores its input and claims (or flags `rescore_pending`) in one store transaction, then returns the claim as a `FollowUp`.

| Entry point | Store call (claim in the same transaction) | Step | Hand-over after | Events it sends |
|---|---|---|---|---|
| Perception result (run) | `complete_job` with `claim: retrieve` | `retrieve` | release, `vlm` job queued | missed perception stage events, `finding` / `finding_retracted` (diff), then retrieve's |
| `vlm` result | `complete_job` with `claim: score` | `score` | question, diagnosis + plan, or verify | missed `vlm_*` stage events, `finding` (diff), `species_suggested` |
| Run `verify` result (outside the set, or a photo answer) | `complete_job` with `claim: score` (clears `question.step`) | `score` | as above; resolves the photo answer | a missed `verify` stage event |
| Indicator answer, in a run | `answer_question` | `score` | question or diagnosis | `question_resolved` (user) |
| Late answer | `answer_question` (`late: true`) | `score` | diagnosis, `plan` (`late_answer`) | `question_resolved` (user) |
| Photo answer, `segmenting` | `add_image` with `PhotoAnswerStart::Segment(job)` | | job in flight | |
| Question photo's segmentation result | `complete_job` moves the question to `measuring` | `measuring` | `verify` job, or `score` | worker stage events |
| Photo answer, `measuring` | `add_image` with `PhotoAnswerStart::Measure`, or the result above | `measuring` | `advance_question_step`: `Verify` (job), or `Score` (claims `score`) | `measurement` stage, `finding` |
| 10-minute continuation | `continue_without_answer` | `score` | diagnosis `uncertain`, `plan` | `diagnosis`, `status_changed` |
| Guided check, `diagnosed` or `planned` | `finish_guided_check` | `score` | new diagnosis; `plan` if the texture changed | `guided_check_done`; no missed stage events ([section 10](#10-events)) |
| Guided check, other statuses | `finish_guided_check` (`NotNeeded`) | | | `guided_check_done`; no missed stage events |
| The sweep | `reclaim_step` / `reclaim_question_step` / `continue_without_answer` | the stale step | as the step | the step's |
| Submit, resubmit | `start_run` | | `perception` job queued | `run_started`, `status_changed` |
| Species change | `confirm_species`, then `start_run` | | `perception` job queued | `species_confirmed`, `run_started`, `question_resolved` (superseded), `status_changed → analysing` |
| Chat revision | `write_revision` under the plan lock | | | `plan_superseded` (`chat_revision`) |

A busy scan (`ClaimAttempt::Busy`) gets no follow-up: the running step hands over to `score` when it commits.

The photo answer's first step is decided before the upload is recorded, so that its step and job go into `add_image`'s transaction. A crash in between can then never leave an `answering` question with nothing in flight:

@@items entry.rs PhotoInfo needs_segmentation@@

**A job that fails for good** ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts); `final_failure`):

| Job | Effect |
|---|---|
| A run's `perception`, `vlm` or `verify` | `failed` with `processing_error`, its question superseded |
| A late `verify` | The photo answer dropped: `question_resolved` with `dropped` |
| A question photo's segmentation | The photo goes to `measuring` unmeasured |
| `guided` | The store marks the check `failed` in the failing transaction; the pipeline sends `guided_check_done` |

`QuestionState::after_failed_photo` agrees with this table: a late photo answer's question is `dropped`; in a run, the `verify` job is a run's job, so the scan fails and the failure supersedes the question in the same transaction ([contracts 3](../architecture/contracts.md#scan-lifecycle): a `failed` scan never holds a question). The question is never reopened. A domain test pins the two rules together.

A result that doesn't decode for the job's recorded kind is completed as failed with `model_error`, `retryable: true` ([section 17](#17-open-questions-and-decisions), question 18).

---

## 10. Events

Events go out after the commit that makes them true. Progress events (`stage_*`, `weather_loaded`) go out as work happens. Each step's last events go in one `emit` call. The writer then checks the resting point once ([contracts 4, when the stream closes](../architecture/contracts.md#when-the-stream-closes)).

`StreamState` gained `step_claimed` ([section 16](#16-domain-changes)). A late answer's `question_resolved` and a guided check's `guided_check_done` are sent while `score` holds the claim taken in their transaction, so the stream stays open until the new `diagnosis` and plan events.

Order of a typical run, as [contracts 4](../architecture/contracts.md#event-catalogue)'s example, with the stages this crate sends marked:

| Events | Sent by |
|---|---|
| `run_started`, `status_changed pending → queued` | `submit` |
| `status_changed queued → analysing` | `api`'s claim |
| perception stages, `finding` | the worker, through `worker_events` |
| `stage_* guided` | a `guided` job's worker, through `worker_events`, on the scan's current run |
| `stage_* measurement`, `stage_* context`, `weather_loaded`, `stage_* retrieve`, `finding` (measurements), `candidates_selected` | `retrieve` |
| `vlm_findings` stages, `finding`, `species_suggested`, `vlm_indicators` stages | the worker and `job_completed` |
| `stage_* scoring`, `stage_* fusion`, `diagnosis`, `status_changed analysing → diagnosed` | `score` |
| `stage_* plan`, `status_changed diagnosed → planned`, `plan_ready` (with `plan_superseded` first after a species change) | `plan` |

A sweep's re-run can repeat progress events (`stage_started` twice). Outcome events come only from the commit that won, so the app sees one `diagnosis` per stored diagnosis.

**Guided jobs' stage events.** A `guided` job has no run of its own (`job.run` is `None`), but contracts 4's `Stage` includes `guided`, and contracts 5 turns every worker's stage events into scan events. So `worker_events` forwards a guided job's `stage_started` and `stage_completed` on the scan's current run, the run its `guided_check_done` carries, and the app can show the check in progress. A guided job sends no partial results. Events that arrive after a job completed never reach `worker_events`: `api` drops them ([api 15](api.md#15-the-job-broker-and-the-worker-api)).

@@items entry.rs stage_event@@

**Stage events `complete` stands in for** ([contracts 5](../architecture/contracts.md#post-internaljobsidevents), backend roadmap gap 9). A worker's `events` calls are best effort: while `api` is down the worker drops them and goes on, because `complete` repeats every stage's output. The findings then come from `complete`, but a lost `stage_completed` would leave its stage open on the stream for good. So `job_completed` reads the run's stored `run_started` and stage events (`EventRepo::run_stage_events`), folds them per stage as the app's reducer does, and sends what the stream is missing for each stage of the job's kind, in one `emit` with the findings and before them:

| The run's last event for the stage | Sent |
|---|---|
| `stage_started` (the worker started it; a requeue's second start counts) | `stage_completed`, with no second `stage_started` |
| none, and the result has output for the stage (for a perception stage, also listed in `run_started.rerun_stages`) | `stage_started`, then `stage_completed` |
| `stage_completed` | nothing, so a job whose events all arrived adds none |

- **`duration_ms: 0`.** `api` never timed the stage, and the worker's `timings_ms` is per job, not per stage. A start-to-complete difference would measure the outage, not the stage.
- **`skipped`** only where the result shows it: `disease_skipped`'s reason, and `all_measured` for a `vlm` result with no indicator answers (contracts 5: the answers are empty only when the indicators call was skipped). A stage isn't marked skipped to say that its events were recovered.
- **The `rerun_stages` check** covers a species-change run that keeps the earlier segmentation: its regions pass through the result, which is no segmentation stage of this run.
- **Best effort.** The result is stored before the read, so a failed read is logged with its error chain and sends no stage events; failing `complete` would make its retry answer `AlreadyCompleted` and send no findings either.
- **Not for `guided` jobs.** The `guided` stage is one stage shared by every guided check of the run, and two checks can run at once. The fold keys on the stage alone, so it can't tell which check's `stage_started` is still open: one check's completion would close another's stage, and two completions could each send a start and a completion. `ResultStages::of` gives a guided result no stages, and `guided_done` sends only `guided_check_done`, which tells the app the check finished.
- **`Done` only.** A job that fails for good sends no catch-up, so a `guided` job or a late `verify` that fails for good on a `planned` scan leaves its started stage open (open question 4).

@@items catch_up.rs ResultStages completion_events@@

@@impl catch_up.rs impl ResultStages@@

---

## 11. Runs

[Contracts 3](../architecture/contracts.md#scan-lifecycle), [architecture 6.2](../architecture/README.md#62-species-identification). `submit` uses `decide_submit` and `run_stages(None, …)`. A species change uses `species_follow` and `run_stages(Some(earlier), …)`. The earlier run's regions travel in the payload when segmentation isn't re-run.

@@items runs.rs earlier_run prior_regions@@

**Label versions** ([contracts 5](../architecture/contracts.md#results), versions a worker hasn't loaded). A `perception` job never names a `label_set_version` or `classifier_labels_version` that no live perception or fallback worker reports. With no such worker live at all, the job is queued and waits. Otherwise the call fails with `PipelineError::LabelsNotLoaded`, which `api` answers with `503 upstream_unavailable`. A heartbeat reports every loaded version of each label key as a comma-separated list ([worker 17](worker.md#17-open-questions)), so each value is split on commas and trimmed before it is compared; a worker that loaded two label sets matches either:

@@items runs.rs LABEL_SET_KEY CLASSIFIER_LABELS_KEY labels_loaded reports@@

---

## 12. Idempotency and recovery

| Situation | What happens |
|---|---|
| `api` restarts inside a step | The claim stays; the sweep re-claims it after 2 minutes (5 for `plan`) by its old stamp and re-runs the step from a fresh snapshot |
| The original step is only slow | Its commit fails with `ClaimLost`, because the sweep's new stamp replaced its own; it stops without events |
| A step of an older run | The snapshot's run differs, so it stops; its commit would be refused anyway |
| `retrieve` runs twice | The store checks for an existing `vlm` job of the run before queueing |
| `score` runs twice on late work | A diagnosis equal to the current one, with no question resolved, commits `Release` |
| `plan` runs twice | `plan_need` finds the plan up to date and commits `Release`; a lost compare-and-set restarts from the new plan; after 3 losses the step releases its claim |
| A photo answer stalls in `measuring` | The sweep re-claims the question step after 2 minutes; re-measuring replaces that photo's measurements |
| A photo answer stalls in `segmenting` or `verifying` | The broker's leases and attempts recover the job; a final failure goes on unmeasured, or drops the question when late |
| Input arrives while a step runs | The entry point sets `rescore_pending`; the step's commit hands over to `score` |
| A late result of an old run | The broker answers `409 lease_lost` before the pipeline sees it |
| Late work fails | Never fails the scan: `question_resolved` (`dropped`) or a failed `guided_check_done` |
| The per-scan writer retires and comes back | `seq` comes from `append_event`'s transaction, so the sequence continues ([section 17](#17-open-questions-and-decisions)) |
| `complete` is retried after a timeout | A lease that is no longer live writes no mask, and `complete_job` answers `AlreadyCompleted`. A failed mask put fails the completion before anything is recorded, so the retry writes the masks again under new ids, and the sweep removes the objects of the attempt that failed |
| A garden is deleted while a job's masks are put | The lease is checked again after the puts; without it the new objects are deleted and nothing is recorded, since the sweep never lists a deleted garden's prefix |
| `api` is down while a job runs | The worker's `events` calls fail and are dropped; `complete` sends the missing stage events before the findings ([section 10](#10-events)) |

---

## 13. Concurrency and lifetimes

- **Where it runs.** Entry points run on the request's task. `follow_up` runs on `api`'s task tracker. Measurement runs on the blocking pool, one photo per permit. The pipeline never holds a `std` lock across an `.await`. `PlanGuard` is an owned tokio guard, which may cross `.await`s ([tokio `Mutex`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html)).
- **Why everything at the ports is owned.** Snapshots, payloads, observations and plans cross `.await`s, trait objects and the database. `#[async_trait]` futures must be `Send` ([domain 10](domain.md#10-concurrency-and-lifetimes)).
- **Where borrowing pays.** Inside a step, the profiles, observations and evidence sit in the step's snapshot, and the rules borrow them.

| Lifetime | Meaning |
|---|---|
| `'o` in `MeasuredObs<'o>` | A measurement borrowed from the step's observations |
| `'k` (knowledge crate) | The profiles the step fetched; candidates and chosen indicators point into them |
| `'k` in `agent::plan_writer::PlanInput<'k>` | The step's owned `PlanContext` and snapshot, borrowed for the plan-writer call. `PlanContext` exists so that the borrowed view has an owner that lives across the `.await` |
| `'a` in `FusionInput<'a>`, `NeighbourPlant<'a>` | The step's records, for one pure call |

No generic parameters are used outside the knowledge crate's rules: the ports are trait objects, and `tasks_for` takes two `FnMut` id generators. Those are closures over `dyn IdGen`, because `IdGenExt::id::<T>` is generic and can't be called through `dyn` ([domain 9](domain.md#9-boundary-traits)).

---

## 14. Errors

@@items error.rs PipelineError@@

@@impl error.rs impl PipelineError@@

| Variant | `api` maps it to |
|---|---|
| `Store(Conflict::ClaimLost \| QuestionStepLost)` inside a step | Nothing: `follow_up` swallows it |
| `Store(…)`, `Domain(…)` | Its tables for `StoreError` and `DomainError` ([contracts 2](../architecture/contracts.md#2-errors)) |
| `LabelsNotLoaded` | `503 upstream_unavailable`, `retryable: true` |
| `Provider`, `PlanWriter`, `Model`, `Blob`, `Join` | Logged by the follow-up; a request path answers `500 internal` |
| `Inconsistent` | Logged; `500 internal` |

---

## 15. Testing

`cargo test -p pipeline` runs the pure decisions with `domain`'s `test-util` builders:

| Area | Tests |
|---|---|
| Measured indicators | The margin table (2-pixel floor at 0.3 m, `yes`, `no`, within the margin, low confidence), best-measurement order, clean images |
| Readable and findings | The readable rule; early blight at 0.71 is a classifier problem, a `healthy` best result |
| Fusion | Humid stretch forecast; a humid stretch in the past week (`longest_humid_run_days`); `growth_stalled` (stalled, growing, too old, no mature height, at mature height); crowding alone adds `fungal_spread` at `low`; frost at −3 °C is `high`; frost at 0.5 °C is `medium`, −2 °C `high`, 0.6 °C none |
| Re-planning | Every `PlanNeed` row; steps to tasks with a neighbour's task and a trigger rule |
| Runs | The label-version rule (nobody live, VLM workers only, an old worker, one matching fallback); comma-separated heartbeat lists split into whole values |
| Organism length | `organism_boxes` picks organisms on photos with depth only; `set_organism_length` rounds to 0.1 mm; the vision adapter sizes a box on an encoded 16-bit depth PNG (30 mm at 0.3 m) and answers `None` for bytes that aren't a PNG |
| Entry points | Segmentation first only for measurable, unsettled indicators the kind can show |
| Results | VLM answers kept only for asked indicators; the species suggestion prefers agreement |
| Missing stage events | Lost after `plant_check` started: one `stage_completed` per stage, before the findings, no second start; every event arrived: none added; a requeue's second start completed; passed-through regions are no segmentation stage; a `vlm` result without answers completes `vlm_indicators` as `all_measured`; a `guided` result sends none, even with its stage open |
| Others | Score modes, local dates in the garden's zone |

The steps' I/O paths are exercised through `api`'s fake store in its integration tests ([api](api.md)). `api`'s `tests/pipeline.rs` drives `Pipeline::job_completed` over that fake store with a recording `ScanWriter`: a perception job whose stage events were lost sends one `emit`, the missing stage events then the findings; a repeated `complete` (`AlreadyCompleted`) sends nothing; a failed `run_stage_events` still sends the findings, without stage events; a `verify` job closes its `verify` stage; a guided completion sends `guided_check_done` alone and never reads the stage events; a perception result with masks writes each as a PNG to an in-memory `BlobStore` and links it to its region, a region without a fitting mask keeps none, a repeated `complete` writes no object, and a failed put fails the completion before `complete_job`; a transposed mask (`[width, height]` on a photo that isn't square) is left out; masks put after the lease is lost are deleted and the worker gets `lease_lost`; a species-change run's passed-through regions write no segmentation and no object; a question photo is segmented with its mask even in a species-change run; and a question photo's partial result covers that photo alone, without replacing what is tied to no image.

---

## 16. Domain changes

These changes were made to `domain` while writing this crate. Each was agreed with the store author, and `cargo check --workspace` passes.

| Change | Why |
|---|---|
| `ScanSnapshot.candidates`, `weather` | The run's candidate set and weather snapshot survive between `retrieve` and `score` (`scan.candidates`, `scan.weather`) |
| `ScanSnapshot.open_question_claim`, `open_question_indicator`, `plan_diagnosis` | The photo answer's step (and "verified" when it is `None` while `answering`); the indicator an answer belongs to; the re-planning rules |
| `StepOutput::Retrieve` gains `candidates`, `weather`; `Score` slimmed; new `Ask`, `Verify`, `Release` | A question stores no diagnosis; the outside-set `verify` job with its measured answers; steps that find nothing to do |
| `NewQuestion.indicator` | `question.indicator`, server-side |
| `NewScanImage.photo_answer_start`, `PhotoAnswerStart` | A photo answer's first step and job in `add_image`'s transaction |
| `JobRepo::live_workers`, `LiveWorker` | The label-version rule |
| `PlantRepo::garden_plants` | Neighbour names and species |
| `StreamState.step_claimed` | The stream stays open while a step holds the claim |
| `ScanWriter::job_queued` removed (pipeline's own port) | The store's `Notify` is the claim long poll's only wake-up; every queueing write signals it |
| `PastWeather.longest_humid_run_days`, `WeatherThresholds::{longest_humid_run, had_humid_stretch, humid_stretch_now_or_past}` | A humid stretch in the past week, for crowding |
| `QuestionState::after_failed_photo(false)` is `Superseded` | It reopened the question, against contracts 5 and `final_failure` |
| `EventRepo::run_stage_events` | `complete` needs the run's stage events to send the ones the stream missed ([section 10](#10-events)); `events_after` needs a member's proof, which a worker's call doesn't have |

The store author changed in turn: `advance_question_step` returns `QuestionAdvance`, `CompletionOutcome::Stored` carries the `ObservationDiff`, and `GuidedOutcome` lost its `check` field.

---

## 17. Open questions and decisions

**Decisions on contracts open questions:**

- **22, the per-scan writer's lifecycle.** Settled jointly with `api` ([api](api.md) §13). The writer actor starts on the first `emit` and retires after 60 seconds without a command. It drains its queue first, and a racing `emit` starts a new actor.
  - The broadcast channel lives in `api`'s hub, created by the first subscriber, and is removed only when `receiver_count() == 0`.
  - The plan mutex is removed only when nobody holds or waits for it.
  - Correctness doesn't depend on the lifetimes: `seq` comes from `append_event`'s transaction.
- **18, job error codes.** The pipeline completes a result that doesn't decode for the job's recorded kind as `model_error` with `retryable: true`. That is the same code a worker sends for output that failed validation twice.

**Open questions** (for [contracts 12](../architecture/contracts.md#12-open-questions)):

1. **Weather fit, humid stretch and the fusion levels** are fixed here and in [knowledge 6](knowledge.md#6-weather-fit): 1, 2–3 or 4+ favourable forecast days; the 0.45 m³/m³ root-rot threshold; the weather thresholds of [domain 7](domain.md#weather-thresholds) (two humid days in a row, forecast or in the past week; frost at 0.5 °C, hard frost at −2 °C). The contracts give the example "2+ days" only. Set them from the evaluation set with the other thresholds of open question 10.
2. **A plan writer outage on a run's first plan** fails the scan with `upstream_unavailable` (contracts 3 lists "LLM API down"). The alternative is to keep the scan `diagnosed` and let the sweep retry the `plan` step every 5 minutes.
3. **Resolved: where the run's evidence lives.** `scan.candidates`, `question.indicator` and the `indicator_answer` table, added by the store author ([domain gap 7](domain.md#12-open-questions)), are in [`schema.surql`](../../backend/migrations/schema.surql) since database v1.4.
4. **A stage left open by a job that fails for good.** The catch-up of [section 10](#10-events) runs on `Done` only. A `guided` job, or a late `verify`, that fails for good after its worker sent `stage_started` leaves that stage open on a `planned` scan, and no later event closes it. A known limit, not fixed (backend roadmap gap 9).
5. **Late worker events can repeat a catch-up.** `job_events` (`api`'s `routes/internal.rs`) checks the lease and forwards the events in calls apart from `complete`'s transaction, so a worker's `events` call that passed its check before `complete` can store its `stage_completed` after the catch-up sent one, and the stream carries that stage's completion twice.
6. **Found bug: `guided_done` finishes the wrong check.** It picks the first `analysing` check in the snapshot, not the job's own. With a squeeze test and a jar test both `analysing` and the jar test's job finishing first, the store finishes the jar test (`finish_guided_check` finds the check from the job's payload image), but the texture observation is built on the squeeze test's photo and `guided_check_done` announces the squeeze test as done, with the jar test's texture. `guided_done` should find the check from the job's payload image, as the store does. Not fixed in this change.

---

## 18. Sources

**Project documents**

- [Architecture 6](../architecture/README.md#6-analysis-pipeline), [8.1](../architecture/README.md#81-scan--diagnosis--plan), [8.3](../architecture/README.md#83-forecast-alerts)
- [Contracts 3](../architecture/contracts.md#scan-lifecycle), [4](../architecture/contracts.md#4-scan-events-sse), [5](../architecture/contracts.md#5-worker-api-and-jobs), [7](../architecture/contracts.md#7-pipeline-records), [12](../architecture/contracts.md#12-open-questions)
- [Database 4, steps between jobs](../database/README.md#steps-between-jobs-), [plan writes](../database/README.md#plan-writes-compare-and-set-)
- [domain](domain.md), [knowledge](knowledge.md), [api](api.md)

**Rust and tokio**

- [The Rust Programming Language, 10.3: lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html); [16: concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html)
- [Rust Reference: trait objects and dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)
- [tokio `spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html), [`Semaphore::acquire_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned), [`Mutex` and `OwnedMutexGuard`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Mutex.html)
- [Alice Ryhl, Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/)

**Crates**

- [async-trait 0.1](https://docs.rs/async-trait/0.1.92/async_trait/), [chrono-tz 0.10](https://docs.rs/chrono-tz/0.10.4/chrono_tz/), [proptest 1.12](https://docs.rs/proptest/1.12.0/proptest/)
