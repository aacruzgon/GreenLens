# AGENTS.md

Guidance for AI agents working in this repository. Read this first, then open the document that covers your task.

## The project

PDM (GitHub repository: GreenLens) is a plant, crop and garden health monitoring app built for [Build North: Ship Something Real](https://luma.com/build-north). Build week runs Oct 16–25, 2026; submissions are due Oct 25 at 11:59 PM ET and Demo Day is Oct 29 in Toronto.

A user photographs a plant. Computer vision on a home GPU rig assesses the plant, the soil and the surroundings. The results are combined with weather, soil data and the plant's history to produce a diagnosis and an action plan, which the user can chat about.

**Stack:** Expo SDK 57 / React Native app with Clerk authentication (and a Swift LiDAR module), Rust backend (axum, tokio) in a Cargo workspace, SurrealDB 3.3.4, RustFS 1.0.1 for images, pull-based GPU workers (`ort`/CUDA and a vLLM or llama.cpp VLM sidecar), Python for model training and knowledge ingestion.

**Current state:** the repository holds the design documents and the Rust backend workspace in `backend/` ([docs/backend §6](docs/backend/README.md#6-workspace-layout)), from which the crate documents in `docs/backend` are generated, the training, export and evaluation code in `ml/` ([ml/README.md](ml/README.md)), and in `infra/` the rig's VLM server ([infra/README.md](infra/README.md)) and the dev services for a laptop (`infra/dev.compose.yml`, [docs/infra §4](docs/infra/README.md#infradevcomposeyml)). CI runs from `.github/workflows/ci.yml` on two self-hosted runners on the rig ([docs/infra §2](docs/infra/README.md#runners-and-budget)). The Expo app is in `app/` ([app/AGENTS.md](app/AGENTS.md)): slice F0 of the [app roadmap](docs/frontend/roadmap.md), with its layers, the api layer, mock mode, Conservatory's design layer and Clerk sign-in. Slice F5's native work is in `app/modules/lidar` (depth capture, the garden mapping session, the 3D `GardenView`) and `app/modules/live-activity` with its widget extension in `app/targets/scan-activity`; it compiles and waits on the iPhone for its checks ([roadmap 11](docs/frontend/roadmap.md#11-f5-the-iphones-native-work)). Slice F6's chat is in `app/src/features/chat`, on mock data; its answer blocks wait on a backend contract held as a proposal in `app/src/domain/proposed.ts` ([roadmap 12](docs/frontend/roadmap.md#12-f6-chat-with-answer-blocks)).

## Where to find things

All design lives in `docs/`. Each document has a status line at the top with its version, date and a changelog, and a numbered table of contents.

| File | What it is | Read it when |
|---|---|---|
| [`docs/architecture/README.md`](docs/architecture/README.md) | The architecture: goals and judging criteria, system diagram, components and Rust crates, users and authorization, data overview, the analysis pipeline, chat and agent core, main flows, API surface, models and GPU placement, security, deployment, repository layout, build plan and open decisions | You need the big picture, how parts fit together, or why a design choice was made |
| [`docs/architecture/contracts.md`](docs/architecture/contracts.md) | The data contracts: every shape that crosses a boundary (app ↔ API, API ↔ workers, worker ↔ VLM, ingestion ↔ database), with error codes, endpoints, events, job payloads, VLM output and pipeline records | You are writing or calling an endpoint, a worker, a job, an SSE event, a VLM prompt, or any type in the `domain` crate |
| [`docs/database/agent-memory.md`](docs/database/agent-memory.md) | SurrealDB Agent Memory, the hosted memory service that shared memories are mirrored into as beliefs: setup and the narrow API key, behaviour measured on the live service, the design (key mapping, scopes, sweep, reads, deletion), privacy, caveats | You touch `memory`, `Memory.topic`, the `BeliefStore` port, `integrations::agent_memory`, the beliefs endpoint or the `AGENT_MEMORY_*` settings |
| [`docs/database/README.md`](docs/database/README.md) | SurrealDB notes: version pin and setup, conventions, tables, verified write patterns (event `seq`, compare-and-set, job claims, idempotency, deletion order), reads and vector search, the Rust SDK, operations and caveats | You are writing a query, a repository function in the `store` crate, or a migration |
| [`backend/migrations/schema.surql`](backend/migrations/schema.surql) | The database schema itself. Idempotent; the `store` crate embeds it and the API applies it on every start | You need a table's exact fields, types, indexes or references |
| [`docs/frontend/README.md`](docs/frontend/README.md) | The frontend architecture: stack and pins, libraries, layers and import rules, directory layout, Clerk, the API and streaming layers, state, platform adapters (phone and in-app browsers, the LiDAR module's JS interface), UI system, mock mode, testing, builds, build plan, open decisions and sources | You are working on anything in `app/`, or need to know how the app consumes an endpoint or event |
| [`docs/frontend/flows.md`](docs/frontend/flows.md) | Sequence diagrams for every user flow and the app's state machines | You are implementing a flow end to end, or a reducer or state machine in a feature's `model/` |
| [`docs/frontend/screens.md`](docs/frontend/screens.md) | Navigation map, screen inventory with data, actions and roles, wireframes, screen rules, shared components and wording rules | You are building or changing a screen |
| [`docs/frontend/roadmap.md`](docs/frontend/roadmap.md) | The app's build-week work in the Conservatory direction as ten slices (F0–F9), each with its goal, work, owner, dependencies on other tracks, exit checks and the decisions it closes; when each slice goes live against the backend, and what the app needs from other tracks | You plan, pick up or review app work |
| [`docs/design/design.md`](docs/design/design.md) | The app's design direction: principles, what Apple's chrome owns and what we own, five directions compared (the F0 baseline, Herbarium, Seed Packet, Instrument, Conservatory) and the team's choice, Conservatory, the demo screen by screen, status and motion rules, the 3D garden and the insights. Companions in the same folder: clickable mockups ([`mockups/index.html`](docs/design/mockups/index.html)), design philosophies, the plant-app landscape, cross-domain inspiration, in-app analytics and 3D rendering notes | You are choosing how a screen looks, moves or is worded, or building the 3D garden or the insights |
| [`docs/storage/README.md`](docs/storage/README.md) | RustFS notes: version pin and setup, bucket and key layout, how images are served through signed API URLs, writes and deletes, the Rust client (`aws-sdk-s3`), operations, caveats and the fallback | You are touching image or depth-map storage, the `blob` crate, or upload handling |
| [`docs/connectors/README.md`](docs/connectors/README.md) | Farm connectors and Era by Eon: a garden's Slack channels and Google Drive folder synced for the chat and the plan writer, the data, OAuth and sealed tokens, the sync, the `search_farm_records` tool, privacy, and Era's simulators as the test bench in dev, CI, the agent evaluation and the demo, with their measured behaviour | You touch a connector, `farm_record`, the `connectors` scheduler task, the Slack or Drive adapters, the Era compose profiles or CI job, or the agent evaluation |
| [`docs/EON/README.md`](docs/EON/README.md) | The development roadmap for the farm connectors and Era by Eon: phases E0–E11 in build order, each task with what it needs, its tests and its state; the test catalogue; what Era's simulators and the vendors' documents showed on Oct 19; the design gaps found on `main` | You pick up, plan or review connector or Era work, or need the exact inject, fault, point-in-time or OAuth calls of Era's simulators |
| [`docs/setup/README.md`](docs/setup/README.md) | Developer setup: what each team member installs on macOS, Windows and Linux, by role (everyone, app, backend, ML, rig and control plane), with the pinned versions and a check list | You are setting up a machine, or a build fails for a missing tool |
| [`docs/infra/README.md`](docs/infra/README.md) | Infrastructure and CI: the GitHub Actions workflow and required checks, the container images and registry, the dev, control-plane and rig compose files, ingress, secrets, deploys by digest and backups | You touch CI, a Dockerfile, a compose file, an env file, a deploy or a backup |
| [`docs/backend/README.md`](docs/backend/README.md) | The Rust backend design: goals, the crate graph and dependency rules, workspace layout, Rust conventions, runtime composition, configuration, testing, builds, build plan. Links a document per crate (`domain`, `domain-types`, `store`, `blob`, `integrations`, `vision`, `knowledge`, `agent`, `pipeline`, `api`, `worker`) with every struct, enum, trait, generic and lifetime, compiled on Rust 1.99.1 | You write or review any Rust, a `Cargo.toml`, a repository method, an endpoint, a worker or a background task |
| [`docs/backend/flows.md`](docs/backend/flows.md) | Server-side sequence diagrams across crates (request pipeline, uploads, a scan from submit to plan, claims and the sweep, SSE, job leases, chat turns, deletion, the scheduler, start-up and shutdown) and the server's state machines | You implement or debug a flow end to end on the server |
| [`docs/backend/roadmap.md`](docs/backend/roadmap.md) | The backend's build-week work as ten slices (S0–S9), each with its goal, work, dependencies on other tracks, exit checks and the decisions it closes; and the gaps still to design | You plan, pick up or review backend work |
| [`docs/vision/README.md`](docs/vision/README.md) | The vision and ML design: the model inventory, placement and VRAM budgets on the three GPUs, runtimes and pins, model files and versions, licences, open decisions and the proposed contract changes. Companions: [`models.md`](docs/vision/models.md) (every perception and CPU model), [`vlm.md`](docs/vision/vlm.md) (the VLM, its serving, prompts and schemas), [`training.md`](docs/vision/training.md) (datasets, training, ONNX export, label lists, knowledge-ingestion model steps), [`evaluation.md`](docs/vision/evaluation.md) (evaluation set, metrics, calibration, benchmarks) and [`devpost.md`](docs/vision/devpost.md) (the ML part of the Devpost write-up); the plan for the whole ML side, models and backend integration, with owners and phases, is [`dev-roadmap.md`](docs/vision/dev-roadmap.md), and model-by-model progress is in [`roadmap.md`](docs/vision/roadmap.md) | You are choosing, training, exporting, prompting or evaluating a model, setting a threshold or a VRAM budget, or working in `ml/` |

### Topic index

| Topic | Where |
|---|---|
| Wire format, IDs, timestamps, units, confidence bands, regions | contracts §1 |
| Roles and access matrix, idempotency keys | contracts §1; architecture §4 |
| Error codes and response shape | contracts §2 |
| Scan endpoints and the scan state machine | contracts §3; architecture §8.1 |
| Live progress over SSE, event catalogue, resuming | contracts §4 |
| Worker leases, heartbeats, job claim, payloads and results | contracts §5; architecture §8.4 |
| VLM image preparation, per-job schemas, findings and indicator calls | contracts §6; architecture §10; vision vlm.md §5–9 |
| Observations, measurements, weather, diagnoses, action plans, tasks | contracts §7 |
| Pipeline stages, species ID, retrieve-then-verify, soil, organisms | architecture §6.1–6.5 |
| Capture guidance | architecture §6.6 |
| LiDAR depth, measurement and the garden map | architecture §6.7–6.8; contracts §3, §7 (Measurement), §9 (Garden map) |
| Chat, tools, memory, pending actions | contracts §8; architecture §7, §8.2 |
| Gardens, members, invites, plants, tasks, account, deletion cascades | contracts §9; database §4 (Deletion) |
| Forecast alerts | architecture §8.3 |
| Agent Memory: shared memories mirrored as beliefs (supersession, scopes, privacy, deletion) | database [agent-memory.md](docs/database/agent-memory.md); contracts §8 (Beliefs), §9; architecture §7 |
| Farm connectors (Slack, Google Drive), Era by Eon's simulators, the farm fixture and the agent evaluation | [connectors](docs/connectors/README.md); backend roadmap S9; the build order, tasks and tests in [docs/EON](docs/EON/README.md) |
| Knowledge base and seed file format | architecture §5 (Knowledge base); contracts §10 |
| Security, privacy and safety (EXIF, face blurring, dose checks) | architecture §11 |
| Deployment, hosting, app delivery, operations | architecture §12; infra |
| CI, images, compose files, secrets, deploys | infra |
| Discord alerts for GitHub mentions and comments on your issues and PRs: setup, secrets, what is covered | infra [discord-mentions.md](docs/infra/discord-mentions.md) |
| Model files: the manifest, where weights live, how they reach the hosts | vision README §6 |
| Applying the schema, migrations and the generated knowledge file | database §1 |
| Installing the tools on a dev machine, the rig or the control plane | setup |
| App layers, layout, libraries and import rules | frontend §4–6 |
| App auth (Clerk), API client, errors, idempotency, SSE handling | frontend §8–10 |
| App flows and state machines | frontend flows |
| App screens, wireframes and wording | frontend screens |
| Design direction, palettes, type, motion, the mockups | design [design.md](docs/design/design.md), [philosophies.md](docs/design/philosophies.md) |
| In-app insights and charts from captured data | design [analytics.md](docs/design/analytics.md) |
| 3D garden rendering | design [garden-3d.md](docs/design/garden-3d.md) |
| Models, licences, placement and VRAM per GPU | vision README §3–4, §9 |
| Proposed contract, architecture and schema changes from the vision design | vision README §12 |
| VLM choice, serving, prompts, box formats | vision vlm.md |
| Datasets, training, ONNX export, label lists | vision training.md |
| Evaluation set, acceptance targets, calibration, day-1 and day-2 checks | vision evaluation.md |
| Rust crates, their dependency rules and conventions | backend README §5–7; architecture §3 |
| A crate's types, traits and modules | backend `<crate>.md` (domain, store, blob, integrations, vision, knowledge, agent, pipeline, api, worker) |
| Server-side flows and state machines | backend flows |
| Weather and trigger thresholds | backend domain §7 (weather thresholds); contracts §7 |
| Build plan by day | architecture §14; frontend §18; vision README §10 |
| Backend work as slices, their order and exit checks | backend roadmap |
| App work as slices, their order, owners and exit checks | frontend [roadmap.md](docs/frontend/roadmap.md) |
| The ML dev plan: who builds what, hand-offs between `ml/` and the Rust crates, integration gaps, phases to Oct 25 | vision dev-roadmap.md (update a task's state in the PR that finishes it) |
| Model progress: done, next, waiting, open PRs, the rig's state | vision roadmap.md (update it at the end of each ML work session) |
| Open decisions and open questions | architecture §15; contracts §12; connectors §12; database §9; frontend §19; backend README §15 and each crate document's open questions; vision README §11; design §10 |

## Sources of truth

- **Contracts:** the Rust types in the `domain` crate (`backend/crates/domain`) are the source of truth, and `contracts.md` explains them. Where the two disagree, the types win and the document is corrected. A contract change updates the Rust type, its exact-JSON test and `contracts.md` in the same pull request.
- **Generated from the types:** `utoipa` generates the OpenAPI spec, committed as `backend/openapi.json` (an `api` test fails when it falls behind the types; regenerate it with `cargo run -p api -- openapi > openapi.json` in `backend/`). `openapi-typescript` generates the app's types from that file (`npm run gen:api` in `app/`; CI fails when they differ). `schemars` generates JSON Schema for the fixed shapes.
- **Stored shapes** are defined in `backend/migrations/schema.surql`. Change it in the same pull request as any contract change that affects what is stored.
- **The architecture document follows the contracts.** When `contracts.md` changes, bring `docs/architecture/README.md` in line, bump both status lines, and add a row to contracts §11 listing the affected architecture sections.
- **Model files:** [`ml/models/FORMATS.md`](ml/models/FORMATS.md) defines the head and label-matrix files and the `label_set_version` recipe. `ml/` writes them and `vision` and `store` read them, so a format change updates both sides and `FORMATS.md` in the same pull request.
- **Copies of `ml/` inside `backend/`:** images build from `backend/` alone, so `store` keeps two copies from `ml/`: the BioCLIP revision (`BIOCLIP_MODEL` in `backend/crates/store/src/repo/knowledge.rs`) and the 80 prompt templates (`backend/crates/store/src/repo/bioclip_templates.json`, copied from `ml/models/templates/openai-imagenet-80.json`). When the BioCLIP export or the templates change, update both copies in the same pull request; the test `the_embedded_templates_and_model_match_ml` fails until you do.
- **Version pins** (SurrealDB `v3.3.4`, RustFS `1.0.1`, both with image digests) were verified by running them. Don't change a pin without re-verifying the patterns that depend on it.
- **App pins** (Expo SDK 57, `@clerk/expo` 4.x) are in docs/frontend §3. Before moving the SDK, check that `@clerk/expo`'s `peerDependencies` include it.

## Repository layout

From architecture §13. `backend/` exists, with `tools/` beside its crates for the `docs/backend` generator and `api`'s test-store generator; `ml/` holds the training and export code; `infra/` holds the rig's VLM server so far; `app/` holds the Expo app with its local native modules.

```
PDM/
├─ app/                Expo app (layout in docs/frontend §6); app/modules/lidar is the native LiDAR module (Swift)
├─ backend/
│  ├─ crates/{domain, store, blob, integrations, agent, vision, knowledge, pipeline}
│  ├─ bins/{api, worker}
│  ├─ migrations/*.surql
│  └─ tests/fixtures/era/  the Era simulators' farm fixture (docs/EON)
├─ ml/
│  ├─ pdm_vision/      shared pre-processing for training and evaluation
│  ├─ models/          the model manifest (manifest.toml) and licence notices
│  ├─ training/        disease head and detector training + ONNX export; eval/ with the field-photo evaluation set
│  └─ knowledge/       ingestion scripts + reviewed seed data
├─ infra/              compose files, Caddyfile, tunnel config
└─ docs/
   ├─ architecture/    the architecture (README.md) and the data contracts (contracts.md)
   ├─ backend/         the Rust backend, crate by crate, down to its types and traits
   ├─ vision/          the computer-vision models, training and evaluation
   ├─ frontend/        the app's architecture (README.md), flows (flows.md) and screens (screens.md)
   ├─ design/          the design direction (design.md), its research and the clickable mockups
   ├─ database/        SurrealDB notes (the schema itself is backend/migrations/schema.surql)
   ├─ storage/         RustFS setup and notes
   ├─ connectors/      farm connectors (Slack, Google Drive) and Era by Eon as their test bench
   ├─ setup/           what each team member installs, per OS
   ├─ infra/           CI, images, compose files, secrets and deploys
   └─ site/            the landing page (index.html), published as the README's interactive overview
```

What each crate does is in architecture §3 (Rust crates); how each is built is in [`docs/backend`](docs/backend/README.md).

## Working conventions

### Documents

- The crate documents in `docs/backend` (all but `README.md`, `flows.md` and `roadmap.md`) are generated. Change the source or the template in `backend/tools/docgen/<doc>.md.tpl`, run `backend/tools/docgen/regen_all.sh`, and commit the documents with the change; don't edit a generated document by hand.
- Keep the status line current: bump the version, set the date, and prepend a one-sentence summary of what changed.
- In the database and storage documents, mark a claim ✓ only if you ran it against the pinned version. Mark anything taken from vendor docs, source or issues as *unverified*.
- In the vision documents, ✓ marks a fact read in its primary source on the date given, and ✓ ran a check that was executed; nothing there has run on the rig's GPUs until a day-1 check or day-2 benchmark records it.
- Write plainly: short sentences, metric units, concrete values. Match the existing tone and structure rather than introducing new formats.
- Relative links between documents must keep working on GitHub.

### Scope

The team builds with AI assistance and has the capacity for the full scope. Don't propose cutting or deferring features to fit the timeline; flag a design as over-engineered only when it adds risk or complexity without benefit.

### Commits

- Conventional Commits with a scope naming the area: `docs(architecture,database,storage): …`, `feat(api): …`, `fix(store): …`.
- Imperative subject; a body that explains what changed and why for anything non-trivial.
- Commits are signed (SSH signing; [setup 2](docs/setup/README.md#2-everyone)). GitHub refuses an unverified commit on any branch.
- One logical change per commit.

### Branches and pull requests

GitHub rulesets protect `main` (and `master`), the same as on the team's hfs-Cassandra fork:

- **No direct pushes to `main`.** Work on a branch named for the change (`docs/…`, `feat/…`, `fix/…`, `chore/…`), push it, and open a pull request.
- **A pull request needs 1 approving review,** and every review thread resolved. Pushing new commits dismisses earlier approvals, and a change pushed by someone other than its author needs an extra approval.
- **No force-pushes to `main`, and it can't be deleted.** Merge, squash and rebase merges are allowed.
- **Admins can bypass the review only through a pull request,** never by pushing to `main`.
- **One required check, `ci`,** the last job of `.github/workflows/ci.yml` ([docs/infra §2](docs/infra/README.md#required-checks)). It fails if any job in its `needs` failed or was cancelled. That is every check before it, `live` included. Add a new check to `ci`'s `needs` in the workflow, not to the ruleset.
- **Agents** commit and push only when the user asks. They push branches, never `main`, and open the pull request with `gh pr create`.

### Secrets

`.env` and `.env.*` are ignored; commit only `.env.example` with placeholder values. Never commit keys, signing material or credentials.
