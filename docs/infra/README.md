# PDM Infrastructure and CI

> **Status:** Draft v0.1.2, 2026-10-16. v0.1.2: section 4's vLLM health check is verified on the rig, and the running `vllm` service is mapped to `vllm-nv`. v0.1.1: `api`'s bucket default is now `pdm-images`, so section 4's note no longer flags it. v0.1: first design of the CI workflows, the container images, the three compose files, ingress, secrets, deploys by digest, operations hooks and the infrastructure work items of the [backend roadmap](../backend/roadmap.md). Design only: nothing here exists yet.

This document turns [architecture 12](../architecture/README.md#12-deployment) and [backend 12](../backend/README.md#12-builds-and-deployment) into files: what goes in `.github/` and `infra/`, how each image is built and pinned, and how the control plane and the rig are started, deployed and rolled back. It closes [roadmap gap 3](../backend/roadmap.md#12-gaps-found-while-writing-this-roadmap).

**Marks.** ✓ marks a fact read in its primary source on 2026-10-16: a GitHub release or the GitHub API for actions and ONNX Runtime, Docker Hub for images, the vendor's documentation otherwise. Nothing in this document has been run. *Unverified* marks anything else.

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

**In scope:** CI on GitHub Actions, the backend's container images and their registry, the compose files for local work, the control plane and the rig, Caddy and the tunnel, env files, deploys, backups, logs and health checks.

**Out of scope, owned elsewhere:**
- How model files reach `/models` on the control plane and the rig: [vision 6](../vision/README.md#6-model-files-versions-and-pinning). This document only mounts the directory read-only.
- How `api` applies the schema and the generated knowledge migrations at start, after `seed-upload`: [database 1](../database/README.md#1-version-and-setup). This document only orders the containers.
- The VLM sidecars' flags and the worker settings per card: [worker 11–12](../backend/worker.md#11-multi-vendor-gpus).
- The app's builds (Expo, EAS): [frontend](../frontend/README.md).

**Where the control plane runs is open** ([architecture open decision 2](../architecture/README.md#15-open-decisions), decided in [S1](../backend/roadmap.md#5-s1-walking-skeleton)). Everything below works in both cases: a small cloud VM with Caddy on ports 80 and 443, or the rig itself behind a Cloudflare Tunnel. The compose files are the same; one variable and one profile differ ([section 5](#5-ingress)).

```
PDM/
├─ .github/
│  ├─ workflows/
│  │  ├─ backend.yml           checks on every pull request and on main (section 2)
│  │  └─ images.yml            builds and pushes the images on merge to main (section 3)
│  └─ dependabot.yml           weekly updates of the pinned action SHAs, nothing else
├─ backend/
│  ├─ Dockerfile               multi-stage: api, worker, worker-cuda (section 3)
│  ├─ docker-bake.hcl          the three targets, built in one BuildKit run
│  └─ .dockerignore            target/, tools/, .git
├─ infra/
│  ├─ dev.compose.yml          SurrealDB and RustFS for a laptop and for the ignored tests
│  ├─ control-plane.compose.yml caddy · api · surrealdb · rustfs · fallback-worker · seed-upload · cloudflared (profile)
│  ├─ rig.compose.yml          perception-worker · vllm-nv · vlm-worker-nv · vllm-amd · vlm-worker-amd
│  ├─ rig-amd.compose.yml      only if the RX 9070 XT sits in a second host
│  ├─ caddy/Caddyfile
│  ├─ cloudflared/config.yml   the tunnel's ingress rules; its credentials are a secret
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

`aacruzgon/GreenLens` is a private repository. GitHub's standard runner for a private repository has **2 vCPUs, 8 GB of RAM and 14 GB of SSD** (✓ runner reference), half a public repository's CPUs. A private repository's Actions time counts against the owner's plan: 2,000 minutes a month on Free, 3,000 on Pro, and 10 GB of cache per repository (✓ Actions billing).

At about 25 runner-minutes for a backend push that runs every job, 120 such pushes use the Pro allowance. Build week will exceed that unless the workflow skips what a change doesn't touch. The design therefore:
- runs a cheap `changes` job first and skips each job whose inputs didn't change. A job skipped by its `if:` reports success, so it can be a required check; a workflow skipped by a `paths:` filter stays pending and blocks the merge (✓ workflow syntax);
- cancels a pull request's running workflow when a new commit arrives;
- runs the `live` job only when `store`, `blob`, `backend/migrations/` or the Rust skills change, and on every push to `main`;
- saves caches only from `main`, so pull requests read them and the 10 GB stays shared;
- builds images only after a merge, never on pull requests.

Whether the account allows spending past the included minutes, or whether a self-hosted runner on the rig takes the heavy jobs, is [open question 1](#10-open-questions).

### Actions, pinned by commit

| Action | Version | Commit |
|---|---|---|
| `actions/checkout` | v7.0.1 (2026-07-20) | `3d3c42e5aac5ba805825da76410c181273ba90b1` ✓ |
| `actions/setup-python` | v7.0.0 (2026-07-20) | `5fda3b95a4ea91299a34e894583c3862153e4b97` ✓ |
| `Swatinem/rust-cache` | v2.9.2 (2026-08-06) | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` ✓ |
| `taiki-e/install-action` | v2.87.29 (2026-10-16) | `8cca0918f8471c33515202b73f56ab27f2bed283` ✓ |
| `docker/setup-buildx-action` | v4.4.1 (2026-09-16) | `f87e5991a6d7451dcb8d9637bfbc97413f497069` ✓ |
| `docker/login-action` | v4.6.0 (2026-07-29) | `dbcb813823bdd20940b903addbd779551569679f` ✓ |

Each is the latest release on 2026-10-16, and each commit is the one its tag points to (`gh api repos/<owner>/<repo>/commits/<tag>`). Workflows reference the full SHA with the version in a comment, so a moved tag changes nothing. Dependabot proposes updates weekly; each goes through a pull request like any other change. No other actions are used: images are built with `docker buildx bake` directly ([section 3](#3-images)).

Rust needs no action: the runner image has rustup, and the first `cargo` call installs 1.99.1 with rustfmt and Clippy from `backend/rust-toolchain.toml`. The runner is `ubuntu-26.04` (✓ a listed label), because its `libheif-dev` is 1.21.2 ([setup 4](../setup/README.md#4-backend-rust)) and the `heic` feature needs 1.19 or newer; Ubuntu 24.04 ships 1.17.6.

### `backend.yml`

**Triggers:** `pull_request` to `main`, `push` to `main`, and `workflow_dispatch`. **Permissions:** `contents: read`. **Concurrency:** one group per pull request (or ref), `cancel-in-progress` on pull requests only. Every job sets `working-directory: backend` where it runs cargo, and passes `--locked`, so a stale `Cargo.lock` fails.

| Job | Runs when | Steps | Warm / cold time (estimate) |
|---|---|---|---|
| `changes` | always | `git diff --name-only` against the base; outputs `rust`, `docs`, `infra`, `live` as booleans | 0.5 min |
| `lint` | `rust` | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo check -p worker --features cuda`; `cargo check -p vision --features cuda,migraphx,webgpu`; `cargo check -p vision --features heic` (installs `libheif-dev`) | 5 / 14 min |
| `test` | `rust` | install cargo-nextest 0.9.148 with `taiki-e/install-action` (`tool: cargo-nextest@0.9.148`); `cargo nextest run --workspace --locked`. This includes the `api` test that fails when `backend/openapi.json` is stale | 7 / 18 min |
| `examples` | `rust` or the skills changed | `setup-python` 3.12; `python3 .claude/skills/rust/scripts/check_examples.py` | 3 / 8 min |
| `docs` | `rust` or `docs` | `backend/tools/docgen/regen_all.sh` then `git diff --exit-code docs/backend`; `check_fences.py docs/backend`; `checklinks.py` over `AGENTS.md`, `CLAUDE.md` and `docs/**/*.md` | 1 min |
| `live` | `live`, or a push to `main` | service containers below; `cargo nextest run -p store -p blob --run-ignored only --locked`; `check_examples.py --live` | 5 / 12 min |
| `infra` | `infra` | `docker compose -f <file> --env-file infra/env/.env.example config -q` for each compose file; `caddy validate` in the pinned Caddy image; `bash -n` and `shellcheck` on `infra/**/*.sh` | 1 min |

The GPU feature checks need no GPU and no CUDA: `ort` uses `load-dynamic`, so nothing links against ONNX Runtime at build time ([versions.md](../../.claude/skills/rust/versions.md#notes-and-things-not-confirmed)). They catch code behind `#[cfg(feature = "cuda")]` that the default build never compiles.

**Caching.** `Swatinem/rust-cache` with `workspaces: backend -> target`, one `shared-key` per job (`lint`, `test`, `examples`, `live`) because each compiles a different profile or feature set, and `save-if: ${{ github.ref == 'refs/heads/main' }}`. Each key holds about 1–2 GB (*unverified*, an estimate from a 754-crate tree); four keys fit the 10 GB. Times above are estimates for 2 vCPUs; the first run on `main` records the real ones here.

**The live job's services.** GitHub Actions service containers accept `command` and `entrypoint` (✓ workflow syntax), so SurrealDB can start with the in-memory engine the store test expects. Ports match the defaults in `crates/store/tests/server.rs` and `crates/blob/tests/rustfs.rs`; the credentials are the throwaway ones those tests hard-code.

```yaml
live:
  needs: changes
  if: needs.changes.outputs.live == 'true' || github.event_name == 'push'
  runs-on: ubuntu-26.04
  timeout-minutes: 30
  services:
    surrealdb:
      image: surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af
      command: start --user root --pass root memory
      ports: ["127.0.0.1:18000:8000"]
      options: >-
        --health-cmd "/surreal is-ready --endpoint http://127.0.0.1:8000"
        --health-interval 5s --health-retries 12
    rustfs:
      image: rustfs/rustfs:1.0.1@sha256:1803faef57627e2d9c2e7d89d655d712ddded5389040054987163043fecb6a3c
      env:
        RUSTFS_VOLUMES: /data
        RUSTFS_ACCESS_KEY: TESTROOT          # the test's hard-coded key; CI only
        RUSTFS_SECRET_KEY: testrootsecret
        RUSTFS_CONSOLE_ENABLE: "false"
        RUSTFS_CHECK_UPDATE: "false"
      ports: ["127.0.0.1:19000:9000"]
      options: >-
        --health-cmd "curl -fsS http://127.0.0.1:9000/health/ready"
        --health-interval 5s --health-retries 12 --health-start-period 30s
  env:
    PDM_TEST_SURREALDB_URL: ws://127.0.0.1:18000
    PDM_TEST_RUSTFS_URL: http://127.0.0.1:19000
  steps:
    - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
    - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2
      with: { workspaces: "backend -> target", shared-key: live, save-if: "${{ github.ref == 'refs/heads/main' }}" }
    - uses: taiki-e/install-action@8cca0918f8471c33515202b73f56ab27f2bed283 # v2.87.29
      with: { tool: cargo-nextest@0.9.148 }
    - run: cargo nextest run -p store -p blob --run-ignored only --locked
      working-directory: backend
    - run: python3 .claude/skills/rust/scripts/check_examples.py --live
```

`check_examples.py --live` starts its own SurrealDB container with `docker run` and an ephemeral port, so it doesn't clash with the service. The images and digests are those of [database 1](../database/README.md#1-version-and-setup) and [storage 1](../storage/README.md#1-version-and-setup) (✓ both digests match Docker Hub on 2026-10-16).

### Required checks

The `main` ruleset ("Require PRs for main and master") gets a `required_status_checks` rule, added with `gh api -X PUT repos/aacruzgon/GreenLens/rulesets/<id>` by an admin:

| When | Checks added | Why then |
|---|---|---|
| The pull request that adds `backend.yml` has merged and run green on `main` | `lint`, `test`, `examples`, `docs`, `infra` | AGENTS.md: add each check once its workflow exists |
| S1 closes, after `live` has passed on three consecutive pushes to `main` | `live` | Service containers are the likeliest source of flakes; prove them first |
| Never | `images` | It runs after the merge |

`strict_required_status_checks_policy` stays `false`: requiring a branch to be up to date with `main` would rerun every job after each merge to `main`, which the minutes can't afford. The check names are the job names, so a renamed job must be renamed in the ruleset in the same pull request.

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

**Why two builders.** `api` links the system libheif for HEIC uploads, which needs 1.19 or newer: Debian 13 (trixie) ships 1.19.8, Debian 12 doesn't ([setup 4](../setup/README.md#4-backend-rust)). The worker binary must also run in the CUDA image, which is Ubuntu 24.04 with glibc 2.39, while trixie's glibc is 2.41; a binary built against a newer glibc fails to start on an older one. So `worker` is built on Debian 12 (glibc 2.36), which runs on both trixie and Ubuntu 24.04, and it doesn't use libheif.

The `rust` tags are a day old, so re-check their digests when the Dockerfile lands. Docker Hub's digest for a multi-architecture tag is the image index, so the same pin serves amd64 and arm64.

### `backend/Dockerfile`

One file, four stages per binary: `cargo-chef` plans the dependency graph, a cached layer builds only the dependencies, then the workspace builds, then a slim runtime copies the binary. `cargo-chef` 0.1.78 (✓ crates.io) is installed with `cargo install --locked` in the official Rust image rather than taken from a third-party builder image. No `# syntax=` line: the frontend built into BuildKit handles cache mounts.

```dockerfile
ARG RUST_TRIXIE=rust:1.99.1-slim-trixie@sha256:4df2ae8cfdfebdfc13a65da7f253b73d68983dc0339f2719bc17a279c913b13d
ARG RUST_BOOKWORM=rust:1.99.1-slim-bookworm@sha256:a24e292b4af7bdee351a31d3a6379972a5652f0ea22a1b6d6883324ac4f5097c
ARG DEBIAN=debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f
ARG CUDA=nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04@sha256:14f6d08d1cd4a96effbfe3101d0b56326f552c199d05e4979ee0bd616df5811b

# --- api: built on trixie for libheif 1.19 ---
FROM ${RUST_TRIXIE} AS chef-api
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config cmake clang libheif-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --version 0.1.78 --locked
WORKDIR /src

FROM chef-api AS plan-api
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef-api AS build-api
COPY --from=plan-api /src/recipe.json .
RUN cargo chef cook --release --locked -p api --features vision/heic --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked -p api --features vision/heic

# --- worker: built on bookworm so it runs on trixie and Ubuntu 24.04 ---
FROM ${RUST_BOOKWORM} AS chef-worker
#   same three stages; the build line is:
#   cargo build --release --locked -p worker --features cuda,migraphx,webgpu

# --- runtimes ---
FROM ${DEBIAN} AS api
ARG ORT_SHA256=cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl libheif1 \
 && curl -fsSL -o /tmp/ort.tgz https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-linux-x64-1.31.0.tgz \
 && echo "${ORT_SHA256}  /tmp/ort.tgz" | sha256sum -c - \
 && mkdir -p /opt/onnxruntime && tar -xzf /tmp/ort.tgz -C /opt/onnxruntime --strip-components=1 \
 && rm /tmp/ort.tgz && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10100 --no-create-home pdm
COPY --from=build-api /src/target/release/api /usr/local/bin/api
ENV ORT_DYLIB_PATH=/opt/onnxruntime/lib/libonnxruntime.so.1.31.0 ORT_EP=cpu
USER 10100
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/api"]
```

- **Release profile.** The images build with the workspace's `[profile.release]` unchanged: `lto = "thin"`, `codegen-units = 1`, `panic = "unwind"`, `debug = "line-tables-only"`, `overflow-checks = true`, and no `strip`, so a panic's backtrace keeps its frames ([backend 12](../backend/README.md#12-builds-and-deployment)).
- **One worker binary for every image.** With `load-dynamic`, the `cuda`, `migraphx` and `webgpu` features can be enabled together; only `download-binaries` forbids that ([versions.md](../../.claude/skills/rust/versions.md#notes-and-things-not-confirmed)). The worker is compiled once per BuildKit run, and each worker image copies the same file. `ORT_EP` picks the provider at start.
- **Non-root.** Every image runs as UID 10100, `pdm`, with no home and no shell login. Compose adds `read_only: true`, a 512 MB tmpfs on `/tmp`, `cap_drop: [ALL]` and `no-new-privileges`. RustFS's UID 10001 and SurrealDB's 65532 are unaffected, since each container owns its own volume.
- **PID 1.** The binaries handle SIGTERM themselves and spawn no children, so no init process is added; the exec-form `ENTRYPOINT` delivers the signal directly.
- **ONNX Runtime archives** are fetched from Microsoft's v1.31.0 release (published 2026-10-09) and checked against the SHA-256 the release lists for each asset (✓ GitHub API): CPU `cc5c72ba…eb03a57`, 11.4 MB; CUDA 13 `2867dec9…2c81135`, 244.6 MB. A changed archive fails the build.
- **Labels.** `org.opencontainers.image.source=https://github.com/aacruzgon/GreenLens` links each package to the repository (✓ GHCR docs), plus `revision` (the commit) and `created`.

### The images

| Image | Runtime base | Binary and features | ONNX Runtime | Runs |
|---|---|---|---|---|
| `api` | trixie-slim + `libheif1`, CA certificates, `curl` for the health check | `api`, `vision/heic` | 1.31.0 CPU, `ORT_EP=cpu` | Control plane |
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
- **Who pushes:** only `images.yml`, on a push to `main` that touches `backend/`, `ml/knowledge/` or the workflow, and on `workflow_dispatch`. It has `packages: write` and logs in with `GITHUB_TOKEN`, which links each package to the repository (✓ GHCR docs). No person pushes images.
- **How:** `docker/setup-buildx-action`, `docker/login-action`, then `docker buildx bake -f backend/docker-bake.hcl --push --metadata-file meta.json`. One bake run builds `api`, `worker` and `worker-cuda`, so the worker's builder stages run once. Layers are cached in the Actions cache (`type=gha`, `mode=max`, one scope per target). `seed-upload` is a separate bake target with `ml/knowledge` as its context, added when its Dockerfile exists.
- **Platform:** `linux/amd64`. If the control plane goes on an arm64 VM, `api` and `worker` gain `linux/arm64`, built natively on the `ubuntu-24.04-arm` runner (✓ listed for private repositories) with ONNX Runtime's `linux-aarch64` archive ([open question 3](#10-open-questions)).
- **Tags:** `sha-<12 hex of the commit>` and `main`. Never `latest`. Tags are for people; deploys use the digest.
- **Digests:** the job writes each image's `containerimage.digest` from `meta.json` into the run summary as a ready line for `deploy.sh`, for example `api ghcr.io/aacruzgon/greenlens/api@sha256:…`.
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

For a laptop: SurrealDB and RustFS on the ports and with the throwaway credentials that the ignored tests and `check_examples.py` expect. Nothing here is a secret, and nothing here is used anywhere else.

```yaml
name: pdm-dev
services:
  surrealdb:
    image: surrealdb/surrealdb:v3.3.4@sha256:649a60a3b9c0177d9923c938e1c2563e48cde2a9d06ac2c0519714be719b02af
    command: start --user root --pass root --log info rocksdb:/data/db
    ports: ["127.0.0.1:18000:8000"]
    volumes: ["surrealdb-dev:/data"]
    healthcheck:
      test: ["CMD", "/surreal", "is-ready", "--endpoint", "http://127.0.0.1:8000"]
      interval: 5s
      retries: 12
  rustfs:
    image: rustfs/rustfs:1.0.1@sha256:1803faef57627e2d9c2e7d89d655d712ddded5389040054987163043fecb6a3c
    environment:
      RUSTFS_VOLUMES: /data
      RUSTFS_ACCESS_KEY: TESTROOT
      RUSTFS_SECRET_KEY: testrootsecret
      RUSTFS_CONSOLE_ENABLE: "false"
      RUSTFS_CHECK_UPDATE: "false"
      RUSTFS_OBS_LOG_STDOUT_ENABLED: "true"
    ports: ["127.0.0.1:19000:9000"]
    volumes: ["rustfs-dev:/data"]
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://127.0.0.1:9000/health/ready"]
      interval: 5s
      retries: 12
      start_period: 30s
  rustfs-init:                      # creates the bucket a local `api` uses, then exits
    image: rustfs/rc:v0.1.36@sha256:ab024bfebee49a750ce886b4c70963ccd9ddaa03f491704a90710641d7a26699
    depends_on: { rustfs: { condition: service_healthy } }
    entrypoint: ["sh", "-c", "rc alias set dev http://rustfs:9000 TESTROOT testrootsecret && rc mb --ignore-existing dev/pdm-images"]
volumes: { surrealdb-dev: {}, rustfs-dev: {} }
```

`docker compose -f infra/dev.compose.yml down -v` resets both; the `-v` is safe only in this file. A local `api` reads `SURREAL_URL=ws://127.0.0.1:18000` and `RUSTFS_ENDPOINT=http://127.0.0.1:19000` from the local section of `infra/env/.env.example`. Whether `rc mb` takes `--ignore-existing` is *unverified*; if it doesn't, the init tolerates the "already exists" error.

### `infra/control-plane.compose.yml`

```yaml
name: pdm
services:
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
    environment: { CADDY_SITE: "${CADDY_SITE:?}" }    # api.example.org on a VM; :80 behind the tunnel
    ports: ["${CADDY_HTTP:-80}:80", "${CADDY_HTTPS:-443}:443"]   # host-only behind the tunnel
    volumes: ["./caddy/Caddyfile:/etc/caddy/Caddyfile:ro", "caddy-data:/data", "caddy-config:/config"]
    depends_on: { api: { condition: service_healthy } }
    networks: [edge]
    logging: *logging
  cloudflared:
    profiles: [tunnel]
    image: cloudflare/cloudflared:2026.10.1@sha256:a15a3b50b8dabd57ac2090e9484c0247dba9dfa1f0df4d298d4317f9d69eb2d5
    restart: unless-stopped
    command: ["tunnel", "--no-autoupdate", "--config", "/etc/cloudflared/config.yml", "run"]
    volumes: ["./cloudflared/config.yml:/etc/cloudflared/config.yml:ro",
              "${PDM_ENV:-/srv/pdm/env}/tunnel.json:/etc/cloudflared/tunnel.json:ro"]
    depends_on: [caddy]
    networks: [edge]
    logging: *logging
networks: { edge: {}, data: {} }
volumes: { models: {}, surrealdb-data: {}, rustfs-data: {}, rustfs-logs: {}, caddy-data: {}, caddy-config: {} }
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

Architecture 12 chose Caddy in both hosting options, with a Cloudflare Tunnel when the control plane runs on the rig.

```caddyfile
{
	servers {
		trusted_proxies static private_ranges   # the tunnel connects from the edge network
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

| | Cloud VM | The rig, behind the tunnel |
|---|---|---|
| `CADDY_SITE` | `api.<domain>` | `:80` |
| Ports published | 80 and 443 on the host | host-only: `CADDY_HTTP=127.0.0.1:8081`, `CADDY_HTTPS=127.0.0.1:8443` |
| TLS | Caddy's automatic HTTPS from Let's Encrypt, certificates in `caddy-data` | Cloudflare's edge; the tunnel carries plain HTTP from Cloudflare to `caddy:80` |
| Started with | `docker compose -f infra/control-plane.compose.yml up -d` | the same, plus `--profile tunnel` |
| Workers on the rig reach `api` at | `https://api.<domain>` | `https://api.<domain>` through Cloudflare, the same as from anywhere |

- **SSE.** Caddy flushes `text/event-stream` responses immediately, whatever `flush_interval` says (✓ Caddy docs), so no proxy setting is needed. `api` already sends `Cache-Control: no-cache` and a keepalive comment every 15 seconds ([architecture 12](../architecture/README.md#12-deployment)), which keeps an idle stream under Cloudflare's idle timeout (100 s, *unverified*).
- **Client addresses.** Caddy ignores incoming `X-Forwarded-For` unless the peer is a trusted proxy (✓ Caddy docs). Behind the tunnel the peer is `cloudflared` on a private address, so the header is trusted; on a VM, private ranges never reach Caddy directly.
- **Upload size.** Caddy sets no body limit; `api` enforces its own. Cloudflare's Free plan caps a request body at 100 MB (*unverified*), far above a photo.
- **The tunnel** is locally managed: `infra/cloudflared/config.yml` (committed, no secrets) names the tunnel and routes `api.<domain>` to `http://caddy:80`, with a final `http_status:404` rule. Its credentials file, `tunnel.json`, is created once with `cloudflared tunnel create` and lives only in `/srv/pdm/env/`.
- **What is never published:** SurrealDB, RustFS, the vLLM sidecars and `api`'s port 8080. Only Caddy faces the internet.

---

## 6. Secrets and configuration

**Rule:** no secret is ever in the repository, an image, a compose file or a command line. `.env` and `.env.*` are ignored by git ([AGENTS.md, secrets](../../AGENTS.md#secrets)); the committed `!.env.example` exception matches only that exact name, so every real env file below is named `.env.<service>` and is ignored automatically.

| File, in `/srv/pdm/env/` (mode 0600) | Read by | Holds |
|---|---|---|
| `.env.compose` | `docker compose --env-file` | Image digests (`API_IMAGE`, `WORKER_IMAGE`, `WORKER_CUDA_IMAGE`, `SEED_UPLOAD_IMAGE`), `CADDY_SITE`, `CADDY_HTTP`, `CADDY_HTTPS`; on the rig the GPU UUIDs, the render node and group ids. Not secret, but host-specific |
| `.env.surrealdb` | `surrealdb` | `SURREAL_USER`, `SURREAL_PASS`: the root user, created on first start |
| `.env.rustfs` | `rustfs` | `RUSTFS_ACCESS_KEY`, `RUSTFS_SECRET_KEY`: the root identity |
| `.env.api` | `api` | `SURREAL_PASS`, `SURREAL_ROOT_PASS` (the schema's namespace statement needs root, database 1), `RUSTFS_ACCESS_KEY=PDMAPI01`, `RUSTFS_SECRET_KEY`, `RUSTFS_BUCKET=pdm-images`, `API_PUBLIC_BASE`, `API_WEB_ORIGINS`, `API_URL_SIGNING_KEY` (32 bytes or more), `WORKER_TOKENS` (`worker_id=token` pairs), `CLERK_ISSUER`, `CLERK_SECRET_KEY`, `CLERK_WEBHOOK_SECRET`, `LLM_API_KEY`, optional `EXPO_ACCESS_TOKEN`, `OPEN_METEO_API_KEY`, `JEV_URL` and `JEV_API_KEY` |
| `.env.seed` | `seed-upload` | `PDMSEED01`'s keys |
| `.env.models` | `model-fetch` | `HF_TOKEN`: a fine-grained Hugging Face token that can only read `pdm-models` ([vision 6](../vision/README.md#delivering-model-files-to-the-backend)), one per host |
| `.env.fallback` | `fallback-worker` | `WORKER_TOKEN`, `LLM_API_KEY` |
| `.env.<worker id>` | each rig worker | `WORKER_TOKEN`; `VLM_API_KEY` for the VLM workers and their sidecar |
| `.env.backup` | `ops/backup.sh` | `SURREAL_ROOT_PASS`, `PDMBACKUP01`'s keys, the off-host remote's credentials |
| `tunnel.json` | `cloudflared` | The tunnel's credentials |

- **`infra/env/.env.example`** lists every variable of every file above, grouped under a `# --- .env.api ---` style heading, with placeholders only (`change-me`, `GPU-00000000-…`, `sk-placeholder`), plus a local section for running `api` and `worker` against `dev.compose.yml`. The `infra` CI job renders each compose file against it, so a variable a compose file needs and the example lacks fails CI.
- **Generating secrets:** `openssl rand -hex 32` for passwords, signing keys and worker tokens. RustFS access keys use A–Z and 0–9 only ([storage 1](../storage/README.md#1-version-and-setup)). A worker's token goes in two places: its own `.env.<worker id>` and `api`'s `WORKER_TOKENS`.
- **Clerk:** the development instance's issuer and keys until the team decides otherwise; never the production instance without asking ([roadmap S1](../backend/roadmap.md#5-s1-walking-skeleton)).
- **CI has no deployment secrets.** It needs only `GITHUB_TOKEN`. Nothing deploys from CI ([section 7](#7-deploying-and-rolling-back)), so no host credentials sit in GitHub.
- **Rotation:** change the value in the env file and `docker compose up -d --no-deps <service>`. Rotating `SURREAL_PASS` also needs `ALTER USER` (or a new `DEFINE USER … OVERWRITE`) as root first; rotating a RustFS key needs `rc admin user add` with the new secret first.

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
7. Set the digests in `.env.compose` and `docker compose up -d` (plus `--profile tunnel` on the rig). Then run `backend/tools/smoke/` against the public URL.

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

**Disk.** Docker's images, logs and volumes on the control plane need about 20 GB plus the photos and backups ([storage 6](../storage/README.md#6-operations) sizes them). `docker image prune` after a deploy removes old layers; images stay pullable from GHCR by digest for rollback.

---

## 9. Roadmap

Each item belongs to a slice of the [backend roadmap](../backend/roadmap.md). Items in the same slice can go in one pull request when they're small.

| # | Work | Slice | Exit check |
|---|---|---|---|
| 1 | `.github/workflows/backend.yml` with `changes`, `lint`, `test`, `examples`, `docs` and `infra`; `dependabot.yml` for the actions | [S0](../backend/roadmap.md#4-s0-foundation) | A pull request with a deliberate Clippy warning fails `lint`; a docs-only pull request finishes in under 3 minutes with the Rust jobs skipped |
| 2 | Add the five checks to the `main` ruleset | S0 | A pull request with a failing `docs` job can't be merged |
| 3 | The `live` job with SurrealDB and RustFS service containers | S0 | Both ignored tests and `check_examples.py --live` pass on a push to `main` |
| 4 | `backend/Dockerfile` and `docker-bake.hcl` with `api`, `worker` and `worker-cuda` | S0 | `docker buildx bake` builds all three on a laptop; `docker run … api` exits with a configuration error naming a missing variable, as UID 10100 |
| 5 | `infra/dev.compose.yml` and `infra/env/.env.example` | S0 | `docker compose -f infra/dev.compose.yml up -d`, then `cargo nextest run --workspace --run-ignored only`, passes on a laptop |
| 6 | `.github/workflows/images.yml`, pushing to GHCR on merge | [S1](../backend/roadmap.md#5-s1-walking-skeleton) | A merge to `main` publishes three images with `sha-` tags and prints their digests in the run summary; a host pulls them with a `read:packages` token |
| 7 | `infra/control-plane.compose.yml`, the Caddyfile, the tunnel config and `deploy.sh` | S1 | The roadmap's S1 smoke script passes against the public URL; `docker compose stop api` during an upload finishes within 190 s and the container exits with code 0, not 137 (SIGKILL) |
| 8 | `infra/bootstrap/` scripts and the first bootstrap | S1 | Rerunning both scripts changes nothing; `api` signs in as `api`, not root, after start-up |
| 9 | Add `live` to the ruleset | S1 | Three consecutive green `live` runs on `main` |
| 10 | `infra/rig.compose.yml` with the perception worker on the 3070 | [S2](../backend/roadmap.md#6-s2-perception-site-data-and-knowledge) | The worker's heartbeat on `/status` shows `ort_ep=cuda` and the 3070's UUID in `nvidia-smi` inside the container, and nothing else |
| 11 | The `seed-upload` image target and its compose service before `api` | S2 | A deliberately failing `seed-upload` keeps `api` down; a passing one lets `api` start, and the species photos load through signed URLs |
| 12 | The vLLM sidecars on the 5070 Ti and the 9070 XT, or llama.cpp on the 9070 XT, with health-gated workers | [S3](../backend/roadmap.md#7-s3-vlm-workers-and-the-fallback) | Each VLM worker claims only after its sidecar is healthy; `rocm-smi` and `nvidia-smi` show each sidecar on its own card only |
| 13 | The fallback worker on the control plane | S3 | With every rig service stopped, the fallback completes a queued job (roadmap S3 exit check) |
| 14 | `infra/ops/backup.sh`, its timer and the off-host remote | [S7](../backend/roadmap.md#11-s7-hardening-and-s8-ship) | A restore of last night's export and bucket copy on a scratch host answers the smoke script |
| 15 | Final deploy by digest and a release tag | S8 | Every running container's image digest equals the one in `deploy.log`, and the tag's commit built those digests |

---

## 10. Open questions

1. **Actions minutes.** Is the owner's plan Free (2,000 minutes) or Pro (3,000), and may the account spend past the included minutes? If not, a self-hosted runner on the rig could take `test`, `live` and `images`; it would need its own user, no access to the rig's env files, and a decision about running pull-request code on a home machine.
2. **Who deploys.** Manual deploys from the host (this design) or a pull-based updater that follows the `main` tag? Pull-based saves a step but deploys without a person choosing the moment.
3. **Architecture of the control-plane VM**, if it's a VM: amd64 needs nothing more; arm64 adds an arm64 build of `api` and `worker`.
4. **The off-host backup remote**: an S3-compatible bucket from a provider the team already has credits with, and who holds its key.
5. **An external uptime check** on `/healthz` during judging, and who gets its alert.
6. **Where the deployed digests are recorded.** `deploy.log` on each host (this design) or a committed file in `infra/`, which would put deploys through pull requests.
7. **`api` and HEIC.** `api` decodes uploads, but only `worker` has a `heic` feature today; the `api` image builds with `--features vision/heic`. Should `api`'s own manifest declare the feature instead?

---

## 11. Sources

- [GitHub Actions: workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax) (service `command` and `entrypoint`, path filters and required checks, concurrency) ✓
- [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) (private-repository runner sizes, labels) ✓
- [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions) and [GitHub Packages billing](https://docs.github.com/en/billing/concepts/product-billing/github-packages) ✓
- [Working with the Container registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry) (classic tokens, `read:packages`, the source label) ✓
- Action releases: [checkout](https://github.com/actions/checkout/releases), [setup-python](https://github.com/actions/setup-python/releases), [rust-cache](https://github.com/Swatinem/rust-cache/releases), [install-action](https://github.com/taiki-e/install-action/releases), [setup-buildx-action](https://github.com/docker/setup-buildx-action/releases), [login-action](https://github.com/docker/login-action/releases) ✓
- [ONNX Runtime v1.31.0 release](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0) (asset names and SHA-256) ✓
- Docker Hub: [rust](https://hub.docker.com/_/rust), [debian](https://hub.docker.com/_/debian), [nvidia/cuda](https://hub.docker.com/r/nvidia/cuda), [python](https://hub.docker.com/_/python), [caddy](https://hub.docker.com/_/caddy), [cloudflare/cloudflared](https://hub.docker.com/r/cloudflare/cloudflared), [rclone/rclone](https://hub.docker.com/r/rclone/rclone), [rocm/onnxruntime](https://hub.docker.com/r/rocm/onnxruntime), [vllm/vllm-openai](https://hub.docker.com/r/vllm/vllm-openai), [rustfs/rc](https://hub.docker.com/r/rustfs/rc) ✓
- [cargo-chef](https://github.com/LukeMathWalker/cargo-chef) 0.1.78 ✓
- [Caddy `reverse_proxy`](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) (SSE flushing, trusted proxies) ✓
- [Compose file reference: services](https://docs.docker.com/reference/compose-file/services/) (`stop_grace_period`, `depends_on` conditions, device reservations)
- Project documents: [architecture 12](../architecture/README.md#12-deployment), [backend 12](../backend/README.md#12-builds-and-deployment), [worker 11–12](../backend/worker.md#11-multi-vendor-gpus), [database 1 and 7](../database/README.md#1-version-and-setup), [storage 1, 2 and 6](../storage/README.md#1-version-and-setup), [setup 4 and 6](../setup/README.md#6-the-gpu-rig-and-the-control-plane), [backend roadmap](../backend/roadmap.md), [vision 5 and 6](../vision/README.md#6-model-files-versions-and-pinning)
