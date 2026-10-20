# PDM Infrastructure and CI

> **Status:** Draft v0.10.17, 2026-10-20. v0.10.17: `era` is in `ci`'s and `images`' `needs` after it passed on five consecutive pushes to `main` (#69, #79, #78, #71, #76), taking 3.5 to 6.3 minutes (section 2). v0.10.16: `.env.api` gains the optional `ERA_DEMO_GARDEN_ID`, the demo garden that start-up seeding gives the two `era` connectors, set once E9.0 has created the garden (section 6). v0.10.15: the Discord mention alerts run on the light runner label too, so their webhook URLs join what pull-request code can reach (section 2). v0.10.14: a third runner, `rig-3`, takes the light jobs (`changes`, `docs`, `infra`, `security`, `ci`, `release`) through `LIGHT_RUNNER_LABELS` and the label `pdm-light`, which `rig-1` and `rig-2` carry too, so those jobs no longer queue behind Rust builds (section 2). v0.10.13: the `era` job runs on the rig's runners like every other job, and its six simulators take ephemeral loopback ports read back from `job.services`, so two `era` runs on `rig-1` and `rig-2` can't collide (section 2). v0.10.12: the `era` job exists in `ci.yml` (E6.1, E6.2, E6.4), with three pairs of Era's simulators as service containers (open mode, faults only, `AUTH_MODE=strict`); it is not yet in `ci`'s `needs` (section 2). v0.10.11: the first run on the rig (pull request #77) confirmed that self-hosted jobs run with the budget exhausted, that `ci` still reports as app 15368, and that the live job's ephemeral ports work under rootless Docker. v0.10.10: CI runs on two self-hosted runners, `rig-1` and `rig-2`, on the 3070 host, because the account's Actions budget ran out; `RUNNER_LABELS` picks the runners, steps that need `sudo` or the Actions cache run only on GitHub's runners, and the rig keeps its build trees and its image-layer cache between jobs (sections 1–3, 6, 8–10; host setup in [setup 6](../setup/README.md#ci-runner)). v0.10.9: `CONNECTOR_RETURN_URL` must be `https` or a custom app scheme, with no query, and the dev example leaves the Era bases commented out until `CONNECTOR_KEY` is set (section 6). v0.10.8: `.env.api` lists the farm connectors' optional settings: `CONNECTOR_KEY` (a secret, 64 hex characters), the Slack and Google OAuth clients (each pair together or not at all), `CONNECTOR_RETURN_URL`, and `ERA_SLACK_BASE` and `ERA_GOOGLE_BASE` for the demo garden's seeding; the `era-seed` service's `SIMCORE_ADMIN_TOKEN` comes with E9 in `.env.era-seed`, and rotating `CONNECTOR_KEY` means every owner reconnects (section 6). v0.10.7: section 2 says `backend/.config/nextest.toml` runs every `era_` test in one serial group (`era`, max-threads 1), and that the `era` CI job comes with E6. v0.10.6: `infra/dev.compose.yml` has an `era` profile with Era by Eon's Slack and Google Drive simulators for the farm connectors (section 4), and `backend/.dockerignore` leaves out `tests/`, where their fixtures live (section 1's tree). v0.10.5: `changes` also sends `ml/knowledge/schema/` to the Rust jobs, since `domain`'s tests compare it with the seed types (section 2). v0.10.4: the link to the roadmap's gaps follows its renumbering to section 13. v0.10.3: `.env.api` lists the optional `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and `AGENT_MEMORY_API_KEY` (a secret) (section 6). v0.10.2: `discord-mentions.yml` also alerts the people an issue's or pull request's description mentions (section 1's tree). v0.10.1: `discord-mentions.yml` also alerts the author of the issue or pull request a new comment is on (section 1's tree). v0.10: `discord-mentions.yml` posts a Discord alert to a team member's channel when a comment mentions them, with its setup in [discord-mentions.md](discord-mentions.md); the `infra` job runs its script's tests, and section 6 lists its secrets and variables. v0.9: the control plane runs on the rig behind Tailscale Funnel, and the ingress provider is a compose profile (`tailscale`, `cloudflare`, or none on a VM), so switching providers changes two env values and no code (sections 1 and 4–7); a Funnel test on the 3070 host checked the certificate, an unbuffered SSE stream and upload speed (section 5). v0.8: `live` is in `ci`'s `needs` after four green pushes to `main` (item 9); the first `images` run on `main` pushed all three images in 32 minutes, cold (item 6). v0.7: S0's work items 2–5 are checked off; the `main` ruleset requires `ci`; open question 8 records that the hackathon needs no tag ruleset. v0.6: `backend.yml` becomes `ci.yml`, in stages after the team's Helios CI: `security` (cargo-deny) waits for `lint`, `live` waits for `lint` and `test`, one required check `ci` gates the merge, and on `main` and `v*` tags `images` builds and pushes the images, with `release` on tags. Section 2 records the disk measured on #23. Its audit added: a tag must be on `main` and a plain version, a tag reuses `main`'s images and digests, and the concurrency group includes the ref. v0.5: CI builds with line tables only (`CARGO_PROFILE_DEV_DEBUG`), `live` frees disk before it starts, and `live` and `examples` print the disk left at the end, after `live`'s first cache save on `main` ran the runner's disk out. Section 2 records the cache sizes measured on `main`. v0.4: `infra/dev.compose.yml` and `infra/env/.env.example` exist, and section 4 records the first run. Its changes from v0.1: `surrealdb-volume` gives the named volume to SurrealDB's user (database 1 had missed it), `surrealdb-init` creates the `api` user, and `rustfs-init` uses `rc bucket create`. Section 6 says how `infra/env/.env.example` differs from the rig's `infra/.env.example`. Its audit added: the local values moved to `infra/dev/.env.example`, with the worker's variables, because keys repeat across the catalogue's sections; `surrealdb-volume` hardened to `CHOWN` only, with no network. v0.3: `backend/Dockerfile`, `docker-bake.hcl` and `.dockerignore` exist and section 3 records the first build. `api` gains its own `heic` feature, which answers open question 7, and the full SHA-256 of both ONNX Runtime archives is recorded. Its audit added: the label arguments after the cached steps, the rustup components installed once, the build files out of the context, and GID 10100. v0.2: `backend.yml` and `dependabot.yml` exist; the `test` job adds the doc tests nextest skips, the `examples` job runs only when the Rust skills change and on pushes to `main`, every Rust job installs 1.99.1 and sets `RUSTUP_TOOLCHAIN`, `checklinks.py` and `check_fences.py` now exit non-zero, and open question 1 is answered. Its audit added: no `--health-cmd` for distroless SurrealDB; `ml/`'s manifest and templates trigger `test`; the examples cache keyed on `versions.md`; `changes` as a required check; one concurrency group per commit on `main`; grouped Dependabot updates. v0.1.2: section 4's vLLM health check is verified on the rig, and the running `vllm` service is mapped to `vllm-nv`. v0.1.1: `api`'s bucket default is now `pdm-images`, so section 4's note no longer flags it. v0.1: first design of the CI workflows, the container images, the three compose files, ingress, secrets, deploys by digest, operations hooks and the infrastructure work items of the [backend roadmap](../backend/roadmap.md). Design only: nothing here exists yet.

This document turns [architecture 12](../architecture/README.md#12-deployment) and [backend 12](../backend/README.md#12-builds-and-deployment) into files: what goes in `.github/` and `infra/`, how each image is built and pinned, and how the control plane and the rig are started, deployed and rolled back. It closes [roadmap gap 3](../backend/roadmap.md#13-gaps-found-while-writing-this-roadmap).

**Marks.** ✓ marks a fact read in its primary source on 2026-10-16: a GitHub release or the GitHub API for actions and ONNX Runtime, Docker Hub for images, the vendor's documentation otherwise. Nothing in this document has been run, except where a line says ✓ run: the runner host's setup on 2026-10-19. *Unverified* marks anything else.

## Contents

1. [Scope and layout](#1-scope-and-layout)
2. [CI](#2-ci)
3. [Images](#3-images)
4. [Compose files](#4-compose-files)
5. [Ingress](#5-ingress)
6. [Secrets and configuration](#6-secrets-and-configuration)
7. [Deploying and rolling back](#7-deploying-and-rolling-back)
8. [Operations hooks](#8-operations-hooks)
9. [Roadmap](#9-roadmap)
10. [Open questions](#10-open-questions)
11. [Sources](#11-sources)

---

## 1. Scope and layout

**In scope:** CI on GitHub Actions, the backend's container images and their registry, the compose files for local work, the control plane and the rig, Caddy and the ingress sidecars, env files, deploys, backups, logs and health checks.

**Out of scope, owned elsewhere:**
- How model files reach `/models` on the control plane and the rig: [vision 6](../vision/README.md#6-model-files-versions-and-pinning). This document only mounts the directory read-only.
- How `api` applies the schema and the generated knowledge migrations at start, after `seed-upload`: [database 1](../database/README.md#1-version-and-setup). This document only orders the containers.
- The VLM sidecars' flags and the worker settings per card: [worker 11–12](../backend/worker.md#11-multi-vendor-gpus).
- The app's builds (Expo, EAS): [frontend](../frontend/README.md).

**Where the control plane runs** ([architecture open decision 2](../architecture/README.md#15-open-decisions), settled for [S1](../backend/roadmap.md#5-s1-walking-skeleton) on 2026-10-16): on the rig host with the RTX 3070, behind Tailscale Funnel, which gives a public HTTPS address without a domain or an open port. The ingress provider is a compose profile, so the same files also run behind a Cloudflare Tunnel (which needs a domain on Cloudflare) or on a cloud VM with Caddy on ports 80 and 443. Switching changes `COMPOSE_PROFILES`, `CADDY_SITE` and `API_PUBLIC_BASE`, never the code or the images ([section 5](#5-ingress)). The RTX 5070 Ti is in a second rig host, which runs only workers and reaches `api` through the public URL; how `rig.compose.yml` splits across the two hosts is settled when [S3](../backend/roadmap.md#7-s3-vlm-workers-and-the-fallback) sets up that host.

```
PDM/
├─ .github/
│  ├─ workflows/
│  │  ├─ ci.yml                checks in stages, one required check `ci`, images on main and tags, releases on tags (sections 2–3)
│  │  └─ discord-mentions.yml  a Discord alert per person a comment or description mentions, and to the author a comment is on (discord-mentions.md)
│  ├─ scripts/discord_mentions.py  its matching and delivery, with test_discord_mentions.py
│  ├─ actions/free-disk/       a composite action that removes unused preinstalled toolchains (GitHub's runners only)
│  ├─ actions/self-hosted-env/ a composite action that points Cargo at a build tree that outlives the job (the rig's runners only)
│  └─ dependabot.yml           daily updates of the pinned action SHAs, nothing else
├─ backend/
│  ├─ Dockerfile               multi-stage: api, worker, worker-cuda (section 3)
│  ├─ docker-bake.hcl          the three targets, built in one BuildKit run
│  └─ .dockerignore            target/, tools/, tests/, .git
├─ infra/
│  ├─ ci-runner/               the CI runners' systemd unit, job-started hook, weekly prune and installer (sections 2 and 8, setup 6)
│  ├─ dev.compose.yml          SurrealDB and RustFS for a laptop and for the ignored tests
│  ├─ dev/surrealdb-users.surql the `api` user dev.compose.yml imports (throwaway password)
│  ├─ dev/.env.example         the local environment for api and the fallback worker against dev.compose.yml
│  ├─ .env.example             the rig's vLLM template today, copied to infra/.env (infra/README.md)
│  ├─ control-plane.compose.yml caddy · api · surrealdb · rustfs · fallback-worker · seed-upload · tailscale or cloudflared (profiles)
│  ├─ rig.compose.yml          perception-worker · vllm-nv · vlm-worker-nv · vllm-amd · vlm-worker-amd
│  ├─ rig-amd.compose.yml      only if the RX 9070 XT sits in a second host
│  ├─ caddy/Caddyfile
│  ├─ ingress/tailscale/serve.json  Funnel's forwarding to caddy:80; the auth key is a secret
│  ├─ ingress/cloudflare/config.yml the tunnel's ingress rules; its credentials are a secret
│  ├─ env/.env.example         every variable of every env file, placeholders only (section 6)
│  ├─ bootstrap/
│  │  ├─ surrealdb-users.sh    namespace, database and the `api` user (section 7)
│  │  ├─ rustfs.sh             bucket, policies and identities (storage 2's script)
│  │  └─ pdm-api.json, pdm-seed.json, pdm-backup.json   the three RustFS policies
│  ├─ deploy.sh                pins a service to a digest, records it, restarts it (section 7)
│  └─ ops/
│     ├─ backup.sh             nightly SurrealDB export and RustFS copy (section 8)
│     └─ pdm-backup.service  pdm-backup.timer
└─ ml/knowledge/Dockerfile     the seed-upload image; written by the knowledge track
```

Hosts keep their state in `/srv/pdm/`: `env/` (secrets, mode 0700), `models/` (vision 6), `backups/` and `deploy.log`. The repository is checked out at `/srv/pdm/repo` for the compose files and scripts only.

---

## 2. CI

### Runners and budget

**Since 2026-10-19, CI runs on two self-hosted runners on the rig's 3070 host.** The account's Actions budget ran out that day, and a hosted job doesn't start without budget, so `ci` would never report. The host is Debian 13 with 16 AMD threads, 15 GB of RAM plus 16 GB of swap, and 251 GB of free disk. The runners are `rig-1` and `rig-2`, both labelled `self-hosted`, `linux`, `x64`, `pdm` and `pdm-light`, and `rig-3`, labelled `pdm-light` but not `pdm`; the host's setup is in [setup 6](../setup/README.md#ci-runner) (✓ run on 2026-10-19; both showed `online` in `gh api repos/aacruzgon/GreenLens/actions/runners`).

- **Choosing the runners.** The jobs that build (`lint`, `test`, `examples`, `app`, `live`, `era`, `images`) have `runs-on: ${{ fromJSON(vars.RUNNER_LABELS || '["ubuntu-26.04"]') }}`. The repository variable `RUNNER_LABELS` is `["self-hosted","linux","x64","pdm"]` (set 2026-10-19), so they run on `rig-1` or `rig-2`. Deleting the variable sends them back to GitHub's `ubuntu-26.04`, the hosted fallback below.
- **Light jobs.** `changes`, `docs`, `infra`, `security`, `ci` and `release` each take seconds (`docs` 12–20 s, `infra` 6–7 s, `ci` 3–4 s), but on 2026-10-19 they waited up to 8 minutes for a runner behind other pull requests' Rust builds. They have `runs-on: ${{ fromJSON(vars.LIGHT_RUNNER_LABELS || vars.RUNNER_LABELS || '["ubuntu-26.04"]') }}`, and `LIGHT_RUNNER_LABELS` is `["self-hosted","linux","x64","pdm-light"]`. `rig-3` carries only `pdm-light`, so it never takes a build; `rig-1` and `rig-2` carry it too, so a light job also runs on whichever of them is idle. GitHub can't fall back from one label set to another, and this way a stopped `rig-3` holds nothing up. A docs-only pull request finishes in about a minute; a Rust pull request's `ci` still waits for its builds. To return to GitHub's runners, delete both variables: all jobs must move together, because a hosted `ci` can't start while the budget is out.
- **Guards.** A step that needs `sudo` or the Actions cache runs only when `runner.environment == 'github-hosted'`: the `libheif-dev` install in `lint` (the rig has libheif-dev 1.23.4), `free-disk` (guarded inside the action), every `Swatinem/rust-cache`, `setup-python` 3.12 in `examples` (the rig uses Debian's `python3` 3.13) and `setup-buildx-action` in `images`.
- **Persistent build trees.** [`.github/actions/self-hosted-env`](../../.github/actions/self-hosted-env/action.yml) runs in `lint`, `test`, `examples` and `live`. On a self-hosted runner it sets `CARGO_TARGET_DIR=~/ci/target/<runner>/<job>/backend`, `PDM_RUST_SKILLS_TARGET_DIR=…/skills` and `CARGO_INCREMENTAL=0`, and touches the directory, so its modification time is its last use. A run starts warm without the Actions cache. The weekly prune ([section 8](#8-operations-hooks)) removes a tree unused for 14 days.
- **Two builds at a time.** Each runner takes one job. `rig-1` and `rig-2` have `CARGO_BUILD_JOBS=6` and `NEXTEST_TEST_THREADS=8`. Two builds fit in 15 GB of RAM; three don't, since linking a test binary takes 1–2 GB. `rig-3`'s light jobs need a few hundred MB, and its `.env` sets `CARGO_BUILD_JOBS=2` and `NEXTEST_TEST_THREADS=2` in case one ever compiles.
- **Isolation.** The runners run as the user `ci-runner`, which is not in `sudo` or `docker` (the `docker` group is root on the host, and pull-request code runs here). They use their own rootless Docker daemon, so CI has its own images and containers. A systemd slice limits the user to 9 GB (`MemoryHigh`) and 11 GB (`MemoryMax`) of RAM, so the developer's desktop and builds survive. `/home/acruzgon` is mode 700, so CI can't read the developer's files or the rig's env files. Only collaborators can open a pull request on the private repository, and the rig runs their code. Keep secrets and SSH keys out of `ci-runner`'s home. Runner groups don't exist for a personal repository.
- **What pull-request code can still reach** (from the review of 2026-10-19). Every job, a pull request's and `main`'s, runs as the same `ci-runner`, and rootless Docker's container root is that user. So a pull request's code can change what later jobs run: the buildx builder's cache that `images` builds from, the tools in `~/.cargo/bin` and `~/.docker/cli-plugins`, the runners' own files and their registration credentials in `~/runner-N/.credentials*`. While `images` runs on the other runner, it can also read the job's `packages: write` login in `~/.docker/config.json`. Every checkout sets `persist-credentials: false`, so no token is left in `.git/config`. Cargo's build scripts and tests run on the host's network, so they can reach the developer's loopback services (the dev SurrealDB on 18000, RustFS on 19000) and the tailnet. The [Discord mention alerts](discord-mentions.md#4-security) run on the same runners, so the four webhook URLs are exposed the same way; each can only post to its channel. The repository is private with four collaborators, and the actions are pinned by commit, so the team accepts this for build week. Two hardening steps are proposed in [open question 9](#10-open-questions).
- **Unchanged.** The `changes` skips, `cancel-in-progress` and the stage order. "Save caches only from `main`" now matters only on GitHub's runners.

**Billing.** GitHub's Actions billing page says that using self-hosted runners is free (✓ read 2026-10-19). A fee of $0.002 a minute for self-hosted runners in private repositories was announced in December 2025 for 1 March 2026. It was postponed, and no 2026 announcement revives it (✓ read 2026-10-19: GitHub's 2026 pricing changes page). ✓ Self-hosted jobs run while the account's budget is exhausted: pull request #77's run (2026-10-19) ran every job on `rig-1` and `rig-2` and passed, while a job on GitHub's runners in the same minute failed without starting. Cold, with empty build trees, the run took 15 minutes from `changes` to `ci`: `lint` 4, `test` 4, `examples` 4, `live` 5, `app` 2.

**The hosted fallback.** `aacruzgon/GreenLens` is a private repository. GitHub's standard runner for a private repository has **2 vCPUs, 8 GB of RAM and 14 GB of SSD** (✓ runner reference), half a public repository's CPUs. A private repository's Actions time counts against the owner's plan: 2,000 minutes a month on Free, 3,000 on Pro, and 10 GB of cache per repository (✓ Actions billing).

At about 25 runner-minutes for a backend push that runs every job, 120 such pushes use the Pro allowance. Build week exceeded it, which is why the runners exist. The workflow still keeps the habits that made hosted runs affordable, and they now save the rig's time:
- runs a cheap `changes` job first and skips each job whose inputs didn't change. A job skipped by its `if:` reports success, so it can be a required check; a workflow skipped by a `paths:` filter stays pending and blocks the merge (✓ workflow syntax);
- cancels a pull request's running workflow when a new commit arrives;
- runs the `live` job only when `store`, `blob`, `backend/migrations/` or the Rust skills change, and on every push to `main`;
- saves caches only from `main` on GitHub's runners, so pull requests read them and the 10 GB stays shared;
- builds images only after a merge, never on pull requests.

### Actions, pinned by commit

| Action | Version | Commit |
|---|---|---|
| `actions/checkout` | v7.0.1 (2026-07-20) | `3d3c42e5aac5ba805825da76410c181273ba90b1` ✓ |
| `actions/setup-python` | v7.0.0 (2026-07-20) | `5fda3b95a4ea91299a34e894583c3862153e4b97` ✓ |
| `actions/setup-node` | v7.1.0 (2026-10-08) | `949feb2413d6458794dcd2491c4babbbce0c15c1` ✓ |
| `Swatinem/rust-cache` | v2.9.2 (2026-08-06) | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` ✓ |
| `taiki-e/install-action` | v2.87.29 (2026-10-16) | `8cca0918f8471c33515202b73f56ab27f2bed283` ✓ |
| `docker/setup-buildx-action` | v4.4.1 (2026-09-16) | `f87e5991a6d7451dcb8d9637bfbc97413f497069` ✓ |
| `docker/login-action` | v4.6.0 (2026-07-29) | `dbcb813823bdd20940b903addbd779551569679f` ✓ |

`setup-python`, `rust-cache` and `setup-buildx-action` run only on GitHub's runners; the rig's jobs skip them. Each is the latest release on 2026-10-16, and each commit is the one its tag points to (`gh api repos/<owner>/<repo>/commits/<tag>`). Workflows reference the full SHA with the version in a comment, so a moved tag changes nothing. Dependabot checks daily and groups whatever it finds into one pull request, because a change to `ci.yml` runs every job; a day with no new release opens none. No other actions are used: images are built with `docker buildx bake` directly ([section 3](#3-images)).

Rust needs no setup action: the runner image has rustup, and each Rust job installs 1.99.1 itself (see the toolchain paragraph below). The rig has rustup too, installed for `ci-runner` ([setup 6](../setup/README.md#ci-runner)). The hosted fallback is `ubuntu-26.04` (✓ a listed label), because its `libheif-dev` is 1.21.2 ([setup 4](../setup/README.md#4-backend-rust)) and the `heic` feature needs 1.19 or newer; Ubuntu 24.04 ships 1.17.6. The rig's Debian 13 ships 1.23.4.

### `ci.yml`

**Triggers:** `pull_request` to `main`, `push` to `main` and to `v*` tags, and `workflow_dispatch`. **Permissions:** `contents: read`. **Concurrency:** one group per pull request, `cancel-in-progress` on pull requests only; on `main` and tags, one group per ref and commit, so a burst of merges, or a tag on a commit `main` is still building, never cancels a waiting run. Every job sets `working-directory: backend` where it runs cargo, and passes `--locked`, so a stale `Cargo.lock` fails.

**Stages,** after the layout of the team's Helios CI. Cheap checks run first, and the expensive ones wait for them, so a broken pull request stops early and spends fewer runner minutes:

| Stage | Jobs | Waits for |
|---|---|---|
| 1 | `changes`, then `lint`, `test`, `examples`, `docs`, `infra`, `app` | `changes` |
| 2 | `security` | `lint` |
| 2 | `live` | `lint` and `test` |
| 2 | `era` | `lint` and `test` |
| 3 | `ci`, the one required check | every job of stages 1 and 2 |
| 4 | `images` (pushes to `main` that change the image's inputs, and `v*` tags) | every check, `live` and `era` included |
| 4 | `release` (`v*` tags only) | `images` |

A job in stage 2 or 4 has `if: !cancelled() && !failure() && …`: it runs when a job it waits for was skipped for having no changes, and doesn't run when one failed. `examples` doesn't wait for `lint`, because it compiles against `versions.md`, not the workspace. A green pull request that changes the workspace takes about `test` plus `live` instead of the longest single job.

| Job | Runs when | Steps | Warm (estimate) / cold (✓ measured) |
|---|---|---|---|
| `changes` | always | `git diff --name-only` against the base; outputs `rust`, `docs`, `skills`, `infra`, `live`, `era` as booleans | under 1 min ✓ |
| `lint` | `rust` | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo check -p worker --features cuda`; `cargo check -p vision --features cuda,migraphx,webgpu`; `cargo check -p vision --features heic` (installs `libheif-dev` on GitHub's runners; the rig has it) | 5 / 7 min |
| `test` | `rust` | install cargo-nextest 0.9.148 with `taiki-e/install-action` (`tool: cargo-nextest@0.9.148`); `cargo nextest run --workspace --locked`. This includes the `api` test that fails when `backend/openapi.json` is stale. Then `cargo test --doc --workspace --locked`, because nextest doesn't run doc tests and the compile-fail tests of the proofs and URL states are doc tests ([backend 11](../backend/README.md#11-testing-and-tooling)) | 7 / 12 min |
| `examples` | the Rust skills changed, or a push to `main` | `setup-python` 3.12 on GitHub's runners, Debian's `python3` 3.13 on the rig; `python3 .claude/skills/rust/scripts/check_examples.py`. The examples compile against `versions.md` and a copy of `backend/migrations/schema.surql` in a crate of their own, not against the workspace crates, so only the skills and the schema can break them | 3 / 13 min |
| `docs` | `rust` or `docs` (any `.md` file, or the generator) | `backend/tools/docgen/regen_all.sh` then `git diff --exit-code docs/backend`; `check_fences.py docs/backend`; `checklinks.py` over `AGENTS.md`, `CLAUDE.md` and `docs/**/*.md`. Both checkers exit 1 on a broken link or an unbalanced listing; a listing rustfmt can't parse is reported, not failed | under 1 min ✓ |
| `app` | `app` (`app/` or `backend/openapi.json`) | Node 22 with `setup-node` and the npm cache; in `app/`: `npm ci`; `npm run gen:api` then `git diff --exit-code src/api/generated/schema.ts`; `npm run typecheck`; `npm run lint`; `npm test -- --ci`; `npm run check:contrast`; `npx expo install --check` | not yet measured |
| `live` | `live`, or a push to `main` | service containers below, on ephemeral loopback ports; `cargo nextest run -p store -p blob --run-ignored only --locked`; `check_examples.py --live --no-test`, since `examples` already runs the examples' own tests | 5 / 15 min |
| `era` | `era`, or a push to `main`; after `lint` and `test` | six service containers, three pairs of Era's Slack and Drive simulators ([below](#the-era-job)); then three `cargo nextest run -p integrations -p api --run-ignored only --locked` steps, one per pair | 3.5–6.3 min on the rig ✓ |
| `infra` | `infra` | copy `infra/.env.example` to `infra/.env`, as [infra/README.md](../../infra/README.md) tells a host to, then `docker compose -f <file> config -q` for each compose file; `bash -n` and `shellcheck` on `infra/**/*.sh`; the tests of `.github/scripts/` ([discord-mentions 5](discord-mentions.md#5-testing)). `caddy validate` in the pinned Caddy image joins it with the Caddyfile in S1 | under 1 min ✓ |
| `security` | `rust`, or a push to `main`; after `lint` | `cargo deny --locked check advisories` with cargo-deny 0.20.2, against [`backend/deny.toml`](../../backend/deny.toml). It checks what ships (`exclude-dev`): SurrealDB's embedded engine, a dev-dependency, brings `bincode`, `quick-xml` (RUSTSEC-2026-0194, -0195) and `rsa` (RUSTSEC-2023-0071), and none reaches a binary. One advisory is accepted: `paste` (RUSTSEC-2024-0436) is unmaintained, not vulnerable, and comes through `tokenizers`. ✓ Passes on the workspace, 2026-10-16 | about 1 min |
| `ci` | always, after stages 1 and 2 | fails if any job it waits for failed or was cancelled; passes if each succeeded or was skipped | seconds |
| `images` | see stage 4 | builds and pushes the three images ([section 3](#registry-tags-and-who-pushes)), and writes their digests to the run summary. On the rig it uses the persistent builder `pdm-ci` | 32 min cold ✓ (a hosted runner) |
| `release` | `v*` tags | `gh release create` with the image digests above GitHub's generated notes. Nothing deploys from CI ([section 7](#7-deploying-and-rolling-back)) | seconds |

**The `era` test group.** [`backend/.config/nextest.toml`](../../backend/.config/nextest.toml) defines the nextest group `era` with `max-threads = 1` and puts every test whose name starts with `era_` in it. Faults are global to an Era simulator, so those tests run one at a time. They are `#[ignore]`d, so `test` skips them. The `era` CI job that runs them against the simulators is described next.

### The era job

**The `era` job** (roadmap [E6.1, E6.2 and E6.4](../EON/README.md#e6-ci)) starts Era's Slack and Google Drive simulators as GitHub service containers, the same images as the `era` profile of `infra/dev.compose.yml`, pinned by tag and digest, with `SIMCORE_SEED=42`, `SIMCORE_TODAY=2026-10-15` and `SIMCORE_FIDELITY=high`. There are three pairs, because a fault is global to a simulator and strict mode changes how it signs in:

| Pair | Services (Slack / Drive) | `AUTH_MODE` | Step and filter | Variables |
|---|---|---|---|---|
| Main | `era-slack` / `era-drive` | `open` | `era_` tests except the two kinds below | `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` |
| Faults | `era-slack-fault` / `era-drive-fault` | `open` | `era_fault_` tests | the same two variables, pointed at this pair |
| Strict | `era-slack-strict` / `era-drive-strict` | `strict` | `era_strict_` tests, the clients' OAuth exchanges | `ERA_STRICT_SLACK_BASE`, `ERA_STRICT_GOOGLE_BASE` |

- **One job, three steps.** The steps share one build and one runner's minutes, and the pairs are separate containers, so a fault set in the second step can't reach a test in the first. The second and third steps run even when an earlier one failed. Separate jobs would each rebuild the tests for no isolation the containers don't already give.
- **Ports** are ephemeral on loopback (`127.0.0.1::8080` for each of the six), as in `live` (below): two jobs run at once on the rig, and two `era` runs on fixed ports would collide. The warm-up step and each nextest step read the ports from `job.services.<name>.ports['8080']` into the variables above.
- **Health** is checked on `GET /_admin/tenants`, which faults never touch, with `python3` (the images have no `curl`). Actions waits for every service to be healthy before the first step.
- **Same toolchain, build tree, caching (`shared-key: era`, GitHub's runners only) and stage as `live`.** It joined `ci`'s `needs` on 2026-10-20 (roadmap E6.3; [required checks](#required-checks)).
- Run locally with the compose `era` profile for the first pair. Faults need their own pair, and so does strict mode: `docker run` the two images (each setting its own `-e`, with `AUTH_MODE=strict` for the strict pair) on other ports. From `backend/`, with `--run-ignored only -p integrations -p api` on each:
  - open: `ERA_SLACK_BASE=… ERA_GOOGLE_BASE=… cargo nextest run … -E 'test(/(^|::)era_/) and not test(/(^|::)era_fault_/) and not test(/(^|::)era_strict_/)'`
  - fault (own pair, same variables): `-E 'test(/(^|::)era_fault_/)'`
  - strict: `ERA_STRICT_SLACK_BASE=… ERA_STRICT_GOOGLE_BASE=… cargo nextest run … -E 'test(/(^|::)era_strict_/)'`


**The toolchain.** Every Rust job runs `rustup toolchain install 1.99.1 --profile minimal --component rustfmt,clippy`, and the workflow sets `RUSTUP_TOOLCHAIN=1.99.1`. `check_examples.py` builds its crate in a temporary directory outside `backend/`, where `rust-toolchain.toml` doesn't apply, and refuses any `rustc` but the one `versions.md` pins.

**How `changes` decides.** It diffs against the pull request's base, or the push's `before`; a manual run, a tag, a new branch, a base missing from history, or a change to `ci.yml` or `.github/actions/` sets every output. `rust` is `backend/crates`, `backend/bins`, `backend/migrations`, the workspace's root files, `ml/models/manifest.toml` and `ml/models/templates/`, which `store`'s tests compare with their copies inside `backend/`, and `ml/knowledge/schema/`, which `domain`'s tests compare with the seed types; `docs` is any `.md` file and `backend/tools/docgen/`; `skills` is `.claude/skills/rust*` and `backend/migrations/`; `infra` is `infra/` and `.github/`; `live` is `store`, `blob`, `migrations`, `Cargo.toml`, `Cargo.lock` and the Rust skills; `era` is anything under `backend/`, the Maple Row fixture included; `images` is what the image build context holds (`backend/` without `tools/`).

The GPU feature checks need no GPU and no CUDA: `ort` uses `load-dynamic`, so nothing links against ONNX Runtime at build time ([versions.md](../../.claude/skills/rust/versions.md#notes-and-things-not-confirmed)). They catch code behind `#[cfg(feature = "cuda")]` that the default build never compiles.

**Caching.** On GitHub's runners only (the rig skips every `rust-cache` step and keeps its trees under `~ci-runner/ci/target`, see Runners and budget), `Swatinem/rust-cache` with `workspaces: backend -> target`, one `shared-key` per job (`lint`, `test`, `examples`, `live`) because each compiles a different profile or feature set, and `save-if: ${{ github.ref == 'refs/heads/main' }}`. `examples` and `live` also cache `~/.cache/pdm-rust-skills-check/target`, where `check_examples.py` builds; `examples` caches nothing else. Both key those caches on `versions.md` as well. ✓ The first saves on `main` (2026-10-16) were `lint` 0.63 GB, `test` 1.06 GB, `live` 1.25 GB and `examples` 2.08 GB: 5.0 GB in all, within the 10 GB. The cold times were measured on 2026-10-16, on pull request #17's first full run (2 vCPUs, empty caches; the jobs run in parallel, so the run took 15 minutes). The warm times stay estimates until runs on `main` have used the saved caches. These paragraphs describe GitHub's runners; on the rig the build trees persist instead (see Runners and budget), and times there are *unverified* until the first runs.

**Disk.** A hosted runner has about 14 GB free (GitHub's figure; `df` in the jobs records the real one). On `main`, after every test of `live` had passed, rust-cache's post step ran it out ("No space left on device", 32 MB free). It runs `cargo metadata --all-features`, which unpacks every platform's crates (`winapi`, `windows-*`) into the registry, and then writes the cache archive, all while the job still holds the workspace's and the examples' build trees. (The rig has 251 GB free; `free-disk` does nothing there, and the weekly prune keeps its use under 100 GB, *unverified*.) So the workflow sets `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, which keeps file and line in backtraces but shrinks every build tree and cache. `examples`, `live` and `images` first run [`.github/actions/free-disk`](../../.github/actions/free-disk/action.yml), which deletes the preinstalled .NET, Android, GHC and CodeQL toolchains. `live` and `examples` end with a step that prints `df -h /` and the build trees' sizes, even after a failure. ✓ Measured on pull request #23 (2026-10-16): the root filesystem is 72 GB with 15 GB free at the start; `live` had 36 GB free after the cleanup and 28 GB at the end, with build trees of 3.7 GB (workspace) and 3.8 GB (examples); `examples`, before it freed disk, ended with 8.8 GB free and a 5.1 GB tree.

**The live job's services.** GitHub Actions service containers accept `command` and `entrypoint` (✓ workflow syntax), so SurrealDB can start with the in-memory engine the store test expects. The tests take their URLs from `PDM_TEST_SURREALDB_URL` and `PDM_TEST_RUSTFS_URL` (defaults in `crates/store/tests/server.rs` and `crates/blob/tests/rustfs.rs`); the credentials are the throwaway ones those tests hard-code.

The job is in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml). What it decides:
- **SurrealDB** starts with `start --user root --pass root memory` through the service's `command`, and no `--health-cmd`. The image is distroless, so a `--health-cmd` (always run through `/bin/sh`) fails every probe. Its own exec-form check, `/surreal isready`, runs at the job's `--health-interval 5s --health-retries 12` (✓ healthy in about 12 s, 2026-10-16).
- **RustFS** uses `curl -fsS http://127.0.0.1:9000/health/ready`; the image has `sh` and `curl`.
- **Ports** are ephemeral on loopback (`127.0.0.1::8000` and `127.0.0.1::9000`). The nextest step reads them from `job.services.<name>.ports['8000']` and `['9000']` into `PDM_TEST_SURREALDB_URL` and `PDM_TEST_RUSTFS_URL`. Two jobs run at once on the rig, and the developer's own [`dev.compose.yml`](#infradevcomposeyml) holds 18000 and 19000 on the same host. The runner parses `docker port` output (`8000/tcp -> 127.0.0.1:NNNNN`; ✓ read in actions/runner v2.338.0, `DockerUtil.ParseDockerPort`). ✓ On pull request #77 (2026-10-19), under the rig's rootless Docker, the services got ports 32768 and 32769 and both ignored tests passed.
- **The examples' build cache** is keyed on `versions.md` as well as the lock file. Otherwise rust-cache finds its key unchanged after a pin moves, and never saves the new build.

`check_examples.py --live` starts its own SurrealDB container with `docker run` and an ephemeral port, so it doesn't clash with the service. The images and digests are those of [database 1](../database/README.md#1-version-and-setup) and [storage 1](../storage/README.md#1-version-and-setup) (✓ both digests match Docker Hub on 2026-10-16).

### Required checks

The `main` ruleset ("Require PRs for main and master") requires one check, `ci`, from GitHub Actions (integration 15368). An admin added it on 2026-10-16 with `gh api -X PUT repos/aacruzgon/GreenLens/rulesets/25252788`, after the first run of `ci.yml` on `main` passed.

- **One check, not one per job.** A renamed or added job changes only `ci`'s `needs` list, never the ruleset. And `ci` fails when a job it waits for failed or was cancelled, so a failed `changes` or `lint`, whose dependent jobs then show "skipped", still blocks the merge.
- **`live` joined `ci`'s `needs`** on 2026-10-16, after it passed on four consecutive pushes to `main` (#23, #18, #25, #24). Service containers are the likeliest source of flakes, so they were proved first.
- **`era` joined `ci`'s `needs`** on 2026-10-20, after it passed on five consecutive pushes to `main` (#69, #79, #78, #71, #76), taking 3.5 to 6.3 minutes on the rig (roadmap [E6.3](../EON/README.md#e6-ci)). A failing `era` test now blocks a merge like any other.
- **`images` and `release`** are never required: they run after the merge.
- **`strict_required_status_checks_policy` stays `false`:** requiring a branch to be up to date with `main` would rerun every job after each merge to `main`, which the two runners' time can't afford.
- **Self-hosted runners.** The required check is unchanged: a job on a self-hosted runner still reports through the GitHub Actions app. ✓ On pull request #77 (2026-10-19), `app.id` of the `ci` check run was 15368 (`gh api repos/aacruzgon/GreenLens/commits/<sha>/check-runs`).

---

## 3. Images

### Base images, pinned by digest

| Use | Image | Digest (✓ Docker Hub, 2026-10-16) |
|---|---|---|
| Builder for `api` | `rust:1.99.1-slim-trixie` | `sha256:4df2ae8cfdfebdfc13a65da7f253b73d68983dc0339f2719bc17a279c913b13d` |
| Builder for `worker` | `rust:1.99.1-slim-bookworm` | `sha256:a24e292b4af7bdee351a31d3a6379972a5652f0ea22a1b6d6883324ac4f5097c` |
| Runtime for `api` and `worker` | `debian:trixie-slim` | `sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f` |
| Runtime for `worker-cuda` | `nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` | `sha256:14f6d08d1cd4a96effbfe3101d0b56326f552c199d05e4979ee0bd616df5811b` |
| Runtime for `seed-upload` | `python:3.12-slim-trixie` | `sha256:a6e34c598f2467ed0e9a8d349809fcd8b5c603269512df273a0bb1784edc11b1` |

**Why two builders.** `api` links the system libheif for HEIC uploads, which needs 1.19 or newer: Debian 13 (trixie) ships it (✓ 1.23.4-1~deb13u1, installed by the build on 2026-10-16), Debian 12 doesn't ([setup 4](../setup/README.md#4-backend-rust)). The worker binary must also run in the CUDA image, which is Ubuntu 24.04 with glibc 2.39, while trixie's glibc is 2.41; a binary built against a newer glibc fails to start on an older one. So `worker` is built on Debian 12 (glibc 2.36), which runs on both trixie and Ubuntu 24.04, and it doesn't use libheif.

✓ All four digests were re-checked with `docker buildx imagetools inspect` on 2026-10-16, when the Dockerfile landed. Docker Hub's digest for a multi-architecture tag is the image index, so the same pin serves amd64 and arm64.

### `backend/Dockerfile`

One file, four stages per binary: `cargo-chef` plans the dependency graph, a cached layer builds only the dependencies, then the workspace builds, then a slim runtime copies the binary. `cargo-chef` 0.1.78 (✓ crates.io) is installed with `cargo install --locked` in the official Rust image rather than taken from a third-party builder image. No `# syntax=` line and no cache mounts: the layers alone carry the cache, in the Actions cache in CI.

- **The chef stages** also run `rustup component add rustfmt clippy`. `rust-toolchain.toml` asks for both, and the slim image has neither, so without this every later `cargo` call downloads them.
- **`REVISION` and `CREATED`** are declared after each runtime stage's `RUN` and `COPY` steps. They change on every commit, and an `ARG` in scope of a `RUN` invalidates its cache. Declared before, every build would rerun `apt-get` and the ONNX Runtime downloads, and every deploy would pull new base layers.
- **`.dockerignore`** also leaves out `Dockerfile`, `docker-bake.hcl`, `.dockerignore` itself, and `.env` files at any depth. An edit to the build files then doesn't change the context and force a full recompile.

The file is [`backend/Dockerfile`](../../backend/Dockerfile), built by [`backend/docker-bake.hcl`](../../backend/docker-bake.hcl) with the context `backend/`; [`backend/.dockerignore`](../../backend/.dockerignore) leaves out `target/`, `tools/` and `.git`. Its stages:

| Stage | Base | What it does |
|---|---|---|
| `chef-api`, `plan-api`, `build-api` | `rust:1.99.1-slim-trixie` with `libheif-dev` | `cargo chef cook`, then `cargo build --release --locked -p api --features heic` |
| `chef-worker`, `plan-worker`, `build-worker` | `rust:1.99.1-slim-bookworm` | the same for `-p worker --features cuda,migraphx,webgpu` |
| `api` | `debian:trixie-slim` | `libheif1`, `curl`, ONNX Runtime 1.31.0 CPU in `/opt/onnxruntime`, `ORT_EP=cpu`, port 8080 |
| `worker` | `debian:trixie-slim` | CA certificates and the binary |
| `worker-cuda` | `nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` | ONNX Runtime 1.31.0 `gpu_cuda13` in `/opt/onnxruntime`, `ORT_EP=cuda`; `curl` is removed after the download |

✓ Built for `linux/amd64` on 2026-10-16, on an Apple-silicon laptop under emulation (OrbStack): `api` 96 MB, `worker` 45 MB, `worker-cuda` 2.26 GB. With no environment, `api` exits 1 with `CLERK_ISSUER is required`. All three run as UID 10100. The worker binary built on bookworm starts in the CUDA image. In the CUDA provider library, `ldd` finds every dependency but `libcuda.so.1`, which the NVIDIA container runtime mounts from the host driver.

- **Release profile.** The images build with the workspace's `[profile.release]` unchanged: `lto = "thin"`, `codegen-units = 1`, `panic = "unwind"`, `debug = "line-tables-only"`, `overflow-checks = true`, and no `strip`, so a panic's backtrace keeps its frames ([backend 12](../backend/README.md#12-builds-and-deployment)).
- **One worker binary for every image.** With `load-dynamic`, the `cuda`, `migraphx` and `webgpu` features can be enabled together; only `download-binaries` forbids that ([versions.md](../../.claude/skills/rust/versions.md#notes-and-things-not-confirmed)). The worker is compiled once per BuildKit run, and each worker image copies the same file. `ORT_EP` picks the provider at start.
- **Non-root.** Every image runs as UID and GID 10100, `pdm`, with the home directory `/nonexistent` and no shell login (`/usr/sbin/nologin`). Compose adds `read_only: true`, a 512 MB tmpfs on `/tmp`, `cap_drop: [ALL]` and `no-new-privileges`. RustFS's UID 10001 is unaffected. SurrealDB's 65532 needs `surrealdb-volume`: the image's `/data` is owned by root, so a new volume is too ([database 1](../database/README.md#1-version-and-setup)).
- **PID 1.** The binaries handle SIGTERM themselves and spawn no children, so no init process is added; the exec-form `ENTRYPOINT` delivers the signal directly.
- **ONNX Runtime archives** are fetched from Microsoft's v1.31.0 release (published 2026-10-09) and checked against the SHA-256 the release lists for each asset (✓ GitHub API): CPU `cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57`, 11.4 MB; CUDA 13 `2867dec99c6a4926e8142361b9d4d1518c16c85b854b4d41d28d65d3b2c81135`, 244.6 MB. A changed archive fails the build.
- **Labels.** `org.opencontainers.image.source=https://github.com/aacruzgon/GreenLens` links each package to the repository (✓ GHCR docs), plus `revision` (the commit) and `created`.

### The images

| Image | Runtime base | Binary and features | ONNX Runtime | Runs |
|---|---|---|---|---|
| `api` | trixie-slim + `libheif1`, CA certificates, `curl` for the health check | `api`, `heic` | 1.31.0 CPU, `ORT_EP=cpu` | Control plane |
| `worker` | trixie-slim + CA certificates | `worker`, `cuda,migraphx,webgpu` | none | The VLM workers on the rig and the fallback worker: they call HTTP servers and load no ONNX model ([worker 11](../backend/worker.md#11-multi-vendor-gpus)) |
| `worker-cuda` | CUDA 13.0.3 + cuDNN 9 runtime, Ubuntu 24.04 | the same file | 1.31.0 `gpu_cuda13`, its provider libraries beside `libonnxruntime.so.1.31.0` in `/opt/onnxruntime/lib`; `ORT_EP=cuda` | The 3070's perception worker; host driver 580.65.06 or newer ([vision 5](../vision/README.md#5-runtimes-and-versions)) |
| `seed-upload` | `python:3.12-slim-trixie` | the knowledge loader's upload step | none | Control plane, once per deploy ([contracts 10](../architecture/contracts.md#10-knowledge-seed-format)) |

**Not built:**
- **`worker-migraphx`.** Perception stays on the 3070. Should it move to the RX 9070 XT, the image is `rocm/onnxruntime:rocm10.1.0_ub24.04_ort1.29_torch2.12.0_py3.12` (✓ `sha256:ece66d2a116c3edeba180a495b9120b0dfbca426ccd649fda1fc2c3318aa8d9c`, 2026-10-05) with the same worker file, `ORT_EP=migraphx` and `ORT_DYLIB_PATH` at that image's 1.29 library. It carries PyTorch and is several gigabytes; a slimmer base is work for that day.
- **`worker-webgpu`.** Microsoft's 1.31.0 release has no Linux WebGPU archive; its Linux x64 assets are CPU, CUDA 12 and CUDA 13 only (✓ GitHub API). A WebGPU image would need ONNX Runtime built from source, which nothing needs now.
- **No image carries model files.** `/models` is a read-only volume filled as [vision 6](../vision/README.md#6-model-files-versions-and-pinning) describes, so a new model doesn't need a new image and the images stay small.

The VLM sidecars are upstream images, not built here: `vllm/vllm-openai:v0.31.0` (✓ `sha256:c1c9f6fd5c109ba7f0546a59f5b2f15fb87f64c77782e90a27b648b42a8e67c3`) and `vllm/vllm-openai-rocm:v0.31.0` (✓ `sha256:749f6f3f944f12af49966ac523c1f8573e4b229594b541954bd5f879b4496b1f`, the digest in [worker 12](../backend/worker.md#12-deployment)).

### Registry, tags and who pushes

- **Registry:** GitHub Container Registry, `ghcr.io/aacruzgon/greenlens/<image>`. Docker repository names must be lowercase, so the repository's `GreenLens` becomes `greenlens`. Container storage and bandwidth on GHCR are currently free (✓ GitHub Packages billing). Packages pushed from a private repository are private.
- **Who pushes:** only the `images` job of `ci.yml`, on a push to `main` that changes the image's inputs (the build context of `backend/`, or the workflow) and on a `v*` tag, after every check has passed. It has `packages: write` and logs in with `GITHUB_TOKEN`, which links each package to the repository (✓ GHCR docs). No person pushes images. `ml/knowledge/` joins its inputs with the `seed-upload` target.
- **How:** `docker/setup-buildx-action` (on GitHub's runners; the rig sets `BUILDX_BUILDER=pdm-ci` instead), `docker/login-action`, then `docker buildx bake -f backend/docker-bake.hcl --push --metadata-file meta.json`. One bake run builds `api`, `worker` and `worker-cuda`, so the worker's builder stages run once. Layers are cached in GHCR as `ghcr.io/aacruzgon/greenlens/buildcache:<target>` (`type=registry`, `mode=max`), not in the Actions cache, whose 10 GB the Rust caches already half fill (5.0 GB). `seed-upload` is a separate bake target with `ml/knowledge` as its context, added when its Dockerfile exists.
- **On the rig,** `pdm-ci` is a persistent `docker-container` builder, so its layer cache outlives the job. It reads GHCR's registry cache (`cache-from`) but writes it (`cache-to`) only on GitHub's runners, so the rig doesn't upload every layer over a home connection. `docker-bake.hcl` needs nothing newer than buildx 0.13.1; the rig has buildx v0.38.0, because the `docker` driver can't export a registry cache. The weekly prune trims the cache to 30 GB ([section 8](#8-operations-hooks)).
- **Platform:** `linux/amd64`. If the control plane goes on an arm64 VM, `api` and `worker` gain `linux/arm64`, built natively on the `ubuntu-24.04-arm` runner (✓ listed for private repositories) with ONNX Runtime's `linux-aarch64` archive ([open question 3](#10-open-questions)).
- **Tags:** `sha-<12 hex of the commit>`, plus `main` on `main` or the version on a `v*` tag (the bake file's `TAGS`, comma-separated). Never `latest`. Tags are for people; deploys use the digest.
- **Digests:** the job writes each image's `containerimage.digest` from `meta.json` into the run summary as a ready line for `deploy.sh`, for example `api ghcr.io/aacruzgon/greenlens/api@sha256:…`. The `created` label is the commit's time, not the build's.
- **Tags reuse `main`'s images.** On a `v*` tag, `images` first checks that the tagged commit is on `main` (so it was reviewed) and that the name is `vMAJOR.MINOR.PATCH[-pre]`. It then adds the version tag to the `sha-<12 hex>` images `main` built for that commit (`docker buildx imagetools create`), so the release lists the very digests `main`'s run summary showed. It builds only when `main` built no images for that commit. `release` puts the lines in the GitHub Release.
- **Time:** a cold release build of both binaries with thin LTO and one codegen unit on 2 vCPUs: 25–35 minutes; warm, when only workspace crates changed: 10–15 minutes (estimates, *unverified*).
- **Pulling on the hosts:** each host runs `docker login ghcr.io` once with a classic personal access token with only `read:packages`. GitHub Packages accepts only classic tokens (✓ GHCR docs). The token belongs to a team member and is stored only in that host's Docker credential store.

---

## 4. Compose files

All files need Docker Engine with Compose v2 ([setup 6](../setup/README.md#6-the-gpu-rig-and-the-control-plane)). Shared settings use YAML anchors at the top of each file:

```yaml
x-logging: &logging
  driver: json-file
  options: { max-size: "10m", max-file: "5" }     # 50 MB of logs per container at most
x-hardened: &hardened
  read_only: true
  tmpfs: ["/tmp:size=512m"]
  cap_drop: [ALL]
  security_opt: ["no-new-privileges:true"]
```

### `infra/dev.compose.yml`

For a laptop: SurrealDB and RustFS on the ports and with the throwaway credentials that the ignored tests and `check_examples.py` expect. Nothing here is a secret, and nothing here is used anywhere else. The file is [`infra/dev.compose.yml`](../../infra/dev.compose.yml):

| Service | What it does |
|---|---|
| `surrealdb-volume` | `chown 65532:65532 /data` on the volume (`busybox:1.38.0`, pinned by digest), then exits. It runs as root, with `cap_drop: [ALL]` plus `CHOWN`, no network and a read-only root. The image's `/data` is owned by root, so a new named volume is too and RocksDB can't write it ([database 1](../database/README.md#1-version-and-setup)) |
| `surrealdb` | `start --user root --pass root rocksdb:/data/db` as the image's own user, on `127.0.0.1:18000` |
| `surrealdb-init` | `surreal import` of [`dev/surrealdb-users.surql`](../../infra/dev/surrealdb-users.surql): the `pdm` namespace, the `main` database and the `api` user with password `api`, so a local `api` can sign in. `surreal import` refuses a file that doesn't start with `OPTION IMPORT` |
| `rustfs` | RustFS with `TESTROOT`/`testrootsecret` on `127.0.0.1:19000` |
| `rustfs-init` | `rc bucket create --ignore-existing dev/pdm-images` (`rustfs/rc:v0.1.36`; `mb` is deprecated there) |
| `era-slack`, `era-drive` | Only with `--profile era`: Era by Eon's Slack and Google Drive simulators, the farm connectors' test bench ([connectors 9.2](../connectors/README.md#92-development-and-ci), [EON 2](../EON/README.md#2-before-you-start)). Pinned by tag and digest, on `127.0.0.1:8091` and `:8092`, with `SIMCORE_SEED=42`, `SIMCORE_TODAY=2026-10-15`, `SIMCORE_FIDELITY=high` and `AUTH_MODE=open`. The health check calls `/_admin/tenants` with the image's `python3`, because the image has no `curl` and faults fail `/_meta` |

✓ Run on 2026-10-16. From an empty volume, `up -d` leaves SurrealDB healthy and running as `nonroot`. Then `cargo nextest run --workspace --run-ignored only` passes (2 of 2), the `api` user signs in, and a wrong password gets `401`. After `down` and `up`, the data is kept, and both init services exit 0 again.

✓ Run on 2026-10-19: `docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive` leaves both simulators healthy, and `/_meta` reports `build.fidelity: high` on each. Without `--profile era`, `up` starts neither.

CI no longer uses ports 18000 and 19000 (its `live` and `era` services take ephemeral ports), so the dev stack and the CI runners coexist on the rig.

`docker compose -f infra/dev.compose.yml down -v` resets both; the `-v` is safe only in this file. A local `api` and fallback worker take their environment from [`infra/dev/.env.example`](../../infra/dev/.env.example): copy it to `infra/dev/.env`, which git ignores, fill in the Clerk, LLM and ONNX Runtime values, then `set -a; . infra/dev/.env; set +a` before `cargo run`.

### `infra/control-plane.compose.yml`

```yaml
name: pdm
services:
  surrealdb-volume:                 # database 1: gives the volume to UID 65532, then exits, with only CHOWN and no network
  surrealdb:                        # database 1, with env_file: .env.surrealdb in place of interpolation, plus:
    logging: *logging
    networks: [data]
    stop_grace_period: 60s          # stops after api (reverse dependency order)
  rustfs:                           # storage 1, with env_file: .env.rustfs in place of interpolation, plus:
    logging: *logging
    networks: [data]
    stop_grace_period: 60s
  seed-upload:
    image: ${SEED_UPLOAD_IMAGE:?}   # ghcr.io/aacruzgon/greenlens/seed-upload@sha256:…
    restart: "no"
    <<: *hardened
    env_file: ${PDM_ENV:-/srv/pdm/env}/.env.seed      # PDMSEED01's keys
    depends_on: { rustfs: { condition: service_healthy } }
    networks: [data]
    logging: *logging
  model-fetch:                      # vision 6: fills the models volume, then exits
    image: python:3.12-slim-trixie@sha256:a6e34c598f2467ed0e9a8d349809fcd8b5c603269512df273a0bb1784edc11b1
    restart: "no"
    <<: *hardened
    command: ["python", "/src/fetch.py", "--manifest", "/src/manifest.toml", "--sources", "/src/sources.toml",
              "--for", "api,fallback", "--dest", "/models"]
    env_file: ${PDM_ENV:-/srv/pdm/env}/.env.models  # HF_TOKEN, read-only, pdm-models only
    volumes: ["../ml/models:/src:ro", "models:/models"]
    networks: [edge]
    logging: *logging
  api:
    image: ${API_IMAGE:?}
    restart: unless-stopped
    <<: *hardened
    env_file: ${PDM_ENV:-/srv/pdm/env}/.env.api
    environment: { MODELS_DIR: /models, ORT_EP: cpu }
    volumes: ["models:/models:ro"]
    depends_on:
      model-fetch: { condition: service_completed_successfully }
      surrealdb: { condition: service_healthy }
      rustfs: { condition: service_healthy }
      seed-upload: { condition: service_completed_successfully }
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://127.0.0.1:8080/healthz"]
      interval: 10s
      timeout: 3s
      retries: 3
      start_period: 60s             # schema, migrations and the knowledge index load first
    stop_grace_period: 190s         # architecture 12
    networks: [edge, data]
    logging: *logging
  fallback-worker:
    image: ${WORKER_IMAGE:?}
    restart: unless-stopped
    <<: *hardened
    command: ["--caps", "fallback"]
    env_file: ${PDM_ENV:-/srv/pdm/env}/.env.fallback  # WORKER_TOKEN, LLM_API_KEY
    environment: { WORKER_ID: cp-fallback-1, API_BASE_URL: "http://api:8080", MODEL_MANIFEST: /models/manifest.toml }
    volumes: ["models:/models:ro"]
    depends_on: { api: { condition: service_healthy } }   # api already waited for model-fetch
    stop_grace_period: 30s          # DRAIN_SECONDS (20) + 10 s, worker 12
    networks: [edge]
    logging: *logging
  caddy:
    image: caddy:2.11.7-alpine@sha256:d8542f48d34a9cf4e4c11a478865229840e87e4c96ea3f439101f31a5d35f75f
    restart: unless-stopped
    environment: { CADDY_SITE: "${CADDY_SITE:?}" }    # api.example.org on a VM; :80 behind an ingress profile
    ports: ["${CADDY_HTTP:-80}:80", "${CADDY_HTTPS:-443}:443"]   # host-only behind an ingress profile
    volumes: ["./caddy/Caddyfile:/etc/caddy/Caddyfile:ro", "caddy-data:/data", "caddy-config:/config"]
    depends_on: { api: { condition: service_healthy } }
    networks: [edge]
    logging: *logging
  tailscale:                        # section 5: Funnel, the default on the rig
    profiles: [tailscale]
    image: tailscale/tailscale:v1.104.1@sha256:87c55d6be56ad85b3d8bceca5f87f73de288c609131460dd1be1874c250cc41d
    restart: unless-stopped
    hostname: pdm-cp
    env_file: ${PDM_ENV:-/srv/pdm/env}/.env.tailscale  # TS_AUTHKEY, used once to join the tailnet
    environment:
      TS_HOSTNAME: pdm-cp                             # the public URL is https://pdm-cp.<tailnet>.ts.net
      TS_STATE_DIR: /var/lib/tailscale                # keeps the node's identity across restarts
      TS_SERVE_CONFIG: /config/serve.json
      TS_USERSPACE: "true"                            # the default; no /dev/net/tun, no added capabilities
    volumes: ["tailscale-state:/var/lib/tailscale", "./ingress/tailscale:/config:ro"]   # a directory, so edits are picked up
    depends_on: [caddy]
    networks: [edge]
    logging: *logging
  cloudflared:                      # section 5: needs a domain on Cloudflare
    profiles: [cloudflare]
    image: cloudflare/cloudflared:2026.10.1@sha256:a15a3b50b8dabd57ac2090e9484c0247dba9dfa1f0df4d298d4317f9d69eb2d5
    restart: unless-stopped
    command: ["tunnel", "--no-autoupdate", "--config", "/etc/cloudflared/config.yml", "run"]
    volumes: ["./ingress/cloudflare/config.yml:/etc/cloudflared/config.yml:ro",
              "${PDM_ENV:-/srv/pdm/env}/tunnel.json:/etc/cloudflared/tunnel.json:ro"]
    depends_on: [caddy]
    networks: [edge]
    logging: *logging
networks: { edge: {}, data: {} }
volumes: { models: {}, surrealdb-data: {}, rustfs-data: {}, rustfs-logs: {}, caddy-data: {}, caddy-config: {}, tailscale-state: {} }
```

- **Start order:** `model-fetch` exits 0, and `surrealdb` and `rustfs` are healthy → `seed-upload` exits 0 → `api` applies the schema and migrations ([database 1](../database/README.md#1-version-and-setup)) and turns healthy → `caddy` and `fallback-worker`. A failed `seed-upload` keeps `api` down, which is the point: the migration must not reference photos that aren't there.
- **Stop order** is the reverse, so `api` drains its deletes into a running RustFS. `docker compose down` waits up to each service's grace period.
- **Networks.** `data` holds the stateful services; `edge` holds what talks to the internet. Only `api` is on both. `data` isn't `internal: true`, because the backups reach SurrealDB and RustFS through their host-only ports (`127.0.0.1:8000`, `127.0.0.1:9000`, as database 1 and storage 1 publish them).
- **Root credentials.** Database 1 and storage 1 interpolate the root credentials (`${SURREAL_ROOT_PASS:?}`, `${RUSTFS_ROOT_SECRET_KEY:?}`) into their services. Here they come from `env_file` instead, so they never sit in `.env.compose`, which every compose command reads.
- **`api`'s bucket.** `RUSTFS_BUCKET` defaults to `pdm-images`, the bucket of [storage 2](../storage/README.md#2-bucket-keys-and-access); `.env.api` still sets it explicitly.
- **`/models`** is the named volume `models`. Only `model-fetch` mounts it read-write; it runs `ml/models/fetch.py` from the host's checkout ([vision 6, how files reach `/models`](../vision/README.md#how-files-reach-models)), which downloads only what changed and checks every hash. A failed fetch keeps `api` down with its old files untouched, so a bad manifest is never deployed. Our own files come from the private Hugging Face repository `pdm-models` with the host's `HF_TOKEN`.

### `infra/rig.compose.yml`

The services, commands and settings are those of [worker 12](../backend/worker.md#12-deployment); this file adds the image names, the shared anchors and the health checks. GPU identities come from an untracked `.env.compose` on the rig, never from the file:

```yaml
x-worker: &worker
  image: ${WORKER_IMAGE:?}          # worker-cuda for perception-worker: ${WORKER_CUDA_IMAGE:?}
  restart: unless-stopped
  <<: *hardened
  stop_grace_period: 30s
  logging: *logging
services:
  model-fetch:                      # vision 6, the perception group
    image: python:3.12-slim-trixie@sha256:a6e34c598f2467ed0e9a8d349809fcd8b5c603269512df273a0bb1784edc11b1
    restart: "no"
    command: ["python", "/src/fetch.py", "--manifest", "/src/manifest.toml", "--sources", "/src/sources.toml",
              "--for", "perception", "--dest", "/models"]
    env_file: ${PDM_ENV:-/srv/pdm/env}/.env.models
    volumes: ["../ml/models:/src:ro", "models:/models"]
  perception-worker:
    <<: *worker
    image: ${WORKER_CUDA_IMAGE:?}
    volumes: ["models:/models:ro"]
    depends_on: { model-fetch: { condition: service_completed_successfully } }
    deploy: { resources: { reservations: { devices:
      [{ driver: nvidia, device_ids: ["${GPU_3070_UUID:?}"], capabilities: [gpu] }] } } }
  vllm-nv:
    image: vllm/vllm-openai:v0.31.0@sha256:c1c9f6fd5c109ba7f0546a59f5b2f15fb87f64c77782e90a27b648b42a8e67c3
    healthcheck: { test: ["CMD", "curl", "-fsS", "http://127.0.0.1:8000/health"], interval: 15s, retries: 4, start_period: 900s }
    deploy: { resources: { reservations: { devices:
      [{ driver: nvidia, device_ids: ["${GPU_5070TI_UUID:?}"], capabilities: [gpu] }] } } }
  vlm-worker-nv:
    <<: *worker
    depends_on: { vllm-nv: { condition: service_healthy } }
  vllm-amd:
    devices: ["/dev/kfd", "${AMD_RENDER_NODE:?}"]      # /dev/dri/by-path/pci-0000:<bus>:00.0-render
    group_add: ["${HOST_RENDER_GID:?}", "${HOST_VIDEO_GID:?}"]
    healthcheck: { test: ["CMD", "curl", "-fsS", "http://127.0.0.1:8000/health"], interval: 15s, retries: 4, start_period: 900s }
  vlm-worker-amd:
    <<: *worker
    depends_on: { vllm-amd: { condition: service_healthy } }
volumes: { models: {} }
```

- **GPU pinning.** NVIDIA cards by UUID (`nvidia-smi -L` prints `GPU-…`), never by index, since indices can change between boots. The AMD sidecar gets `/dev/kfd` and only its own render node by PCI path, never all of `/dev/dri`, which also holds the NVIDIA nodes ([worker 12](../backend/worker.md#12-deployment)). `getent group render video` gives the two group ids.
- **Health.** A VLM worker waits for its sidecar's `/health`, so it doesn't claim jobs while the model is still loading. The 900 s start period covers a first download of the checkpoint into `hf-cache`; vLLM v0.31.0 at this digest serves `/health` and the image has `/usr/bin/curl` (✓ ran on the rig, 2026-10-16). The running `vllm` service in PR #7's `infra/rig.compose.yml` is this section's `vllm-nv` and is renamed when the workers are added ([dev-roadmap](../vision/dev-roadmap.md) task P6). The perception worker has no port to probe; its heartbeats on `/status` are its health.
- **The worker settings** (`WORKER_ID`, `ORT_EP`, `ORT_DYLIB_PATH`, the `VLM_*` values) stay in `environment:` exactly as worker 12 lists them, since they aren't secrets. The tokens are in `.env.<worker id>` files.
- **If llama.cpp serves the 9070 XT**, `vllm-amd` is replaced as [worker 12](../backend/worker.md#12-deployment) describes; its image tag and digest are pinned once the day-1 check picks it.
- **A second rig host** uses `infra/rig-amd.compose.yml` with only the two AMD services.

---

## 5. Ingress

Caddy fronts `api` in every case. What sits in front of Caddy is a compose profile, chosen per host with `COMPOSE_PROFILES` in `.env.compose`. The rig uses `tailscale` (decided 2026-10-16, [architecture 12](../architecture/README.md#hosting-the-control-plane)): it needs no domain and no open port, and anyone can reach the URL without joining the tailnet, so teammates test against it from their own phones and laptops.

```caddyfile
{
	servers {
		trusted_proxies static private_ranges   # the ingress sidecar connects from the compose network
	}
}

{$CADDY_SITE} {
	reverse_proxy api:8080
	log {
		output stdout
		format json
	}
}
```

| | `tailscale` (the rig) | `cloudflare` | none (a cloud VM) |
|---|---|---|---|
| Public URL | `https://pdm-cp.<tailnet>.ts.net` | `https://api.<domain>` | `https://api.<domain>` |
| Needs | A Tailscale account; MagicDNS, HTTPS certificates and the `funnel` attribute on the tailnet | A domain whose DNS is on Cloudflare | A domain and a public IP |
| Where TLS ends | The `tailscale` sidecar, with a certificate Tailscale provisions | Cloudflare's edge | Caddy, with Let's Encrypt; certificates in `caddy-data` |
| `CADDY_SITE` | `:80` | `:80` | `api.<domain>` |
| Ports published | host-only: `CADDY_HTTP=127.0.0.1:8081`, `CADDY_HTTPS=127.0.0.1:8443` | the same | 80 and 443 on the host |
| `COMPOSE_PROFILES` | `tailscale` | `cloudflare` | empty |
| Secret | `.env.tailscale` (`TS_AUTHKEY`) | `tunnel.json` | none |

`api` doesn't know which row it runs under. Its rate limits are keyed by the signed-in user, not by a client address ([api limits](../../backend/bins/api/src/limits.rs)), and the only URL it needs is its own, `API_PUBLIC_BASE`, for signed image links.

**Switching providers:** set `COMPOSE_PROFILES` and `CADDY_SITE` in `.env.compose` and `API_PUBLIC_BASE` in `.env.api`, then `docker compose up -d` (Compose removes nothing on its own, so `docker compose --profile <old> stop <old sidecar>` first). Then point two things at the new URL: the Clerk webhook (`POST /webhooks/auth`) and each rig worker's `API_BASE_URL`. Run the S1 smoke script against it. That Compose reads `COMPOSE_PROFILES` from the file `--env-file` names is *unverified*; `--profile <name>` on each command does the same.

**`infra/ingress/tailscale/serve.json`** (committed, no secrets) forwards the Funnel address to Caddy:

```json
{
  "TCP": { "443": { "HTTPS": true } },
  "Web": { "${TS_CERT_DOMAIN}:443": { "Handlers": { "/": { "Proxy": "http://caddy:80" } } } },
  "AllowFunnel": { "${TS_CERT_DOMAIN}:443": true }
}
```

The container replaces `${TS_CERT_DOMAIN}` with its own `ts.net` name at start (✓ ran, 2026-10-16; Compose doesn't touch a mounted file). The file is mounted as a directory, because the container watches the directory for changes (✓ Tailscale docs).

**Setting up Funnel**, once per tailnet, in the admin console:
1. Turn on MagicDNS and HTTPS certificates (DNS page). Optionally rename the tailnet, because its name is part of the public URL.
2. In the policy file, give the tag `tag:pdm-cp` an owner and the `funnel` node attribute: `"tagOwners": { "tag:pdm-cp": ["autogroup:admin"] }` and `"nodeAttrs": [{ "target": ["tag:pdm-cp"], "attr": ["funnel"] }]`.
3. Create an auth key: one-off, not ephemeral, pre-approved, tagged `tag:pdm-cp`. Put it in `/srv/pdm/env/.env.tailscale` as `TS_AUTHKEY`. It's used once; after that the node's identity lives in the `tailscale-state` volume, so deleting that volume needs a new key. Tagged nodes don't expire (*unverified*).
4. `docker compose up -d`, then from a phone on mobile data, outside the tailnet, `curl https://pdm-cp.<tailnet>.ts.net/healthz`. The first requests fail with a TLS error for a few seconds, while the node fetches its certificate.

**Funnel test on the 3070 host** (✓ ran, 2026-10-16). The `tailscale` sidecar above, on `v1.104.1` with userspace networking and a `serve.json` like this one, in front of a small Python server instead of Caddy, reached from the host through public DNS (`curl --resolve` to the address `1.1.1.1` returned), so the traffic went through Funnel and not the tailnet:
- the node joined with a tagged one-off key and Funnel turned on for `${TS_CERT_DOMAIN}:443`, with no extra capabilities or devices;
- the certificate was Let's Encrypt's, for the node's `ts.net` name, issued about 30 s after start;
- an SSE stream of 5 events sent 2 s apart arrived 2 s apart, so Funnel doesn't buffer `text/event-stream`;
- uploads ran at 0.6–1.2 MB/s (4 MB in 7.3 s, 20 MB in 17.6 s). The path left the home connection and came back in, so this bounds the home upload and Funnel together; a phone's upload uses only the download direction.

**Notes**
- **SSE.** Caddy flushes `text/event-stream` responses immediately, whatever `flush_interval` says (✓ Caddy docs), so no proxy setting is needed. `api` already sends `Cache-Control: no-cache` and a keepalive comment every 15 seconds ([architecture 12](../architecture/README.md#12-deployment)). Funnel passes an SSE stream through unbuffered (✓ ran, the Funnel test above); its idle timeout, and Cloudflare's 100 s, are *unverified*, and the S1 smoke script's stream and resume steps test both. Cloudflare's quick tunnels (`trycloudflare.com`) don't support SSE (✓ Cloudflare docs), so they are not an option.
- **Bandwidth.** Funnel traffic is subject to bandwidth limits that Tailscale doesn't publish and that can't be configured (✓ Tailscale docs). Photo uploads are a few MB, and the rig's workers download every scan's images through the same URL, so the S1 smoke run should time an upload; the Funnel test measured 0.6–1.2 MB/s through the home connection. If the limit bites, the rig's workers can reach `api` over the tailnet instead (`API_BASE_URL` on the tailnet name, which MagicDNS resolves to a tailnet address on any host in the tailnet), leaving Funnel to the app.
- **Ports.** Funnel listens only on 443, 8443 and 10000 (✓ Tailscale docs); we use 443.
- **Client addresses.** Caddy ignores incoming `X-Forwarded-For` unless the peer is a trusted proxy (✓ Caddy docs). Behind a sidecar the peer is on a private compose address, so the header is trusted; on a VM, private ranges never reach Caddy directly. Nothing in `api` depends on the client address today.
- **Upload size.** Caddy sets no body limit; `api` enforces its own. Cloudflare's Free plan caps a request body at 100 MB (*unverified*); Funnel publishes no cap.
- **The Cloudflare profile** is a locally managed tunnel: `infra/ingress/cloudflare/config.yml` (committed, no secrets) names the tunnel and routes `api.<domain>` to `http://caddy:80`, with a final `http_status:404` rule. Its credentials file, `tunnel.json`, is created once with `cloudflared tunnel create` and lives only in `/srv/pdm/env/`.
- **What is never published:** SurrealDB, RustFS, the vLLM sidecars and `api`'s port 8080. Only Caddy faces the internet, through the sidecar or on a VM.

---

## 6. Secrets and configuration

**Rule:** no secret is ever in the repository, an image, a compose file or a command line. `.env` and `.env.*` are ignored by git ([AGENTS.md, secrets](../../AGENTS.md#secrets)); the committed `!.env.example` exception matches only that exact name, so every real env file below is named `.env.<service>` and is ignored automatically.

| File, in `/srv/pdm/env/` (mode 0600) | Read by | Holds |
|---|---|---|
| `.env.compose` | `docker compose --env-file` | Image digests (`API_IMAGE`, `WORKER_IMAGE`, `WORKER_CUDA_IMAGE`, `SEED_UPLOAD_IMAGE`), `CADDY_SITE`, `CADDY_HTTP`, `CADDY_HTTPS`, `COMPOSE_PROFILES` (the ingress, section 5); on the rig the GPU UUIDs, the render node and group ids. Not secret, but host-specific |
| `.env.surrealdb` | `surrealdb` | `SURREAL_USER`, `SURREAL_PASS`: the root user, created on first start |
| `.env.rustfs` | `rustfs` | `RUSTFS_ACCESS_KEY`, `RUSTFS_SECRET_KEY`: the root identity |
| `.env.api` | `api` | `SURREAL_PASS`, `SURREAL_ROOT_PASS` (the schema's namespace statement needs root, database 1), `RUSTFS_ACCESS_KEY=PDMAPI01`, `RUSTFS_SECRET_KEY`, `RUSTFS_BUCKET=pdm-images`, `API_PUBLIC_BASE`, `API_WEB_ORIGINS`, `API_URL_SIGNING_KEY` (32 bytes or more), `WORKER_TOKENS` (`worker_id=token` pairs), `CLERK_ISSUER`, `CLERK_SECRET_KEY`, `CLERK_WEBHOOK_SECRET`, `LLM_API_KEY`, optional `EXPO_ACCESS_TOKEN`, `OPEN_METEO_API_KEY`, `JEV_URL` and `JEV_API_KEY`, and optionally all three of `AGENT_MEMORY_URL`, `AGENT_MEMORY_CONTEXT_ID` and `AGENT_MEMORY_API_KEY` (the key is a secret, limited to `gardens/*`: [database: Agent Memory](../database/agent-memory.md#2-setup)). The farm connectors, all optional ([connectors 4–5](../connectors/README.md#4-data)): `CONNECTOR_KEY` (a secret: 32 bytes as 64 hex characters, which seals the vendors' tokens; unset turns connectors off), `SLACK_CLIENT_ID` and `SLACK_CLIENT_SECRET`, `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` (the secrets are secrets; each pair is set together or not at all, and a partial pair stops `api` at start naming the missing variable), `CONNECTOR_RETURN_URL` (where the OAuth callback sends the app, `pdm://connectors` by default; `https` or a custom app scheme, never `http`, `javascript`, `data`, `file` or `vbscript`, and no query), `ERA_SLACK_BASE` and `ERA_GOOGLE_BASE` (Era's simulators behind the demo garden's seeded connectors, roadmap E4.6), and `ERA_DEMO_GARDEN_ID` (the demo garden's id, a ULID: E9.0 creates the garden through the API, so the deploy sets it afterwards; seeding needs it, both bases and `CONNECTOR_KEY`, and skips a garden that doesn't exist yet) |
| `.env.seed` | `seed-upload` | `PDMSEED01`'s keys |
| `.env.models` | `model-fetch` | `HF_TOKEN`: a fine-grained Hugging Face token that can only read `pdm-models` ([vision 6](../vision/README.md#delivering-model-files-to-the-backend)), one per host |
| `.env.fallback` | `fallback-worker` | `WORKER_TOKEN`, `LLM_API_KEY` |
| `.env.<worker id>` | each rig worker | `WORKER_TOKEN`; `VLM_API_KEY` for the VLM workers and their sidecar |
| `.env.backup` | `ops/backup.sh` | `SURREAL_ROOT_PASS`, `PDMBACKUP01`'s keys, the off-host remote's credentials |
| `.env.era-seed` | `era-seed` | `SIMCORE_ADMIN_TOKEN`: the demo simulators' admin token, held by the one-shot service that injects the farm. Comes with the `demo` profile (roadmap E9, [docs/EON](../EON/README.md#e9-the-demo-garden)); nothing reads it yet |
| `.env.tailscale` | `tailscale` | `TS_AUTHKEY`: a one-off, tagged auth key, used once to join the tailnet (section 5) |
| `tunnel.json` | `cloudflared` | The tunnel's credentials, with the `cloudflare` profile only |

- **[`infra/env/.env.example`](../../infra/env/.env.example)** lists every variable of every file above, grouped under a `# --- .env.api ---` style heading, with placeholders only (`change-me`, `GPU-00000000-…`, `sk-placeholder`), and says never to load it whole, because a key repeats across sections and the last one wins. The local values for running `api` and the fallback worker against `dev.compose.yml` are in [`infra/dev/.env.example`](../../infra/dev/.env.example).
- **`infra/.env.example`** is a different file: the rig's vLLM template from PR #7, which [infra/README.md](../../infra/README.md) copies to `infra/.env`, and which compose loads into the vLLM container. So `api`'s secrets never go in it. It folds into the per-service files when `rig.compose.yml` grows into section 4. The `infra` CI job renders each compose file with `infra/.env.example` copied to `infra/.env`, the file the rig's compose file reads, so a variable a compose file needs and that example lacks fails CI.
- **Generating secrets:** `openssl rand -hex 32` for passwords, signing keys, worker tokens and `CONNECTOR_KEY`, which must be exactly that: 64 hex characters, refused at start otherwise. RustFS access keys use A–Z and 0–9 only ([storage 1](../storage/README.md#1-version-and-setup)). A worker's token goes in two places: its own `.env.<worker id>` and `api`'s `WORKER_TOKENS`.
- **Clerk:** the development instance's issuer and keys until the team decides otherwise; never the production instance without asking ([roadmap S1](../backend/roadmap.md#5-s1-walking-skeleton)).
- **`RUNNER_LABELS`** is a repository variable, not a secret: the JSON array of labels every job of `ci.yml` runs on, `["self-hosted","linux","x64","pdm"]` since 2026-10-19. Set it with `gh variable set RUNNER_LABELS --body '["self-hosted","linux","x64","pdm"]'`; delete it to return to GitHub's runners ([section 2](#runners-and-budget)). **`LIGHT_RUNNER_LABELS`**, `["self-hosted","linux","x64","pdm-light"]` since 2026-10-19, is the same for the light jobs; delete it together with `RUNNER_LABELS`. The label `pdm-light` on `rig-1` and `rig-2` was added through the API (`gh api -X POST repos/aacruzgon/GreenLens/actions/runners/<id>/labels -f 'labels[]=pdm-light'`), so it lives on GitHub, not in the runners' local configuration; registering either runner again drops it. The runners' own settings (`DOCKER_HOST`, `CARGO_BUILD_JOBS`, `NEXTEST_TEST_THREADS`) sit in each runner's `.env` on the host ([setup 6](../setup/README.md#ci-runner)).
- **CI has no deployment secrets.** It needs only `GITHUB_TOKEN`. The one other workflow, `discord-mentions.yml`, reads four Discord webhook secrets (`DISCORD_WEBHOOK_*`) and four username variables (`GH_USER_*`), set as in [discord-mentions 2](discord-mentions.md#add-the-secrets-and-variables). Nothing deploys from CI ([section 7](#7-deploying-and-rolling-back)), so no host credentials sit in GitHub.
- **Rotation:** change the value in the env file and `docker compose up -d --no-deps <service>`. Rotating `SURREAL_PASS` also needs `ALTER USER` (or a new `DEFINE USER … OVERWRITE`) as root first; rotating a RustFS key needs `rc admin user add` with the new secret first. A new `CONNECTOR_KEY` can't open the tokens sealed under the old one, so every connector's owner reconnects.

---

## 7. Deploying and rolling back

**Deploys are manual, by digest, from the host.** CI builds and pushes; a team member on the host decides when a digest goes live. This keeps host credentials out of GitHub and lets the rig, which sits behind home NAT, deploy the same way as a VM.

`infra/deploy.sh <service-variable> <image@sha256:…>`:
1. checks that the reference has a digest and that `docker pull` gets it;
2. copies `.env.compose` to `.env.compose.prev`, sets the variable, and appends `date, host, variable, old digest, new digest, operator` to `/srv/pdm/deploy.log`;
3. runs `docker compose up -d --no-deps <services using it>`, then waits for the health check (`api`) or for the worker's heartbeat on `/status` (workers);
4. prints the rollback command.

**Rolling back** is the same script with the previous digest from `deploy.log`. Two cases need more:
- **A schema or migration change.** `api` applies `backend/migrations/` at start ([database 1](../database/README.md#1-version-and-setup)). An older image may not run against a database a newer one migrated. Take `surreal export` before any deploy that changes `backend/migrations/`; roll back only to an image whose migrations the database has already applied, otherwise restore the export into a fresh datastore ([database 7](../database/README.md#7-operations)).
- **A seed change.** `seed-upload` only adds content-hashed keys ([storage 2](../storage/README.md#2-bucket-keys-and-access)), so an older `api` still finds its photos.

**Deploy order** when both change: control plane first (`api` serves the job and worker API), then the rig. Workers only pull, so a rig deploy never needs downtime on the control plane.

### First-time bootstrap

Once per environment, on the control-plane host:

1. Install Docker Engine with Compose v2; create `/srv/pdm/{env,backups}`; clone the repository to `/srv/pdm/repo`.
2. Write the env files from `infra/env/.env.example` with generated values; `docker login ghcr.io`.
3. `docker compose up -d surrealdb rustfs` and wait for both to be healthy.
4. **Database users** ([database 1](../database/README.md#1-version-and-setup)): `infra/bootstrap/surrealdb-users.sh` pipes these statements into `surreal sql --endpoint http://127.0.0.1:8000 --user root` from the pinned SurrealDB image, with the password from `.env.api` escaped into the statement (`\` and `'` escaped), never as an argument:
   `DEFINE NAMESPACE IF NOT EXISTS pdm; USE NS pdm; DEFINE DATABASE IF NOT EXISTS main; USE DB main; DEFINE USER IF NOT EXISTS api ON DATABASE PASSWORD '…' ROLES EDITOR;`
   That `surreal sql` reads statements from standard input non-interactively is *unverified*.
5. **Bucket and identities** ([storage 2](../storage/README.md#2-bucket-keys-and-access)): `infra/bootstrap/rustfs.sh` runs storage 2's setup script with `pdm-api.json` and `pdm-seed.json`, and adds a third identity, `PDMBACKUP01` with `pdm-backup.json` (`s3:ListBucket` and `s3:GetObject` on `pdm-images` only), for the backup that [storage 6](../storage/README.md#6-operations) says needs a read-only key.
6. Put the host's own read-only `HF_TOKEN` in `.env.models`. `model-fetch` fills the `models` volume on every `up` ([vision 6](../vision/README.md#delivering-model-files-to-the-backend)).
7. Set the digests and the ingress (`COMPOSE_PROFILES`, `CADDY_SITE`, section 5) in `.env.compose`, and `docker compose up -d`. Then run `backend/tools/smoke/` against the public URL.

The rig's bootstrap is steps 1, 2 (only its own env files, `.env.models` included) and 6, then `docker compose -f infra/rig.compose.yml up -d`. Its worker tokens must already be in `api`'s `WORKER_TOKENS`.

Both scripts are idempotent: they use `IF NOT EXISTS` and tolerate an existing bucket, so rerunning one is safe.

---

## 8. Operations hooks

**Backups.** `infra/ops/backup.sh`, run nightly at 03:30 local time by a systemd timer (`pdm-backup.timer`) on the control-plane host, and by hand before every upgrade or migration deploy:
- `surreal export` of `pdm`/`main` from the pinned SurrealDB image to `/srv/pdm/backups/surreal-<date>.surql`, as [database 7](../database/README.md#7-operations) gives it, gzipped. The password comes from `.env.backup` through the environment, not the command line (whether `surreal export` reads `SURREAL_PASS` from the environment is *unverified*).
- `rclone sync` of `pdm-images` with the `PDMBACKUP01` key, as [storage 6](../storage/README.md#6-operations) gives it, from `rclone/rclone:1.75.2` (✓ `sha256:2687085f718d3c628f7fdfb77c52a1d344332aed543110bb88088f2c70d43eb5`) on the host network.
- Both copied to the off-host remote ([open question 4](#10-open-questions)); 14 nightly exports kept locally and remotely.
- The script exits non-zero on any failure, and the timer's unit logs to the journal. A restore drill on a scratch host is an S7 exit check ([section 9](#9-roadmap)).

**Logs.** Every service logs to stdout: `api` and the workers as JSON ([backend 10](../backend/README.md#10-observability)), Caddy as JSON, RustFS with `RUSTFS_OBS_LOG_STDOUT_ENABLED`. Docker's `json-file` driver keeps 5 files of 10 MB per container. Read them with `docker compose logs -f --since 10m <service>`; filter JSON with `jq`. No log shipping for the hackathon.

**Health.**

| Probe | What it says | Used by |
|---|---|---|
| `api` `GET /healthz` | The process serves requests | Compose health check; `deploy.sh`; an external uptime check every minute ([open question 5](#10-open-questions)) |
| `api` `GET /status` | Workers, their heartbeats and versions, queue depth | People during the demo; `deploy.sh` after a worker deploy |
| SurrealDB `/surreal is-ready` | Ready for queries | Compose |
| RustFS `/health/ready` | Storage and IAM ready | Compose |
| vLLM `/health` | Model loaded | Compose, on the rig |

**The CI runners.** Two hooks in [`infra/ci-runner/`](../../infra/ci-runner/), installed for `ci-runner` by `install.sh` ([setup 6](../setup/README.md#ci-runner)); both are *unverified* until they have run on the rig.
- **`job-started.sh`** is the runners' job-started hook (`ACTIONS_RUNNER_HOOK_JOB_STARTED` in each runner's `.env`). It removes containers and networks older than 2 hours, left by cancelled jobs, and never fails a job. No job runs longer than 90 minutes, so nothing live is touched, and it keeps the buildx builder's container (`buildx_buildkit_pdm-ci0`).
- **`ci-prune.timer`** runs `prune.sh` weekly: it trims the buildx cache to 30 GB, removes unused images built more than 7 days ago (Docker's `until` filter reads the build date, so the pinned SurrealDB and RustFS images are pulled again after each prune) and unused volumes, and deletes build trees unused for 14 days. Expected disk use is under 100 GB. A tree in daily use is never pruned and grows with each toolchain or dependency change; if the disk runs low, delete `~/ci/target` while no job runs.
- Restart the runners when idle after installing: `systemctl --user restart gh-runner@1 gh-runner@2`. Check them with `systemctl --user status gh-runner@1 gh-runner@2 --no-pager`.

**Disk.** Docker's images, logs and volumes on the control plane need about 20 GB plus the photos and backups ([storage 6](../storage/README.md#6-operations) sizes them). `docker image prune` after a deploy removes old layers; images stay pullable from GHCR by digest for rollback.

---

## 9. Roadmap

Each item belongs to a slice of the [backend roadmap](../backend/roadmap.md). Items in the same slice can go in one pull request when they're small.

| # | Work | Slice | Exit check |
|---|---|---|---|
| 1 | `.github/workflows/ci.yml` (first as `backend.yml`) with `changes`, `lint`, `test`, `examples`, `docs` and `infra`; `dependabot.yml` for the actions | [S0](../backend/roadmap.md#4-s0-foundation) | ✓ A pull request with a deliberate Clippy warning fails `lint` (#21, `clippy::len_zero`, 2026-10-16); ✓ a docs-only pull request finishes in under 3 minutes with the Rust jobs skipped (#27: 43 s, 2026-10-16) |
| 2 | Add the required check `ci` to the `main` ruleset | S0 | ✓ The ruleset requires `ci` (2026-10-16). Not yet tested: a pull request with a failing `docs` job can't be merged |
| 3 | The `live` job with SurrealDB and RustFS service containers | S0 | ✓ Both ignored tests and `check_examples.py --live --no-test` pass on a push to `main` (2026-10-16) |
| 4 | `backend/Dockerfile` and `docker-bake.hcl` with `api`, `worker` and `worker-cuda` | S0 | ✓ `docker buildx bake` builds all three on a laptop; `docker run … api` exits with a configuration error naming a missing variable, as UID 10100 |
| 5 | `infra/dev.compose.yml` and `infra/env/.env.example` | S0 | ✓ `docker compose -f infra/dev.compose.yml up -d`, then `cargo nextest run --workspace --run-ignored only`, passes on a laptop |
| 6 | The `images` job of `ci.yml`, pushing to GHCR on merge (written in S0, first run on a merge to `main`) | [S1](../backend/roadmap.md#5-s1-walking-skeleton) | ✓ A merge to `main` publishes three images with `sha-` tags and prints their digests in the run summary (#24's merge, run 38987473159, 2026-10-16: `api` `sha256:3233f5a7db2eae06b90501ba403cb06987913fb97995a91efcc204311f173fb2`, `worker` `sha256:1d7324d1fdb70d1541054eb8915148bb29a79ad8286b55577cf47cf6e017f2c4`, `worker-cuda` `sha256:3d313397fe77a00cd48f945c66fe3de9dd61131e29bbd9217780b799bde1c5e1`). Not yet tested: a host pulls them with a `read:packages` token |
| 7 | `infra/control-plane.compose.yml` with the `tailscale` and `cloudflare` profiles, the Caddyfile, `infra/ingress/` and `deploy.sh` | S1 | The roadmap's S1 smoke script passes against the public URL; `docker compose stop api` during an upload finishes within 190 s and the container exits with code 0, not 137 (SIGKILL) |
| 8 | `infra/bootstrap/` scripts and the first bootstrap | S1 | Rerunning both scripts changes nothing; `api` signs in as `api`, not root, after start-up |
| 9 | Add `live` to `ci`'s `needs` in `ci.yml` | S1 | ✓ Three consecutive green `live` runs on `main` (four: #23, #18, #25, #24) |
| 10 | `infra/rig.compose.yml` with the perception worker on the 3070 | [S2](../backend/roadmap.md#6-s2-perception-site-data-and-knowledge) | The worker's heartbeat on `/status` shows `ort_ep=cuda` and the 3070's UUID in `nvidia-smi` inside the container, and nothing else |
| 11 | The `seed-upload` image target and its compose service before `api` | S2 | A deliberately failing `seed-upload` keeps `api` down; a passing one lets `api` start, and the species photos load through signed URLs |
| 12 | The vLLM sidecars on the 5070 Ti and the 9070 XT, or llama.cpp on the 9070 XT, with health-gated workers | [S3](../backend/roadmap.md#7-s3-vlm-workers-and-the-fallback) | Each VLM worker claims only after its sidecar is healthy; `rocm-smi` and `nvidia-smi` show each sidecar on its own card only |
| 13 | The fallback worker on the control plane | S3 | With every rig service stopped, the fallback completes a queued job (roadmap S3 exit check) |
| 14 | `infra/ops/backup.sh`, its timer and the off-host remote | [S7](../backend/roadmap.md#11-s7-hardening-and-s8-ship) | A restore of last night's export and bucket copy on a scratch host answers the smoke script |
| 15 | Final deploy by digest and a release tag | S8 | Every running container's image digest equals the one in `deploy.log`, and the tag's commit built those digests |
| 16 | Self-hosted runners `rig-1` and `rig-2` on the 3070 host, `RUNNER_LABELS` and `infra/ci-runner/` | S0 | A pull request's every job runs on `rig-1` or `rig-2` and `ci` passes; `app.id` of the `ci` check run is 15368. ✓ Pull request #77 (2026-10-19): every job ran on `rig-1` or `rig-2`, `ci` passed, and its `app.id` was 15368 |

---

## 10. Open questions

1. **Actions minutes.** *Answered on 2026-10-16:* the team runs the design as written on GitHub's runners. *Answered on 2026-10-19:* the budget ran out, so every job runs on two self-hosted runners on the rig, as [section 2](#runners-and-budget) describes: their own unprivileged user, a rootless Docker daemon, no access to the rig's env files, and `RUNNER_LABELS` to switch back. Whether self-hosted jobs run with the budget exhausted is *unverified* until the first run. Plan B, if they don't: a local script that reproduces `ci.yml`, and our own GitHub App that posts the `ci` check.
2. **Who deploys.** Manual deploys from the host (this design) or a pull-based updater that follows the `main` tag? Pull-based saves a step but deploys without a person choosing the moment.
3. **Architecture of the control-plane VM**, if it's a VM: amd64 needs nothing more; arm64 adds an arm64 build of `api` and `worker`.
4. **The off-host backup remote**: an S3-compatible bucket from a provider the team already has credits with, and who holds its key.
5. **An external uptime check** on `/healthz` during judging, and who gets its alert.
6. **Where the deployed digests are recorded.** `deploy.log` on each host (this design) or a committed file in `infra/`, which would put deploys through pull requests.
7. **`api` and HEIC.** *Answered on 2026-10-16:* `api`'s manifest declares `heic = ["vision/heic"]`, like `worker`'s, and the image builds with `--features heic`.
8. **Who may create `v*` tags.** *Answered on 2026-10-16:* no tag ruleset for the hackathon. Anyone with write access can push a `v*` tag, but `images` refuses one whose commit isn't on `main` or whose name isn't `vMAJOR.MINOR.PATCH[-pre]`, so only reviewed code is released. Revisit if more people get write access: a tag ruleset can limit creating, moving and deleting `v*` tags to admins.
9. **Hardening the CI runners.** Two steps would close most of what [Runners and budget](#runners-and-budget) lists. (a) A third runner as its own user, `ci-release`, with its own rootless daemon and builder, taking only `images` and `release`; its job-started hook exits 1 unless `GITHUB_EVENT_NAME` is `push` and `GITHUB_REF` is `refs/heads/main` or a `v*` tag, because a pull request can rewrite `runs-on`. (b) nftables rules for `meta skuid <ci-runner's uid>` that drop traffic to the tailnet (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`) and to loopback ports below 32768, which the live job's ephemeral ports never use. Both need `sudo` once on the rig.

---

## 11. Sources

- [GitHub Actions: workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax) (service `command` and `entrypoint`, path filters and required checks, concurrency) ✓
- [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) (private-repository runner sizes, labels) ✓
- [Self-hosted runners](https://docs.github.com/en/actions/reference/runners/self-hosted-runners) · [running scripts before or after a job](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/running-scripts-before-or-after-a-job) · [the runner's releases](https://github.com/actions/runner/releases) (v2.338.0; `DockerUtil.ParseDockerPort` read in its source ✓)
- [Rootless Docker](https://docs.docker.com/engine/security/rootless/) · [`docker buildx prune`](https://docs.docker.com/reference/cli/docker/buildx/prune/)
- [GitHub's 2026 pricing changes for Actions](https://resources.github.com/actions/2026-pricing-changes-for-github-actions/) ✓ (read 2026-10-19)
- [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions) and [GitHub Packages billing](https://docs.github.com/en/billing/concepts/product-billing/github-packages) ✓
- [Working with the Container registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry) (classic tokens, `read:packages`, the source label) ✓
- Action releases: [checkout](https://github.com/actions/checkout/releases), [setup-python](https://github.com/actions/setup-python/releases), [rust-cache](https://github.com/Swatinem/rust-cache/releases), [install-action](https://github.com/taiki-e/install-action/releases), [setup-buildx-action](https://github.com/docker/setup-buildx-action/releases), [login-action](https://github.com/docker/login-action/releases) ✓
- [ONNX Runtime v1.31.0 release](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0) (asset names and SHA-256) ✓
- Docker Hub: [rust](https://hub.docker.com/_/rust), [debian](https://hub.docker.com/_/debian), [nvidia/cuda](https://hub.docker.com/r/nvidia/cuda), [python](https://hub.docker.com/_/python), [caddy](https://hub.docker.com/_/caddy), [tailscale/tailscale](https://hub.docker.com/r/tailscale/tailscale), [cloudflare/cloudflared](https://hub.docker.com/r/cloudflare/cloudflared), [rclone/rclone](https://hub.docker.com/r/rclone/rclone), [rocm/onnxruntime](https://hub.docker.com/r/rocm/onnxruntime), [vllm/vllm-openai](https://hub.docker.com/r/vllm/vllm-openai), [rustfs/rc](https://hub.docker.com/r/rustfs/rc) ✓
- [cargo-chef](https://github.com/LukeMathWalker/cargo-chef) 0.1.78 ✓
- [Caddy `reverse_proxy`](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) (SSE flushing, trusted proxies) ✓
- [Tailscale Funnel](https://tailscale.com/kb/1223/funnel) (plans, ports, bandwidth limits, where TLS ends) and [Docker configuration parameters](https://tailscale.com/docs/features/containers/docker/docker-params) (`TS_AUTHKEY`, `TS_STATE_DIR`, `TS_SERVE_CONFIG`, userspace networking) ✓
- [Cloudflare quick tunnels](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/) (no SSE, changing hostname, no uptime guarantee) ✓
- [Compose file reference: services](https://docs.docker.com/reference/compose-file/services/) (`stop_grace_period`, `depends_on` conditions, device reservations)
- Project documents: [architecture 12](../architecture/README.md#12-deployment), [backend 12](../backend/README.md#12-builds-and-deployment), [worker 11–12](../backend/worker.md#11-multi-vendor-gpus), [database 1 and 7](../database/README.md#1-version-and-setup), [storage 1, 2 and 6](../storage/README.md#1-version-and-setup), [setup 4 and 6](../setup/README.md#6-the-gpu-rig-and-the-control-plane), [backend roadmap](../backend/roadmap.md), [vision 5 and 6](../vision/README.md#6-model-files-versions-and-pinning)
