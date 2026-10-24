# PDM Architecture

> **Status:** Draft v0.7.37, 2026-10-23. v0.7.37: in line with contracts v0.10.34, which lets the dose check read a clock time after an opening parenthesis; section 11 lists no exemptions, so no section changes. v0.7.36: decision 17's spot colour thresholds and speck filter are set, fitted on PlantSeg's lesion masks (M15), and 6.7's spots leave out specks under that share of the leaf, in line with contracts v0.10.33. v0.7.35: open decision 4 is reopened only by the public pest and beneficial cases, as the team's full cases are dropped (#52). v0.7.34: the face blur is measured (M7): on 93 public photos of people near plants it catches 94.7% of faces against 98%, and it boxes leaf spots, fruit and stems on about half the dev plant photos, so the judging table and section 11 say so and link its open fix (#159). v0.7.33: the rig is a host per card, and section 12's compose files say which host runs what; the 5070 Ti in `sam-linux` serves Qwen3.5-9B AWQ at 0.78 of the card beside its desktop, so the demo needs no headless step, with the memory that leaves; decision 19's host question is settled (3, 10, 12, 15; D11, #13). v0.7.32: open decision 3 is decided: the disease head's five crops, with the other crops and common garden plants as `label_only` species (15). v0.7.31: the knowledge base stores excerpts as far as each source's terms allow, not only short ones (5). v0.7.30: where SoilGrids has no data at a Canadian point, the soil profile comes from Soil Landscapes of Canada v3.2, shipped as a 0.9 MB lookup, before any nearby probe; open decision 10 records the evidence and the proposal for the team to confirm; a GPS garden whose reverse lookup fails is created with its time zone's label (contracts v0.10.26) (6.4, 8.5, 15). v0.7.29: the knowledge photo upload only adds photos and never deletes one (contracts v0.10.25) (5). v0.7.28: open decision 4 is decided: the VLM is Qwen3.5-9B AWQ, with Qwen3-VL-8B as the fallback; open decision 15 is decided for the 5070 Ti, `VLM_SLOTS` 2 at `VLM_MAX_PIXELS` 1,003,520 (contracts v0.10.24) (10, 15). v0.7.27: the dose check's ratio rules let dates, years, clock times and labels through ("bed 1 in 2026", "Bed 3: 2 July", "at 18:40") (contracts v0.10.23; EON gap G22) (11). v0.7.26: a species the user names must be one the knowledge base holds, or the write gets `400 invalid_request` (contracts v0.10.22) (6.2). v0.7.25: when a job other than a guided check completes, the API sends the stage events its worker couldn't deliver (contracts v0.10.21; backend roadmap gap 9) (8.1). v0.7.24: open decision 6 records E8.3's third run, after gap G18 was fixed: Sonnet then cited every named record, leaving its unasked mentions of the planted message as the reason Opus stays (15). v0.7.23: the dose check's rates include rates per field measure, container or area ("a bag per acre", "a capful per tank") and the volume abbreviations (contracts v0.10.20; EON gap G19) (11). v0.7.22: the dose check's rate fails a sentence with a unit phrase only beside an amount, or with at most three words before it, so a refusal that only names the unit passes (contracts v0.10.19; EON gap G13) (11). v0.7.21: `search_farm_records`' `since` and `until` window Slack messages only; a Drive document is returned whatever the window (contracts v0.10.18; EON gap G18) (7). v0.7.20: open decision 6 holds after the agent evaluation's re-run: Sonnet 5.5 matched Opus 5.5 on answers, cross-source questions and safety at less than half the cost, but brought the planted message up unasked in 16 of the 17 other answers whose records held it (Opus 0 of 7) and missed one question's citations, so `claude-opus-5-5` stays the chat and plan model (15). v0.7.19: `search_farm_records` takes `until?` and `author?`, a search without `kind` keeps 3 of its 8 hits for each kind, and a farm record is embedded under a line naming its author and title (contracts v0.10.17; EON gaps G12 and G16) (5, 7). v0.7.18: open decision 6 is decided by the agent evaluation: `claude-opus-5-5` for chat and plan writing (the plan writer provisionally, and both until a re-run after EON gap G16), with `claude-haiku-5-5` kept on the cheap calls (15). v0.7.17: in a garden with farm records, the router counts a question about those records (its Slack channels and shared documents) as on topic, so it reaches the chat model; a garden without them is routed as before, and a question about the doses or products the records hold stays safety-sensitive (7, router and safety gate; [EON](../EON/README.md) gap G10). v0.7.16: synced with [contracts.md](contracts.md) v0.10.14–v0.10.15: a farm connector goes with the owner who created it when that owner is demoted, removed, leaves or deletes the account, not only with its garden, and its token is kept while another connector holds the same account (5, 11). v0.7.15: synced with [contracts.md](contracts.md) v0.10.13: the farm connector routes work once connectors are configured (9). v0.7.14: synced with contracts v0.10.12: when `search_farm_records` is offered, how `since` is read and what `Message.farm_citations` holds (7). v0.7.13: the build plan (14) moves the LiDAR module and its day-1 check to Oct 20–21 and aligns the app's dates with [frontend 18](../frontend/README.md#18-build-plan). v0.7.12: section 11 lists tool results among the text framed as data. v0.7.11: synced with contracts v0.10.10: `api` revokes a connector's token after the deletion step commits, not before (section 7). v0.7.10: synced with [contracts.md](contracts.md) v0.10.9: the farm connectors' tables, routes and `search_farm_records` are declared in the `domain` types and the schema, and the garden's deletion cascade deletes its farm records and connectors (5, 7, 9). v0.7.9: open decision 5 is settled for Eon: farm connectors sync a garden's Slack channels and Google Drive folder for the chat and the plan writer, with Era by Eon's simulators as the test bench in dev, CI, the agent evaluation and the demo, designed in [docs/connectors](../connectors/README.md); the contract changes land with the code (1, 2, 3, 7, 9, 11, 14, 15). v0.7.8: SurrealDB Agent Memory is integrated (open decision 5): shared memories are mirrored into it as beliefs that supersede by topic, `memory` gains `topic`, and `GET /plants/{plant_id}/beliefs` is added, synced with [contracts.md](contracts.md) v0.10.7 (1, 5, 7, 8.2, 9, 10, 11, 15). v0.7.7: open decision 2 is settled: the control plane runs on the rig behind Tailscale Funnel, and the ingress is a compose profile, so moving to a Cloudflare Tunnel or a cloud VM needs no code change (12, 15). v0.7.6: section 12 links the infrastructure and CI design, [docs/infra](../infra/README.md). v0.7.5: section 9 says the `domain` types are the contracts' source of truth and the spec is committed as `backend/openapi.json` (contracts v0.10.4). v0.7.4: sections 5 and 13 say the schema now lives at `backend/migrations/schema.surql`. v0.7.3: section 13 lists `docs/setup`, the developer setup. v0.7.2: section 12 gives the services' stop grace periods (190 s for `api`). v0.7.1 synced with [contracts.md](contracts.md) v0.10.1: the account tombstone keeps a deleted account's tokens out and the provider deletion running across restarts (4). v0.7 synced with [contracts.md](contracts.md) v0.10, which records the decisions of the [backend design](../backend/README.md):
>   - the crate list matches the code: `integrations` holds every outbound HTTP adapter, and `knowledge` reads through a port without depending on `store` (3);
>   - the user record is created by the first authenticated request, and the auth webhook only deletes users (4, 11);
>   - the chat tools take no garden id, `view_image` and `revise_plan` name their scan, and model thinking is replayed only within a turn (7);
>   - a failed weather fetch fails the run, SoilGrids is retried every 15 minutes after an outage and after 7 days where it has no data, and geocoding uses Nominatim and Zippopotam (6.4, 8.1, 8.5);
>   - no lease hand-back, kind-filtered claims, the job error codes, the label-version rule and the per-scan writer's lifecycle (8.4, 12);
>   - photos without depth stored at 1600 px, the fallback's 1.15-megapixel cap and thinking budget, and two embedding modes (6.7, 10);
>   - the weather thresholds have one owner, [backend domain 7](../backend/domain.md#weather-thresholds), and a humid stretch in the past week is detectable from `past_7_days.longest_humid_run_days` (6.8, 8.3, 15).
>
> v0.6, 2026-10-15, synced with [contracts.md](contracts.md) v0.9 and adds the [backend design](../backend/README.md) and the [computer-vision design](../vision/README.md): the models chosen there (a Qwen3-VL-class VLM with 32-pixel cells and 0–1000 boxes, an RF-DETR leaf detector, a disease head on BioCLIP 2, arctic-embed-xs text embeddings), an AMD RX 9070 XT as a second VLM worker, ONNX Runtime 1.31 on CUDA 13, per-model perception batch caps, and the new `integrations` crate for outbound HTTP adapters. v0.5.6 synced with contracts v0.8.2 after the third audit: the knowledge loader's photo upload is a one-shot `seed-upload` container on the control plane that `api` waits for, species photos are keyed by a content hash, `low_evidence` keeps its medium-confidence threshold, and a failed scan supersedes its question and keeps a late guided check for a resubmit. v0.5.5 synced with contracts v0.8.1 after its audit: the stream stays open while a guided check runs, `low_evidence` is narrowed, the knowledge photo upload runs on the control plane, and the table summary and deletion text show the new fields.
>
> v0.5.4 synced with [contracts.md](contracts.md) v0.8, which adds what the app's screens need:
>   - Clerk (with Svix webhooks) is the auth provider, and the frontend is designed in [docs/frontend](../frontend/README.md);
>   - species have reference photos and a list endpoint, plants and timeline entries carry thumbnails, and gardens have their own weather;
>   - diagnoses mark low evidence and name what beneficial organisms eat, and pushes have a defined payload;
>   - gardens can be located by city, and a scan can be deleted before it is submitted.
>
> v0.5.3 synced with [contracts.md](contracts.md) v0.7.3 after the third LiDAR audit: step claims are tied to their run and handed from step to step, deletion stops running steps, and the ground is the highest plane beneath the plant's base. v0.5.2 synced with contracts v0.7.2 after the LiDAR re-audit: the sweep recovers `api` work left unfinished between jobs, question photos wait for the perception worker, the ground is the lowest plane, and tasks are only created on live plants. v0.5.1 synced with contracts v0.7.1 after the LiDAR audit: the question-photo trigger, the leaf distance range, map uploads racing a plant deletion, stale map uploads refused, growth points kept across reruns, ground removed before plant height, stalled photo answers recovered, and the database deletion order fixed so plans go after their tasks and trigger rules. v0.5 synced with contracts v0.7: LiDAR on the phones that have it. Depth is captured with each photo, the capture screen shows live distance hints, a measurement stage sizes plants, leaves, spots and holes and answers measurable indicators without the VLM (a photo sent for a question is segmented first so it can be measured too), the timeline gets a growth curve, and a garden map gives plant positions, spacing and neighbour checks (6.7, 6.8). Phones without LiDAR, and the web build, don't offer the capture side, and everything else works as before. v0.4.6: the SurrealDB schema now lives in [docs/database](../database/README.md) and the RustFS setup in [docs/storage](../storage/README.md), both checked against the pinned versions (SurrealDB 3.3.4, RustFS 1.0.1); section 5 summarises them. v0.4.5 synced with contracts v0.6: batching in perception, worker slots so the VLM can run two jobs when they fit, weather fetched once per ~1 km cell and shared by scans and the scheduler, and batched ingestion embeddings. v0.4.4 synced with contracts v0.5: plan-trigger task keys scoped to their plan, the dose check on every model-written tool argument, chat revisions stored exactly as previewed, `healthy` only on positive evidence, a diagnosis for every guided check, and SAM-class masks. v0.4.3 synced with contracts v0.4: guided checks outside runs, serialised re-plans that keep chat revisions, provisional plans tied to uncertain diagnoses, the readable-plant rule, and the dose check on every chat reply. v0.4.2 synced with contracts v0.3: guided soil checks and the full soil aspects back in scope, per-call photo routing to the VLM, recurring and trigger-based plan steps, lease deadlines, late answers that always re-plan, plan tasks dismissed with their plan, shared-only memories in plans, invites, deletion cascades, signed image URLs and a stronger dose check. v0.4.1: Review of v0.4 added: conversations owned by a user, a pre-VLM candidate set that also covers organisms the VLM finds, a scan state machine without `awaiting_species`, a cap of 3 indicators per candidate, a scheduler sweep for unanswered verification questions, deletion of chat images and chat-derived memory, and a cap of about 9 profiles on the pre-VLM candidate set. v0.4 audit fixes: text embeddings move to the control plane with one pinned model, the VLM detects organisms and answers verification indicators in its main pass, scans get a submit step and a state machine with resumable events, stale workers are requeued from heartbeats, deletion rules for shared gardens, garden-scoped memory, deterministic verification scoring, and schema fixes. v0.3 added soil data sources and soil findings from photos, the detection of organisms (pests, beneficial insects, wildlife, invasive species) and the damage they cause, and capture guidance. v0.2 added species identification, a knowledge base with retrieve-then-verify, and an optional fast text classifier (Jev).
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
| Innovation | Combines vision findings, weather and per-plant memory. Checks each candidate diagnosis against curated disease profiles before giving advice. Sends forecast alerts before problems appear. Shows a health timeline for each plant, with a measured growth curve from LiDAR scans. Measures spot and hole sizes in millimetres to tell look-alikes apart. Chat can re-examine the photos. Shared memories become beliefs in SurrealDB Agent Memory, where a newer fact supersedes the older one and the history stays traceable. Farm connectors bring a small farm's Slack crew reports and Drive spray log into the chat and the plan, with citations. |
| Real-World Impact | Clear users: home gardeners and small farms. Catches disease early. Fits the Canadian frost and short-season context. Recognizes beneficial insects so they aren't killed by mistake. Helps users report invasive species. User feedback improves the model over time. A farm keeps using the Slack and Google Drive it already has. |
| UI & Design | Guided capture with hints to retake bad photos. Live progress for each stage. Boxes drawn on the affected leaves. The plan appears as a checklist. Chat answers cite their sources. Optional guided soil checks. On LiDAR phones: live distance hints while framing, and a map of the garden's beds and plants. |
| Tech & Deployment | Live public link. GPU workers pull jobs from the API, with a fallback to API credits. Public repository. Mobile build. The farm connectors are tested in CI against Era by Eon's simulators, with injected records, faults and a quarter of changes. |
| Security & Ethics | Managed authentication with MFA. Every garden-scoped query checks membership. Image storage is private. Photo metadata is stripped and faces are blurred before storage; measured on public photos, the blur misses some faces and also covers leaf spots on about half the plant photos, and its fix is open ([#159](https://github.com/aacruzgon/GreenLens/issues/159)). Location needs consent and only the garden has one. Deleting an account removes the user's own data and any garden they alone use. Depth maps never leave the server and have face regions removed; the garden map holds no images, 3D mesh or coordinates. Wildlife advice is humane. Pesticide advice is limited, uncertain results are shown as uncertain, and advice cites its sources. |

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
│ LiDAR depth · garden map      │                       │
└──────────────┬────────────────┘                       │
               │ HTTPS: REST + SSE                      │
═══════════════╪════════════════════════════════════════╪═════ CONTROL PLANE
               ▼                                        ▼      (rig + Funnel; or a VM)
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
                          │ depth measurement (CPU) · garden map            │──▶ Slack · Google Drive
                          │ farm connector sync                             │    (farm connectors, read-only)
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
┌─────────────────────────────────────┴───────────────────────────────────────────────────────┐
│ GPU RIG (home · no open ports · a host per card)                                            │
│                                                                                             │
│  RTX 3070 (8 GB)                     RTX 5070 Ti (16 GB)          RX 9070 XT (16 GB, AMD)   │
│  perception-worker (Rust, ort/CUDA)  vlm-worker (Rust)            vlm-worker (Rust)         │
│   plant check · leaf detector ·        → vLLM sidecar (CUDA)        → vLLM sidecar (ROCm)   │
│   SAM masks · BioCLIP species ·      same 8B VLM checkpoint on both cards:                  │
│   disease head · embeddings          organisms · soil · damage · indicator answers · one job │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Design principles

- **Pull-based GPU workers.** The rig opens no ports. It asks the API for jobs over outbound HTTPS. Where the control plane runs is a deployment choice, not a change to the architecture. To add GPU capacity, start another worker.
- **Vision produces facts, rules produce risk, the LLM produces language.** Every finding carries a confidence score. The LLM explains structured findings and never invents a diagnosis.
- **Retrieve, then verify.** Candidate diagnoses are checked against curated species and disease profiles before any advice is given. When the evidence is ambiguous, the app asks the user a targeted question instead of guessing. Advice cites its sources.
- **Optional sensors add evidence; they never gate it.** LiDAR depth is used when the phone has it. Every stage works without it, and an indicator a measurement can't settle goes to the VLM as before.
- **Use the cheapest model that can do the job.** Small classifiers make the routine decisions (routing, gating, tagging). The strongest LLM is kept for reasoning and writing.
- **One agent core with two callers.** The plan writer, running after the pipeline, and the chat use the same context builder, tools and safety policy, so their answers can't contradict each other.
- **A trait at every external boundary:** `VisionModel`, `SpeciesIdentifier`, `TextClassifier`, `TextEmbedder`, `ChatModel`, `KnowledgeStore`, `WeatherProvider`, `SoilProvider`, `BlobStore`, `Store`, `MemoryStore`, `Notifier`, `FarmSource`. Swapping a vendor or sponsor technology means writing one adapter.
- **Stream results, not video.** The app uploads a few still images (see 6.6). The progress and findings stream back over SSE, and every event is persisted so a phone that drops the connection can resume.
- **Workers only pull.** Nothing in `api` waits on the rig synchronously. Anything a request needs right away (query embeddings, the blur check) runs on the control plane's CPU.

---

## 3. Components

| Component | Runs on | Tech | Responsibility |
|---|---|---|---|
| Mobile app | Phones, web build | Expo SDK 57 / React Native. TypeScript types generated from OpenAPI. Designed in [docs/frontend](../frontend/README.md) | Guided capture, location consent, live progress, results with overlays, chat, timeline, tasks, push. On LiDAR iPhones and iPads, a native module (`app/modules/lidar`) captures depth with each photo, shows distance hints and maps the garden |
| Auth provider | Clerk (managed) | JWT + JWKS, webhooks delivered by Svix | Credentials, MFA, sessions. Never stores domain data |
| `api` | Control plane | axum, tower-http, governor, utoipa | All public endpoints, authorization, uploads (EXIF stripping, blur and exposure check, face blurring), text embeddings on CPU, depth processing and the measurement stage on CPU, scan state machine, job broker, SSE hub, scheduler, agent core |
| Agent core | Inside `api` | `ChatModel` trait, tool registry | Context assembly, the tool-use loop, safety policy |
| SurrealDB | Control plane | Graph edges, HNSW vectors, geometry types | All structured data, the job queue, memory |
| RustFS | Control plane, private | S3 API via `aws-sdk-s3` | Images and the species reference photos. `api` reads and writes it; the knowledge loader's one-shot container writes `knowledge/` only ([storage 2](../storage/README.md#2-bucket-keys-and-access)) |
| `perception-worker` | Rig: the RTX 3070, in the control plane's host (12) | `ort` + CUDA, ONNX models | Plant-present check, segmentation, species identification (BioCLIP), disease classification per leaf, image embeddings. BioCLIP as a second opinion on organism crops is a stretch goal |
| `vlm-worker` + sidecar | Rig: the RTX 5070 Ti in `sam-linux`, at 0.78 of the card beside its desktop, and the RX 9070 XT in its own host; one worker per card (12) | vLLM (CUDA on the 5070 Ti, ROCm on the 9070 XT) or llama.cpp, OpenAI-compatible API; the same model checkpoint on both | One `vlm` job per scan, in two VLM calls (findings, then indicator answers, skipped when measurements answered them all) over at most 4 images: soil condition, organisms with bounding boxes, damage patterns, species cross-check, and the answers to the candidates' verification indicators |
| `fallback-worker` | Control plane | `VisionModel` with the LLM API as backend | Claims every job kind. Keeps scans working while the rig is offline |
| Training | Dev machines / the 5070 Ti with vLLM stopped, until the models freeze ([dev-roadmap 8](../vision/dev-roadmap.md#8-gpu-time-on-the-rig)) | Python, PyTorch → ONNX ([docs/vision](../vision/README.md)) | Trains the disease head on BioCLIP 2 features and fine-tunes the leaf detector. Not deployed |
| Knowledge ingestion | Dev machines; the photo upload on the control plane | Python scripts + LLM API, reviewed by a teammate | Builds the knowledge base from curated sources, loaded through migrations. Its photo upload runs on the control plane as a one-shot container (`seed-upload`), before `api` starts (5, Knowledge base). Nothing else of it is deployed |
| Text classifier (optional) | Jev API, cheapest LLM as fallback | `TextClassifier` trait | Chat routing, safety gate, memory-extraction gate, chunk tagging. Not on the diagnosis path |

### Rust crates

Each crate is designed in detail, down to its types and traits, in [docs/backend](../backend/README.md).

| Crate | Purpose |
|---|---|
| `domain` | Shared types, IDs, errors, the scan state machine, and every boundary trait (`VisionModel`, `TextClassifier`, `TextEmbedder`, `KnowledgeStore`, …), so no crate depends on another just to see a trait |
| `store` | SurrealDB repository layer and migrations; implements the domain's repository ports |
| `blob` | RustFS client (custom endpoint, path-style addressing) |
| `integrations` | Every outbound HTTP adapter: the LLM API client (shared by `agent` and the fallback worker), the OpenAI-compatible VLM client, Open-Meteo, SoilGrids, geocoding, Expo Push, Clerk and Svix verification, and the Slack and Google Drive farm connectors |
| `agent` | Context builder, tool registry, safety policy, the plan writer, and the `ChatModel` and `TextClassifier` implementations (Jev, LLM fallback) over the clients in `integrations` |
| `vision` | Perception stages and `ort` sessions for the workers; the CPU blur and exposure check, depth processing and measurement (plane fits, spot and hole sizing), and the `TextEmbedder` implementation for `api` |
| `knowledge` | `KnowledgeStore` implementation over an in-memory index of the curated records, profile lookups, look-alike graph, chunk retrieval, deterministic verification scoring. It reads through the domain's `KnowledgeRepo` port, which `store` implements and `api` wires, so it doesn't depend on `store` |
| `pipeline` | Stage orchestration for a scan: measurement, context, retrieval, scoring, fusion rules, the plan-writer call. Used by `api` |
| `api` (binary) | Routes, auth middleware, job broker, SSE, scheduler |
| `worker` (binary) | One binary started with `--caps perception`, `--caps vlm` or `--caps fallback` |

---

## 4. Users and authorization

### Identity

- Clerk, a managed auth provider, handles sign-up, sign-in, social login, MFA and sessions ([frontend 8](../frontend/README.md#8-authentication-clerk)).
- The app attaches the provider's JWT to every request. The API checks it against the provider's JWKS (`jsonwebtoken`) and maps the `sub` claim to a `user` record.
- The `user` record is created by the user's **first authenticated request**, as an upsert on `auth_id`. The provider's `user.created` webhook is acknowledged and changes nothing: Clerk doesn't guarantee event order, and a late `user.created` must never recreate a user who was deleted. `user.deleted` (signature-verified) triggers the deletion cascade described in section 5 ([contracts 9, auth webhook](contracts.md#auth-webhook)). An account's deletion first writes a tombstone keyed by `auth_id`: while it exists (until 24 hours after the provider confirms the deletion) the account's still-valid session tokens get `401` and never create a new user, and the API resumes an unfinished deletion, the provider call included, after a restart.

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
              status) · invite (garden, code, role, created_by, expires_at, used_at, revoked_at) ·
              garden_map (garden, version, beds, pins, north_deg, created_by)
analysis      scan (status and run, see 8.1; plan, weather snapshot, candidate set, event counter, pipeline step, deleting_at) · scan_image
              (kind, answers_question?, storage key, embedding, depth_key?, depth_meta?) ·
              mask (region, storage key) ·
              scan_event ([scan, seq], run, type, payload) · observation (run, stage,
              pipeline_stage, image?, label, measurement?, confidence, region, model_version) · question (run, kind, indicator,
              candidates, status, step) · indicator_answer ([scan, run, indicator]) · guided_check · diagnosis (run, supersedes, low_evidence) · scan_feedback
plans         action_plan (version, supersedes, diagnosis, provisional, constraints) · plan_trigger (plan, plant, step, trigger, until) ·
              task (scan?, due, repeat, trigger, source, plan_step, done_at, dismissed_at,
              dismissed_reason, alert_key unique)
chat          conversation (user, garden, anchor: scan | plant | garden, summary) · message
              (role, content, tool_calls, model, prompt_version, tokens) · chat_image ·
              pending_action (proposed | confirmed | rejected | expired) · message_feedback
connectors    connector (garden, kind, status, endpoint, account, sources, sealed secret, oauth, cursor, next_sync_at, sync_requested_at, last_error) ·
              farm_record ([connector, vendor_id, n]; garden, kind, title, author?, link, text, text_hash (of the author, title and text embedded), embedding, at) · see [connectors 4](../connectors/README.md#4-data)
memory        memory (garden, plant?, kind, topic, text, embedding, source: pipeline | chat | user,
              shared, author?, source_msg?, belief_synced_at?)
knowledge     species (full | label_only, reference_images) · disease (every kind of problem, see below) · organism ·
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

The schema is [`backend/migrations/schema.surql`](../../backend/migrations/schema.surql), with its design notes, query patterns and caveats in [docs/database](../database/README.md). It was applied and exercised against SurrealDB 3.3.4, the pinned version. The decisions that shape the rest of this document:

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
- The measurement algorithm's version and the phone model on every measurement observation, so a size can be traced to the code and the device that produced it.
- `disease.sources` and `disease.reviewed_by`: every profile points to its sources and records which teammate reviewed it.

### RustFS layout

Setup, access keys, client configuration and caveats are in [docs/storage](../storage/README.md). RustFS 1.0.1 runs on one disk, never exposed publicly, and the API streams every image itself behind its own signed URLs (RustFS's presigned URLs break behind a proxy that changes the host).

The bucket is private. Before anything is written, the image's EXIF orientation is applied to the pixels, then its metadata is stripped, faces are blurred and it is resized. The app and the workers reach images only through short-lived signed URLs issued by the API. Depth maps (6.7) are rotated with their photo, have the face regions zeroed, and are stored as 16-bit PNGs; no URL is ever issued for them, and each is deleted with its photo.

```
gardens/{garden_id}/scans/{scan_id}/{image_id}.webp
gardens/{garden_id}/scans/{scan_id}/masks/{mask_id}.png
gardens/{garden_id}/scans/{scan_id}/depth/{image_id}.png
gardens/{garden_id}/scans/{scan_id}/depth_confidence/{image_id}.png
gardens/{garden_id}/chat/{conversation_id}/{image_id}.webp
knowledge/species/{species_id}/{hash}.webp
```

Keys are prefixed by garden, except the species reference photos under `knowledge/`, which the knowledge loader writes and the API only reads, so deleting a garden is a prefix delete plus a cascade in SurrealDB. Deleting a user is a different operation, below.

### Deletion

`DELETE /me` and the provider's `user.deleted` webhook run the same cascade. The app explains it on the confirmation screen.

1. Gardens where the user is the only member are deleted outright: prefix delete in RustFS, cascade in SurrealDB.
2. Gardens where the user is the only `owner` but others are members: ownership passes to the longest-standing editor. If there is none, the garden is deleted and the remaining viewers are told by push.
3. Shared gardens the user doesn't solely own: the membership, push tokens, conversations (found through `conversation.user`), their messages, their images under `gardens/{garden_id}/chat/{conversation_id}/` in RustFS, pending actions and message feedback are deleted. The user's unshared chat memories are deleted too; memories they chose to share stay with the garden. The user's scans, observations and the `source = pipeline` memories derived from them stay with the garden, because the other members rely on the plant history. `scan.created_by` and `garden_map.created_by` are unset, so nothing in the garden points at the user any more.
4. The user's scan feedback is deleted.
5. The `user` record and its auth mapping are deleted, then the account is deleted at the auth provider too, retried until it succeeds, so signing in again doesn't recreate the user.

**Removing a member** from a garden, by an owner or by leaving, runs step 3 for that garden only, so their private chats and unshared chat memories don't survive a later re-invite.

**An owner who stops being one** (demoted to editor or viewer, removed, leaving, or deleting the account, which first hands their shared gardens on) loses the farm connectors they created in that garden, because the consent was theirs: the store marks them `deleting` in the same transaction as the role or membership change, and `api` revokes their tokens after it commits and then deletes the connectors and their records. A token another live connector of the same Slack workspace or Google account holds is kept, since revoking it would stop that one too ([connectors 8](../connectors/README.md#8-privacy-and-security)).

**Deleting a scan before it is submitted** (`DELETE /scans/{id}`) removes its photos, guided checks, events, the conversations anchored to it and its jar-test task; a submitted scan is history and goes only with its plant or garden.

**Deleting a plant or a garden** cascades to everything under it, including conversations anchored to it, a garden's farm records and then its farm connectors (`api` revokes their tokens after the store step commits, as for an owner's demotion, removal, leaving or account deletion above), tasks (with the neighbour tasks a plant's plans created on other plants), plan trigger rules, its pin or the garden map, invites and memberships (for a garden), and cancels its queued and leased jobs. The full list is in [contracts.md](contracts.md#deletion-cascades).

The judging claim is therefore "deleting an account removes the user's own data and any garden they alone use", not "everything".

### Knowledge base

The knowledge base lets the system check its diagnoses and base its advice on reliable sources, instead of on the LLM's general knowledge.

| Kind | Contents | Used by |
|---|---|---|
| Species profiles | Care needs, hardiness, water, light, soil pH, spacing and mature height, whether the disease classifier supports the species, and up to 3 licensed reference photos with their credit for the confirmation card. Common garden plants outside the supported crops get `label_only` records: names and a BioCLIP label, so they can be recognised, with no care profile or problems yet | Species confirmation, plan writer, chat |
| Disease profiles | Symptoms written as checkable indicators, some with a measurable range (spot or hole size) that LiDAR measurements can answer (6.7), where symptoms start, the temperature, humidity and leaf-wetness ranges that favour the disease, how it spreads | Verification. Fusion rules take their risk thresholds from here instead of hard-coding them |
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
4. Extract the structured profile fields (indicators, conditions, look-alikes, damage patterns) with an LLM, in the seed format of [contracts.md](contracts.md#10-knowledge-seed-format). Each symptom becomes an indicator with a weight; a symptom the source gives a size for ("spots 1.5–3 mm across") also gets a measurable range (6.7). `looks_like` edges are written in both directions. For each profile, mark the 3 most discriminating indicators: the highest-weighted ones that differ from its `looks_like` neighbours. Only those are asked during verification.
5. A teammate reviews every profile before it's loaded. Bad data here produces confident wrong advice.
6. Load everything through SurrealDB migrations, so every environment has the same knowledge base.
7. Generate the BioCLIP label list from the `species` and `organism` tables in the same step, so the classifier's labels and the knowledge base can't drift.
8. Upload each species' reference photos to RustFS under `knowledge/`, with the loader's own storage identity ([storage 2](../storage/README.md#2-bucket-keys-and-access)). RustFS is reachable only on the control plane, so this step runs there as a one-shot container, before the API starts with the migration that references the photos. It only adds photos and never deletes one, so an older API still finds its own after a rollback. Each photo comes from a source whose licence allows reuse, and its credit is shown with it.

**Licensing.** Store excerpts only as far as each source's terms allow (whole pages where they permit it, none from a source with no reuse grant), keep `source.url` on every chunk and profile, link back to the source in the app, and check each source's terms before ingesting it.

---

## 6. Analysis pipeline

### 6.1 Stages

| Stage | Output | Where it runs |
|---|---|---|
| Quality gate | Blur and exposure score for each shot as it is uploaded, of every scan kind (guided checks and question photos included; chat images skip it), so the retake prompt is instant. Whether a plant is present is the leaf detector's score or a plant probe on the BioCLIP embedding, over the plant shots only | api (CPU) for blur and exposure, perception-worker for plant present |
| Segmentation | Leaf regions from a detector fine-tuned on PlantDoc's leaf boxes, with masks from a SAM-class model prompted with those boxes, one prompt per box (PlantDoc has no mask outlines); plant, soil and surroundings regions when the model has them. Masks are optional, and later stages work without them | perception-worker (RF-DETR detector + SAM 2.1; [vision models](../vision/models.md)) |
| Species identification | Top 3 species with scores, cross-checked with the VLM. Skipped once the plant's species is confirmed. See 6.2 | perception-worker (BioCLIP) + vlm-worker |
| Disease classification | The top 3 problems for each leaf region of the close-up and underside shots, or for the whole close-up when no region was found, restricted to the working species' labels. The API takes the 3 distinct problems with the highest confidence as the classifier candidates. Runs only for species the classifier was trained on | perception-worker (a classification head on BioCLIP 2 features of each leaf crop, trained on PlantDoc-style field images) |
| Measurement (LiDAR phones only) | Sizes in real units from each photo's depth map: plant height and canopy width, leaf length and area, spot diameter, spot count, affected leaf area, hole diameter; organism lengths once the VLM's boxes arrive. Not run when no photo has depth. See 6.7 | api (CPU), after the perception result, alongside context |
| Image embeddings | Image vectors for finding similar past cases: BioCLIP 2's whole-image vector, 768 dimensions. Text embeddings for memory and knowledge queries are not a stage: they run in `api` on CPU, see 10 | perception-worker |
| Context | Current weather, the past 7 days of weather, a 7–14 day forecast, soil moisture and temperature, evapotranspiration, climate zone, plus the garden's stored soil profile | api (Open-Meteo, cached per ~1 km cell and hour and shared with the scheduler, see 8.3; SoilGrids once per garden). A run whose weather can't be fetched fails with `upstream_unavailable`, which can be resubmitted; it never continues without weather |
| Retrieve | The pre-VLM candidate set, capped at about 9 profiles: the disease classifier's top 3 (when it ran) plus the problems linked to the working species through `susceptible_to`, including its pests and wildlife damage, ranked by classifier score, then by how well the current weather fits each profile's favourable conditions, then by prior prevalence for the crop. Look-alikes of the top-ranked candidates fill the remaining slots. Each profile brings its 3 most discriminating indicators. Anything outside the cap is still caught by the `verify` rule below. Indicators that a measurement settles are answered here and left out of the VLM job (6.7). Runs before the VLM job so the VLM can answer the rest in the same job. See 6.3 | api (`knowledge`) |
| VLM pass | One `vlm` job per scan, in two calls of at most 4 images each (the second is skipped when measurements answered every indicator): (a) findings, on the shots in a fixed order: soil condition (see 6.4), organisms with bounding boxes and damage patterns (see 6.5), a species cross-check; (b) indicator answers, on the shots the questions name: a yes / no / unsure answer, with a short reason, for each indicator of each candidate that no measurement settled (see 6.3). Guided soil checks get their own single-image `guided` job | vlm-worker (`vlm` and `guided` jobs) |
| Verify | Candidates scored deterministically from the indicator answers, the weather and the profiles. If the VLM reported an organism or damage pattern outside the candidate set, one `verify` job asks that candidate's indicators first. Ambiguous cases end in a question for the user. A `verify` job also runs for a new photo, after the photo is segmented and measured when it has depth (6.7). See 6.3 | api (scoring), vlm-worker (`verify` job) |
| Fusion | Risk scores from rules over the verified findings, weather, soil and history, plus the growth curve and plant spacing where LiDAR provided them (6.7, 6.8). Thresholds come from the disease profiles (e.g. humidity > 90% at 15–25 °C for 2+ days raises fungal risk). Evidence is ranked user input, then photo, then modelled data (see 6.4) | api |
| Plan writer | Steps, timing, what to watch for, tasks. Based on the confirmed disease's management guidance, with citations | api → agent core → LLM API |

**Batching.** The perception-worker runs each model once per job over all of the scan's images or leaf crops, split only above a configured batch size and each model's own smaller cap, rather than once per image. The VLM pass already asks every indicator in one job. Whether each vlm-worker runs two scans' jobs at once, so vLLM can batch them, is set from the day-2 benchmark of its card (8.4).

If the confidence is low, the app asks for a better photo or suggests contacting a local extension office. It does not give a confident diagnosis. The diagnosis says so with `low_evidence`: it is `uncertain` and nothing points to any problem, with no candidate above zero, no pest or damage found with medium confidence or more, and no disease from the classifier with medium confidence or more ([contracts 7](contracts.md#diagnosis)). The closer photo answers the open photo question when there is one, and starts a new scan otherwise.

### 6.2 Species identification

1. BioCLIP compares the photos against a label list of the supported crops plus common garden plants (`label_only` species records), and returns the top 3 species with scores.
2. The VLM names the species independently.
3. If both agree with a high score (BioCLIP's top result in the high band, and the VLM naming the same species with medium or high certainty), the species is stored as `suggested` and the user confirms it with one tap. If they disagree, or the score is low, the app shows the top 3 with reference photos from the knowledge base, plus a "something else" option. The app's species picker uses the same list (`GET /species`).
4. The pipeline continues with the best candidate, so results aren't held up. If the user picks a different species, the plant's most recent scan (created in the last 24 hours and not `failed`) follows it. A scan that is still `pending` or `queued` simply uses the new species when it runs. A scan already past that starts a new **run**: its status returns to `analysing`, jobs of the old run are cancelled and their late results rejected, any open question is superseded, and species, disease, the VLM stages and scoring run again as a new `perception` job whose payload names the stages, the species and the earlier run's segmentation regions. The plant check, segmentation, measurements and image embeddings of the earlier run are kept, unless the earlier run hadn't finished them, in which case the new run does every perception stage and measures again; if only the measuring was unfinished, the new run measures again. Choosing "something else" tells perception to skip species identification. If the scan already had a plan, the new plan supersedes it.
5. The confirmed species is stored on the `plant` with `species_status = confirmed`. Later scans of the same plant skip this stage. A species the user names (the confirmation card, the picker, a new plant) must be one the knowledge base holds; any other is refused with `400 invalid_request` and nothing is stored ([contracts 9](contracts.md#plants)).
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
  1. on LiDAR photos, api first answers the indicators a measurement
     settles ("spots smaller than 3 mm?"); the VLM answers the rest of
     every candidate's indicators in the `vlm` job's second call, on the
     shots the questions name:
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

Answering specific indicator questions is more accurate than asking one broad question ("what disease is this?"). It also makes the diagnosis explainable: the app shows which indicators matched. The scoring is arithmetic over the VLM's answers, so no text classifier sits on the diagnosis path, and the indicators are asked in one VLM job per scan, never one call per question. On LiDAR photos, size questions, which a small VLM answers worst, are measured instead of asked (6.7).

Two caps keep the job within what a quantized 7–8B model answers reliably: about 9 profiles in the candidate set, and 3 indicators per profile, so roughly 27 answers rather than 45 or more. A crop like tomato has 8–10 known problems, so without the profile cap the set with look-alikes would reach 15 or more. The indicators are chosen at ingestion (section 5), not at run time. A problem left outside the cap is not lost: if the VLM reports an organism or damage pattern outside the set, the `verify` job above asks that candidate's indicators.

A run asks at most 2 questions, all before the scan is `planned`. If it is still ambiguous after them, it continues with the best candidate marked uncertain. When the scheduler continues a scan without an answer, the question stays open on the scan, and the plan writer produces a provisional plan: general steps that are safe for every remaining candidate, labelled as provisional, with the open question kept at the top of the plan until the user answers it. A plan is provisional exactly when its diagnosis is `uncertain`. An answer or photo that arrives later always produces a new diagnosis and a new plan version, even when the winner is the same; the plan stops being provisional if the answer settles the question, and the user gets a push notification. While a photo for a question is being analysed, the question is `answering` and accepts nothing else. If that photo can't be analysed, the question is dropped and the diagnosis and plan stay: late work never fails the scan, whether it arrives in `diagnosed` or `planned`.

When a plan is superseded, its open tasks are dismissed and its trigger rules removed, so the user never sees steps from an outdated diagnosis.

A plan is rewritten only for a late answer, a species change, a guided check whose texture differs from the one the plan used, or a confirmed chat revision. Every plan write of a scan, the run's first plan included, runs one at a time, and each checks before storing that it still replaces the current plan. An automatic rewrite that loses starts again from the newer plan; a chat revision stores exactly the preview the user confirmed, or returns 409 if the plan changed, and is never retried automatically. A guided check that finishes after the diagnosis writes a new diagnosis with the texture, and the plan is rewritten from it when the texture changed. Constraints from a confirmed chat revision ("no copper") are stored on the plan and carried into every later version.

A plant is called **healthy** only on positive evidence: no candidate scores above zero, nothing points to a disease, pest or damage, and either the classifier's top result is healthy or the candidates' indicators are answered no. A plant where every indicator came back unsure is ambiguous, not healthy, and gets a question, usually a request for a close-up. A measured `no` counts toward that positive evidence only when the photo it came from shows no spot on any measured leaf and no hole (6.7).

### 6.4 Soil

Soil information comes from three sources. Each one covers something the others can't.

| Source | What it gives | When it's fetched |
|---|---|---|
| **SoilGrids** (ISRIC REST API) | Properties that don't change: pH, texture (clay, sand, silt), organic carbon, nitrogen, cation exchange capacity, bulk density, at depths from 0–5 cm down to 100–200 cm. About 250 m resolution | Once, when the garden is created. Stored as `garden.soil_profile` and refreshed rarely |
| **Open-Meteo** (part of the weather call) | Conditions that change hourly: soil moisture at 0–1, 1–3, 3–9, 9–27 and 27–81 cm; soil temperature at 0, 6, 18 and 54 cm; evapotranspiration, including the FAO reference rate (ET₀) used to estimate watering needs | On every scan (stored on the scan as its weather snapshot) and in the scheduler's forecast checks |
| **The photo** (vlm-worker) | What the soil looks like now: moisture, surface (crusted or cracked), structure, colour, cover (bare, mulched, residue), deposits (salt crust, moss, algae, mould), erosion and weeds. Earthworms are reported as organisms | On every scan |
| **Guided checks** (vlm-worker, optional) | Texture from a squeeze test or a jar test | When the user does one |

SoilGrids is officially in beta, comes with no uptime guarantee, and allows 5 calls per minute. Fetching it once per garden keeps us well within that. It also has **no data** in many cities, because it masks built-up land: central Toronto, Scarborough and Guelph answered null for every property on 2026-10-15, and again on 2026-10-20 with North York and Ottawa. When the garden's point is null, a Canadian point takes its profile from Agriculture and Agri-Food Canada's **Soil Landscapes of Canada** v3.2 (SLC), which covers the agricultural regions, cities included, at 1:1,000,000: the dominant soil of the point's polygon, averaged onto the same six depth bands, without nitrogen, which SLC doesn't map. The API ships it as a 0.9 MB lookup built from the published files and reads it offline ([backend integrations 8](../backend/integrations.md#8-soilgrids)); open decision 10 has the evidence. Elsewhere, nearby points 2 km and 5 km away are tried; if all are null, the garden has no soil profile, and the fetch is retried after 7 days rather than every 15 minutes as after an outage ([contracts 9](contracts.md#gardens)). SLC data is under the Open Government Licence – Canada, which asks for the attribution "Contains information licensed under the Open Government Licence – Canada".

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
| Insect and other pests | Aphids, whiteflies, spider mites, caterpillars and hornworms, Colorado potato beetle, flea beetles, slugs and snails | The VLM, which names each organism and returns its bounding box (Qwen-VL-class models, the chosen Qwen3-VL included, do this grounding natively; tiny pests are about one image token at 1 megapixel, so recall on them is measured, and the leaf-underside close-up is the remedy). The segmentation model has no organism class, so the VLM is the detector. BioCLIP on those crops as a second opinion is a stretch goal. Webbing, slime trails and droppings also come from the VLM. Tiny pests need a close-up of the leaf underside |
| Damage patterns | Small "shot holes" (flea beetles), ragged holes with slime (slugs), tunnels inside leaves (leaf miners), sticky residue and black mould (aphids), cleanly cut stems (rabbits), torn stems (deer) | The VLM, checked against the knowledge base's `looks_like` comparisons |
| Larger animals | Rabbits, deer, groundhogs, squirrels and raccoons digging, vole and mole tunnels, birds pecking fruit | Mostly from damage, holes, tracks and droppings |
| Beneficial organisms | Ladybugs and their larvae, lacewing larvae, ground beetles, spiders, bees, earthworms | The VLM. Never flagged as pests |
| Invasive species | For example jumping worms, Japanese beetle, box tree moth | The VLM, plus reporting guidance from the knowledge base |

Organism and damage findings join the disease candidates in retrieve-then-verify (6.3). "Slug damage vs. flea beetle damage" is verified the same way as "early blight vs. Septoria leaf spot".

**Beneficial organisms.** When one is found, the app says so and explains what it does, using the `preys_on` graph, which the diagnosis carries for each beneficial organism: "those are ladybug larvae, leave them, they eat aphids". Ladybug larvae are often mistaken for pests and killed.

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

**On LiDAR phones** the native LiDAR module provides the camera view, the capture screen shows the distance to the subject while framing and says when to move closer or back (the ranges per shot are in 6.7), and each plant and soil shot is uploaded with its depth map. Phones without LiDAR, and the web build, show the same screen without the distance hint.

### 6.7 Depth and measurement (LiDAR)

Phones with a LiDAR scanner (iPhone 12 Pro and later Pro models, iPad Pro 2020 and later, on iOS 15.4 or later) capture a depth map with each photo, and PDM uses it to measure what the photos show in real units. **The capture side is offered only on those devices:** without LiDAR there is no depth capture, no distance hint and no garden mapping. Android phones and the web build don't offer it. Results that already exist are data, so in a shared garden every member sees the measurements, the growth curve and the map on any device. The backend treats depth as optional evidence: every stage works without it, exactly as before.

**Capability detection.** The native module (`app/modules/lidar`, Swift, built with the Expo Modules API) exports `getCapabilities(): { depth: boolean; mapping: boolean }`. `depth` is true when AVFoundation has a back `builtInLiDARDepthCamera`; `mapping` is true when ARKit supports scene reconstruction (`ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh)`). The app asks once at start-up. On Android, in the web build and in Expo Go the native module is absent, and the JavaScript side reports both as false.

**What LiDAR can and can't see.** The iPhone's LiDAR projects a sparse grid of dots, and the phone fuses it with the camera image into a depth map a few hundred pixels across, accurate to around a centimetre at arm's length. That is too coarse to see a 2 mm spot, but more than enough to know how far away a leaf is and how it is tilted. So every size comes from the **photo's pixels**, and the depth map supplies only the **scale**: a plane is fitted to the depth inside a leaf, and the photo's pixels are projected onto that plane through the camera intrinsics. A spot 40 pixels across on a leaf 30 cm away becomes a diameter in millimetres, corrected for the leaf's tilt. Sizes are measured on the stored image, so the size it is stored at sets the smallest spot that can be sized: a 4:3 photo whose long side is 2048 pixels covers about 40 cm along it at 30 cm, about 0.2 mm per pixel. LiDAR shots whose depth survives validation are therefore stored with a long side of at least 2048 pixels; other photos, including one whose depth was dropped, are stored at 1600. A spot or hole under 4 pixels across is counted but not sized.

**Capture.** On a LiDAR device the module provides the camera view for every shot, because only one capture session can use the camera at a time. It captures the photo through AVFoundation's LiDAR depth camera, which delivers an `AVDepthData` registered to the photo, with the camera calibration (intrinsics and their reference size). The module:

- asks for unfiltered depth, so that missing depth stays missing instead of being filled in, converts disparity to depth when needed, and sends depth only when `AVDepthData` reports absolute accuracy;
- encodes the photo as JPEG and keeps its EXIF orientation, so the photo and its depth are in the same sensor frame. The app uploads that file unchanged: no HEIC conversion, rotation or resizing on the phone;
- reads gravity from Core Motion at the moment of capture and converts it from the device's axes to the camera's, for plant height and canopy width;
- streams depth to the preview and shows the median depth of the centre of the frame as the distance hint.

| Shot | Distance hint | Measured |
|---|---|---|
| Whole plant | 0.5–3 m, with the whole plant in the frame | Plant height, canopy width |
| Leaf close-up, leaf underside | 25–45 cm (the lower bound is LiDAR's shortest reliable distance, confirmed on day 1) | Leaf length and area, spot diameter, spot count, affected area, hole diameter |
| Soil surface, flower or fruit | 30–80 cm | Nothing of its own; the depth is stored |
| Guided soil checks | None | Depth is not taken |

On every shot with depth, organisms the VLM boxes also get a length.

The day-1 check (section 10) confirms the depth map's size, the shortest reliable distance, whether unfiltered depth keeps its holes, the gravity conversion, and whether the photo and its depth carry lens distortion. If they do, the module also sends the calibration's inverse distortion table, and the API undistorts pixel positions before projecting them. If the AVFoundation path fails the check, the module captures through ARKit's scene depth instead, which has a lower photo resolution but adds a per-pixel confidence map; the upload contract covers both.

**Upload.** The depth map, a confidence map when the capture path has one, and a small JSON record of the intrinsics and gravity travel as extra parts of the same `POST /scans/{id}/images` call. The API:

1. rotates the depth and confidence maps with the photo's EXIF orientation, and transforms the intrinsics and gravity to match, so depth pixels line up with the stored image: a quarter turn swaps `fx` and `fy`, and the principal point and gravity turn with the pixels. A mirrored orientation can't come from the rear camera, so its depth is dropped;
2. scales the intrinsics to the stored image's size. Photo and depth cover the same field of view, so the depth map's own intrinsics follow by scaling to its size;
3. sets the depth, and the confidence map, to zero inside every face box the face detector found, grown slightly when scaled to the depth map's resolution, because depth keeps a face's shape after the photo is blurred and the confidence map can outline it;
4. writes it to RustFS as a 16-bit PNG in millimetres.

A depth map that doesn't fit its photo is dropped and the photo is kept, so a good photo is never retaken because of its depth. A depth map is deleted with its photo, including a shot removed or replaced before submit. Depth maps stay on the server: no URL is ever issued for one, and the workers never receive them.

**The measurement stage.** It runs in `api` on CPU (the `vision` crate, on a blocking thread), after the perception result is stored and only when a plant was found, alongside the weather context, so it is done before retrieval. It is deterministic geometry, not a model: its RANSAC is seeded from the image and region, and its medians and percentiles are nearest-rank, so the same photo always measures the same. Masks are shrunk by two depth pixels before depth is read inside them, because depth along an outline mixes the leaf with what lies behind it. The exact definitions are in [contracts.md](contracts.md#measurement).

1. **Plant height and canopy width**, from the whole-plant shot. A ground plane is found first: the highest flat surface at or below the plant's base, which is estimated from the lowest points in the plant's outline or box, so a raised bed's soil wins over the path beside it and flat leaves are never taken for ground. The soil is then never counted as plant, and a shot with no ground beneath the base in view gets no height. The plant's points are the `plant` region's mask when segmentation produced one, otherwise the connected depth layer around the centre of the frame, grown only through points above the ground. That fallback is the common case, because segmentation often has no `plant` region and the fallback worker never does. The points are projected into 3D. Height runs from the ground plane to the top of the plant; canopy width is their spread along the horizontal direction across the line of sight, because the back of the plant is hidden. Both use the 98th percentile at the top or edges, so stray points don't count. A plant that touches the top or bottom edge of the frame gets no height, and one that touches a side gets no width, because either would only be a lower bound. Without a `plant` mask a pot counts as plant, so the confidence is at most medium.
2. **Leaves.** For each `leaf` region with a mask, a plane is fitted to the confident depth inside the mask (RANSAC), and the mask is projected onto it. Leaf length is the longest distance across the projected outline, and leaf area the projected area.
3. **Spots.** Inside each leaf mask, pixels that are brown, black or pale against the leaf's own green are grouped into spots. A group under a set share of its leaf (`min_spot_share`, [open decision 17](#15-open-decisions)) is a speck of glare or grain and counts as leaf, so it neither makes a spot nor stops a photo from showing a clean leaf. A spot's diameter is that of the circle with the same projected area. Yellowing around a spot counts toward the affected area but not toward its diameter, because the sources give lesion sizes without the halo. Spot count and affected area are per leaf. This needs a mask, so it doesn't run on the fallback worker's whole-image regions. The colour thresholds sit behind a `SpotSegmenter` trait, so a lesion-segmentation model can replace them later.
4. **Holes.** Gaps enclosed by a leaf's outline are holes. Their diameters are measured the same way, which separates flea beetle shot holes (1–3 mm) from slug damage.
5. **Organisms.** When the VLM's findings are stored, before their `finding` events are sent, each organism box on a photo with depth gets a length in millimetres: the box's longer side at the median depth inside it. It is shown on the finding but not used in scoring, because a box is only a rough outline.

Each measurement becomes an observation with a value, a unit and a confidence. The confidence comes from rules (how much of the region has confident depth, how well the plane fits, whether the shot was within its distance range, whether there was a mask) and maps to the same 0.4 / 0.65 / 0.85 as the VLM's certainty words. A failure while measuring never fails the scan: the stage completes as skipped and the run continues without measurements.

**Measurable indicators.** An indicator in a problem profile can carry a range in a measured quantity, taken from its sources: for example, Septoria leaf spot's spots are about 1.5–3 mm across, while early blight's lesions are around a centimetre. Such an indicator states the size and nothing else, so a measurement answers the whole question. Only a profile's 3 discriminating indicators are ever asked, so only their ranges are used. Before a `vlm` or `verify` job is queued, `api` answers each measurable indicator from a measurement of medium confidence or more: clearly inside the range is `yes`, clearly outside is `no`, and anything within the margin of an edge (10% of the bound, and never less than two pixels at the measured distance) is left to the VLM. A settled indicator is removed from the job. When none is left, the `vlm` job skips its indicator call and a `verify` job isn't queued at all. The VLM is therefore never asked about sizes it can measure, which a small VLM judges worst, and its job gets shorter. Every measurable indicator keeps its question, so photos without depth are asked it as before. The VLM never sees the measurements, so its answers stay independent of them.

Each indicator keeps one answer, ranked user, then measurement, then VLM, just as user input outranks the photo (6.4). Questions are chosen only from indicators that neither the user nor a measurement has answered. A measured `no` counts against its candidate, but not as positive evidence of health (6.3) unless the photo shows no spot on any measured leaf and no hole: a spot of the wrong size is still a spot, and a median of zero spots can hide a spotted leaf.

**Photos sent for a question.** A photo uploaded with depth for a question, when one of the question's candidates has a measurable indicator that the photo's kind can show and that neither the user nor a measurement has answered (the VLM's earlier answer, often `unsure`, doesn't count), first goes through a `perception` job with only the segmentation stage, on that photo alone. `api` then measures it, answers what the measurements settle, and queues the `verify` job for the rest, or rescores straight away when nothing is left. That segmentation job never fails the scan or drops the question: if it fails, or gives no mask, only what needs no mask is measured (a whole-plant photo's height and width) before the photo goes to its `verify` job. The fallback worker may take the job only when no perception worker is alive, since it has no masks to give, and then completes it at once with no regions. The question records which step its photo is in, so the scheduler's sweep can recover one that stalled between steps (8.3).

**Growth.** Plant height and canopy width are stored per scan, so the plant's timeline shows a growth curve. Each scan's point comes from its current measurements, whichever run produced them, and is dated by when the scan was taken, so a species-change rerun or a re-measure never drops or moves it. Fusion adds a low `growth_stalled` risk when the plant's species has a mature height, the latest height in the series is from the past week and below it, and the latest height measured at least 14 days earlier is within the measurement noise of it (2 cm or 5%, whichever is larger, set from the evaluation set). Only heights of medium confidence or more count. The plan writer and the chat see the curve.

**Runs.** Measurement depends only on the photos, their depth and the segmentation. A species-change run therefore keeps it along with segmentation, and measures again only when segmentation re-runs or the earlier run hadn't finished measuring. Which indicators it settles is worked out again in retrieval, because the candidates change.

### 6.8 Garden map (LiDAR)

On a LiDAR device, an editor can map the garden. The app opens an ARKit world-tracking session with scene reconstruction; the user walks the beds, taps each bed's corners on the ground, and taps each plant to drop a pin, choosing which plant it is or naming a new one. The app uploads only the result: bed outlines and pin positions in metres on a local ground plane, a canopy radius for each plant from the reconstructed mesh around its pin (shown on the map, not used by any rule), and the direction of north from the phone's compass when its reported accuracy is 20° or better. The mesh, the camera frames and any coordinates never leave the phone.

Every ARKit session has its own origin. To change a map, the app relocalises against the `ARWorldMap` it saved on the phone after the last mapping, which also never leaves the phone, and edits the existing pins. When relocalisation fails, or on another device, the garden is mapped again from the start, as it is when tracking is lost during mapping. Each upload is a whole new version: every pin moves to it, and a plant left without a pin loses its position. The app tags the saved `ARWorldMap` with the version it produced and offers an edit only when that is still the current version. Every upload names the version its session started from, and the API refuses it with `409` if another member uploaded since, so a stale edit never silently replaces a newer map; the user then reloads and maps again, or confirms replacing it. A pin for a plant that is being deleted is dropped from an upload.

The map tells the backend three things it couldn't know before:

- **Neighbours:** the plants within 1.5 m of a plant on the current map, centre to centre.
- **Spacing:** when a plant's nearest pinned plant is closer than three quarters of its species' recommended spacing, fusion adds "crowded, poor airflow" as a driver of fungal risk whenever a humid stretch is forecast or has just happened, raising the risk one level, or creating it at low.
- **Spread:** when a diagnosis names a problem that spreads (fungal, bacterial, viral or pest), the plan writer gets the neighbours whose species is susceptible to it, nearest first, and may add a `monitoring` step for each. That step becomes a task on the neighbour: "Septoria on Tomato 1: check the lower leaves of Tomato 2, 40 cm away". A new map version doesn't re-plan; it is used the next time a plan is written.

The app shows the map as a top-down plan, with each pin coloured by its plant's status and linking to that plant's timeline. Every member can view it; editors and owners map. A garden doesn't need a map, and nothing else works differently without one.

---

## 7. Chat and agent core

Chat runs **inside `api`, not on the job queue**, because it is interactive. Tokens stream straight from the LLM to the app over SSE. Only heavy actions, such as re-analysing a new photo, go through the workers. The query embedding for memory and knowledge search is computed in `api` on CPU (section 10), so a chat turn never waits on the rig.

### Conversation anchors

| Anchor | Opened from | Context loaded |
|---|---|---|
| `scan` | "Ask about this result" | That diagnosis, its observations and image regions, the weather, the plan |
| `plant` | Plant timeline | All scans of the plant, its memory, open tasks, its growth curve when it has one |
| `garden` | Garden home | Summaries of each plant, garden-level memory, the forecast, upcoming tasks, the garden map when there is one |

The context is rebuilt from the database on every turn. It uses the conversation summary, the most recent messages and the top memory hits, not the full transcript. The chat therefore always sees the latest data, such as a frost task the scheduler created an hour earlier. Because of that rebuild, the model's thinking blocks are replayed unchanged only within one turn's tool loop; earlier turns are sent as text only ([contracts 8](contracts.md#sending-a-message)).

### Tools

| Tool | Purpose | Kind |
|---|---|---|
| `get_scan(scan_id)` | Findings, confidence, regions | read |
| `view_image(image_id, scan_id?, region?)` | Sends the photo, or a crop of it, back to the model. With `scan_id`, an image of that scan of the garden; without, one of this conversation's own images | read |
| `get_forecast(days)` | Weather forecast | read |
| `search_memory(query, plant_id?)` | Vector search over the plant's facts and events plus the garden-level ones, and Agent Memory's beliefs. Returns `MemorySearch { memories, beliefs }` | read |
| `find_similar_cases(image_id)` | Embedding search over the garden's past scans | read |
| `search_knowledge(query, topic?)` | Retrieval over curated guides, returned with citations | read |
| `get_species_profile(species)` | Care needs, and the problems the species is susceptible to | read |
| `get_disease_profile(disease)` | Symptoms, favourable conditions, management with its treatments, sources | read |
| `compare_lookalikes(disease)` | Look-alike diseases and how to tell them apart | read |
| `get_organism_profile(organism)` | How to recognize it, its role (pest, beneficial, invasive), the pests it preys on, natural enemies, reporting guidance | read |
| `get_soil_profile()` | The garden's SoilGrids profile, plus the latest soil moisture and temperature, and in a scan conversation the scan's latest guided-check texture | read |
| `get_plant_measurements(plant_id, quantity?)` | The plant's measured sizes over time (height, canopy width), from LiDAR scans | read |
| `get_garden_map()` | The current garden map: beds, plant positions, neighbours and spacing | read |
| `list_tasks(state?)` | The garden's tasks: open by default, or done or dismissed | read |
| `create_task(title, detail?, plant_id?, due?, repeat_every_days?)` / `complete_task(task_id)` | Manage the task list | **write: user confirms** |
| `revise_plan(scan_id, summary, changed_steps, removed_step_ids, constraints)` | Proposes a new version of that scan's current plan (a plant or garden conversation has no single plan, so the scan is named); the card names the plan it is based on and says the plan is shared with the garden | **write: user confirms** |
| `request_rescan(plant_id, photo_kind, instructions)` | Asks the user for a specific photo; the app offers to open the capture flow for a new scan | read (UI action) |
| `remember(fact, plant_id?)` | Saves a private chat memory about the plant, or the garden when no plant is given; the app shows it with Undo, and Share with garden for editors and owners. Offered to every role, because it only writes the caller's own memory | write (automatic, shown to the user) |
| `search_farm_records(query, kind?, since?, until?, author?)` | Searches the Slack messages and Drive documents synced from the garden's farm connectors; each hit cites its author, time and link. Offered to every role when the garden has a farm record or an `active` or `reauth_needed` connector; `since` and `until` (`YYYY-MM-DD`) are days in the garden's time zone, both included, and window Slack messages only (a Drive document's time is its last edit, so documents are returned whatever the window); `author` matches part of a display name and searches Slack messages only, and a thread is matched by its first message's author and time; without `kind`, each kind keeps 3 of the hits when it has them, so Slack messages can't crowd the Drive documents out; at most 8 hits, framed as `kind="farm_records"`. The reply's `Message.farm_citations` are the records the turn's tool calls returned, once per record, with no SSE event ([connectors 7](../connectors/README.md#7-the-agent)) | read |

No tool takes a garden id: the garden is always the conversation's, because identity and scope come from the session, never from the model (section 11). Tool arguments are strict: a call with an unknown field or a malformed id is refused with the reason, and the model corrects it.

Viewers and reviewers are only offered the read tools and `remember`, so they never get a confirmation card.

Write tools return a `pending_action`. The app shows it as a confirmation card, and the model never changes data by itself.

The pipeline's verify and plan-writer stages use the same knowledge tools.

### Router and safety gate

Before the LLM runs, a fast text classifier (`TextClassifier`: Jev if available, otherwise the cheapest LLM with structured output) labels each message:

| Label | Effect |
|---|---|
| `diagnosis_question`, `care_question` | Normal turn with the relevant context |
| `task_change` | Normal turn. Write tools are likely |
| `safety_sensitive` (pesticide doses, a person or pet poisoned) | The safety policy is applied first. Poisoning questions get poison-control guidance, with the local numbers from deployment configuration |
| `off_topic` | A short, polite redirect, using the cheapest model |
| `farm_records` (only in a garden with farm records) | A question about what the garden's own records say: what the crew saw, reported, did or posted in its Slack channels or documents. Routed as `care_question`, and the chat model searches the records with `search_farm_records`; a question about the doses or products the records hold stays `safety_sensitive` |

The scores are compared with each other rather than treated as certainties, and they are calibrated on a small labelled set of example messages. A classifier that doesn't answer within 3 seconds is not an error: the message is routed as a care question.

The router knows whether the garden has farm records (a synced record, or a connector that is active or needs reconnecting: the same condition that offers `search_farm_records`). Only then is the classifier given the `farm_records` label, and told that `off_topic` excludes the garden's records, so "What did Riley post in #field-reports on August 31?" is answered from the records rather than redirected. A garden without farm records is classified exactly as before, and questions that are about neither the garden nor its records stay off-topic either way.

### Farm connectors

A garden owner can connect the farm's Slack and Google Drive ([docs/connectors](../connectors/README.md)). The owner picks the channels and one folder. A `connectors` scheduler task syncs them every 15 minutes into `farm_record`, paced to Slack's limits, and embeds the text like memories. The adapters only read. The chat searches the records with `search_farm_records`, and the plan writer passes its top 4 hits to the model so a plan doesn't repeat a treatment the spray log shows. Records are data, never instructions, and the dose check still applies, so a rate in the spray log is never repeated. A plan step lists the records it cites in `farm_citations` and the plan resolves them in `farm_records`, and an assistant message carries `farm_citations`, all optional fields beside the knowledge `citations` ([contracts 7](contracts.md#actionplan), [8](contracts.md#8-chat-and-memory)). Era by Eon's Slack and Google Drive simulators, loaded with a synthetic farm, are the test bench in dev, in CI, in the agent evaluation and behind the demo garden.

### Memory

Each memory belongs to a garden and, usually, to one plant. Facts that apply to the whole garden, such as "I garden organically", "everything is in containers" or "the beds are on a balcony", are stored with no plant and are recalled in every conversation and every plan for that garden. The plan writer treats them as constraints.

`memory` is written from two places:

- **The pipeline**, after each scan: a summary of the findings.
- **The chat**, after each turn: the text classifier first decides whether the turn contains a fact that will still matter later. Only if it does, a cheap model extracts it, such as "watered yesterday", "applied neem oil Oct 18" or "it's in a container".

Memories with `source = chat` are private to their author: chat recall returns them only in the author's own conversations. The author can share one with the garden from the "Remembered" card, which turns it into a `user` memory, or undo it. Pipeline memories and shared memories are visible to every member. **The plan writer reads only shared memories**, because every member sees the plan, so a plan never quotes something said in a private chat.

A memory is embedded in document mode when it is written, and searched with a query embedded in query mode (section 10). Memory persists across conversations and scans, which keeps advice consistent over time. 

**Topic.** Each memory has a `topic` (watering, feeding, treatment, pruning, planting, location, container, soil, light, health, harvest or other, default other). The chat extraction model and the `remember` tool pick it, a garden note may name it, and the pipeline's summary uses `health`.

**Beliefs in SurrealDB Agent Memory.** The `memory` table stays the system of record. Only shared memories (`pipeline` or `user`) are mirrored, one triple each, into [SurrealDB Agent Memory](../database/agent-memory.md), the hosted memory product of the Build North sponsor pack. The entity is `plant_<id>` or `garden_<id>`, the scope path `gardens/<garden id>[/plants/<plant id>]`, with the ids lowercased because Agent Memory refuses uppercase path segments. A memory's key comes from its kind and topic (`scan_summary`, the topic, `last_<topic>`, or `note_<id>` for topic `other`), and a newer memory with the same key about the same subject supersedes the older one, with the history kept. Private chat memories never leave our database: sharing one is the consent to send it. A `BeliefStore` port in `domain`, implemented over REST in `integrations`, does the calls. Only one scheduler task in `api` writes to Agent Memory, every 10 s, so no request waits on it: requests write rows, and deleting a shared memory, a plant or a garden queues its scope for erasure in the same transaction. Each run erases the queued scopes first, then records the unsynced shared memories; Agent Memory keeps the newest value of a key by its date, so retries and rebuilds can't bring an older one back. The chat context, the plan writer (shared beliefs only) and `search_memory` read the beliefs, with their dates, and `GET /plants/{plant_id}/beliefs` shows them. Without the three `AGENT_MEMORY_*` settings nothing is recorded and nothing else changes.

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
2. The app uploads each shot to `POST /scans/{id}/images` with its kind. The API runs the blur and exposure check on CPU, on every scan kind, and rejects a failing shot at once with a retake hint. For a good shot it applies the EXIF orientation to the pixels, strips the metadata, blurs any faces, resizes the image and writes it to RustFS. HEIC uploads are decoded first. On a LiDAR phone the depth map, intrinsics and gravity travel in the same call; the API rotates the depth map with the photo, zeroes its face regions and stores it, or drops it and keeps the photo if it doesn't fit (6.7).
3. The app calls `POST /scans/{id}/submit`. The scan becomes `queued` and a `perception` job is created. The app opens SSE on `/scans/{id}/events`, sending `Last-Event-ID` if it is reconnecting.
4. The perception-worker claims the job. The scan becomes `analysing` in the claim's transaction. The worker runs the plant-present check, segmentation, species identification (unless the plant's species is already confirmed), disease classification and image embeddings. It sends each stage's output as a partial result when the stage finishes, so findings appear before the job ends; the final result repeats them and wins if they differ. If the worker's stage events were lost (the API was down), the API sends the missing ones when the job completes, so no stage is left in progress.
5. The API stores the observations and, when any photo has depth, runs the measurement stage (6.7) while it fetches the weather, including soil moisture and temperature, and loads the garden's soil profile. If the weather can't be fetched, the run fails with `upstream_unavailable`, which the user can resubmit. It builds the candidate set, capped at about 9 profiles: the classifier's top 3 plus what the working species is susceptible to, ranked by classifier score, weather fit and crop prevalence, with look-alikes of the top-ranked ones filling the remaining slots. It retrieves each profile and its 3 indicators, answers the ones a measurement settles, and queues a `vlm` job whose payload carries the rest. Problems outside the cap are caught in step 7 if the VLM finds them.
6. The vlm-worker runs the `vlm` job in two VLM calls of at most 4 images each, findings then indicators: soil condition, organisms with boxes, damage patterns, its own species suggestion, and an answer for every indicator. If the species isn't confirmed, `plant.species_status` becomes `suggested` and the app shows the confirmation card. The scan's status doesn't change, and the pipeline continues with the best candidate. If the user later picks a different species, the re-run from 6.2 step 4 puts the scan back to `analysing`.
7. If the VLM found an organism or damage pattern outside the candidate set, the API queues one `verify` job for that candidate's indicators and waits for it. It then scores all candidates and checks the weather against their conditions. A clear winner becomes a `diagnosis` with citations and the scan is `diagnosed`. An ambiguous result puts the scan in `awaiting_answer` and sends the user one question, at most 2 per run. The answer is rescored in `api`. A new photo runs a `verify` job; a photo with depth may first be segmented and measured, which can settle its indicators without the VLM (6.7). After 10 minutes without an answer, the scheduler's sweep (8.3) continues the scan with the best candidate marked uncertain; the question stays open. A later answer, or a photo for it, always produces a new diagnosis and a new plan version, even when the winner is the same.
8. Fusion computes the risk scores. The agent core writes the `action_plan` from the confirmed disease's management guidance, or a provisional plan if the diagnosis is still uncertain, and creates tasks. The scan is `planned`.
9. The API appends a summary to memory, updates the plant's status, sends `status_changed → planned` and then `plan_ready`, closes the stream unless a question is still open or a guided check is still analysing, and sends a push notification in case the app is closed.

### 8.2 Chat turn

1. The app sends `POST /conversations/{id}/messages`. The API checks auth, the conversation's owner, garden membership and the user's token budget. The text classifier labels the message (see section 7).
2. The context builder loads the anchor's data, the conversation summary, recent messages, the top memory hits and, as their own data block, the current beliefs of the plant (or of the garden) from Agent Memory, read directly by entity under a 1.5 s timeout and left out when it is slow or down.
3. The LLM call streams over SSE. Read tools run as the session user. Write tools, offered only to editors and owners, return confirmation cards. `request_rescan` lets the app offer flow 8.1. Every assistant reply, whatever its route, is checked one sentence at a time against the dose check before it is streamed. Every string the model writes into a tool call (task text, card text, plan revisions and constraints, rescan instructions, memories) is dose-checked before the tool runs, and a `revise_plan` preview also passes plan validation, so a card shows exactly what confirming stores; a failing call goes back to the model with the reason.
4. The API stores the message with its model, prompt version and token counts. If the classifier finds a fact worth remembering, a cheap model extracts it into a private chat memory, sent as `memory_saved` before `message_completed`, and the app shows it with Undo and Share with garden. Long conversations are summarized. A turn that drops or fails before `message_completed` is stored as interrupted, and the app offers to resend it.

### 8.3 Forecast alerts

At a fixed interval, the scheduler fetches the forecast and checks every garden's trigger rules. Gardens are grouped by weather cache cell (coordinates rounded to about 1 km), each cell is fetched once, and missing cells go to Open-Meteo in multi-location requests of up to 50, so a hundred gardens in one city cost a handful of requests. The results land in the weather cache that scans read, so a scan in the same hour reuses them. The rules are frost, a humid stretch (fungal risk), heat, heavy rain, and dry soil (a watering reminder based on soil moisture and evapotranspiration). Their thresholds are configuration with one owner, [backend domain 7, weather thresholds](../backend/domain.md#weather-thresholds), which the forecast's `frost_risk`, these rules and fusion all read ([backend api 16](../backend/api.md#16-the-scheduler); numbers to be confirmed, [contracts open question 27](contracts.md#12-open-questions)). When a rule fires, it creates a `task` with `source = forecast_alert` and sends a push notification to every member ([contracts 9](contracts.md#push-notifications) defines the payload). Each alert has an `alert_key` of garden, trigger and date, with a unique index on it, so a frost warning is created once rather than on every run even when two scheduler runs overlap. The in-app alert inbox is the list of these tasks, and it works in every build, with or without push.

The same scheduler also:

- sweeps scans that have been in `awaiting_answer` for more than 10 minutes and continues them with the best candidate marked uncertain, so the timeout has an owner and works even when the app is closed. A scan whose question has a photo being analysed (its segmentation job when it has depth, then its `verify` job) is skipped until that finishes. The same sweep recovers work `api` started and never finished, for example after a restart: a question photo stuck while being measured for more than 2 minutes, and a scan stuck in one of `api`'s own steps between jobs (retrieval, scoring or plan writing). A scan holds at most one step claim, tied to its run; every entry point claims it first (or, if it is busy, flags that new input needs scoring), each step hands the next one over in the same commit, and a commit goes through only if its claim, run and stamp still hold and nothing is being deleted. So the sweep and a slow original never both complete a step, and a step from an older run never commits ([contracts.md](contracts.md#scan-lifecycle));
- evaluates the **plan trigger rules** that `on_trigger` plan steps register (after rain, frost or heat forecast, a humid stretch, dry soil), and creates the step's task when its rule fires. The task's `alert_key` is `plan:<plan_id>:<step_id>:<date>` under the same unique index as forecast alerts, so it is created at most once a day per step of that plan version even when scheduler runs overlap; step ids are only unique within a plan. Plan triggers and forecast alerts share one trigger vocabulary (frost, heat, humid stretch, heavy rain, after rain, dry soil);
- nothing is needed for **recurring** plan steps: completing a repeating task creates the next one.

Stretch: when several gardens within about 5 km report the same disease in a week, the nearby gardens get a regional alert (the geo query in section 5).

### 8.4 Job leasing and fallback

- `POST /internal/jobs/claim` is a long poll. Inside a transaction, it leases the oldest queued job of a kind the worker's capabilities cover, for 2 minutes, and moves a run's scan from `queued` to `analysing` on its first claim. On the fallback worker the job's `max_runtime` is doubled at claim. A job whose kind or payload the worker can't read anyway is completed as failed with `unsupported_version`, not retryable.
- Each claim returns a lease token that every later call about the job must carry; a requeue invalidates it, so a worker that dropped off and came back gets `409 lease_lost` instead of writing duplicate results.
- Each worker declares `slots`, the most jobs it holds at once, and the broker never leases it more. The perception-worker has 1 and batches inside the job. Each vlm-worker, one on the 5070 Ti and one on the 9070 XT, has its own `VLM_SLOTS`, 1 or 2: 2 only if that card's day-2 benchmark shows two 4-image jobs fit its KV cache and each still finishes well within its `max_runtime`, in which case vLLM batches the two jobs' calls. Both serve the same model checkpoint, and the broker's claim gives each job to exactly one of them. The fallback worker has several, bounded by the LLM API's rate limit.
- Workers heartbeat through `POST /internal/workers/heartbeat` every 10 seconds. A heartbeat that lists a job with its lease token extends that job's lease, so a slow VLM or fallback job isn't requeued while it runs, but only for the worker that holds it and **never past `claimed_at + max_runtime`** for the job's kind (2 minutes for perception, 5 for vlm, 3 for verify and guided, doubled on the fallback worker). A worker whose model hangs while its heartbeat thread keeps running still loses the job.
- The broker requeues the jobs of any worker whose heartbeat is older than 30 seconds without waiting for the lease to expire, and resets `queued_at`. A stopping worker hands nothing back: its leases are requeued this way. Attempts count retryable failures plus every requeue after the 2nd, up to 3, so a job that keeps crashing its worker stops. A run's job that runs out of attempts fails the scan and supersedes its open question, so a failed scan holds none. A guided check that finishes on a failed scan only stores its result, which a resubmitted run reads. Late work (a late answer's verify job, any guided check) only reports its failure; a guided job out of attempts marks its check failed in the same transaction. A failed job reports one of five codes: `unsupported_version` and `unknown_label_version` (not retryable), `model_error`, `image_fetch_failed` and `max_runtime_exceeded` (retryable) ([contracts 5](contracts.md#post-internaljobsidcomplete)). The segmentation job for a question photo never fails anything: the photo goes on to its `verify` job unmeasured.
- When a scan starts a new run, its old jobs are cancelled; workers learn it from the heartbeat response. Any call about a requeued, expired or cancelled job, including an older run's, gets `409 lease_lost`.
- The `vlm` capability claims `vlm`, `verify` and `guided` jobs; `fallback` claims every kind.
- Jobs still unclaimed 15 seconds after `queued_at` also become available to the `fallback-worker`, which uses the LLM API. The exception is a question photo's segmentation-only job, which exists only to get masks: the fallback worker may take it only when no perception worker has sent a heartbeat for 30 seconds. A rig that dies mid-scan therefore hands over within about 45 seconds, not after the 2-minute lease.
- A `perception` job never names label versions that no live perception or fallback worker has loaded: with no such worker live, it waits in the queue; with live workers that all lack them (a deployment mismatch), the request that would queue it gets `503 upstream_unavailable`.
- A small status page shows the health of each GPU worker.

### 8.5 Garden setup

The garden's centroid comes from the phone's GPS when the user has given location consent, otherwise from a typed postal code or city name, geocoded once: a postal code through Nominatim, then Zippopotam by its forward sortation area, a city through Open-Meteo's geocoder ([contracts 9](contracts.md#gardens)). A location that can't be geocoded gets `400`. It is the only location the system stores; scans have none of their own. The garden's time zone is derived from it and sets every local date (task due dates, alert dates). A GPS location is named by Nominatim's reverse lookup; when that fails, the garden is still created, labelled by its time zone ("Toronto time zone"). When a garden is created, the API fetches its SoilGrids profile in the background and stores it as `garden.soil_profile`; where SoilGrids has no data at a Canadian point, the profile comes from Soil Landscapes of Canada (6.4). If SoilGrids is unavailable, the garden is created anyway and the fetch is retried: every 15 minutes after an outage, and after 7 days when no source has data at the location (6.4). Until it succeeds, the pipeline runs without baseline soil properties, and the app shows both cases the same way. On a LiDAR device an editor can map the garden at any time afterwards (6.8); a garden doesn't need a map.

---

## 9. API surface

```
Scans (garden member; writes need editor)         Worker API (per-worker token)
POST   /scans                                     POST /internal/workers/heartbeat   (extends lease)
POST   /scans/{id}/images                         POST /internal/jobs/claim          (long-poll; lease token)
DELETE /scans/{id}/images/{image_id}              POST /internal/jobs/{id}/events    (X-Lease-Token)
DELETE /scans/{id}                 (pending only)
GET    /scans/{id}/images/{image_id}              POST /internal/jobs/{id}/complete  (X-Lease-Token)
GET    /scans/{id}/masks/{mask_id}
POST   /scans/{id}/submit                         GET  /internal/jobs/{id}/images/{image_id}
GET    /scans/{id}
GET    /scans/{id}/events          (SSE, resumable with Last-Event-ID)
POST   /scans/{id}/answers                        Webhooks
POST   /scans/{id}/feedback                       POST /webhooks/auth                (signature-verified)

Gardens, plants, tasks                            Ops
CRUD   /gardens                                   GET  /healthz
GET    /gardens/{id}/weather
GET    /gardens/{id}/members                      GET  /status                       (worker health)
PATCH  /gardens/{id}/members/{user_id}
DELETE /gardens/{id}/members/{user_id}
GET    /gardens/{id}/invites · POST /gardens/{id}/invites   (owner)
DELETE /gardens/{id}/invites/{invite_id}   (owner)
POST   /invites/{code}/accept
GET    /gardens/{id}/map                 (any member)
PUT    /gardens/{id}/map                 (editor, from a LiDAR device)
CRUD   /plants  · GET /plants/{id}/timeline · GET /plants/{id}/measurements
POST   /plants/{id}/species        (confirm species)
GET    /species · GET /species/{id}   (signed in) · GET /species/{id}/images/{hash}   (signed URL)
CRUD   /tasks                      (?source=forecast_alert is the alert inbox)

Account
GET    /me · PATCH /me · DELETE /me
POST   /me/push-tokens · DELETE /me/push-tokens/{token}

Memory
GET    /memories · POST /memories   (garden note with optional topic, editor)
POST   /memories/{id}/share · DELETE /memories/{id}
GET    /plants/{plant_id}/beliefs   (current and superseded beliefs; available: false when Agent Memory is off)

Farm connectors (docs/connectors; owner, except reading the list; 409 invalid_state while CONNECTOR_KEY is unset, and on a demo `era` connector)
GET    /gardens/{id}/connectors · POST /gardens/{id}/connectors
GET    /connectors/oauth/callback   (signed state) · POST /connectors/{id}/complete   (device-bound)
GET    /connectors/{id}/available-sources · PUT /connectors/{id}/sources
POST   /connectors/{id}/sync · DELETE /connectors/{id}

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

Request and response bodies, the scan and chat event catalogues, worker job payloads and results, and the error format are the `domain` crate's types, which are the source of truth; [contracts.md](contracts.md) explains them ([contracts 1, source of truth](contracts.md#source-of-truth)).

`utoipa` generates the OpenAPI spec from the routes, committed as `backend/openapi.json`. `openapi-typescript` generates the app's TypeScript types from that file.

Both SSE endpoints are consumed with a streaming `fetch` response (Expo SDK 52+ provides one), because the browser `EventSource` can only GET and cannot carry the auth header. Scan events are persisted in `scan_event`, so the app reconnects with `Last-Event-ID` after being backgrounded and receives everything it missed. Chat turns are not resumable: a dropped chat stream is shown as an interrupted message the user can resend.

---

## 10. Models and GPU placement

| Task | Primary | Fallback |
|---|---|---|
| Blur and exposure check | `api`, CPU (Laplacian variance and a histogram) | None needed |
| Plant-present check, segmentation, disease classification, image embeddings | RTX 3070, ONNX via `ort` | LLM API vision. Embeddings are skipped |
| Species identification | RTX 3070, BioCLIP (ONNX), cross-checked with the VLM | LLM API vision. Optionally Pl@ntNet or Kindwise/plant.id |
| Face detection (for blurring on upload) | `api`, CPU, small ONNX face detector | None. Fails closed: an image isn't stored until blurring succeeds |
| Memory embeddings inside Agent Memory | Hosted by SurrealDB Cloud: fixed to `gemini-embedding-2`, so the text of shared memories is embedded by Google (*unverified*, vendor docs). Our own memory vectors still use the row below | None: when Agent Memory is down, chat and plans run on the `memory` table alone |
| Text embeddings (queries, memory, knowledge chunks) | `api`, CPU: `snowflake-arctic-embed-xs` (384 dimensions, fp32) run with `ort` and `tokenizers` directly, pinned by file hash in `TextEmbedder` and recorded on every vector ([vision models](../vision/models.md)) | None. A different model would make the existing vectors incomparable, so the only acceptable substitute is a hosted endpoint serving the identical model |
| Soil condition, organisms with bounding boxes, damage patterns, species cross-check, verification indicator answers, in one job of two calls | RTX 5070 Ti and RX 9070 XT, one vlm-worker each, both serving the same checkpoint: Qwen3.5-9B (4-bit AWQ, thinking off), chosen by the day-2 benchmark, with Qwen3-VL-8B-Instruct as the fallback, with structured output ([vision VLM](../vision/vlm.md)). On the 5070 Ti, `VLM_SLOTS` is 2 at 0.78 of the card, which leaves 3,575 MiB beside vLLM for its desktop: 330 MiB freshly logged in, 2.3–2.9 GB with its apps open (12); the fallback checkpoint needs 0.90 and a headless card | LLM API vision |
| Depth measurement: plant, leaf, spot and hole sizes | `api`, CPU: plane fits over the LiDAR depth map and colour thresholding in the photo. No model | None: a scan without depth has no measurements, and its indicators go to the VLM |
| Plan writer, chat | LLM API, strongest model | none |
| Summaries, memory extraction | LLM API, cheapest model | None. `api` has no synchronous path to the rig |
| Chat routing, safety gate, memory gate, chunk tagging | Jev (optional, early access) | LLM API, cheapest model, with structured output |

Notes:

- The RTX 3070 models (BioCLIP 2 with its disease head, the RF-DETR leaf detector, SAM 2.1) need about 5 GB of VRAM together with activations, by estimate, within the card's 8 GB. The perception batch size (default 16) and each model's own cap (the SAM encoder about 2 images) keep batching within the remaining headroom ([vision 4](../vision/README.md#4-placement-and-vram-budgets)).
- **Three cards, two vendors, three hosts** (12). The RX 9070 XT (RDNA 4, gfx1201, 16 GB) adds VLM capacity as a second `vlm` worker, because the VLM job is the slowest. ROCm 6.4.1 was the first release to support it. Its container gets `/dev/kfd` and that card's render node only, and the NVIDIA services are pinned by GPU UUID ([backend worker](../backend/worker.md)). Perception stays on the 3070: ONNX Runtime's ROCm execution provider was removed in 1.23, and its MIGraphX builds lag behind the CUDA ones. **AMD day-1 check:** its host's power supply, `rocminfo` shows gfx1201 while `nvidia-smi` still works, the vLLM ROCm container starts with only its render node, and the checkpoint answers a findings call with our schema. If that fails, the card serves llama.cpp with Vulkan by the end of day 2 (open decision 19).
- The text-embedding model runs on the control plane because chat and knowledge search need a query vector synchronously, and workers only pull. A short query embeds in a few milliseconds on CPU. The same model, loaded from the same file, is used by the ingestion scripts, so the knowledge-chunk vectors match the query vectors. It has two modes: knowledge chunks and memories are embedded as documents, with no prefix, and chat questions and search queries as queries, with the model's query prefix ([contracts 10](contracts.md#other-entity-shapes)).
- **The fallback worker** sends the hosted model images it won't resize (at most 1568 pixels on the long side and 1.15 megapixels), so its pixel boxes match the prepared image, strips the schema keywords the hosted API rejects, and allows 8,000 output tokens per call, because the hosted model always thinks first ([contracts 6](contracts.md#after-decoding)). Whether that fits its doubled `max_runtime` is measured on day 2.
- **Jev** is text only and zero-shot, at around 150 ms per call. Its probabilities run somewhat high, so compare scores between candidates and calibrate them on the evaluation set. Nothing on the critical path depends on it. If early access hasn't arrived by Oct 17, use the fallback.
- **No multi-GPU model sharding.** With two mismatched consumer cards, the "cluster" is a pool of workers pulling from one queue, each advertising what it can do. More GPUs means more workers.
- **LiDAR day-1 check:** build the native module in a development build on a LiDAR iPhone, and record the photo and depth map sizes from the LiDAR depth camera, the shortest distance at which its depth is still reliable, whether unfiltered depth keeps its holes and reports absolute accuracy, and whether the photo and its depth carry lens distortion. Check the gravity conversion by photographing a spirit level, and the whole chain by measuring a ruler lying on a leaf at 30 cm. The close-up distance hint's lower bound comes from it. If the AVFoundation path fails, capture moves to ARKit's scene depth (6.7).
- **Day-1 check:** the 5070 Ti (Blackwell, sm_120) needs CUDA 13 builds: ONNX Runtime 1.31, which the pinned `ort` 2.0.0-rc.14 wraps, ships CUDA 13 builds only, and PyTorch and vLLM 0.31 default to CUDA 13, so the rig needs NVIDIA driver 580.65.06 or later ([vision 5](../vision/README.md#5-runtimes-and-versions)). Confirm inference runs before building on it.
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
- Auth webhooks are signature-verified, and a bad signature or a timestamp more than 5 minutes off gets `401`. The webhook only deletes users; it never creates one, so a late `user.created` can't bring back a deleted account.

**Privacy**
- Photo metadata (EXIF) is stripped before storage, after its orientation has been applied to the pixels.
- Faces that appear in photos are blurred on upload, before storage, and are never analysed. Measured on 93 public photos of people near plants (M7), the shipped detector catches 94.7% of faces against a 98% target, and it puts boxes on leaf spots, fruit and stems in about half the plant photos (67 of 130 on the dev split), so the blur also covers the symptoms the models diagnose. How to fix both is open ([#159](https://github.com/aacruzgon/GreenLens/issues/159); [evaluation 7](../vision/evaluation.md#7-day-1-checks)).
- Location requires consent. Only the garden has one: its centroid, at the precision the user chooses (exact, or rounded to about 1 km, which is enough for weather). Scans store no coordinates. Without consent, the user types a postal code or city.
- `DELETE /me` runs the deletion cascade in section 5: gardens the user alone uses are removed from SurrealDB and RustFS, shared gardens keep their scans with the user reference cleared, and the account is deleted at the auth provider too. Removing a member from a garden deletes their private chats there.
- Chat memories are private to their author until shared. The plan writer, whose output every member sees, reads only shared memories.
- **Agent Memory** receives only shared memories (the text, its topic, its date and ids), over HTTPS with an API key that is limited to `gardens/*` and can't change grants. Private chat memories, photos and location never go there. SurrealDB Cloud, and Google for the embeddings, process the text ([database: Agent Memory, privacy](../database/agent-memory.md#5-privacy)). Deleting a shared memory, a plant or a garden queues its scope in the same transaction, and the belief sweep erases it for good within seconds while Agent Memory is up.
- **Farm connectors** read only the channels and folder an owner picks, with read scopes. Tokens are sealed with AES-256-GCM, never returned or logged, and revoked on disconnect. PDM keeps the text, the author's display name, the time and the link for 180 days, and deletes them on disconnect, with the garden, or when the owner who connected them is demoted, removed, leaves or deletes the account ([connectors 8](../connectors/README.md#8-privacy-and-security)).
- Withdrawing location consent rounds every garden the user owns to about 10 km.
- Depth maps from LiDAR phones are stored only on the server, with face regions zeroed; no URL is issued for them and the workers never receive them. The garden map holds bed outlines and plant positions in metres on a local ground plane: no images, no 3D mesh and no coordinates. The mesh and the AR world map saved for later edits stay on the phone.

**Abuse and cost**
- Rate limits, a daily chat token budget and a daily scan limit (30) for each user. Scans cost API credits when the fallback worker runs them.
- `Idempotency-Key` on every create call, so a retried request never creates duplicates.
- Upload size limits.

**Safety of advice**
- Integrated pest management (IPM) and non-chemical steps come first.
- Advice is based on reviewed knowledge-base profiles and cites its sources.
- The text classifier flags safety-sensitive chat messages before the LLM runs, so the safety policy is applied to them first.
- Plans and chat replies never give doses. The plan shape has no dose field, and a dose check (quantities with units or number words, rates per volume, field measure, container or area in a sentence with an amount, ratios but not dates, years, clock times or labels, percentages of a mixture) runs on every step of every plan, on every assistant chat sentence before it is streamed, and on every string the model writes into a tool call before the tool runs. A step that names a pest-control product is treated as chemical, and the server adds a fixed safety note to every chemical step: use only a product registered for this use where you live, and follow its label exactly.
- A step's category comes from its knowledge-base treatment, not from the model.
- Questions about a person or pet being poisoned are directed to poison control.
- Beneficial organisms are never flagged as pests, never targeted by a plan step, and a broad-spectrum product is rejected when beneficials were seen in the scan.
- Wildlife advice uses humane deterrents and never recommends poison baits.
- Invasive species reports are suggested to the user, never sent automatically.
- Low confidence leads to a request for a better photo or a referral to a local extension office.
- Photos, retrieved documents, tool results and user messages are treated as data, never as instructions. This guards against prompt injection, for example text on a seed packet in a photo.

---

## 12. Deployment

```
infra/control-plane.compose.yml   caddy · api · surrealdb · rustfs · fallback-worker · seed-upload (one-shot)
                                  · perception-worker (the RTX 3070 by UUID, `perception` profile)
infra/rig.compose.yml             vlm-worker · vllm (sam-linux: the RTX 5070 Ti by UUID)
infra/rig-amd.compose.yml         vlm-worker-amd · vllm-amd (a teammate's host: the RX 9070 XT by its render node)
```

The files themselves, the images and their registry, CI, ingress, secrets, deploys by digest and backups are designed in [docs/infra](../infra/README.md).

The rig is a host per card (D11, [#13](https://github.com/aacruzgon/GreenLens/issues/13)), and each host starts only its own card's services:

- **The RTX 3070** shares the control plane's host (below) and runs `perception-worker`.
- **The RTX 5070 Ti** is the only GPU of `sam-linux`, a Fedora 44 desktop with SELinux enforcing, where Docker CE doesn't label its containers ([infra README 1](../../infra/README.md#1-prerequisites)). The card also draws the KDE desktop, so vLLM takes 0.78 of it: Qwen3.5-9B AWQ fits there with 2 slots, and the demo needs no headless step. That leaves 3,575 MiB beside vLLM, measured with the desktop freshly logged in (330 MiB); with its apps open (2.3–2.9 GB) about 700–1,300 MiB remain, by arithmetic, so nothing else that draws on the card runs while it serves ([infra README 3](../../infra/README.md#3-desktop-mode-and-headless-mode), demo day). Only the fallback checkpoint, Qwen3-VL-8B FP8, needs the card headless at 0.90 ([infra README 3](../../infra/README.md#3-desktop-mode-and-headless-mode)). It is also the training card, and the perception worker can't share it with vLLM; when it does what, to Demo Day, is in [dev-roadmap 8](../vision/dev-roadmap.md#8-gpu-time-on-the-rig).
- **The RX 9070 XT** sits in a teammate's machine, so its two services move to their own compose file (`infra/rig-amd.compose.yml`) unchanged, because workers only pull.

On the control plane, `seed-upload` runs after `rustfs` is healthy and exits; `api` starts only once it has completed (`depends_on` with `service_completed_successfully`), so the migration that references the species photos never runs before they exist.

**Stop grace periods.** Compose sends SIGKILL 10 seconds after SIGTERM unless a service sets `stop_grace_period`. `api` sets 190 seconds: up to 60 seconds for the slowest request in flight, then one full 120-second attempt for pending RustFS deletes (an interrupted delete can leak objects, [storage 4](../storage/README.md#4-writes-and-deletes)), plus 10 seconds of margin ([backend api 4](../backend/api.md#4-start-up-and-shutdown)). Each worker service sets its own from its drain ([backend worker](../backend/worker.md)).

### Hosting the control plane

**Decided on 2026-10-16:** the control plane runs on the rig host with the RTX 3070, behind Tailscale Funnel. The RTX 5070 Ti sits in a second rig host; its workers reach `api` through the same public URL, as any worker does, because workers only pull. Funnel gives the rig a public HTTPS address (`https://pdm-cp.<tailnet>.ts.net`) with no domain and no open port, free on every Tailscale plan. Anyone can reach it without joining the tailnet, so the whole team tests against one deployed URL.

| | All on the rig + a tunnel (chosen) | Small cloud VM for the control plane, rig for GPUs |
|---|---|---|
| Cost | $0 with Tailscale Funnel; a Cloudflare Tunnel also needs a domain (about $10 a year) | A few dollars a month, or partner credits, plus a domain |
| Live link during judging | Goes down if home power, internet or the PC fails | Always up. Uses API vision while the rig is offline |
| Uploads | Limited by home upload bandwidth, and Funnel's unpublished bandwidth limits | Fine |
| Setup | Caddy + the `tailscale` (or `cloudflare`) compose profile | VM + Caddy. The rig runs only workers |

The ingress provider is a compose profile ([infra 5](../infra/README.md#5-ingress)), and `api` never sees it: switching from Funnel to a Cloudflare Tunnel or a VM changes three env values and two URLs (the Clerk webhook and the workers' `API_BASE_URL`), not the code. We can still move the control plane to a VM before submitting, if the live link's uptime during judging calls for it.

Behind a tunnel, SSE responses need `Cache-Control: no-cache` and a keepalive comment every ~15 seconds, so idle connections are not dropped.

### App delivery

- The live link the judges open is the Expo web build, so the capture screen accepts a file picker as well as the camera.
- The web build also runs in phone browsers and inside other apps' browsers, where capture uses the phone's own camera; inside apps with an embedded browser (Instagram, Facebook, LinkedIn), Google sign-in isn't available ([frontend 13](../frontend/README.md#13-platform-adapters-and-the-lidar-module)).
- Remote push does not work in Expo Go on Android since SDK 53 and needs Apple credentials on iOS. Push therefore goes through the Expo Push API from a development build, and the demo-safe path is the in-app alert inbox (section 8.3), which works in every build.
- SSE is consumed with streaming `fetch` (section 9); no `EventSource` polyfill.
- The LiDAR features need a native module, so they run only in the iOS development build on LiDAR devices. The web build and Expo Go don't offer them; the demo video shows them.

### Operations

- `tracing` with JSON logs. `/healthz` and `/status` endpoints. The job table doubles as an audit trail.
- Nightly SurrealDB export (`surreal export`) and RustFS bucket copy (`rclone sync`) to storage off the host. Single-disk RustFS has no redundancy, so the copy is the only protection against disk loss.
- The SSE hub uses an in-process `tokio::sync::broadcast`. This assumes a single `api` instance. Running more than one would need SurrealDB `LIVE SELECT` or a message bus.
- Each scan's events go through one serialised writer that assigns `seq`, stores the event and broadcasts it in that order, because worker events, answers, the scheduler and species changes can all write to the same scan. A subscriber that falls behind the broadcast replays from the database. The writer starts on a scan's first event and retires after 60 seconds idle; `seq` comes from the event's transaction, so a writer started later continues the sequence ([contracts 4](contracts.md#transport)). A scan's stream stays open while a step holds its claim.

---

## 13. Repository layout

```
PDM/
├─ app/                Expo app; app/modules/lidar is the native LiDAR module (Swift)
├─ backend/
│  ├─ crates/{domain, store, blob, integrations, agent, vision, knowledge, pipeline}
│  ├─ bins/{api, worker}
│  └─ migrations/*.surql
├─ ml/
│  ├─ pdm_vision/      shared pre-processing for training and evaluation
│  ├─ models/          the model manifest (manifest.toml) and licence notices
│  ├─ training/        disease head and detector training + ONNX export; eval/ with the field-photo evaluation set
│  └─ knowledge/       ingestion scripts + reviewed seed data
├─ infra/              compose files, Caddyfile, tunnel config
└─ docs/
   ├─ architecture/    this document (README.md) and the data contracts (contracts.md)
   ├─ backend/         the Rust backend, crate by crate, down to its types and traits
   ├─ vision/          the computer-vision models, training and evaluation
   ├─ frontend/        the app's architecture (README.md), flows (flows.md) and screens (screens.md)
   ├─ database/        SurrealDB notes (the schema itself is backend/migrations/schema.surql)
   ├─ storage/         RustFS setup and notes
   └─ setup/           what each team member installs, per OS
```

---

## 14. Build plan

| Dates | Goal |
|---|---|
| Oct 16–17 | End-to-end skeleton: auth, upload with the CPU blur check, submit, scan state machine, job broker with stub vision and heartbeat requeue, resumable SSE, text embeddings in `api`, control plane deployed. Choose the crops and the text-embedding model, list the knowledge sources, request Jev access |
| Oct 18–19 | Real models on the rig (BioCLIP, disease classifier, VLM). Species confirmation. Weather with soil moisture and temperature; garden soil profile. Face blurring on upload. Knowledge ingestion running. Backend side of depth: depth upload and storage |
| Oct 20–21 | Knowledge profiles reviewed and loaded. The single VLM pass: soil, organisms with boxes, damage, indicator answers. Deterministic verification, questions and the provisional plan. Fusion and plan writer with citations. Measurement stage and measurable indicators. App: project, layers, mock mode and Clerk (F0, Oct 20); garden home and capture (F1–F2). LiDAR module and its day-1 check (10), owned by the app and backend and not yet run ([vision evaluation](../vision/evaluation.md)); depth capture and the evaluation captures for vision by Oct 21 (F5) |
| Oct 21–22 | App: the live analysis and the results screens with overlays, the plan and the Tasks tab (F3–F4), joining the backend's S4 exit checks on Oct 22 |
| Oct 22–23 | App: chat with answer blocks (F6), the 3D garden, Insights, members and farm connectors (F7), the mapping session and `GardenView` (F5). Chat with tools, router, memory, confirmation cards, timeline with the growth curve. Garden map: AR capture, map view, neighbour and spacing rules. Farm connectors: Slack and Drive adapters tested on Era's simulators in CI |
| Oct 23 | Forecast alerts with dedupe, alert inbox, push from a development build (needs a paid Apple Developer account; the alert inbox covers alerts until then). Account deletion cascade. Test the fallback by killing the rig in the middle of a scan: hand-over must happen within about 45 seconds |
| Oct 24 | Feature freeze, security review, UI polish, evaluation on field photos, agent evaluation on the Era farm |
| Oct 25 | Demo video (2 minutes or less), Devpost page, README, final deploy |

---

## 15. Open decisions

1. **Auth provider and app framework.** Decided: Expo and Clerk, each with its own prize. The app pins Expo SDK 57, because `@clerk/expo` doesn't support SDK 58 yet ([frontend 3](../frontend/README.md#3-stack-and-version-pins)).
2. **Where the control plane runs.** Decided on 2026-10-16: on the rig, behind Tailscale Funnel; the ingress is a compose profile, so a Cloudflare Tunnel or a cloud VM can replace it without a code change ([12](#hosting-the-control-plane), [infra 5](../infra/README.md#5-ingress)).
3. **Which crops to support.** This sets the classifier's classes, BioCLIP's label list and the scope of the knowledge base. PlantDoc covers tomato, potato, pepper, apple, grape and others. **Decided on 2026-10-22** ([#4](https://github.com/aacruzgon/GreenLens/issues/4#issuecomment-6344093645)): the disease classifier supports tomato, apple, grape, pepper and corn, which get full profiles. Potato, strawberry, squash and bean are `label_only` for now, with full profiles after the build week; potato joins the classifier once there is a source of healthy-potato photos. Blueberry, cherry, peach, raspberry, soybean, basil, lettuce, rose, hosta, cucumber and onion are `label_only`, so BioCLIP can name them and the VLM diagnoses them.
4. **Which VLM runs on the VLM workers.** **Decided on 2026-10-20** by the day-2 benchmark ([vision VLM 13](../vision/vlm.md#results-and-the-choice-2026-10-20--ran)): Qwen3.5-9B, 4-bit AWQ, thinking off. On 65 one-photo dev cases its indicator answers diagnosed 35% of the sure cases, against 12% for Qwen3-VL-8B-Instruct in FP8 and in AWQ, at the same speed and with every call valid. Qwen3-VL-8B stays the fallback, a configuration change. The choice is reopened only if Qwen3.5 reports a beneficial as a pest on the public pest and beneficial cases (vision dev-roadmap M10), which replace the team's full cases ([#52](https://github.com/aacruzgon/GreenLens/issues/52)). Both cards serve it; Qwen2.5-VL-7B is a baseline only.
5. **What Eon and SurrealDB Agent Memory offer.** **Agent Memory decided (v0.7.8):** integrated as described in section 7 and [database: Agent Memory](../database/agent-memory.md), for the "Best Use of SurrealDB Agent Memory" prize. **Eon decided (v0.7.9):** Era by Eon is the test bench for the farm connectors (Slack and Google Drive): its simulators, loaded with a synthetic farm, run in dev, in CI, in the agent evaluation and behind the demo garden ([docs/connectors](../connectors/README.md)), for the "Best Use of Eon" prize.
6. **Which LLM API models** to use for chat and plan writing, and for summaries and memory extraction. **Decided on 2026-10-20** by the agent evaluation (EON E8.3, [connectors 9.3](../connectors/README.md#93-the-agent-evaluation)): `claude-opus-5-5` for chat and plan writing. Opus answered the questions that need both Slack and Drive in 9 of 9 runs, Sonnet 5.5 in 4, at about US$0.13 a question against US$0.054; the difference came from one search behaviour (EON gap G16). **Kept after the re-run** of the same day, with gaps G12, G14, G15 and G16 fixed: Sonnet then matched Opus on answers (54 of 54), cross-source (9 of 9) and safety (no rate, no instruction followed) at US$0.058 a question against US$0.126, but brought the planted message up unasked in 16 of the 17 other answers whose records held it, against Opus's 0 of 7 (EON gap G17). **Kept after the third run**, with gaps G13, G18 and G19 fixed: Sonnet then cited every named record (51 of 51), but still brought the message up in 11 of 17, against Opus's 1 of 8. `LLM_CHAT_MODEL` moves to `claude-sonnet-5-5` once a re-run shows Sonnet mentioning the planted message in none of the runs whose records hold it, outside the questions about it, and with every citation. The plan writer's choice is provisional until the evaluation's plan question runs (E5.4). `claude-haiku-5-5` stays on summaries, memory extraction, off-topic replies and the classifier fallback as the default; no other cheap model was compared.
7. **Which knowledge sources** to ingest (extension and government guides), and whether their terms allow it.
8. **Jev:** whether early access arrives by Oct 17. If not, every `TextClassifier` call uses the LLM fallback.
9. **External species-identification backup** (Pl@ntNet, Kindwise/plant.id): whether it's worth adding, after checking free tiers and terms.
10. **SoilGrids reliability:** test it on day 1. If it's unreliable, load Soil Landscapes of Canada data instead. Tested on 2026-10-15: it has no data in central Toronto, Scarborough or Guelph. Decided: nearby points are probed, and a location with no data keeps `soil_profile_status: unavailable` and is retried after 7 days, an outage every 15 minutes ([contracts 9](contracts.md#gardens)). **Proposed on 2026-10-20, for the team to confirm:** Soil Landscapes of Canada v3.2 answers a null Canadian point before the probes, as built ([backend integrations 8](../backend/integrations.md#8-soilgrids)). The evidence:
    - **SoilGrids today.** 17 live calls on 2026-10-20: still null at central Toronto (43.65, −79.38), Scarborough, North York (43.75, −79.55), Guelph and Ottawa, and at all four probes 2 km around central Toronto; of the 5 km ring only the north point had data. High Park, rural Wellington County and Iowa had data, Iowa's values unchanged since 2026-10-15. So a central Toronto garden gets a profile from 5 km away after 7 paced calls, and Ottawa's depends on the probes.
    - **SLC covers those points.** Each lies in a polygon with a described soil: central Toronto, Scarborough and High Park in 563001 (Chinguacousy clay loam, 0–27 cm pH 7.4, 29 % clay, 1.9 % organic carbon), North York in 562001 (Peel), Guelph in 560004 (Burford), Ottawa in 545001 (Ste. Rosalie). It has pH, sand, silt, clay, organic carbon, CEC and bulk density by layer; no nitrogen.
    - **Its terms.** AAFC publishes it under the Open Government Licence – Canada 2.0: any use, with the attribution statement. The downloads are a 53 MB GeoPackage of 12,353 polygons and CSV tables (components 1.6 MB, soil layers 9.0 MB). No AAFC query service was found (the `agriculture.canada.ca/atlas/rest/services` path answers 404); the Esri-hosted SLC layers are a cartographic blend of v3.1 and v2.2 and carry no layer table.
    - **The build.** `backend/tools/slc/build_slc.py` (standard library only) keeps each polygon's dominant soil and rasterises the 3,923 polygons with one on a 0.01° grid, inside the map's accuracy: 0.9 MB, embedded in the API. No new crate.
    - **Limits.** 1:1,000,000 is coarse: a polygon is tens of kilometres across, and its dominant soil is the soil before the city was built, which a garden bed rarely is. That is true of SoilGrids too, and the evidence priority (6.4) already puts the user's word and the photo first. SLC v3.2 covers only agricultural regions; outside them (central Montréal, the north) the probes still run. `PH2` is "pH as per project report", read as pH in water (*unverified* method). A profile doesn't say which source it came from; adding a `source` field would be a contract change, left open.
11. **Invasive species list and reporting contacts:** which species to include for Ontario, and where each should be reported.
12. **Guided soil checks:** in scope (squeeze and jar tests, each analysed in its own `guided` job). Decide on day 2 whether the jar test's delayed photo makes the demo video.
13. **Text-embedding model** and its dimension. Decided: `snowflake-arctic-embed-xs`, 384 dimensions, fp32, pinned by file hash for the life of the database, since the memory and knowledge vectors must all come from the same model ([vision models](../vision/models.md)).
14. **Verification-answer timeout.** 10 minutes is proposed before the pipeline continues with the best candidate marked uncertain.
15. **VLM concurrency (`VLM_SLOTS`)**, per VLM worker, and the perception batch size and per-model caps: measured in the day-2 benchmark. See [contracts.md](contracts.md#12-open-questions). **The 5070 Ti decided on 2026-10-20:** `VLM_SLOTS` 2 at `VLM_MAX_PIXELS` 1,003,520. The 9070 XT and the perception caps are still open.
16. **LiDAR capture:** the shortest reliable distance, the depth map's size, holes in unfiltered depth and lens distortion, from the day-1 check, and with them whether capture stays on AVFoundation or moves to ARKit's scene depth. The distance sets the leaf distance hint and the measurement's valid range.
17. **Measurement thresholds:** the margin for answering a measurable indicator (10% of the bound, at least two pixels, proposed), the confidence rules, the spot colour thresholds and the share of a leaf under which a necrotic patch is a speck, not a spot (set on 2026-10-22, fitted on PlantSeg's lesion masks: [evaluation 6](../vision/evaluation.md#6-thresholds-set-from-the-evaluation-set)), the smallest spot that is sized (4 pixels) and the stored size of LiDAR shots (a long side of at least 2048 pixels), the growth noise floor (2 cm or 5%), the crowding factor (0.75) and the neighbour radius (1.5 m). The ones not yet set are set from LiDAR photos in the evaluation set.
18. **Further depth quantities:** leaf angle (for wilting) and soil-surface relief are measurable in principle, but untested at LiDAR's resolution. Add them only if the evaluation photos show they are reliable.
19. **The RX 9070 XT's VLM server:** vLLM on ROCm (proposed) or llama.cpp with Vulkan, decided by the AMD day-1 check (section 10), with the same checkpoint as the 5070 Ti either way ([contracts open question 25](contracts.md#12-open-questions)). Its host is settled: a teammate's machine, with its own compose file (12; D11, [#13](https://github.com/aacruzgon/GreenLens/issues/13)).
20. **Weather and trigger thresholds:** frost, heat, humid stretch, heavy rain, after rain, dry soil, the weather-fit day counts and the fusion levels. The weather thresholds have one definition and one owner, [backend domain 7, weather thresholds](../backend/domain.md#weather-thresholds) (defaults: frost ≤ 0.5 °C at 2 m, hard frost ≤ −2 °C, heat ≥ 30 °C, a humid day ≥ 10 h over 90%, a humid stretch of 2 such days in a row, forecast or in the past week, heavy rain ≥ 20 mm a day, after rain ≥ 5 mm in 24 h, dry soil < 0.15 m³/m³ with ET₀ ≥ 3 mm); their numbers are set from the evaluation set with the measurement thresholds ([contracts open question 27](contracts.md#12-open-questions)).
