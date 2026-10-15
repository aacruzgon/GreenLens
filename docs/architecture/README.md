# PDM Architecture

> **Status:** Draft v0.4.6, 2026-10-15. The SurrealDB schema now lives in [docs/database](../database/README.md) and the RustFS setup in [docs/storage](../storage/README.md), both checked against the pinned versions (SurrealDB 3.3.4, RustFS 1.0.1); section 5 summarises them. v0.4.5 synced with contracts v0.6: batching in perception, worker slots so the VLM can run two jobs when they fit, weather fetched once per ~1 km cell and shared by scans and the scheduler, and batched ingestion embeddings. v0.4.4 synced with contracts v0.5: plan-trigger task keys scoped to their plan, the dose check on every model-written tool argument, chat revisions stored exactly as previewed, `healthy` only on positive evidence, a diagnosis for every guided check, and SAM-class masks. v0.4.3 synced with contracts v0.4: guided checks outside runs, serialised re-plans that keep chat revisions, provisional plans tied to uncertain diagnoses, the readable-plant rule, and the dose check on every chat reply. v0.4.2 synced with contracts v0.3: guided soil checks and the full soil aspects back in scope, per-call photo routing to the VLM, recurring and trigger-based plan steps, lease deadlines, late answers that always re-plan, plan tasks dismissed with their plan, shared-only memories in plans, invites, deletion cascades, signed image URLs and a stronger dose check. v0.4.1: Review of v0.4 added: conversations owned by a user, a pre-VLM candidate set that also covers organisms the VLM finds, a scan state machine without `awaiting_species`, a cap of 3 indicators per candidate, a scheduler sweep for unanswered verification questions, deletion of chat images and chat-derived memory, and a cap of about 9 profiles on the pre-VLM candidate set. v0.4 audit fixes: text embeddings move to the control plane with one pinned model, the VLM detects organisms and answers verification indicators in its main pass, scans get a submit step and a state machine with resumable events, stale workers are requeued from heartbeats, deletion rules for shared gardens, garden-scoped memory, deterministic verification scoring, and schema fixes. v0.3 added soil data sources and soil findings from photos, the detection of organisms (pests, beneficial insects, wildlife, invasive species) and the damage they cause, and capture guidance. v0.2 added species identification, a knowledge base with retrieve-then-verify, and an optional fast text classifier (Jev).
> **Event:** [Build North: Ship Something Real](https://luma.com/build-north). Build week Oct 16–25, submissions due Oct 25 at 11:59 PM ET, Demo Day Oct 29 in Toronto.

PDM is a plant, crop and garden health monitoring app. A user photographs a plant. Computer vision on the backend assesses the plant, the soil and the surroundings. The results are combined with the weather and soil data for the garden's location and with the plant's own history, to produce a diagnosis and an action plan. Users can chat about the results, and the system warns them ahead of risky weather.

The shapes of the data that crosses each boundary (app, API, workers, VLM, knowledge ingestion) are in [contracts.md](contracts.md).

## Contents

1. [Goals and judging criteria](#1-goals-and-judging-criteria)
2. [System overview](#2-system-overview)
3. [Components](#3-components)
4. [Users and authorization](#4-users-and-authorization)
5. [Data](#5-data)
6. [Analysis pipeline](#6-analysis-pipeline)
7. [Chat and agent core](#7-chat-and-agent-core)
8. [Main flows](#8-main-flows)
9. [API surface](#9-api-surface)
10. [Models and GPU placement](#10-models-and-gpu-placement)
11. [Security, privacy and safety](#11-security-privacy-and-safety)
12. [Deployment](#12-deployment)
13. [Repository layout](#13-repository-layout)
14. [Build plan](#14-build-plan)
15. [Open decisions](#15-open-decisions)

---

## 1. Goals and judging criteria

The judges score five criteria, with no weights published. This table shows where the architecture addresses each one.

| Criterion | How the architecture addresses it |
|---|---|
| Innovation | Combines vision findings, weather and per-plant memory. Checks each candidate diagnosis against curated disease profiles before giving advice. Sends forecast alerts before problems appear. Shows a health timeline for each plant. Chat can re-examine the photos. |
| Real-World Impact | Clear users: home gardeners and small farms. Catches disease early. Fits the Canadian frost and short-season context. Recognizes beneficial insects so they aren't killed by mistake. Helps users report invasive species. User feedback improves the model over time. |
| UI & Design | Guided capture with hints to retake bad photos. Live progress for each stage. Boxes drawn on the affected leaves. The plan appears as a checklist. Chat answers cite their sources. Optional guided soil checks. |
| Tech & Deployment | Live public link. GPU workers pull jobs from the API, with a fallback to API credits. Public repository. Mobile build. |
| Security & Ethics | Managed authentication with MFA. Every garden-scoped query checks membership. Image storage is private. Photo metadata is stripped and faces are blurred before storage. Location needs consent and only the garden has one. Deleting an account removes the user's own data and any garden they alone use. Wildlife advice is humane. Pesticide advice is limited, uncertain results are shown as uncertain, and advice cites its sources. |

Submission requirements: a demo video of 2 minutes or less, a public GitHub repository, a live link, and a Devpost page listing the tech stack, the partner platforms used, and any prior work.

---

## 2. System overview

```
                                  ┌────────────────────────────┐
                                  │ Auth provider (managed)    │
                                  │ sign-in · MFA · JWKS       │
                                  └─────┬───────────────┬──────┘
                                   JWT  │               │ webhooks: user created/deleted
┌───────────────────────────────┐       │               │
│ MOBILE APP (Expo / RN)        │◀──────┘               │
│ capture + GPS · live results  │                       │
│ chat · plant timeline · tasks │                       │
└──────────────┬────────────────┘                       │
               │ HTTPS: REST + SSE                      │
═══════════════╪════════════════════════════════════════╪═════ CONTROL PLANE
               ▼                                        ▼      (cloud VM, or rig + tunnel)
        ┌─────────────┐   ┌─────────────────────────────────────────────────┐
        │ Caddy (TLS) │──▶│ api  (Rust · axum · tokio)                      │
        └─────────────┘   │                                                 │
                          │ auth mw → membership check → rate limit/budget  │
                          │ ┌─────────┐ ┌────────┐ ┌─────────┐ ┌─────────┐  │
                          │ │ scans & │ │  chat  │ │ gardens │ │ tasks & │  │
                          │ │ uploads │ │        │ │ plants  │ │ alerts  │  │
                          │ └────┬────┘ └───┬────┘ └─────────┘ └─────────┘  │
                          │      │          ▼                               │
                          │      │  ┌─────────────────────────────┐         │──▶ LLM API
                          │      │  │ agent core · safety         │─────────│    chat · plans ·
                          │      │  │ context · tools · knowledge │         │    vision fallback
                          │      │  └─────────────────────────────┘         │──▶ Jev (optional)
                          │      ▼                                          │──▶ Weather / soil APIs
                          │ job broker · SSE hub · scheduler · text embed   │──▶ Push (Expo Push)
                          └────┬───────────────┬──────────────────▲─────────┘
                               │               │                  │
                      ┌────────▼─────────┐ ┌───▼──────────┐ ┌─────┴────────────┐
                      │ SurrealDB        │ │ RustFS       │ │ fallback-worker  │
                      │ records · graph  │ │ private      │ │ vision via LLM   │
                      │ vector · geo     │ │ images only  │ │ API if no GPU    │
                      │ jobs · memory    │ └──────────────┘ │ claims in ~15 s  │
                      └──────────────────┘                  └──────────────────┘
══════════════════════════════════════▲═════════════════════════ outbound HTTPS only
                                      │ claim jobs · post events · fetch images
┌─────────────────────────────────────┴───────────────────────────────┐
│ GPU RIG (home · no open ports)                                      │
│                                                                     │
│  RTX 3070 (8 GB)                     RTX 5070 Ti (16 GB)            │
│  perception-worker (Rust, ort/CUDA)  vlm-worker (Rust)              │
│   plant check · leaf detector ·        → vLLM / llama.cpp sidecar   │
│   SAM masks · BioCLIP species ·          7–8B VLM, quantized:       │
│   disease classifier · embeddings      organisms · soil · damage ·  │
│                                        indicator answers · one job  │
└─────────────────────────────────────────────────────────────────────┘
```

### Design principles

- **Pull-based GPU workers.** The rig opens no ports. It asks the API for jobs over outbound HTTPS. Where the control plane runs is a deployment choice, not a change to the architecture. To add GPU capacity, start another worker.
- **Vision produces facts, rules produce risk, the LLM produces language.** Every finding carries a confidence score. The LLM explains structured findings and never invents a diagnosis.
- **Retrieve, then verify.** Candidate diagnoses are checked against curated species and disease profiles before any advice is given. When the evidence is ambiguous, the app asks the user a targeted question instead of guessing. Advice cites its sources.
- **Use the cheapest model that can do the job.** Small classifiers make the routine decisions (routing, gating, tagging). The strongest LLM is kept for reasoning and writing.
- **One agent core with two callers.** The plan writer, running after the pipeline, and the chat use the same context builder, tools and safety policy, so their answers can't contradict each other.
- **A trait at every external boundary:** `VisionModel`, `SpeciesIdentifier`, `TextClassifier`, `TextEmbedder`, `ChatModel`, `KnowledgeStore`, `WeatherProvider`, `SoilProvider`, `BlobStore`, `Store`, `MemoryStore`, `Notifier`. Swapping a vendor or sponsor technology means writing one adapter.
- **Stream results, not video.** The app uploads a few still images (see 6.6). The progress and findings stream back over SSE, and every event is persisted so a phone that drops the connection can resume.
- **Workers only pull.** Nothing in `api` waits on the rig synchronously. Anything a request needs right away (query embeddings, the blur check) runs on the control plane's CPU.

---

## 3. Components

| Component | Runs on | Tech | Responsibility |
|---|---|---|---|
| Mobile app | Phones, web build | Expo / React Native. TypeScript types generated from OpenAPI | Guided capture, location consent, live progress, results with overlays, chat, timeline, tasks, push |
| Auth provider | Managed service | JWT + JWKS, webhooks | Credentials, MFA, sessions. Never stores domain data |
| `api` | Control plane | axum, tower-http, governor, utoipa | All public endpoints, authorization, uploads (EXIF stripping, blur and exposure check, face blurring), text embeddings on CPU, scan state machine, job broker, SSE hub, scheduler, agent core |
| Agent core | Inside `api` | `ChatModel` trait, tool registry | Context assembly, the tool-use loop, safety policy |
| SurrealDB | Control plane | Graph edges, HNSW vectors, geometry types | All structured data, the job queue, memory |
| RustFS | Control plane, private | S3 API via `aws-sdk-s3` | Images. Only `api` talks to it |
| `perception-worker` | Rig, RTX 3070 | `ort` + CUDA, ONNX models | Plant-present check, segmentation, species identification (BioCLIP), disease classification per leaf, image embeddings. BioCLIP as a second opinion on organism crops is a stretch goal |
| `vlm-worker` + sidecar | Rig, RTX 5070 Ti | vLLM or llama.cpp, OpenAI-compatible API | One `vlm` job per scan, in two VLM calls (findings, then indicator answers) over at most 4 images: soil condition, organisms with bounding boxes, damage patterns, species cross-check, and the answers to the candidates' verification indicators |
| `fallback-worker` | Control plane | `VisionModel` with the LLM API as backend | Claims every job kind. Keeps scans working while the rig is offline |
| Training | Dev machines / 5070 Ti | Python, PyTorch → ONNX | Fine-tunes the disease classifier. Not deployed |
| Knowledge ingestion | Dev machines | Python scripts + LLM API, reviewed by a teammate | Builds the knowledge base from curated sources. Loaded through migrations. Not deployed |
| Text classifier (optional) | Jev API, cheapest LLM as fallback | `TextClassifier` trait | Chat routing, safety gate, memory-extraction gate, chunk tagging. Not on the diagnosis path |

### Rust crates

| Crate | Purpose |
|---|---|
| `domain` | Shared types, IDs, errors, the scan state machine, and every boundary trait (`VisionModel`, `TextClassifier`, `TextEmbedder`, `KnowledgeStore`, …), so no crate depends on another just to see a trait |
| `store` | SurrealDB repository layer and migrations |
| `blob` | RustFS client (custom endpoint, path-style addressing) |
| `agent` | Context builder, tool registry, safety policy, `ChatModel` and `TextClassifier` implementations (Jev, LLM fallback) |
| `vision` | Perception stages and `ort` sessions for the workers; the CPU blur and exposure check and the `TextEmbedder` implementation for `api` |
| `knowledge` | `KnowledgeStore` implementation, profile lookups, look-alike graph, chunk retrieval, deterministic verification scoring |
| `pipeline` | Stage orchestration for a scan: context, retrieval, scoring, fusion rules, the plan-writer call. Used by `api` |
| `api` (binary) | Routes, auth middleware, job broker, SSE, scheduler |
| `worker` (binary) | One binary started with `--caps perception`, `--caps vlm` or `--caps fallback` |

---

## 4. Users and authorization

### Identity

- A managed auth provider handles sign-up, sign-in, social login, MFA and sessions.
- The app attaches the provider's JWT to every request. The API checks it against the provider's JWKS (`jsonwebtoken`) and maps the `sub` claim to a `user` record.
- The `user` record is created by the provider's `user.created` webhook (signature-verified), or on the first request as a fallback. The two can race, so creation is an upsert on `auth_id`. `user.deleted` triggers the deletion cascade described in section 5.

### Personas

| Persona | Need | Role |
|---|---|---|
| Home gardener | "What's wrong with my tomato, and what do I do this week?" | `owner` of their garden |
| Small farm / community garden | Several people scanning the same plots | `owner`, `editor` or `viewer` through memberships |
| Expert (stretch goal) | Reviews diagnoses with low confidence | `reviewer` |

### Authorization model

- All domain data belongs to a **garden**. Access depends on one question: is there a `member_of` edge from this user to the garden, and with which role?
- A single middleware step resolves the user, and every garden-scoped repository function takes the user and checks membership.
- Workers authenticate with their own tokens and can only act on the job they have leased.
- The SSE stream checks ownership before it sends anything.
- A non-member, or a member who doesn't own the conversation, gets `404`, never `403`, so a response never confirms that an ID exists. `403` means a member's role is too low. What each role may do is in the role matrix in [contracts.md](contracts.md#roles-and-access): viewers read and chat; editors scan, answer, confirm species and manage tasks; owners manage the garden and its members, who join with invite codes.
- Conversations are private to the user who opened them. Every chat endpoint and every pending-action endpoint checks the conversation's owner and the garden membership. Other members of a shared garden see the scans and plans, never each other's chats.

---

## 5. Data

### SurrealDB tables, grouped by area

```
identity      user · member_of (user → garden, role, since) · push_token
garden        garden (centroid point, boundary polygon, time_zone, soil_profile) · plant (name,
              species, custom_species_name, species_status, species_candidates, position,
              status) · invite (garden, code, role, created_by, expires_at, used_at, revoked_at)
analysis      scan (status and run, see 8.1; plan, weather snapshot, event counter) · scan_image
              (kind, answers_question?, storage key, embedding) · mask (region, storage key) ·
              scan_event ([scan, seq], run, type, payload) · observation (run, stage,
              pipeline_stage, label, confidence, region, model_version) · question (run, kind,
              candidates, status) · guided_check · diagnosis (run, supersedes) · scan_feedback
plans         action_plan (version, supersedes, diagnosis, provisional, constraints) · plan_trigger (plan, step, trigger, until) ·
              task (due, repeat, trigger, source, plan_step, done_at, dismissed_at,
              dismissed_reason, alert_key unique)
chat          conversation (user, garden, anchor: scan | plant | garden, summary) · message
              (role, content, tool_calls, model, prompt_version, tokens) · chat_image ·
              pending_action (proposed | confirmed | rejected | expired) · message_feedback
memory        memory (garden, plant?, kind, text, embedding, source: pipeline | chat | user,
              shared, author?, source_msg?)
knowledge     species (full | label_only) · disease (every kind of problem, see below) · organism ·
              treatment · product (name, broad_spectrum) · classifier_label · soil_condition ·
              source (url, publisher, licence) ·
              knowledge_chunk (source, crop, disease, organism, topic, text, embedding)
              edges: susceptible_to · looks_like · manages · caused_by · preys_on
ops           job (kind: perception | vlm | verify | guided, run, payload, status, lease_until,
              claimed_at, queued_at, worker, attempts, requeues) · worker (caps, slots,
              last_heartbeat, gpu) · usage_daily (tokens, requests, scans) · idempotency_key ·
              weather_cache ([hour, cell])
```

### Schema

The schema is [`docs/database/schema.surql`](../database/schema.surql), with its design notes, query patterns and caveats in [docs/database](../database/README.md). It was applied and exercised against SurrealDB 3.3.4, the pinned version. The decisions that shape the rest of this document:

- **SurrealDB 3.3.4**, RocksDB storage, with the Rust crate pinned to the same version. The schema file is idempotent and applied by `api` on start.
- **Natural keys where the key is the rule:**
  - `member_of:[user, garden]`: one membership per pair, checked by a direct read;
  - `scan_event:[scan, seq]`: SSE replay is a key-range read;
  - `idempotency_key:[user, route, key]`;
  - `usage_daily:[user, day]`;
  - `weather_cache:[hour, cell]`: cleanup is one range delete.
- **A gap-free event `seq`:** `scan.last_event_seq` is incremented in the same transaction that stores the event.
- **Plan writes compare and set `scan.plan`**, and job claims re-check `status`, so concurrent writers conflict instead of overwriting each other. Conflicts are retried by the API.
- **Deletion fails loudly rather than orphaning:** parent links reject the delete of a record that still has children, and the API deletes bottom-up in small batches.
- **One HNSW index**, on knowledge chunks, with filters applied before the search. Memories and image embeddings are searched exactly within one garden, which is small, always correct, and avoids an open bug in filtered vector search after deletes.
- **No TTL and no spatial index exist:** the scheduler deletes expired rows, and regional alerts use an indexed geohash.

### Fields that exist for traceability

- `model_version` and `prompt_version` on every observation, plan and message, so any output can be reproduced and audited.
- `action_plan.version` / `supersedes`: when chat revises a plan, it creates a new version rather than overwriting the old one.
- `memory.source`: tracks where each remembered fact came from. `memory.plant` being null marks a garden-level fact.
- `scan_event.seq`: every event the app was sent, so a result can be replayed exactly.
- `feedback`: user corrections, which become labelled training data.
- `plant.species_status`: whether the species was suggested by the models or confirmed by the user.
- `disease.sources` and `disease.reviewed_by`: every profile points to its sources and records which teammate reviewed it.

### RustFS layout

Setup, access keys, client configuration and caveats are in [docs/storage](../storage/README.md). RustFS 1.0.1 runs on one disk, never exposed publicly, and the API streams every image itself behind its own signed URLs (RustFS's presigned URLs break behind a proxy that changes the host).

The bucket is private. Before anything is written, the image's EXIF orientation is applied to the pixels, then its metadata is stripped, faces are blurred and it is resized. The app and the workers reach images only through short-lived signed URLs issued by the API.

```
gardens/{garden_id}/scans/{scan_id}/{image_id}.webp
gardens/{garden_id}/scans/{scan_id}/masks/{mask_id}.png
gardens/{garden_id}/chat/{conversation_id}/{image_id}.webp
```

Keys are prefixed by garden, so deleting a garden is a prefix delete plus a cascade in SurrealDB. Deleting a user is a different operation, below.

### Deletion

`DELETE /me` and the provider's `user.deleted` webhook run the same cascade. The app explains it on the confirmation screen.

1. Gardens where the user is the only member are deleted outright: prefix delete in RustFS, cascade in SurrealDB.
2. Gardens where the user is the only `owner` but others are members: ownership passes to the longest-standing editor. If there is none, the garden is deleted and the remaining viewers are told by push.
3. Shared gardens the user doesn't solely own: the membership, push tokens, conversations (found through `conversation.user`), their messages, their images under `gardens/{garden_id}/chat/{conversation_id}/` in RustFS, pending actions and message feedback are deleted. The user's unshared chat memories are deleted too; memories they chose to share stay with the garden. The user's scans, observations and the `source = pipeline` memories derived from them stay with the garden, because the other members rely on the plant history. `scan.user` is set to null, so nothing in the garden points at the user any more.
4. The user's scan feedback is deleted.
5. The `user` record and its auth mapping are deleted, then the account is deleted at the auth provider too, retried until it succeeds, so signing in again doesn't recreate the user.

**Removing a member** from a garden, by an owner or by leaving, runs step 3 for that garden only, so their private chats and unshared chat memories don't survive a later re-invite.

**Deleting a plant or a garden** cascades to everything under it, including conversations anchored to it, tasks, plan trigger rules, invites and memberships (for a garden), and cancels its queued and leased jobs. The full list is in [contracts.md](contracts.md#deletion-cascades).

The judging claim is therefore "deleting an account removes the user's own data and any garden they alone use", not "everything".

### Knowledge base

The knowledge base lets the system check its diagnoses and base its advice on reliable sources, instead of on the LLM's general knowledge.

| Kind | Contents | Used by |
|---|---|---|
| Species profiles | Care needs, hardiness, water, light, soil pH, and whether the disease classifier supports the species. Common garden plants outside the supported crops get `label_only` records: names and a BioCLIP label, so they can be recognised, with no care profile or problems yet | Species confirmation, plan writer, chat |
| Disease profiles | Symptoms written as checkable indicators, where symptoms start, the temperature, humidity and leaf-wetness ranges that favour the disease, how it spreads | Verification. Fusion rules take their risk thresholds from here instead of hard-coding them |
| Organism profiles | Pests, beneficial insects, wildlife and invasive species: how to recognize them and the damage they cause, their role, natural enemies, and reporting contacts for invasive species | Organism identification, verification, plan writer |
| Soil conditions | Short descriptions of soil conditions visible in photos (compaction, crusting, waterlogging, salt build-up) and what to do about each | Soil findings, plan writer |
| Graph relations | `susceptible_to`, `looks_like` (with how to tell them apart), `manages`, `caused_by` (damage → organism), `preys_on` (beneficial → pest) | Candidate lists, look-alike checks, natural-enemy advice |
| Treatments | Cultural, biological, mechanical and chemical options, with IPM first. A plan step that applies a treatment takes its category from it | Plan writer |
| Products | Names of pest-control products, each marked broad-spectrum or not | The plan validator (contracts section 7) |
| Source chunks | Text from the sources, chunked, embedded and tagged by crop, disease and topic | Plan writer and chat, with citations |

**Scope.** Only the supported crops: about 10–15 crops and their top 3–5 diseases each, roughly 50–100 disease profiles. In addition: the top pests of those crops, the common beneficial insects and garden wildlife, the invasive species reported in Ontario, and a short list of soil conditions.

**Sources.** Curated only: university extension fact sheets and Canadian and Ontario government agriculture guides. No open web search. If a fallback is needed later, it should be a search restricted to an allowlist of domains.

**Ingestion** (`ml/knowledge/`):

1. Fetch the source documents, then clean and chunk them.
2. Tag each chunk with crop, disease and topic using the text classifier, and drop irrelevant chunks.
3. Embed the chunks with the same text-embedding model `api` uses at query time (section 10), so the vectors are comparable. Chunks are embedded in batches (64 per call by default), and a chunk whose text hash hasn't changed since the last run keeps its vector.
4. Extract the structured profile fields (indicators, conditions, look-alikes, damage patterns) with an LLM, in the seed format of [contracts.md](contracts.md#10-knowledge-seed-format). Each symptom becomes an indicator with a weight, and `looks_like` edges are written in both directions. For each profile, mark the 3 most discriminating indicators: the highest-weighted ones that differ from its `looks_like` neighbours. Only those are asked during verification.
5. A teammate reviews every profile before it's loaded. Bad data here produces confident wrong advice.
6. Load everything through SurrealDB migrations, so every environment has the same knowledge base.
7. Generate the BioCLIP label list from the `species` and `organism` tables in the same step, so the classifier's labels and the knowledge base can't drift.

**Licensing.** Store only short excerpts, keep `source.url` on every chunk and profile, link back to the source in the app, and check each source's terms before ingesting it.

---

## 6. Analysis pipeline

### 6.1 Stages

| Stage | Output | Where it runs |
|---|---|---|
| Quality gate | Blur and exposure score for each shot as it is uploaded, so the retake prompt is instant. Whether a plant is present comes from segmentation | api (CPU) for blur and exposure, perception-worker for plant present |
| Segmentation | Leaf regions from a detector fine-tuned on PlantDoc's leaf boxes, with masks from a SAM-class model prompted with those boxes (PlantDoc has no mask outlines); plant, soil and surroundings regions when the model has them. Masks are optional, and later stages work without them | perception-worker (YOLO detector + SAM-class model) |
| Species identification | Top 3 species with scores, cross-checked with the VLM. Skipped once the plant's species is confirmed. See 6.2 | perception-worker (BioCLIP) + vlm-worker |
| Disease classification | The top 3 problems for each leaf region, or for the whole close-up when no region was found, restricted to the working species' labels. The API takes the 3 distinct problems with the highest confidence as the classifier candidates. Runs only for species the classifier was trained on | perception-worker (classifier fine-tuned on PlantDoc-style field images) |
| Image embeddings | Image vectors for finding similar past cases. Text embeddings for memory and knowledge queries are not a stage: they run in `api` on CPU, see 10 | perception-worker |
| Context | Current weather, the past 7 days of weather, a 7–14 day forecast, soil moisture and temperature, evapotranspiration, climate zone, plus the garden's stored soil profile | api (Open-Meteo, cached per ~1 km cell and hour and shared with the scheduler, see 8.3; SoilGrids once per garden) |
| Retrieve | The pre-VLM candidate set, capped at about 9 profiles: the disease classifier's top 3 (when it ran) plus the problems linked to the working species through `susceptible_to`, including its pests and wildlife damage, ranked by classifier score, then by how well the current weather fits each profile's favourable conditions, then by prior prevalence for the crop. Look-alikes of the top-ranked candidates fill the remaining slots. Each profile brings its 3 most discriminating indicators. Anything outside the cap is still caught by the `verify` rule below. Runs before the VLM job so the VLM can answer the indicators in the same job. See 6.3 | api (`knowledge`) |
| VLM pass | One `vlm` job per scan, always two calls of at most 4 images each: (a) findings, on the shots in a fixed order: soil condition (see 6.4), organisms with bounding boxes and damage patterns (see 6.5), a species cross-check; (b) indicator answers, on the shots the questions name: a yes / no / unsure answer, with a short reason, for each indicator of each candidate (see 6.3). Guided soil checks get their own single-image `guided` job | vlm-worker (`vlm` and `guided` jobs) |
| Verify | Candidates scored deterministically from the indicator answers, the weather and the profiles. If the VLM reported an organism or damage pattern outside the candidate set, one `verify` job asks that candidate's indicators first. Ambiguous cases end in a question for the user. A `verify` job also runs for a new photo. See 6.3 | api (scoring), vlm-worker (`verify` job) |
| Fusion | Risk scores from rules over the verified findings, weather, soil and history. Thresholds come from the disease profiles (e.g. humidity > 90% at 15–25 °C for 2+ days raises fungal risk). Evidence is ranked user input, then photo, then modelled data (see 6.4) | api |
| Plan writer | Steps, timing, what to watch for, tasks. Based on the confirmed disease's management guidance, with citations | api → agent core → LLM API |

**Batching.** The perception-worker runs each model once per job over all of the scan's images or leaf crops, split only above a configured batch size, rather than once per image. The VLM pass already asks every indicator in one job. Whether the vlm-worker runs two scans' jobs at once, so vLLM can batch them, is set from the day-2 benchmark (8.4).

If the confidence is low, the app asks for a better photo or suggests contacting a local extension office. It does not give a confident diagnosis.

### 6.2 Species identification

1. BioCLIP compares the photos against a label list of the supported crops plus common garden plants (`label_only` species records), and returns the top 3 species with scores.
2. The VLM names the species independently.
3. If both agree with a high score, the species is stored as `suggested` and the user confirms it with one tap. If they disagree, or the score is low, the app shows the top 3 with reference photos, plus a "something else" option.
4. The pipeline continues with the best candidate, so results aren't held up. If the user picks a different species, the plant's most recent scan (created in the last 24 hours and not `failed`) follows it. A scan that is still `pending` or `queued` simply uses the new species when it runs. A scan already past that starts a new **run**: its status returns to `analysing`, jobs of the old run are cancelled and their late results rejected, any open question is superseded, and species, disease, the VLM stages and scoring run again as a new `perception` job whose payload names the stages, the species and the earlier run's segmentation regions. The plant check, segmentation and image embeddings of the earlier run are kept, unless the earlier run hadn't finished them, in which case the new run does every perception stage. Choosing "something else" tells perception to skip species identification. If the scan already had a plan, the new plan supersedes it.
5. The confirmed species is stored on the `plant` with `species_status = confirmed`. Later scans of the same plant skip this stage.
6. The disease classifier runs only if `species.classifier_supported` is true. For other species, the pipeline relies on the VLM's findings and marks them as lower confidence. This stops an unsupported plant being diagnosed as the closest supported one, such as basil being reported as "pepper, healthy".

The capture flow (see 6.6) asks for a whole-plant photo plus a leaf close-up, and a flower or fruit if there is one, because identification is much more accurate with several views.

An external identification API (Pl@ntNet, Kindwise/plant.id) can be added later behind `SpeciesIdentifier` as a backup, after checking its free tier and terms.

### 6.3 Retrieve, then verify

Classifiers and VLMs mostly fail on look-alikes: early blight vs. Septoria leaf spot, a nitrogen deficiency vs. a disease, sunscald vs. a fungal lesion. Verification checks each candidate against its profile before anything is shown as a diagnosis.

```
working species → pre-VLM candidate set (api), capped at about 9 profiles:
                  disease classifier top 3 (if it ran) + the problems the species
                  is susceptible_to, including pests and wildlife damage, ranked by
                  classifier score, then weather fit, then crop prevalence;
                  look-alikes of the top-ranked candidates fill the remaining slots
        │
        ▼
retrieve each candidate's profile (api, before the VLM job):
  its 3 most discriminating indicators, with weights · where symptoms start ·
  favourable weather · how to tell apart
        │
        ▼
verify:
  1. the VLM answers the indicators of every candidate in the `vlm` job's
     second call, on the shots the questions name:
     yes | no | unsure, plus a short reason
     ("concentric rings?", "started on lower leaves?", "yellow halo?")
  2. if the VLM found an organism or damage pattern that is not in the set,
     one `verify` job asks that candidate's indicators (the only other time
     `verify` runs is for a new photo)
  3. api scores each candidate: sum of the weights of matched indicators,
     minus the weights of contradicted ones; unsure counts for nothing
  4. the weather is checked against each profile's favourable conditions
        │
        ├─ clear winner → diagnosis, with citations and the matched indicators
        └─ ambiguous    → one targeted question to the user (at most 2 per run,
                          none once the scan is planned),
                          or a request for a specific photo. An answer is
                          rescored in api; a new photo runs a `verify` job.
                          No answer within 10 minutes: the scheduler continues
                          the scan with the best candidate marked uncertain,
                          and the question stays open
```

Answering specific indicator questions is more accurate than asking one broad question ("what disease is this?"). It also makes the diagnosis explainable: the app shows which indicators matched. The scoring is arithmetic over the VLM's answers, so no text classifier sits on the diagnosis path, and the indicators are asked in one VLM job per scan, never one call per question.

Two caps keep the job within what a quantized 7–8B model answers reliably: about 9 profiles in the candidate set, and 3 indicators per profile, so roughly 27 answers rather than 45 or more. A crop like tomato has 8–10 known problems, so without the profile cap the set with look-alikes would reach 15 or more. The indicators are chosen at ingestion (section 5), not at run time. A problem left outside the cap is not lost: if the VLM reports an organism or damage pattern outside the set, the `verify` job above asks that candidate's indicators.

A run asks at most 2 questions, all before the scan is `planned`. If it is still ambiguous after them, it continues with the best candidate marked uncertain. When the scheduler continues a scan without an answer, the question stays open on the scan, and the plan writer produces a provisional plan: general steps that are safe for every remaining candidate, labelled as provisional, with the open question kept at the top of the plan until the user answers it. A plan is provisional exactly when its diagnosis is `uncertain`. An answer or photo that arrives later always produces a new diagnosis and a new plan version, even when the winner is the same; the plan stops being provisional if the answer settles the question, and the user gets a push notification. While a photo for a question is being analysed, the question is `answering` and accepts nothing else. If that photo can't be analysed, the question is dropped and the diagnosis and plan stay: late work never fails the scan, whether it arrives in `diagnosed` or `planned`.

When a plan is superseded, its open tasks are dismissed and its trigger rules removed, so the user never sees steps from an outdated diagnosis.

A plan is rewritten only for a late answer, a species change, a guided check whose texture differs from the one the plan used, or a confirmed chat revision. Every plan write of a scan, the run's first plan included, runs one at a time, and each checks before storing that it still replaces the current plan. An automatic rewrite that loses starts again from the newer plan; a chat revision stores exactly the preview the user confirmed, or returns 409 if the plan changed, and is never retried automatically. A guided check that finishes after the diagnosis writes a new diagnosis with the texture, and the plan is rewritten from it when the texture changed. Constraints from a confirmed chat revision ("no copper") are stored on the plan and carried into every later version.

A plant is called **healthy** only on positive evidence: no candidate scores above zero, nothing points to a disease, pest or damage, and either the classifier's top result is healthy or the candidates' indicators are answered no. A plant where every indicator came back unsure is ambiguous, not healthy, and gets a question, usually a request for a close-up.

### 6.4 Soil

Soil information comes from three sources. Each one covers something the others can't.

| Source | What it gives | When it's fetched |
|---|---|---|
| **SoilGrids** (ISRIC REST API) | Properties that don't change: pH, texture (clay, sand, silt), organic carbon, nitrogen, cation exchange capacity, bulk density, at depths from 0–5 cm down to 100–200 cm. About 250 m resolution | Once, when the garden is created. Stored as `garden.soil_profile` and refreshed rarely |
| **Open-Meteo** (part of the weather call) | Conditions that change hourly: soil moisture at 0–1, 1–3, 3–9, 9–27 and 27–81 cm; soil temperature at 0, 6, 18 and 54 cm; evapotranspiration, including the FAO reference rate (ET₀) used to estimate watering needs | On every scan (stored on the scan as its weather snapshot) and in the scheduler's forecast checks |
| **The photo** (vlm-worker) | What the soil looks like now: moisture, surface (crusted or cracked), structure, colour, cover (bare, mulched, residue), deposits (salt crust, moss, algae, mould), erosion and weeds. Earthworms are reported as organisms | On every scan |
| **Guided checks** (vlm-worker, optional) | Texture from a squeeze test or a jar test | When the user does one |

SoilGrids is officially in beta, comes with no uptime guarantee, and allows 5 calls per minute. Fetching it once per garden keeps us well within that. If it proves unreliable, the fallback is Agriculture and Agri-Food Canada's Soil Landscapes of Canada data, loaded as a static map layer.

**Evidence priority.** Both APIs return modelled values for the area around the garden, not for the user's actual soil. They know nothing about raised beds, containers, potting mix, compost or recent watering. Fusion therefore ranks the evidence:

1. **User input:** "it's in a container", a pH test kit result, "watered an hour ago". This already arrives through plant memory.
2. **The photo:** what the VLM sees.
3. **Modelled data:** Open-Meteo for current conditions, SoilGrids for baseline properties.

The plan writer says which source it relied on.

**Soil findings are a list of observations, not a score.** For example: "moisture: dry (high confidence)", "surface: crusted (medium)", "structure: compacted (medium)". A single "soil health: 72/100" would claim a precision the photo can't support. A photo can't show nutrient levels, pH, contaminants or microbial life. For those, the app suggests a home test kit or a lab soil test, and accepts the result as user input.

**Guided checks (optional).** The app walks the user through a simple test and analyses a photo of the result. Each check photo is analysed alone, in its own single-image `guided` job, so it never competes with the plant photos for the VLM's 4 images. A check can be added to a scan at any point, even after its plan, and as many times as needed: a jar that hadn't settled is simply photographed again. Guided jobs don't belong to a run, so a species change never cancels them, and they never fail the scan. A result whose texture differs from the one the current plan used produces a new plan version:

- **Squeeze test:** squeeze a handful of moist soil. Whether it crumbles or holds together as a ball gives a rough texture estimate.
- **Jar test:** shake soil with water in a jar and let it settle. The layers show the sand, silt and clay proportions. Settling takes hours, so the app creates a task to photograph the jar later.

### 6.5 Organisms and damage

The photos are checked for living things and for the damage they leave. Insects can often be seen directly. Larger animals mostly show up as damage, holes, tracks or droppings.

| Category | Examples | Detected by |
|---|---|---|
| Insect and other pests | Aphids, whiteflies, spider mites, caterpillars and hornworms, Colorado potato beetle, flea beetles, slugs and snails | The VLM, which names each organism and returns its bounding box (Qwen2.5-VL-class models do this grounding natively). The segmentation model has no organism class, so the VLM is the detector. BioCLIP on those crops as a second opinion is a stretch goal. Webbing, slime trails and droppings also come from the VLM. Tiny pests need a close-up of the leaf underside |
| Damage patterns | Small "shot holes" (flea beetles), ragged holes with slime (slugs), tunnels inside leaves (leaf miners), sticky residue and black mould (aphids), cleanly cut stems (rabbits), torn stems (deer) | The VLM, checked against the knowledge base's `looks_like` comparisons |
| Larger animals | Rabbits, deer, groundhogs, squirrels and raccoons digging, vole and mole tunnels, birds pecking fruit | Mostly from damage, holes, tracks and droppings |
| Beneficial organisms | Ladybugs and their larvae, lacewing larvae, ground beetles, spiders, bees, earthworms | The VLM. Never flagged as pests |
| Invasive species | For example jumping worms, Japanese beetle, box tree moth | The VLM, plus reporting guidance from the knowledge base |

Organism and damage findings join the disease candidates in retrieve-then-verify (6.3). "Slug damage vs. flea beetle damage" is verified the same way as "early blight vs. Septoria leaf spot".

**Beneficial organisms.** When one is found, the app says so and explains what it does, using the `preys_on` graph: "those are ladybug larvae, leave them, they eat aphids". Ladybug larvae are often mistaken for pests and killed.

**Invasive species.** If an invasive species is likely, the app shows how to report it (for example to the CFIA or the Invasive Species Centre), using `organism.reporting_url`. The user decides whether to report it. Nothing is sent automatically. The species list and reporting contacts are confirmed during knowledge-base curation.

**Wildlife rules.**

- Humane deterrents come first: fencing, netting, row covers, repellents.
- Never recommend poison baits for mammals. They can kill pets, and birds that eat the poisoned animals.
- The app reports the damage and how to protect the plants. It doesn't treat the animal as something to get rid of.

### 6.6 Capture guidance

| Shot | When | Used for |
|---|---|---|
| Whole plant (1) | Always | Species, overall health, surroundings, larger-animal damage |
| Close-up of the affected leaf (up to 2) | When something looks wrong | Disease, damage patterns |
| Leaf underside (1) | Suggested when pests are suspected | Aphids, whiteflies, mites, eggs |
| Soil surface around the base (1) | Suggested | Soil condition, organisms in the soil |
| Flower or fruit (1) | If present | Species identification |
| Squeeze test (1), jar test (1) | Optional guided soil checks | Soil texture, each in its own `guided` job |

Each image is stored with its `scan_image.kind`, so each stage knows which photos to use. Perception sees every plant and soil shot. Each VLM call gets at most 4: the findings call takes them in the order whole plant, first leaf close-up, leaf underside, soil surface, second leaf close-up, flower or fruit; the indicator call takes the shots its questions name (each question lists the shot kinds that can show it); guided-check photos go alone to their own call. The rules are in [contracts.md](contracts.md#5-worker-api-and-jobs). The blur and exposure check runs in `api` as each shot arrives and rejects a failing shot with a retake hint before the next one is taken. Analysis starts only when the app calls `POST /scans/{id}/submit`, so the API always knows the set of shots is complete. On the web build, each shot can also be chosen from a file picker.

---

## 7. Chat and agent core

Chat runs **inside `api`, not on the job queue**, because it is interactive. Tokens stream straight from the LLM to the app over SSE. Only heavy actions, such as re-analysing a new photo, go through the workers. The query embedding for memory and knowledge search is computed in `api` on CPU (section 10), so a chat turn never waits on the rig.

### Conversation anchors

| Anchor | Opened from | Context loaded |
|---|---|---|
| `scan` | "Ask about this result" | That diagnosis, its observations and image regions, the weather, the plan |
| `plant` | Plant timeline | All scans of the plant, its memory, open tasks |
| `garden` | Garden home | Summaries of each plant, garden-level memory, the forecast, upcoming tasks |

The context is rebuilt from the database on every turn. It uses the conversation summary, the most recent messages and the top memory hits, not the full transcript. The chat therefore always sees the latest data, such as a frost task the scheduler created an hour earlier.

### Tools

| Tool | Purpose | Kind |
|---|---|---|
| `get_scan(scan_id)` | Findings, confidence, regions | read |
| `view_image(image_id, region?)` | Sends the photo, or a crop of it, back to the model. Limited to the garden's scan images and this conversation's own images | read |
| `get_forecast(garden_id, days)` | Weather forecast | read |
| `search_memory(query, plant_id?)` | Vector search over the plant's facts and events plus the garden-level ones | read |
| `find_similar_cases(image_id)` | Embedding search over the garden's past scans | read |
| `search_knowledge(query)` | Retrieval over curated guides, returned with citations | read |
| `get_species_profile(species)` | Care needs, diseases the species is susceptible to | read |
| `get_disease_profile(disease)` | Symptoms, favourable conditions, management, sources | read |
| `compare_lookalikes(disease)` | Look-alike diseases and how to tell them apart | read |
| `get_organism_profile(organism)` | How to recognize it, its role (pest, beneficial, invasive), natural enemies, reporting guidance | read |
| `get_soil_profile(garden_id)` | The garden's SoilGrids profile, plus the latest soil moisture and temperature | read |
| `list_tasks` | The garden's tasks | read |
| `create_task` / `complete_task` | Manage the task list | **write: user confirms** |
| `revise_plan(constraints)` | Proposes a new version of the current plan; the card names the plan it is based on and says the plan is shared with the garden | **write: user confirms** |
| `request_rescan(plant_id, photo_kind, instructions)` | Asks the user for a specific photo; the app offers to open the capture flow for a new scan | read (UI action) |
| `remember(fact, plant_id?)` | Saves a private chat memory about the plant, or the garden when no plant is given; the app shows it with Undo, and Share with garden for editors and owners. Offered to every role, because it only writes the caller's own memory | write (automatic, shown to the user) |

Viewers and reviewers are only offered the read tools and `remember`, so they never get a confirmation card.

Write tools return a `pending_action`. The app shows it as a confirmation card, and the model never changes data by itself.

The pipeline's verify and plan-writer stages use the same knowledge tools.

### Router and safety gate

Before the LLM runs, a fast text classifier (`TextClassifier`: Jev if available, otherwise the cheapest LLM with structured output) labels each message:

| Label | Effect |
|---|---|
| `diagnosis_question`, `care_question` | Normal turn with the relevant context |
| `task_change` | Normal turn. Write tools are likely |
| `safety_sensitive` (pesticide doses, a person or pet poisoned) | The safety policy is applied first. Poisoning questions get poison-control guidance |
| `off_topic` | A short, polite redirect, using the cheapest model |

The scores are compared with each other rather than treated as certainties, and they are calibrated on a small labelled set of example messages.

### Memory

Each memory belongs to a garden and, usually, to one plant. Facts that apply to the whole garden, such as "I garden organically", "everything is in containers" or "the beds are on a balcony", are stored with no plant and are recalled in every conversation and every plan for that garden. The plan writer treats them as constraints.

`memory` is written from two places:

- **The pipeline**, after each scan: a summary of the findings.
- **The chat**, after each turn: the text classifier first decides whether the turn contains a fact that will still matter later. Only if it does, a cheap model extracts it, such as "watered yesterday", "applied neem oil Oct 18" or "it's in a container".

Memories with `source = chat` are private to their author: chat recall returns them only in the author's own conversations. The author can share one with the garden from the "Remembered" card, which turns it into a `user` memory, or undo it. Pipeline memories and shared memories are visible to every member. **The plan writer reads only shared memories**, because every member sees the plan, so a plan never quotes something said in a private chat.

Memory persists across conversations and scans, which keeps advice consistent over time. It sits behind a `MemoryStore` trait, so it can be mapped onto SurrealDB Agent Memory if we compete for that prize.

---

## 8. Main flows

### 8.1 Scan → diagnosis → plan

```
pending → queued → analysing → [awaiting_answer, at most 2 per run] → diagnosed → planned
analysing | awaiting_answer | diagnosed | planned → analysing   (new run: species changed)
any state after pending → failed (with a reason) → queued      (new run: resubmit of a
                                                                 retryable failure only)
```

The scan's status is the state machine above, and it only models waits that block the pipeline. A pending species confirmation is not one: it lives on `plant.species_status = suggested`, and the app shows the confirmation card from that while the scan proceeds. Every transition and every finding is written to `scan_event` with a sequence number before it is sent over SSE. A **run** is one pass of the pipeline; jobs and events carry it, and results from an older run are discarded. A healthy plant gets a short care plan and ends in `planned`. A scan with no plant found, or with a plant but nothing readable from the photos (no leaf region, no classifier result including "healthy", no confident species and no VLM finding), ends in `failed` (`no_plant_found` or `photos_unusable`); a readable plant always reaches scoring instead, and is called healthy only on positive evidence (the classifier's top result is healthy, or the candidates' indicators are answered no), otherwise uncertain; those aren't retryable, and the app starts a new scan for new photos. Late work (a late answer or photo while the scan is `diagnosed` or `planned`, and every guided soil check) never changes the scan's status or fails the scan; it produces a new diagnosis and, when needed, a new plan version, or is dropped with an event if it fails. Event order and stream closing are defined in [contracts.md](contracts.md#4-scan-events-sse).

1. The app sends `POST /scans` with the plant, or a name for a new plant, which the API creates. The API checks membership and creates the scan with status `pending`.
2. The app uploads each shot to `POST /scans/{id}/images` with its kind. The API runs the blur and exposure check on CPU and rejects a failing shot at once with a retake hint. For a good shot it applies the EXIF orientation to the pixels, strips the metadata, blurs any faces, resizes the image and writes it to RustFS. HEIC uploads are decoded first.
3. The app calls `POST /scans/{id}/submit`. The scan becomes `queued` and a `perception` job is created. The app opens SSE on `/scans/{id}/events`, sending `Last-Event-ID` if it is reconnecting.
4. The perception-worker claims the job. The scan becomes `analysing`. The worker runs the plant-present check, segmentation, species identification (unless the plant's species is already confirmed), disease classification and image embeddings. It sends each stage's output as a partial result when the stage finishes, so findings appear before the job ends; the final result repeats them and wins if they differ.
5. The API stores the observations, fetches the weather, including soil moisture and temperature, and loads the garden's soil profile. It builds the candidate set, capped at about 9 profiles: the classifier's top 3 plus what the working species is susceptible to, ranked by classifier score, weather fit and crop prevalence, with look-alikes of the top-ranked ones filling the remaining slots. It retrieves each profile and its 3 indicators, and queues a `vlm` job whose payload carries those indicators. Problems outside the cap are caught in step 7 if the VLM finds them.
6. The vlm-worker runs the `vlm` job in two VLM calls of at most 4 images each, findings then indicators: soil condition, organisms with boxes, damage patterns, its own species suggestion, and an answer for every indicator. If the species isn't confirmed, `plant.species_status` becomes `suggested` and the app shows the confirmation card. The scan's status doesn't change, and the pipeline continues with the best candidate. If the user later picks a different species, the re-run from 6.2 step 4 puts the scan back to `analysing`.
7. If the VLM found an organism or damage pattern outside the candidate set, the API queues one `verify` job for that candidate's indicators and waits for it. It then scores all candidates and checks the weather against their conditions. A clear winner becomes a `diagnosis` with citations and the scan is `diagnosed`. An ambiguous result puts the scan in `awaiting_answer` and sends the user one question, at most 2 per run. The answer is rescored in `api`. A new photo runs a `verify` job. After 10 minutes without an answer, the scheduler's sweep (8.3) continues the scan with the best candidate marked uncertain; the question stays open. A later answer, or a photo for it, always produces a new diagnosis and a new plan version, even when the winner is the same.
8. Fusion computes the risk scores. The agent core writes the `action_plan` from the confirmed disease's management guidance, or a provisional plan if the diagnosis is still uncertain, and creates tasks. The scan is `planned`.
9. The API appends a summary to memory, updates the plant's status, sends `status_changed → planned` and then `plan_ready`, closes the stream unless a question is still open, and sends a push notification in case the app is closed.

### 8.2 Chat turn

1. The app sends `POST /conversations/{id}/messages`. The API checks auth, the conversation's owner, garden membership and the user's token budget. The text classifier labels the message (see section 7).
2. The context builder loads the anchor's data, the conversation summary, recent messages and the top memory hits.
3. The LLM call streams over SSE. Read tools run as the session user. Write tools, offered only to editors and owners, return confirmation cards. `request_rescan` lets the app offer flow 8.1. Every assistant reply, whatever its route, is checked one sentence at a time against the dose check before it is streamed. Every string the model writes into a tool call (task text, card text, plan revisions and constraints, rescan instructions, memories) is dose-checked before the tool runs, and a `revise_plan` preview also passes plan validation, so a card shows exactly what confirming stores; a failing call goes back to the model with the reason.
4. The API stores the message with its model, prompt version and token counts. If the classifier finds a fact worth remembering, a cheap model extracts it into a private chat memory, and the app shows it with Undo and Share with garden. Long conversations are summarized.

### 8.3 Forecast alerts

At a fixed interval, the scheduler fetches the forecast and checks every garden's trigger rules. Gardens are grouped by weather cache cell (coordinates rounded to about 1 km), each cell is fetched once, and missing cells go to Open-Meteo in multi-location requests of up to 50, so a hundred gardens in one city cost a handful of requests. The results land in the weather cache that scans read, so a scan in the same hour reuses them. The rules are frost, a humid stretch (fungal risk), heat, heavy rain, and dry soil (a watering reminder based on soil moisture and evapotranspiration). When a rule fires, it creates a `task` with `source = forecast_alert` and sends a push notification. Each alert has an `alert_key` of garden, trigger and date, with a unique index on it, so a frost warning is created once rather than on every run even when two scheduler runs overlap. The in-app alert inbox is the list of these tasks, and it works in every build, with or without push.

The same scheduler also:

- sweeps scans that have been in `awaiting_answer` for more than 10 minutes and continues them with the best candidate marked uncertain, so the timeout has an owner and works even when the app is closed. A scan whose question has a photo `verify` job in flight is skipped until that job finishes;
- evaluates the **plan trigger rules** that `on_trigger` plan steps register (after rain, frost or heat forecast, a humid stretch, dry soil), and creates the step's task when its rule fires. The task's `alert_key` is `plan:<plan_id>:<step_id>:<date>` under the same unique index as forecast alerts, so it is created at most once a day per step of that plan version even when scheduler runs overlap; step ids are only unique within a plan. Plan triggers and forecast alerts share one trigger vocabulary (frost, heat, humid stretch, heavy rain, after rain, dry soil);
- nothing is needed for **recurring** plan steps: completing a repeating task creates the next one.

Stretch: when several gardens within about 5 km report the same disease in a week, the nearby gardens get a regional alert (the geo query in section 5).

### 8.4 Job leasing and fallback

- `POST /internal/jobs/claim` is a long poll. Inside a transaction, it leases the oldest queued job that matches the worker's capabilities, for 2 minutes.
- Each claim returns a lease token that every later call about the job must carry; a requeue invalidates it, so a worker that dropped off and came back gets `409 lease_lost` instead of writing duplicate results.
- Each worker declares `slots`, the most jobs it holds at once, and the broker never leases it more. The perception-worker has 1 and batches inside the job. The vlm-worker has `VLM_SLOTS`, 1 or 2: 2 only if the day-2 benchmark shows two 4-image jobs fit the 5070 Ti's KV cache and each still finishes well within its `max_runtime`, in which case vLLM batches the two jobs' calls. The fallback worker has several, bounded by the LLM API's rate limit.
- Workers heartbeat through `POST /internal/workers/heartbeat` every 10 seconds. A heartbeat that lists a job with its lease token extends that job's lease, so a slow VLM or fallback job isn't requeued while it runs, but only for the worker that holds it and **never past `claimed_at + max_runtime`** for the job's kind (2 minutes for perception, 5 for vlm, 3 for verify and guided, doubled on the fallback worker). A worker whose model hangs while its heartbeat thread keeps running still loses the job.
- The broker requeues the jobs of any worker whose heartbeat is older than 30 seconds without waiting for the lease to expire, and resets `queued_at`. Attempts count retryable failures plus every requeue after the 2nd, up to 3, so a job that keeps crashing its worker stops. A run's job that runs out of attempts fails the scan; late work (a late answer's verify job, any guided check) only reports its failure.
- When a scan starts a new run, its old jobs are cancelled; workers learn it from the heartbeat response. Any call about a requeued, expired or cancelled job, including an older run's, gets `409 lease_lost`.
- The `vlm` capability claims `vlm`, `verify` and `guided` jobs; `fallback` claims every kind.
- Jobs still unclaimed 15 seconds after `queued_at` also become available to the `fallback-worker`, which uses the LLM API. A rig that dies mid-scan therefore hands over within about 45 seconds, not after the 2-minute lease.
- A small status page shows the health of each GPU worker.

### 8.5 Garden setup

The garden's centroid comes from the phone's GPS when the user has given location consent, otherwise from a typed postal code or city, geocoded once. It is the only location the system stores; scans have none of their own. The garden's time zone is derived from it and sets every local date (task due dates, alert dates). When a garden is created, the API fetches its SoilGrids profile in the background and stores it as `garden.soil_profile`. If SoilGrids is unavailable, the garden is created anyway and the fetch is retried later. Until it succeeds, the pipeline runs without baseline soil properties.

---

## 9. API surface

```
Scans (garden member; writes need editor)         Worker API (per-worker token)
POST   /scans                                     POST /internal/workers/heartbeat   (extends lease)
POST   /scans/{id}/images                         POST /internal/jobs/claim          (long-poll; lease token)
DELETE /scans/{id}/images/{image_id}              POST /internal/jobs/{id}/events    (X-Lease-Token)
GET    /scans/{id}/images/{image_id}              POST /internal/jobs/{id}/complete  (X-Lease-Token)
GET    /scans/{id}/masks/{mask_id}
POST   /scans/{id}/submit                         GET  /internal/jobs/{id}/images/{image_id}
GET    /scans/{id}
GET    /scans/{id}/events          (SSE, resumable with Last-Event-ID)
POST   /scans/{id}/answers                        Webhooks
POST   /scans/{id}/feedback                       POST /webhooks/auth                (signature-verified)

Gardens, plants, tasks                            Ops
CRUD   /gardens                                   GET  /healthz
GET    /gardens/{id}/members                      GET  /status                       (worker health)
PATCH  /gardens/{id}/members/{user_id}
DELETE /gardens/{id}/members/{user_id}
GET    /gardens/{id}/invites · POST /gardens/{id}/invites   (owner)
DELETE /gardens/{id}/invites/{invite_id}   (owner)
POST   /invites/{code}/accept
CRUD   /plants  · GET /plants/{id}/timeline
POST   /plants/{id}/species        (confirm species)
CRUD   /tasks                      (?source=forecast_alert is the alert inbox)

Account
GET    /me · PATCH /me · DELETE /me
POST   /me/push-tokens · DELETE /me/push-tokens/{token}

Memory
GET    /memories · POST /memories   (garden note, editor)
POST   /memories/{id}/share · DELETE /memories/{id}

Chat (conversation owner + garden member)
POST   /conversations · GET /conversations
GET    /conversations/{id}
POST   /conversations/{id}/messages (SSE)
POST   /conversations/{id}/images
GET    /conversations/{id}/images/{image_id}
POST   /messages/{id}/feedback
POST   /pending-actions/{id}/confirm
POST   /pending-actions/{id}/reject
```

Request and response bodies, the scan and chat event catalogues, worker job payloads and results, and the error format are defined in [contracts.md](contracts.md).

`utoipa` generates the OpenAPI spec from the routes. `openapi-typescript` generates the app's TypeScript types from that spec.

Both SSE endpoints are consumed with a streaming `fetch` response (Expo SDK 52+ provides one), because the browser `EventSource` can only GET and cannot carry the auth header. Scan events are persisted in `scan_event`, so the app reconnects with `Last-Event-ID` after being backgrounded and receives everything it missed. Chat turns are not resumable: a dropped chat stream is shown as an interrupted message the user can resend.

---

## 10. Models and GPU placement

| Task | Primary | Fallback |
|---|---|---|
| Blur and exposure check | `api`, CPU (Laplacian variance and a histogram) | None needed |
| Plant-present check, segmentation, disease classification, image embeddings | RTX 3070, ONNX via `ort` | LLM API vision. Embeddings are skipped |
| Species identification | RTX 3070, BioCLIP (ONNX), cross-checked with the VLM | LLM API vision. Optionally Pl@ntNet or Kindwise/plant.id |
| Face detection (for blurring on upload) | `api`, CPU, small ONNX face detector | None. Fails closed: an image isn't stored until blurring succeeds |
| Text embeddings (queries, memory, knowledge chunks) | `api`, CPU, one small open text-embedding model via `ort` (`fastembed`-style), pinned by name and version in `TextEmbedder` and recorded on every vector | None. A different model would make the existing vectors incomparable, so the only acceptable substitute is a hosted endpoint serving the identical model |
| Soil condition, organisms with bounding boxes, damage patterns, species cross-check, verification indicator answers, in one job of two calls | RTX 5070 Ti, a Qwen2.5-VL-class 7–8B VLM, 4- or 8-bit quantized, with structured output | LLM API vision |
| Plan writer, chat | LLM API, strongest model | none |
| Summaries, memory extraction | LLM API, cheapest model | None. `api` has no synchronous path to the rig |
| Chat routing, safety gate, memory gate, chunk tagging | Jev (optional, early access) | LLM API, cheapest model, with structured output |

Notes:

- The RTX 3070 models (BioCLIP, the YOLO leaf detector, a small SAM-class mask model such as MobileSAM or SAM 2 small, the disease classifier) need roughly 4 GB of VRAM together, within the card's 8 GB. Batching adds activation memory, which the perception batch size (default 16) keeps within the remaining headroom.
- The text-embedding model runs on the control plane because chat and knowledge search need a query vector synchronously, and workers only pull. A short query embeds in a few milliseconds on CPU. The same model, loaded from the same file, is used by the ingestion scripts, so the knowledge-chunk vectors match the query vectors.
- **Jev** is text only and zero-shot, at around 150 ms per call. Its probabilities run somewhat high, so compare scores between candidates and calibrate them on the evaluation set. Nothing on the critical path depends on it. If early access hasn't arrived by Oct 17, use the fallback.
- **No multi-GPU model sharding.** With two mismatched consumer cards, the "cluster" is a pool of workers pulling from one queue, each advertising what it can do. More GPUs means more workers.
- **Day-1 check:** the 5070 Ti (Blackwell) needs CUDA 12.8+ builds of PyTorch, vLLM and ONNX Runtime. Confirm inference runs before building on it.
- **Training:** fine-tune the classifier in Python on PlantDoc and similar datasets of field photos, and export it to ONNX. Models trained only on PlantVillage generalize poorly to real photos.
- Keep a small evaluation set of real field photos, and record the accuracy in the Devpost write-up.

---

## 11. Security, privacy and safety

**Identity and access**
- Managed authentication with MFA. The JWT is verified on every request.
- Membership is checked on every query scoped to a garden.
- Agent tools take identity from the session, never from model output. Write tools are offered only to editors and owners, and need the user's confirmation.
- Non-members get `404`, never `403`, so a response never confirms that an ID exists. Members join with single-use invite codes of 128 random bits; accepting is rate-limited.

**Infrastructure**
- The rig opens no ports. Workers connect outbound and authenticate with their own tokens.
- RustFS is never exposed publicly. Images reach the app and the workers only through the API, by signed URLs that expire after 15 minutes.
- Auth webhooks are signature-verified.

**Privacy**
- Photo metadata (EXIF) is stripped before storage, after its orientation has been applied to the pixels.
- Faces that appear in photos are blurred on upload, before storage, and are never analysed.
- Location requires consent. Only the garden has one: its centroid, at the precision the user chooses (exact, or rounded to about 1 km, which is enough for weather). Scans store no coordinates. Without consent, the user types a postal code or city.
- `DELETE /me` runs the deletion cascade in section 5: gardens the user alone uses are removed from SurrealDB and RustFS, shared gardens keep their scans with the user reference cleared, and the account is deleted at the auth provider too. Removing a member from a garden deletes their private chats there.
- Chat memories are private to their author until shared. The plan writer, whose output every member sees, reads only shared memories.
- Withdrawing location consent rounds every garden the user owns to about 10 km.

**Abuse and cost**
- Rate limits, a daily chat token budget and a daily scan limit (30) for each user. Scans cost API credits when the fallback worker runs them.
- `Idempotency-Key` on every create call, so a retried request never creates duplicates.
- Upload size limits.

**Safety of advice**
- Integrated pest management (IPM) and non-chemical steps come first.
- Advice is based on reviewed knowledge-base profiles and cites its sources.
- The text classifier flags safety-sensitive chat messages before the LLM runs, so the safety policy is applied to them first.
- Plans and chat replies never give doses. The plan shape has no dose field, and a dose check (quantities with units or number words, rates, ratios, percentages of a mixture) runs on every step of every plan, on every assistant chat sentence before it is streamed, and on every string the model writes into a tool call before the tool runs. A step that names a pest-control product is treated as chemical, and the server adds a fixed safety note to every chemical step: use only a product registered for this use where you live, and follow its label exactly.
- A step's category comes from its knowledge-base treatment, not from the model.
- Questions about a person or pet being poisoned are directed to poison control.
- Beneficial organisms are never flagged as pests, never targeted by a plan step, and a broad-spectrum product is rejected when beneficials were seen in the scan.
- Wildlife advice uses humane deterrents and never recommends poison baits.
- Invasive species reports are suggested to the user, never sent automatically.
- Low confidence leads to a request for a better photo or a referral to a local extension office.
- Photos, retrieved documents and user messages are treated as data, never as instructions. This guards against prompt injection, for example text on a seed packet in a photo.

---

## 12. Deployment

```
infra/control-plane.compose.yml   caddy · api · surrealdb · rustfs · fallback-worker
infra/rig.compose.yml             perception-worker · vlm-worker · vllm (GPU pinned per service)
```

### Hosting the control plane

| | All on the rig + Cloudflare Tunnel | Small cloud VM for the control plane, rig for GPUs (recommended) |
|---|---|---|
| Cost | $0 | A few dollars a month, or partner credits |
| Live link during judging | Goes down if home power, internet or the PC fails | Always up. Uses API vision while the rig is offline |
| Uploads | Limited by home upload bandwidth | Fine |
| Setup | Tunnel + Caddy | VM + Caddy. The rig runs only workers |

The code is the same in both cases. We can start with everything on the rig and move the control plane to a VM before submitting.

If the control plane runs behind a tunnel, SSE responses need `Cache-Control: no-cache` and a keepalive comment every ~15 seconds, so idle connections are not dropped.

### App delivery

- The live link the judges open is the Expo web build, so the capture screen accepts a file picker as well as the camera.
- Remote push does not work in Expo Go on Android since SDK 53 and needs Apple credentials on iOS. Push therefore goes through the Expo Push API from a development build, and the demo-safe path is the in-app alert inbox (section 8.3), which works in every build.
- SSE is consumed with streaming `fetch` (section 9); no `EventSource` polyfill.

### Operations

- `tracing` with JSON logs. `/healthz` and `/status` endpoints. The job table doubles as an audit trail.
- Nightly SurrealDB export (`surreal export`) and RustFS bucket copy (`rclone sync`) to storage off the host. Single-disk RustFS has no redundancy, so the copy is the only protection against disk loss.
- The SSE hub uses an in-process `tokio::sync::broadcast`. This assumes a single `api` instance. Running more than one would need SurrealDB `LIVE SELECT` or a message bus.
- Each scan's events go through one serialised writer that assigns `seq`, stores the event and broadcasts it in that order, because worker events, answers, the scheduler and species changes can all write to the same scan. A subscriber that falls behind the broadcast replays from the database.

---

## 13. Repository layout

```
PDM/
├─ app/                Expo app
├─ backend/
│  ├─ crates/{domain, store, blob, agent, vision, knowledge, pipeline}
│  ├─ bins/{api, worker}
│  └─ migrations/*.surql
├─ ml/
│  ├─ training/        classifier fine-tuning + ONNX export, evaluation set of real field photos
│  └─ knowledge/       ingestion scripts + reviewed seed data
├─ infra/              compose files, Caddyfile, tunnel config
└─ docs/
   ├─ architecture/    this document (README.md) and the data contracts (contracts.md)
   ├─ database/        SurrealDB schema (schema.surql) and its notes
   └─ storage/         RustFS setup and notes
```

---

## 14. Build plan

| Dates | Goal |
|---|---|
| Oct 16–17 | End-to-end skeleton: auth, upload with the CPU blur check, submit, scan state machine, job broker with stub vision and heartbeat requeue, resumable SSE, text embeddings in `api`, control plane deployed. Choose the crops and the text-embedding model, list the knowledge sources, request Jev access |
| Oct 18–19 | Real models on the rig (BioCLIP, disease classifier, VLM). Species confirmation. Weather with soil moisture and temperature; garden soil profile. Face blurring on upload. Knowledge ingestion running |
| Oct 20–21 | Knowledge profiles reviewed and loaded. The single VLM pass: soil, organisms with boxes, damage, indicator answers. Deterministic verification, questions and the provisional plan. Fusion and plan writer with citations. Results UI with overlays |
| Oct 22 | Chat with tools, router, memory, confirmation cards, timeline |
| Oct 23 | Forecast alerts with dedupe, alert inbox, push from a development build. Account deletion cascade. Test the fallback by killing the rig in the middle of a scan: hand-over must happen within about 45 seconds |
| Oct 24 | Feature freeze, security review, UI polish, evaluation on field photos |
| Oct 25 | Demo video (2 minutes or less), Devpost page, README, final deploy |

---

## 15. Open decisions

1. **Auth provider and app framework.** Expo and Clerk fit these roles directly, and each has its own prize.
2. **Where the control plane runs:** a cloud VM or the rig.
3. **Which crops to support.** This sets the classifier's classes, BioCLIP's label list and the scope of the knowledge base. PlantDoc covers tomato, potato, pepper, apple, grape and others.
4. **Which VLM runs on the 5070 Ti.** Benchmark two candidates on day 2.
5. **What Eon and SurrealDB Agent Memory offer.** Confirm at kickoff on Oct 16.
6. **Which LLM API models** to use for chat and plan writing, and for summaries and memory extraction.
7. **Which knowledge sources** to ingest (extension and government guides), and whether their terms allow it.
8. **Jev:** whether early access arrives by Oct 17. If not, every `TextClassifier` call uses the LLM fallback.
9. **External species-identification backup** (Pl@ntNet, Kindwise/plant.id): whether it's worth adding, after checking free tiers and terms.
10. **SoilGrids reliability:** test it on day 1. If it's unreliable, load Soil Landscapes of Canada data instead.
11. **Invasive species list and reporting contacts:** which species to include for Ontario, and where each should be reported.
12. **Guided soil checks:** in scope (squeeze and jar tests, each analysed in its own `guided` job). Decide on day 2 whether the jar test's delayed photo makes the demo video.
13. **Text-embedding model** and its dimension. Picked on day 1 and pinned for the life of the database, since the memory and knowledge vectors must all come from the same model.
14. **Verification-answer timeout.** 10 minutes is proposed before the pipeline continues with the best candidate marked uncertain.
15. **VLM concurrency (`VLM_SLOTS`)** and the perception batch size: measured in the day-2 benchmark. See [contracts.md](contracts.md#12-open-questions).
