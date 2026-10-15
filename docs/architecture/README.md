# PDM Architecture

> **Status:** Draft v0.3, 2026-10-15. Adds soil data sources and soil findings from photos, the detection of organisms (pests, beneficial insects, wildlife, invasive species) and the damage they cause, and capture guidance. v0.2 added species identification, a knowledge base with retrieve-then-verify, and an optional fast text classifier (Jev).
> **Event:** [Build North: Ship Something Real](https://luma.com/build-north). Build week Oct 16–25, submissions due Oct 25 at 11:59 PM ET, Demo Day Oct 29 in Toronto.

PDM is a plant, crop and garden health monitoring app. A user photographs a plant. Computer vision on the backend assesses the plant, the soil and the surroundings. The results are combined with the weather and soil data for the photo's location and with the plant's own history, to produce a diagnosis and an action plan. Users can chat about the results, and the system warns them ahead of risky weather.

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
| Security & Ethics | Managed authentication with MFA. Every garden-scoped query checks membership. Image storage is private. Photo metadata is stripped and faces are blurred before storage. Location needs consent. Deleting an account removes everything. Wildlife advice is humane. Pesticide advice is limited, uncertain results are shown as uncertain, and advice cites its sources. |

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
                          │ job broker (leases) · SSE hub · scheduler       │──▶ Push (APNs / FCM)
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
│   quality gate · YOLO-seg ·            → vLLM / llama.cpp sidecar   │
│   BioCLIP species · disease classifier   7–8B VLM, quantized        │
│   image + text embeddings                                           │
└─────────────────────────────────────────────────────────────────────┘
```

### Design principles

- **Pull-based GPU workers.** The rig opens no ports. It asks the API for jobs over outbound HTTPS. Where the control plane runs is a deployment choice, not a change to the architecture. To add GPU capacity, start another worker.
- **Vision produces facts, rules produce risk, the LLM produces language.** Every finding carries a confidence score. The LLM explains structured findings and never invents a diagnosis.
- **Retrieve, then verify.** Candidate diagnoses are checked against curated species and disease profiles before any advice is given. When the evidence is ambiguous, the app asks the user a targeted question instead of guessing. Advice cites its sources.
- **Use the cheapest model that can do the job.** Small classifiers make the routine decisions (routing, gating, tagging). The strongest LLM is kept for reasoning and writing.
- **One agent core with two callers.** The plan writer, running after the pipeline, and the chat use the same context builder, tools and safety policy, so their answers can't contradict each other.
- **A trait at every external boundary:** `VisionModel`, `SpeciesIdentifier`, `TextClassifier`, `ChatModel`, `KnowledgeStore`, `WeatherProvider`, `SoilProvider`, `BlobStore`, `Store`, `MemoryStore`, `Notifier`. Swapping a vendor or sponsor technology means writing one adapter.
- **Stream results, not video.** The app uploads a few still images (see 6.6). The progress and findings stream back over SSE.

---

## 3. Components

| Component | Runs on | Tech | Responsibility |
|---|---|---|---|
| Mobile app | Phones, web build | Expo / React Native. TypeScript types generated from OpenAPI | Guided capture, location consent, live progress, results with overlays, chat, timeline, tasks, push |
| Auth provider | Managed service | JWT + JWKS, webhooks | Credentials, MFA, sessions. Never stores domain data |
| `api` | Control plane | axum, tower-http, governor, utoipa | All public endpoints, authorization, uploads (EXIF stripping, face blurring), job broker, SSE hub, scheduler, agent core |
| Agent core | Inside `api` | `ChatModel` trait, tool registry | Context assembly, the tool-use loop, safety policy |
| SurrealDB | Control plane | Graph edges, HNSW vectors, geometry types | All structured data, the job queue, memory |
| RustFS | Control plane, private | S3 API via `aws-sdk-s3` | Images. Only `api` talks to it |
| `perception-worker` | Rig, RTX 3070 | `ort` + CUDA, ONNX models | Quality gate, segmentation, species and organism identification (BioCLIP), disease classification per leaf, image and text embeddings |
| `vlm-worker` + sidecar | Rig, RTX 5070 Ti | vLLM or llama.cpp, OpenAI-compatible API | Soil condition, organisms and damage patterns, verification indicator questions, overall structured findings |
| `fallback-worker` | Control plane | `VisionModel` with the LLM API as backend | Keeps scans working while the rig is offline |
| Training | Dev machines / 5070 Ti | Python, PyTorch → ONNX | Fine-tunes the disease classifier. Not deployed |
| Knowledge ingestion | Dev machines | Python scripts + LLM API, reviewed by a teammate | Builds the knowledge base from curated sources. Loaded through migrations. Not deployed |
| Text classifier (optional) | Jev API, cheapest LLM as fallback | `TextClassifier` trait | Chat routing, safety gate, memory-extraction gate, chunk tagging, verification scoring |

### Rust crates

| Crate | Purpose |
|---|---|
| `domain` | Shared types, IDs, errors |
| `store` | SurrealDB repository layer and migrations |
| `blob` | RustFS client (custom endpoint, path-style addressing) |
| `agent` | Context builder, tool registry, safety policy, `ChatModel` and `TextClassifier` implementations (Jev, LLM fallback) |
| `vision` | `VisionModel` and `SpeciesIdentifier` traits, pipeline stages, `ort` sessions |
| `knowledge` | `KnowledgeStore` trait, profile lookups, look-alike graph, chunk retrieval, verification scoring |
| `api` (binary) | Routes, auth middleware, job broker, SSE, scheduler |
| `worker` (binary) | One binary started with `--caps perception,embed`, `--caps vlm` or `--caps fallback` |

---

## 4. Users and authorization

### Identity

- A managed auth provider handles sign-up, sign-in, social login, MFA and sessions.
- The app attaches the provider's JWT to every request. The API checks it against the provider's JWKS (`jsonwebtoken`) and maps the `sub` claim to a `user` record.
- The `user` record is created by the provider's `user.created` webhook (signature-verified), or on the first request as a fallback. `user.deleted` triggers the full data-deletion cascade.

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

---

## 5. Data

### SurrealDB tables, grouped by area

```
identity      user · member_of (user → garden, role) · push_token
garden        garden (centroid point, boundary polygon, soil_profile) · plant (species,
              species_status, species_candidates, position, status)
analysis      scan · scan_image (kind, storage key, embedding) · observation (stage:
              species | disease | soil | organism | damage, label, confidence, region,
              model_version) · weather_snapshot (weather + soil moisture, soil
              temperature, ET₀) · diagnosis
plans         action_plan (version, supersedes) · task (due_at, trigger, source, done_at)
chat          conversation (anchor: scan | plant | garden, summary) · message
              (role, content, tool_calls, model, prompt_version, tokens) ·
              pending_action (proposed | confirmed | rejected) · message_feedback
memory        memory (plant, kind, text, embedding, source: pipeline | chat | user)
knowledge     species · disease · organism · treatment · source (url, publisher, licence) ·
              knowledge_chunk (source, crop, disease, topic, text, embedding)
              edges: susceptible_to · looks_like · manages · caused_by · preys_on
feedback      feedback (scan, verdict, corrected_label, notes)
ops           job (kind, status, lease_until, worker, attempts) · worker (caps,
              last_heartbeat, gpu) · usage_daily (tokens, requests) · weather_cache
```

### Schema sketch

This is a sketch. Check the syntax against the SurrealDB version installed.

```sql
DEFINE TABLE user SCHEMAFULL;
DEFINE FIELD auth_id          ON user TYPE string;
DEFINE FIELD display_name     ON user TYPE string;
DEFINE FIELD location_consent ON user TYPE bool DEFAULT false;
DEFINE FIELD created_at       ON user TYPE datetime DEFAULT time::now();
DEFINE INDEX user_auth ON user FIELDS auth_id UNIQUE;

DEFINE TABLE garden SCHEMAFULL;
DEFINE FIELD name     ON garden TYPE string;
DEFINE FIELD centroid ON garden TYPE geometry<point>;
DEFINE FIELD boundary ON garden TYPE option<geometry<polygon>>;
DEFINE FIELD soil_profile ON garden TYPE option<object>;  -- SoilGrids: pH, texture, organic carbon, CEC by depth, fetched_at

DEFINE TABLE member_of TYPE RELATION IN user OUT garden SCHEMAFULL;
DEFINE FIELD role ON member_of TYPE string ASSERT $value IN ['owner', 'editor', 'viewer'];

DEFINE TABLE plant SCHEMAFULL;
DEFINE FIELD garden  ON plant TYPE record<garden>;
DEFINE FIELD species            ON plant TYPE option<record<species>>;
DEFINE FIELD species_status     ON plant TYPE string DEFAULT 'unknown';  -- unknown | suggested | confirmed
DEFINE FIELD species_candidates ON plant TYPE array<object> DEFAULT [];  -- [{species, score, source}]
DEFINE FIELD status             ON plant TYPE string DEFAULT 'healthy';

DEFINE TABLE scan SCHEMAFULL;
DEFINE FIELD garden   ON scan TYPE record<garden>;
DEFINE FIELD plant    ON scan TYPE option<record<plant>>;
DEFINE FIELD user     ON scan TYPE record<user>;
DEFINE FIELD location ON scan TYPE option<geometry<point>>;
DEFINE FIELD status   ON scan TYPE string DEFAULT 'pending';

DEFINE TABLE memory SCHEMAFULL;
DEFINE FIELD plant     ON memory TYPE record<plant>;
DEFINE FIELD kind      ON memory TYPE string;          -- fact | event | summary
DEFINE FIELD text      ON memory TYPE string;
DEFINE FIELD source    ON memory TYPE string;          -- pipeline | chat | user
DEFINE FIELD embedding ON memory TYPE array<float>;
DEFINE INDEX memory_vec ON memory FIELDS embedding HNSW DIMENSION 768 DIST COSINE;

DEFINE TABLE job SCHEMAFULL;
DEFINE FIELD kind        ON job TYPE string;           -- perception | vlm | verify
DEFINE FIELD scan        ON job TYPE record<scan>;
DEFINE FIELD status      ON job TYPE string DEFAULT 'queued';  -- queued | leased | done | failed
DEFINE FIELD lease_until ON job TYPE option<datetime>;
DEFINE FIELD worker      ON job TYPE option<string>;
DEFINE FIELD attempts    ON job TYPE int DEFAULT 0;
DEFINE FIELD created_at  ON job TYPE datetime DEFAULT time::now();

-- knowledge base
DEFINE TABLE source SCHEMAFULL;
DEFINE FIELD url       ON source TYPE string;
DEFINE FIELD publisher ON source TYPE string;
DEFINE FIELD licence   ON source TYPE option<string>;

DEFINE TABLE species SCHEMAFULL;
DEFINE FIELD scientific_name      ON species TYPE string;
DEFINE FIELD common_names         ON species TYPE array<string>;
DEFINE FIELD care                 ON species TYPE object;   -- water, light, hardiness, soil pH
DEFINE FIELD classifier_supported ON species TYPE bool DEFAULT false;

DEFINE TABLE disease SCHEMAFULL;
DEFINE FIELD name        ON disease TYPE string;
DEFINE FIELD kind        ON disease TYPE string;          -- fungal | bacterial | viral | pest | wildlife | abiotic
DEFINE FIELD symptoms    ON disease TYPE array<object>;   -- [{indicator, question, where}]
DEFINE FIELD conditions  ON disease TYPE object;          -- temperature, humidity, leaf-wetness ranges
DEFINE FIELD sources     ON disease TYPE array<record<source>>;
DEFINE FIELD reviewed_by ON disease TYPE option<string>;

DEFINE TABLE organism SCHEMAFULL;
DEFINE FIELD name            ON organism TYPE string;
DEFINE FIELD scientific_name ON organism TYPE string;
DEFINE FIELD taxon_group     ON organism TYPE string;     -- insect | mite | mollusc | worm | mammal | bird
DEFINE FIELD role            ON organism TYPE string;     -- pest | beneficial | neutral | invasive
DEFINE FIELD reporting_url   ON organism TYPE option<string>;  -- invasive species only
DEFINE FIELD sources         ON organism TYPE array<record<source>>;

DEFINE TABLE treatment SCHEMAFULL;
DEFINE FIELD kind ON treatment TYPE string;               -- cultural | biological | mechanical | chemical
DEFINE FIELD text ON treatment TYPE string;

DEFINE TABLE susceptible_to TYPE RELATION IN species OUT disease;
DEFINE TABLE looks_like     TYPE RELATION IN disease OUT disease;
DEFINE FIELD how_to_tell ON looks_like TYPE string;
DEFINE TABLE manages        TYPE RELATION IN treatment OUT disease;
DEFINE TABLE caused_by      TYPE RELATION IN disease OUT organism;   -- pest and wildlife damage
DEFINE TABLE preys_on       TYPE RELATION IN organism OUT organism;  -- beneficial → pest

DEFINE TABLE knowledge_chunk SCHEMAFULL;
DEFINE FIELD source    ON knowledge_chunk TYPE record<source>;
DEFINE FIELD crop      ON knowledge_chunk TYPE option<record<species>>;
DEFINE FIELD disease   ON knowledge_chunk TYPE option<record<disease>>;
DEFINE FIELD topic     ON knowledge_chunk TYPE string;    -- symptoms | conditions | management | care
DEFINE FIELD text      ON knowledge_chunk TYPE string;
DEFINE FIELD embedding ON knowledge_chunk TYPE array<float>;
DEFINE INDEX chunk_vec ON knowledge_chunk FIELDS embedding HNSW DIMENSION 768 DIST COSINE;
```

The vector dimensions depend on the embedding models chosen.

Example queries:

```sql
-- membership check
SELECT role FROM member_of WHERE in = $user AND out = $garden;

-- memory recall for the agent
SELECT text, kind FROM memory WHERE plant = $plant AND embedding <|5|> $query_vec;

-- nearby gardens, for regional disease alerts
SELECT * FROM garden WHERE geo::distance(centroid, $point) < 5000;

-- candidate diseases for a confirmed species
SELECT ->susceptible_to->disease.* AS candidates FROM $species;

-- look-alikes for a candidate, with how to tell them apart
SELECT out.name AS lookalike, how_to_tell FROM looks_like WHERE in = $disease;

-- natural enemies of a pest ("leave the ladybugs, they eat aphids")
SELECT <-preys_on<-organism.name AS natural_enemies FROM $pest;

-- grounding chunks for the plan writer
SELECT text, source.url FROM knowledge_chunk
  WHERE disease = $disease AND topic = 'management' AND embedding <|5|> $query_vec;
```

### Fields that exist for traceability

- `model_version` and `prompt_version` on every observation, plan and message, so any output can be reproduced and audited.
- `action_plan.version` / `supersedes`: when chat revises a plan, it creates a new version rather than overwriting the old one.
- `memory.source`: tracks where each remembered fact came from.
- `feedback`: user corrections, which become labelled training data.
- `plant.species_status`: whether the species was suggested by the models or confirmed by the user.
- `disease.sources` and `disease.reviewed_by`: every profile points to its sources and records which teammate reviewed it.

### RustFS layout

The bucket is private. Photo metadata (EXIF) is stripped, faces are blurred and images are resized before anything is written.

```
gardens/{garden_id}/scans/{scan_id}/{image_id}.webp
gardens/{garden_id}/chat/{conversation_id}/{image_id}.webp
```

Keys are prefixed by garden, so deleting a garden or a user's data is a prefix delete plus a cascade in SurrealDB.

### Knowledge base

The knowledge base lets the system check its diagnoses and base its advice on reliable sources, instead of on the LLM's general knowledge.

| Kind | Contents | Used by |
|---|---|---|
| Species profiles | Care needs, hardiness, water, light, soil pH, and whether the disease classifier supports the species | Species confirmation, plan writer, chat |
| Disease profiles | Symptoms written as checkable indicators, where symptoms start, the temperature, humidity and leaf-wetness ranges that favour the disease, how it spreads | Verification. Fusion rules take their risk thresholds from here instead of hard-coding them |
| Organism profiles | Pests, beneficial insects, wildlife and invasive species: how to recognize them and the damage they cause, their role, natural enemies, and reporting contacts for invasive species | Organism identification, verification, plan writer |
| Soil conditions | Short descriptions of soil conditions visible in photos (compaction, crusting, waterlogging, salt build-up) and what to do about each | Soil findings, plan writer |
| Graph relations | `susceptible_to`, `looks_like` (with how to tell them apart), `manages`, `caused_by` (damage → organism), `preys_on` (beneficial → pest) | Candidate lists, look-alike checks, natural-enemy advice |
| Treatments | Cultural, biological, mechanical and chemical options, with IPM first | Plan writer |
| Source chunks | Text from the sources, chunked, embedded and tagged by crop, disease and topic | Plan writer and chat, with citations |

**Scope.** Only the supported crops: about 10–15 crops and their top 3–5 diseases each, roughly 50–100 disease profiles. In addition: the top pests of those crops, the common beneficial insects and garden wildlife, the invasive species reported in Ontario, and a short list of soil conditions.

**Sources.** Curated only: university extension fact sheets and Canadian and Ontario government agriculture guides. No open web search. If a fallback is needed later, it should be a search restricted to an allowlist of domains.

**Ingestion** (`ml/knowledge/`):

1. Fetch the source documents, then clean and chunk them.
2. Tag each chunk with crop, disease and topic using the text classifier, and drop irrelevant chunks.
3. Embed the chunks with a local text-embedding model on the RTX 3070.
4. Extract the structured profile fields (symptoms, conditions, look-alikes) with an LLM.
5. A teammate reviews every profile before it's loaded. Bad data here produces confident wrong advice.
6. Load everything through SurrealDB migrations, so every environment has the same knowledge base.

**Licensing.** Store only short excerpts, keep `source.url` on every chunk and profile, link back to the source in the app, and check each source's terms before ingesting it.

---

## 6. Analysis pipeline

### 6.1 Stages

| Stage | Output | Where it runs |
|---|---|---|
| Quality gate | Blur and exposure score, whether a plant is present. Prompts a retake if the photo fails | perception-worker |
| Segmentation | Plant, leaf, soil and surroundings regions | perception-worker (YOLO-seg) |
| Species identification | Top 3 species with scores, cross-checked with the VLM. Skipped once the plant's species is confirmed. See 6.2 | perception-worker (BioCLIP) + vlm-worker |
| Disease classification | `{label, confidence, region}` for each leaf crop. Runs only for species the classifier was trained on | perception-worker (classifier fine-tuned on PlantDoc-style field images) |
| Soil condition | Observations with confidence: moisture, crusting and cracking, structure, colour, mulch or bare soil, worm castings, salt crust, moss or mould, erosion, weeds. See 6.4 | vlm-worker |
| Organisms | Pests, beneficial insects, wildlife and invasive species seen in the photo, each with a role and confidence. See 6.5 | perception-worker (BioCLIP on crops of detected organisms) + vlm-worker |
| Damage patterns | Chewing, holes, leaf-mining tunnels, sticky residue and sooty mould, cut or torn stems, digging, tracks, droppings | vlm-worker |
| Embeddings | Image vectors for finding similar past cases | perception-worker |
| Context | Current weather, the past 7 days of weather, a 7–14 day forecast, soil moisture and temperature, evapotranspiration, climate zone, plus the garden's stored soil profile | api (Open-Meteo per scan, cached by rounded location and hour; SoilGrids once per garden) |
| Retrieve | Profiles of the top 3 candidate problems (diseases, pests, wildlife damage, abiotic causes) and their look-alikes | api (`knowledge`) |
| Verify | Candidates re-ranked against the photo, the weather and the profiles. Ambiguous cases end in a question for the user. See 6.3 | vlm-worker (indicator questions, as a `verify` job) + api (scoring) |
| Fusion | Risk scores from rules over the verified findings, weather, soil and history. Thresholds come from the disease profiles (e.g. humidity > 90% at 15–25 °C for 2+ days raises fungal risk). Evidence is ranked user input, then photo, then modelled data (see 6.4) | api |
| Plan writer | Steps, timing, what to watch for, tasks. Based on the confirmed disease's management guidance, with citations | api → agent core → LLM API |

If the confidence is low, the app asks for a better photo or suggests contacting a local extension office. It does not give a confident diagnosis.

### 6.2 Species identification

1. BioCLIP compares the photos against a label list of the supported crops plus common garden plants, and returns the top 3 species with scores.
2. The VLM names the species independently.
3. If both agree with a high score, the species is stored as `suggested` and the user confirms it with one tap. If they disagree, or the score is low, the app shows the top 3 with reference photos, plus a "something else" option.
4. The pipeline continues with the best candidate, so results aren't held up. If the user picks a different species, the stages from disease classification onwards run again.
5. The confirmed species is stored on the `plant` with `species_status = confirmed`. Later scans of the same plant skip this stage.
6. The disease classifier runs only if `species.classifier_supported` is true. For other species, the pipeline relies on the VLM's findings and marks them as lower confidence. This stops an unsupported plant being diagnosed as the closest supported one, such as basil being reported as "pepper, healthy".

The capture flow (see 6.6) asks for a whole-plant photo plus a leaf close-up, and a flower or fruit if there is one, because identification is much more accurate with several views.

An external identification API (Pl@ntNet, Kindwise/plant.id) can be added later behind `SpeciesIdentifier` as a backup, after checking its free tier and terms.

### 6.3 Retrieve, then verify

Classifiers and VLMs mostly fail on look-alikes: early blight vs. Septoria leaf spot, a nitrogen deficiency vs. a disease, sunscald vs. a fungal lesion. Verification checks each candidate against its profile before anything is shown as a diagnosis.

```
species confirmed → top-3 problem candidates (disease classifier, VLM,
                    organism and damage findings)
        │
        ▼
retrieve each candidate's profile and its look-alikes:
  symptom indicators · where symptoms start · favourable weather · how to tell apart
        │
        ▼
verify:
  1. the VLM answers each indicator question about the photo, in text
     ("concentric rings?", "started on lower leaves?", "yellow halo?")
  2. the text classifier scores each candidate profile against those answers
  3. the weather is checked against each profile's favourable conditions
        │
        ├─ clear winner → diagnosis, with citations
        └─ ambiguous    → one targeted question to the user, or a request
                          for a specific photo; then verify again
```

Answering specific indicator questions is more accurate than asking one broad question ("what disease is this?"). It also makes the diagnosis explainable: the app can show which indicators matched.

### 6.4 Soil

Soil information comes from three sources. Each one covers something the others can't.

| Source | What it gives | When it's fetched |
|---|---|---|
| **SoilGrids** (ISRIC REST API) | Properties that don't change: pH, texture (clay, sand, silt), organic carbon, nitrogen, cation exchange capacity, bulk density, at depths from 0–5 cm down to 100–200 cm. About 250 m resolution | Once, when the garden is created. Stored as `garden.soil_profile` and refreshed rarely |
| **Open-Meteo** (part of the weather call) | Conditions that change hourly: soil moisture at 0–1, 1–3, 3–9, 9–27 and 27–81 cm; soil temperature at 0, 6, 18 and 54 cm; evapotranspiration, including the FAO reference rate (ET₀) used to estimate watering needs | On every scan (stored in `weather_snapshot`) and in the scheduler's forecast checks |
| **The photo** (vlm-worker) | What the soil looks like now: moisture, crusting and cracking, structure, colour, mulch or bare soil, worm castings, salt crust, moss or mould, erosion, weeds | On every scan |

SoilGrids is officially in beta, comes with no uptime guarantee, and allows 5 calls per minute. Fetching it once per garden keeps us well within that. If it proves unreliable, the fallback is Agriculture and Agri-Food Canada's Soil Landscapes of Canada data, loaded as a static map layer.

**Evidence priority.** Both APIs return modelled values for the area around the garden, not for the user's actual soil. They know nothing about raised beds, containers, potting mix, compost or recent watering. Fusion therefore ranks the evidence:

1. **User input:** "it's in a container", a pH test kit result, "watered an hour ago". This already arrives through plant memory.
2. **The photo:** what the VLM sees.
3. **Modelled data:** Open-Meteo for current conditions, SoilGrids for baseline properties.

The plan writer says which source it relied on.

**Soil findings are a list of observations, not a score.** For example: "moisture: dry (high confidence)", "structure: crusted (medium)", "biology: worm castings (medium)". A single "soil health: 72/100" would claim a precision the photo can't support. A photo can't show nutrient levels, pH, contaminants or microbial life. For those, the app suggests a home test kit or a lab soil test, and accepts the result as user input.

**Guided checks (optional).** The app walks the user through a simple test and analyses a photo of the result:

- **Squeeze test:** squeeze a handful of moist soil. Whether it crumbles or holds together as a ball gives a rough texture estimate.
- **Jar test:** shake soil with water in a jar and let it settle. The layers show the sand, silt and clay proportions. Settling takes hours, so the app creates a task to photograph the jar later.

### 6.5 Organisms and damage

The photos are checked for living things and for the damage they leave. Insects can often be seen directly. Larger animals mostly show up as damage, holes, tracks or droppings.

| Category | Examples | Detected by |
|---|---|---|
| Insect and other pests | Aphids, whiteflies, spider mites, caterpillars and hornworms, Colorado potato beetle, flea beetles, slugs and snails | BioCLIP on crops of detected organisms (it covers insects as well as plants). The VLM for webbing, slime trails and droppings. Tiny pests need a close-up of the leaf underside |
| Damage patterns | Small "shot holes" (flea beetles), ragged holes with slime (slugs), tunnels inside leaves (leaf miners), sticky residue and black mould (aphids), cleanly cut stems (rabbits), torn stems (deer) | The VLM, checked against the knowledge base's `looks_like` comparisons |
| Larger animals | Rabbits, deer, groundhogs, squirrels and raccoons digging, vole and mole tunnels, birds pecking fruit | Mostly from damage, holes, tracks and droppings |
| Beneficial organisms | Ladybugs and their larvae, lacewing larvae, ground beetles, spiders, bees, earthworms | BioCLIP / VLM. Never flagged as pests |
| Invasive species | For example jumping worms, Japanese beetle, box tree moth | BioCLIP / VLM, plus reporting guidance from the knowledge base |

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
| Whole plant | Always | Species, overall health, surroundings, larger-animal damage |
| Close-up of the affected leaf | When something looks wrong | Disease, damage patterns |
| Leaf underside | Suggested when pests are suspected | Aphids, whiteflies, mites, eggs |
| Soil surface around the base | Suggested | Soil condition, organisms in the soil |
| Flower or fruit | If present | Species identification |
| Guided checks (squeeze test, jar test) | Optional | Soil texture |

Each image is stored with its `scan_image.kind`, so each stage knows which photos to use. The quality gate checks each shot separately and asks for a retake of only the shots that fail.

---

## 7. Chat and agent core

Chat runs **inside `api`, not on the job queue**, because it is interactive. Tokens stream straight from the LLM to the app over SSE. Only heavy actions, such as re-analysing a new photo, go through the workers.

### Conversation anchors

| Anchor | Opened from | Context loaded |
|---|---|---|
| `scan` | "Ask about this result" | That diagnosis, its observations and image regions, the weather, the plan |
| `plant` | Plant timeline | All scans of the plant, its memory, open tasks |
| `garden` | Garden home | Summaries of each plant, the forecast, upcoming tasks |

The context is rebuilt from the database on every turn. It uses the conversation summary, the most recent messages and the top memory hits, not the full transcript. The chat therefore always sees the latest data, such as a frost task the scheduler created an hour earlier.

### Tools

| Tool | Purpose | Kind |
|---|---|---|
| `get_scan(scan_id)` | Findings, confidence, regions | read |
| `view_image(image_id, region?)` | Sends the photo, or a crop of it, back to the model | read |
| `get_forecast(garden_id, days)` | Weather forecast | read |
| `search_plant_memory(plant_id, query)` | Vector search over past events and facts | read |
| `find_similar_cases(image_id)` | Embedding search over the user's own past scans | read |
| `search_knowledge(query)` | Retrieval over curated guides, returned with citations | read |
| `get_species_profile(species)` | Care needs, diseases the species is susceptible to | read |
| `get_disease_profile(disease)` | Symptoms, favourable conditions, management, sources | read |
| `compare_lookalikes(disease)` | Look-alike diseases and how to tell them apart | read |
| `get_organism_profile(organism)` | How to recognize it, its role (pest, beneficial, invasive), natural enemies, reporting guidance | read |
| `get_soil_profile(garden_id)` | The garden's SoilGrids profile, plus the latest soil moisture and temperature | read |
| `list_tasks` / `create_task` / `complete_task` | Manage the task list | **write: user confirms** |
| `revise_plan(diagnosis_id, constraints)` | Produces a new plan version | **write: user confirms** |
| `request_rescan(plant_id, instructions)` | Asks the user for a specific photo, then runs the pipeline | async (worker) |
| `remember(plant_id, fact)` | Saves a fact to plant memory | write (automatic, shown to the user) |

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

`memory` is written from two places:

- **The pipeline**, after each scan: a summary of the findings.
- **The chat**, after each turn: the text classifier first decides whether the turn contains a fact that will still matter later. Only if it does, a cheap model extracts it, such as "watered yesterday", "applied neem oil Oct 18" or "it's in a container".

Memory persists across conversations and scans, which keeps advice consistent over time. It sits behind a `MemoryStore` trait, so it can be mapped onto SurrealDB Agent Memory if we compete for that prize.

---

## 8. Main flows

### 8.1 Scan → diagnosis → plan

1. The app sends `POST /scans` with the plant and GPS location. The API checks membership and creates the scan with status `pending`.
2. The app uploads to `POST /scans/{id}/images`. The API strips the metadata, blurs any faces, resizes the image, writes it to RustFS and queues a `perception` job.
3. The app opens SSE on `/scans/{id}/events`.
4. The perception-worker claims the job. It runs the quality gate, segmentation, species identification (unless the plant's species is already confirmed), organism identification, disease classification and embeddings, and streams each result back as an event.
5. The API stores the observations and queues a `vlm` job. The vlm-worker adds the soil condition, organism and damage findings and its own species suggestion. If the species isn't confirmed, the app shows the confirmation card.
6. The API fetches the weather, including soil moisture and temperature, and loads the garden's soil profile. It retrieves the profiles of the top candidate problems and their look-alikes.
7. The API queues a `verify` job, and the vlm-worker answers each candidate's indicator questions. The API scores the candidates and checks the weather against their conditions. It then either stores a `diagnosis` with citations or sends the user a question.
8. Fusion computes the risk scores. The agent core writes the `action_plan` from the confirmed disease's management guidance and creates tasks.
9. The API appends a summary to memory, updates the plant's status and sends `plan_ready`.

### 8.2 Chat turn

1. The app sends `POST /conversations/{id}/messages`. The API checks auth, membership and the user's token budget. The text classifier labels the message (see section 7).
2. The context builder loads the anchor's data, the conversation summary, recent messages and the top memory hits.
3. The LLM call streams over SSE. Read tools run as the session user. Write tools return confirmation cards. `request_rescan` starts flow 8.1.
4. The API stores the message with its model, prompt version and token counts. If the classifier finds a fact worth remembering, a cheap model extracts it into memory. Long conversations are summarized.

### 8.3 Forecast alerts

At a fixed interval, the scheduler fetches each garden's forecast and checks the trigger rules: frost, a humid stretch (fungal risk), heat, heavy rain, and dry soil (a watering reminder based on soil moisture and evapotranspiration). When a rule fires, it creates a `task` with `source = forecast_alert` and sends a push notification.

### 8.4 Job leasing and fallback

- `POST /internal/jobs/claim` is a long poll. Inside a transaction, it leases the oldest queued job that matches the worker's capabilities, for 2 minutes.
- If a worker misses its lease deadline, the job goes back to the queue and `attempts` increases.
- Jobs still unclaimed after about 15 seconds also become available to the `fallback-worker`, which uses the LLM API.
- Workers report a heartbeat to the `worker` table. A small status page shows the health of each GPU worker.

### 8.5 Garden setup

When a garden is created, the API fetches its SoilGrids profile in the background and stores it as `garden.soil_profile`. If SoilGrids is unavailable, the garden is created anyway and the fetch is retried later. Until it succeeds, the pipeline runs without baseline soil properties.

---

## 9. API surface

```
User API (auth provider JWT)               Worker API (per-worker token)
POST   /scans                              POST /internal/jobs/claim         (long-poll, caps)
POST   /scans/{id}/images                  POST /internal/jobs/{id}/events
GET    /scans/{id}                         POST /internal/jobs/{id}/complete
GET    /scans/{id}/events          (SSE)   GET  /internal/jobs/{id}/images/{n}
POST   /scans/{id}/feedback
POST   /scans/{id}/answers                 (verification questions)
POST   /plants/{id}/species                (confirm species)
CRUD   /gardens · /gardens/{id}/members    Webhooks
CRUD   /plants  · GET /plants/{id}/timeline POST /webhooks/auth              (signature-verified)
CRUD   /tasks
POST   /conversations
POST   /conversations/{id}/messages (SSE)  Ops
POST   /pending-actions/{id}/confirm       GET  /healthz
POST   /pending-actions/{id}/reject        GET  /status                      (worker health)
DELETE /me
```

`utoipa` generates the OpenAPI spec from the routes. `openapi-typescript` generates the app's TypeScript types from that spec.

---

## 10. Models and GPU placement

| Task | Primary | Fallback |
|---|---|---|
| Quality gate, segmentation, disease classification, image embeddings | RTX 3070, ONNX via `ort` | LLM API vision. Embeddings are skipped |
| Species and organism identification | RTX 3070, BioCLIP (ONNX), cross-checked with the VLM | LLM API vision. Optionally Pl@ntNet or Kindwise/plant.id for species |
| Face detection (for blurring on upload) | `api`, CPU, small ONNX face detector | None. Fails closed: an image isn't stored until blurring succeeds |
| Text embeddings (queries, memory) | RTX 3070, a small open text-embedding model | An embeddings API |
| Soil condition, organisms and damage patterns, verification indicator questions | RTX 5070 Ti, a Qwen-VL-class 7–8B VLM, 4- or 8-bit quantized | LLM API vision |
| Plan writer, chat | LLM API, strongest model | none |
| Summaries, memory extraction | LLM API, cheapest model | local VLM in text-only mode |
| Chat routing, safety gate, memory gate, chunk tagging, verification scoring | Jev (optional, early access) | LLM API, cheapest model, with structured output |

Notes:

- The RTX 3070 models (BioCLIP, YOLO-seg, the disease classifier, the text-embedding model) need roughly 3–4 GB of VRAM together.
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
- Agent tools take identity from the session, never from model output. Write tools need the user's confirmation.

**Infrastructure**
- The rig opens no ports. Workers connect outbound and authenticate with their own tokens.
- RustFS is never exposed publicly. Images reach the app and the workers only through the API.
- Auth webhooks are signature-verified.

**Privacy**
- Photo metadata (EXIF) is stripped before storage.
- Faces that appear in photos are blurred on upload, before storage, and are never analysed.
- Location requires consent and can be stored at reduced precision.
- `DELETE /me` cascades through SurrealDB and deletes the user's images from RustFS.

**Abuse and cost**
- Rate limits and a daily token budget for each user.
- Upload size limits.

**Safety of advice**
- Integrated pest management (IPM) and non-chemical steps come first.
- Advice is based on reviewed knowledge-base profiles and cites its sources.
- The text classifier flags safety-sensitive chat messages before the LLM runs, so the safety policy is applied to them first.
- Any pesticide step says to follow the product label and local regulations, and never gives doses.
- Questions about a person or pet being poisoned are directed to poison control.
- Beneficial organisms are never flagged as pests.
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

### Operations

- `tracing` with JSON logs. `/healthz` and `/status` endpoints. The job table doubles as an audit trail.
- Nightly SurrealDB export and RustFS volume snapshot.
- The SSE hub uses an in-process `tokio::sync::broadcast`. This assumes a single `api` instance. Running more than one would need SurrealDB `LIVE SELECT` or a message bus.

---

## 13. Repository layout

```
PDM/
├─ app/                Expo app
├─ backend/
│  ├─ crates/{domain, store, blob, agent, vision, knowledge}
│  ├─ bins/{api, worker}
│  └─ migrations/*.surql
├─ ml/
│  ├─ training/        classifier fine-tuning + ONNX export, evaluation set of real field photos
│  └─ knowledge/       ingestion scripts + reviewed seed data
├─ infra/              compose files, Caddyfile, tunnel config
└─ docs/
   └─ architecture/    this document
```

---

## 14. Build plan

| Dates | Goal |
|---|---|
| Oct 16–17 | End-to-end skeleton: auth, upload, job broker with stub vision, SSE, control plane deployed. Choose the crops, list the knowledge sources, request Jev access |
| Oct 18–19 | Real models on the rig (BioCLIP, disease classifier, VLM). Species confirmation. Weather with soil moisture and temperature; garden soil profile. Face blurring on upload. Knowledge ingestion running |
| Oct 20–21 | Knowledge profiles reviewed and loaded. Soil, organism and damage findings. Retrieve-then-verify. Fusion and plan writer with citations. Results UI with overlays |
| Oct 22 | Chat with tools, router, memory, confirmation cards, timeline |
| Oct 23 | Forecast alerts and push notifications. Test the fallback by killing the rig in the middle of a scan |
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
12. **Guided soil checks:** whether the squeeze and jar tests make it into the demo, or stay a stretch goal.
