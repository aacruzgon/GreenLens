# AGENTS.md

Guidance for AI agents working in this repository. Read this first, then open the document that covers your task.

## The project

PDM (GitHub repository: GreenLens) is a plant, crop and garden health monitoring app built for [Build North: Ship Something Real](https://luma.com/build-north). Build week runs Oct 16–25, 2026; submissions are due Oct 25 at 11:59 PM ET and Demo Day is Oct 29 in Toronto.

A user photographs a plant. Computer vision on a home GPU rig assesses the plant, the soil and the surroundings. The results are combined with weather, soil data and the plant's history to produce a diagnosis and an action plan, which the user can chat about.

**Stack:** Expo SDK 57 / React Native app with Clerk authentication (and a Swift LiDAR module), Rust backend (axum, tokio) in a Cargo workspace, SurrealDB 3.3.4, RustFS 1.0.1 for images, pull-based GPU workers (`ort`/CUDA and a vLLM or llama.cpp VLM sidecar), Python for model training and knowledge ingestion.

**Current state:** the repository holds design documents only. No code exists yet. When code lands, follow the planned layout below and update this file.

## Where to find things

All design lives in `docs/`. Each document has a status line at the top with its version, date and a changelog, and a numbered table of contents.

| File | What it is | Read it when |
|---|---|---|
| [`docs/architecture/README.md`](docs/architecture/README.md) | The architecture: goals and judging criteria, system diagram, components and Rust crates, users and authorization, data overview, the analysis pipeline, chat and agent core, main flows, API surface, models and GPU placement, security, deployment, repository layout, build plan and open decisions | You need the big picture, how parts fit together, or why a design choice was made |
| [`docs/architecture/contracts.md`](docs/architecture/contracts.md) | The data contracts: every shape that crosses a boundary (app ↔ API, API ↔ workers, worker ↔ VLM, ingestion ↔ database), with error codes, endpoints, events, job payloads, VLM output and pipeline records | You are writing or calling an endpoint, a worker, a job, an SSE event, a VLM prompt, or any type in the `domain` crate |
| [`docs/database/README.md`](docs/database/README.md) | SurrealDB notes: version pin and setup, conventions, tables, verified write patterns (event `seq`, compare-and-set, job claims, idempotency, deletion order), reads and vector search, the Rust SDK, operations and caveats | You are writing a query, a repository function in the `store` crate, or a migration |
| [`docs/database/schema.surql`](docs/database/schema.surql) | The database schema itself. Idempotent; safe to run on every API start | You need a table's exact fields, types, indexes or references |
| [`docs/frontend/README.md`](docs/frontend/README.md) | The frontend architecture: stack and pins, libraries, layers and import rules, directory layout, Clerk, the API and streaming layers, state, platform adapters (phone and in-app browsers, the LiDAR module's JS interface), UI system, mock mode, testing, builds, build plan, open decisions and sources | You are working on anything in `app/`, or need to know how the app consumes an endpoint or event |
| [`docs/frontend/flows.md`](docs/frontend/flows.md) | Sequence diagrams for every user flow and the app's state machines | You are implementing a flow end to end, or a reducer or state machine in a feature's `model/` |
| [`docs/frontend/screens.md`](docs/frontend/screens.md) | Navigation map, screen inventory with data, actions and roles, wireframes, screen rules, shared components and wording rules | You are building or changing a screen |
| [`docs/storage/README.md`](docs/storage/README.md) | RustFS notes: version pin and setup, bucket and key layout, how images are served through signed API URLs, writes and deletes, the Rust client (`aws-sdk-s3`), operations, caveats and the fallback | You are touching image or depth-map storage, the `blob` crate, or upload handling |
| [`docs/backend/README.md`](docs/backend/README.md) | The Rust backend design: goals, the crate graph and dependency rules, workspace layout, Rust conventions, runtime composition, configuration, testing, builds, build plan. Links a document per crate (`domain`, `domain-types`, `store`, `blob`, `integrations`, `vision`, `knowledge`, `agent`, `pipeline`, `api`, `worker`) with every struct, enum, trait, generic and lifetime, compiled on Rust 1.99.1 | You write or review any Rust, a `Cargo.toml`, a repository method, an endpoint, a worker or a background task |
| [`docs/backend/flows.md`](docs/backend/flows.md) | Server-side sequence diagrams across crates (request pipeline, uploads, a scan from submit to plan, claims and the sweep, SSE, job leases, chat turns, deletion, the scheduler, start-up and shutdown) and the server's state machines | You implement or debug a flow end to end on the server |

### Topic index

| Topic | Where |
|---|---|
| Wire format, IDs, timestamps, units, confidence bands, regions | contracts §1 |
| Roles and access matrix, idempotency keys | contracts §1; architecture §4 |
| Error codes and response shape | contracts §2 |
| Scan endpoints and the scan state machine | contracts §3; architecture §8.1 |
| Live progress over SSE, event catalogue, resuming | contracts §4 |
| Worker leases, heartbeats, job claim, payloads and results | contracts §5; architecture §8.4 |
| VLM image preparation, per-job schemas, findings and indicator calls | contracts §6; architecture §10 |
| Observations, measurements, weather, diagnoses, action plans, tasks | contracts §7 |
| Pipeline stages, species ID, retrieve-then-verify, soil, organisms | architecture §6.1–6.5 |
| Capture guidance | architecture §6.6 |
| LiDAR depth, measurement and the garden map | architecture §6.7–6.8; contracts §3, §7 (Measurement), §9 (Garden map) |
| Chat, tools, memory, pending actions | contracts §8; architecture §7, §8.2 |
| Gardens, members, invites, plants, tasks, account, deletion cascades | contracts §9; database §4 (Deletion) |
| Forecast alerts | architecture §8.3 |
| Knowledge base and seed file format | architecture §5 (Knowledge base); contracts §10 |
| Security, privacy and safety (EXIF, face blurring, dose checks) | architecture §11 |
| Deployment, hosting, app delivery, operations | architecture §12 |
| App layers, layout, libraries and import rules | frontend §4–6 |
| App auth (Clerk), API client, errors, idempotency, SSE handling | frontend §8–10 |
| App flows and state machines | frontend flows |
| App screens, wireframes and wording | frontend screens |
| Build plan by day | architecture §14; frontend §18 |
| Rust crates, their dependency rules and conventions | backend README §5–7; architecture §3 |
| A crate's types, traits and modules | backend `<crate>.md` (domain, store, blob, integrations, vision, knowledge, agent, pipeline, api, worker) |
| Server-side flows and state machines | backend flows |
| Weather and trigger thresholds | backend domain §7 (weather thresholds); contracts §7 |
| Open decisions and open questions | architecture §15; contracts §12; database §9; frontend §19; backend README §15 and each crate document's open questions |

## Sources of truth

- **Contracts** are defined in `contracts.md` until code exists. After that, the Rust types in the `domain` crate are the source of truth: `utoipa` generates the OpenAPI spec, `openapi-typescript` generates the app's types, and `schemars` generates JSON Schema for fixed shapes. A contract change updates the Rust type and `contracts.md` in the same pull request.
- **Stored shapes** are defined in `docs/database/schema.surql`. Change it in the same pull request as any contract change that affects what is stored.
- **The architecture document follows the contracts.** When `contracts.md` changes, bring `docs/architecture/README.md` in line, bump both status lines, and add a row to contracts §11 listing the affected architecture sections.
- **Version pins** (SurrealDB `v3.3.4`, RustFS `1.0.1`, both with image digests) were verified by running them. Don't change a pin without re-verifying the patterns that depend on it.
- **App pins** (Expo SDK 57, `@clerk/expo` 4.x) are in docs/frontend §3. Before moving the SDK, check that `@clerk/expo`'s `peerDependencies` include it.

## Planned repository layout

From architecture §13. None of these directories exist yet.

```
PDM/
├─ app/                Expo app (layout in docs/frontend §6); app/modules/lidar is the native LiDAR module (Swift)
├─ backend/
│  ├─ crates/{domain, store, blob, integrations, agent, vision, knowledge, pipeline}
│  ├─ bins/{api, worker}
│  └─ migrations/*.surql
├─ ml/
│  ├─ training/        classifier fine-tuning + ONNX export, evaluation set
│  └─ knowledge/       ingestion scripts + reviewed seed data
├─ infra/              compose files, Caddyfile, tunnel config
└─ docs/
```

What each crate does is in architecture §3 (Rust crates); how each is built is in [`docs/backend`](docs/backend/README.md).

## Working conventions

### Documents

- Keep the status line current: bump the version, set the date, and prepend a one-sentence summary of what changed.
- In the database and storage documents, mark a claim ✓ only if you ran it against the pinned version. Mark anything taken from vendor docs, source or issues as *unverified*.
- Write plainly: short sentences, metric units, concrete values. Match the existing tone and structure rather than introducing new formats.
- Relative links between documents must keep working on GitHub.

### Scope

The team builds with AI assistance and has the capacity for the full scope. Don't propose cutting or deferring features to fit the timeline; flag a design as over-engineered only when it adds risk or complexity without benefit.

### Commits

- Conventional Commits with a scope naming the area: `docs(architecture,database,storage): …`, `feat(api): …`, `fix(store): …`.
- Imperative subject; a body that explains what changed and why for anything non-trivial.
- Commits are signed (SSH signing is configured).
- One logical change per commit.

### Secrets

`.env` and `.env.*` are ignored; commit only `.env.example` with placeholder values. Never commit keys, signing material or credentials.
