# PDM Developer Setup

> **Status:** v0.1.2, 2026-10-16. v0.1.2: backend developers start SurrealDB and RustFS with `infra/dev.compose.yml`. v0.1.1: Debian 13's libheif is 1.23.4, as the `api` image build installed it. v0.1: first version: what each team member installs on macOS, Windows and Linux, by role, with the versions the other documents pin.
> **Checked:** on 2026-10-16. The release files of ONNX Runtime 1.31.0 on GitHub, libheif's versions in Ubuntu and Debian, and the Node versions that React Native 0.86.3 and EAS CLI accept were read from their sources (✓). Items marked *unverified* weren't tried on that platform.

## Contents

1. [Who needs what](#1-who-needs-what)
2. [Everyone](#2-everyone)
3. [App (Expo)](#3-app-expo)
4. [Backend (Rust)](#4-backend-rust)
5. [Machine learning (Python)](#5-machine-learning-python)
6. [The GPU rig and the control plane](#6-the-gpu-rig-and-the-control-plane)
7. [Checking your setup](#7-checking-your-setup)
8. [Sources](#8-sources)

---

## 1. Who needs what

| Role | Sections | Works on |
|---|---|---|
| Everyone | [2](#2-everyone) | Git, Claude Code, Docker |
| App developer | 2, [3](#3-app-expo) | `app/` ([frontend](../frontend/README.md)) |
| Backend developer | 2, [4](#4-backend-rust) | `backend/` ([backend](../backend/README.md)) |
| ML developer | 2, [5](#5-machine-learning-python) | `ml/` ([vision](../vision/README.md)) |
| Whoever runs the rig or the control plane | 2, 4, [6](#6-the-gpu-rig-and-the-control-plane) | `infra/` ([architecture 12](../architecture/README.md#12-deployment)) |

What each operating system can do:

| | macOS (Apple silicon) | Windows 11 | Linux |
|---|---|---|---|
| App: iOS simulator and local iOS builds | Yes | No: use EAS Build in the cloud and a real iPhone | No: the same |
| App: Android emulator, web | Yes | Yes | Yes |
| Backend | Native | **In WSL 2** (Ubuntu); native Windows isn't supported | Native |
| ML training on CUDA | No: CPU only, for small checks | NVIDIA GPU, *unverified* | NVIDIA GPU, as on the rig |
| Running the GPU rig | No | No | Yes |

On Windows, do the backend and ML work inside WSL 2, and clone the repository inside the WSL file system (`~/…`), not under `/mnt/c`, which is slow and breaks file watching. The app can live on either side; keep one clone per tool chain.

## 2. Everyone

| Tool | Version | macOS | Windows | Linux |
|---|---|---|---|---|
| Git | any current | `xcode-select --install` or `brew install git` | [Git for Windows](https://git-scm.com/download/win), and `sudo apt install git` inside WSL | Your package manager |
| GitHub CLI | any current | `brew install gh` | `winget install GitHub.cli` | [Install guide](https://github.com/cli/cli/blob/trunk/docs/install_linux.md) |
| Node.js | 24 LTS or newer | `brew install node@24`, or [fnm](https://github.com/Schniz/fnm) | [Node.js installer](https://nodejs.org/) or `winget install OpenJS.NodeJS.LTS` | fnm, or [NodeSource](https://github.com/nodesource/distributions) |
| Python | 3.12 | `brew install python@3.12` | [python.org](https://www.python.org/downloads/) or `winget install Python.Python.3.12`; also in WSL | Your package manager, or [uv](https://docs.astral.sh/uv/) |
| Docker | Docker Desktop, or Engine with Compose v2 | [Docker Desktop](https://docs.docker.com/desktop/setup/install/mac-install/) | [Docker Desktop](https://docs.docker.com/desktop/setup/install/windows-install/) with the WSL 2 back end | [Docker Engine](https://docs.docker.com/engine/install/) and the Compose plugin |
| Claude Code | current | [Install](https://docs.anthropic.com/en/docs/claude-code/setup) | The same | The same |

- **Why Node and Python for everyone:**
  - The project's Claude Code start-up hook (`.claude/hooks/check-plugins.mjs`) runs on `node`.
  - The skills' scripts run on `python3`: the HIG page reader, and the Rust example checker, which needs Python 3.11 or newer.
  - React Native 0.86.3 accepts Node `^20.19.4 || ^22.13.0 || ^24.3.0 || >= 25.0.0` ✓.
- **Signed commits.** Every commit is signed with SSH ([AGENTS.md](../../AGENTS.md#commits)). Add your SSH key on GitHub as a *signing* key too, then:

  ```bash
  git config --global gpg.format ssh
  git config --global user.signingkey ~/.ssh/id_ed25519.pub
  git config --global commit.gpgsign true
  ```

- **Claude Code plugins.** Once per clone, run these, then `/reload-plugins` ([CLAUDE.md](../../CLAUDE.md#plugins-claude-code-only)):

  ```bash
  claude plugin install expo@claude-plugins-official --scope project
  claude plugin install clerk --marketplace clerk/skills --scope project
  ```

- **Secrets.** Copy each `.env.example` to `.env` and fill it in from the team's password manager. `.env` files never go into git.

## 3. App (Expo)

The pins are in [frontend 3](../frontend/README.md#3-stack-and-version-pins): Expo SDK 57, React Native 0.86.3 and `@clerk/expo` 4.12. The package manager is npm, with `package-lock.json` committed. Install native packages with `npx expo install`, never plain `npm install`, so they match the SDK.

| Tool | macOS | Windows | Linux |
|---|---|---|---|
| EAS CLI (`npm i -g eas-cli`, then `eas login`) | Yes | Yes | Yes |
| Expo account, invited to the team's Expo organisation | Yes | Yes | Yes |
| Xcode, from the App Store, with an iOS simulator runtime | Yes | — | — |
| CocoaPods (`brew install cocoapods`), only if a local iOS build asks for it | Yes | — | — |
| JDK 17 (Expo's guide uses Azul Zulu 17) | `brew install --cask zulu@17` | `winget install Azul.Zulu.17.JDK` | Your distribution's OpenJDK 17, or Zulu 17 |
| Android Studio, with an SDK Platform, Build-Tools, Platform-Tools and the Emulator from its SDK Manager | Yes | Yes | Yes |
| On a phone: Expo Go, for UI work in mock mode | Yes | Yes | Yes |

- **Environment variables for Android.** Set `JAVA_HOME` to the JDK 17 and `ANDROID_HOME` to the SDK folder Android Studio shows, and put `$ANDROID_HOME/platform-tools` on `PATH`. Expo's [environment guide](https://docs.expo.dev/get-started/set-up-your-environment/) shows each step per OS.
- **Watchman isn't needed** with SDK 57.
- **What runs where:**
  - **Expo Go and mock mode** (`EXPO_PUBLIC_API_MODE=mock`, [frontend 15](../frontend/README.md#15-mock-mode)) need no backend.
  - **The LiDAR module and push** need a development build ([frontend 17](../frontend/README.md#17-builds-environments-and-delivery)), built on EAS.
- **The LiDAR iPhone** needs Developer Mode turned on, and must be registered for ad-hoc installs with `eas device:create`. The team's Apple Developer Program membership signs the builds. One teammate holds it; the others are invited to the team.
- **Windows and Linux** can't build for iOS locally. Use `eas build --profile development --platform ios` in the cloud, and install the build on the registered iPhone.

## 4. Backend (Rust)

The pins are in [backend 4](../backend/README.md#4-stack-and-version-pins) and the Rust skills' [versions.md](../../.claude/skills/rust/versions.md).

| Tool | Version | How |
|---|---|---|
| rustup | current | [rustup.rs](https://rustup.rs/). macOS and Linux: the shell script; Windows: inside WSL |
| Rust | 1.99.1, with rustfmt and clippy | Chosen automatically by `backend/rust-toolchain.toml`. Before `backend/` exists: `rustup toolchain install 1.99.1 -c rustfmt -c clippy` |
| cargo-nextest | 0.9.148 | `cargo install cargo-nextest --version 0.9.148 --locked` |
| cargo-insta | 1.49.0 | `cargo install cargo-insta --version 1.49.0 --locked` |
| Build tools | | macOS: `xcode-select --install`. Ubuntu or WSL: `sudo apt install build-essential pkg-config cmake clang` |
| libheif | **1.19 or newer** | Needed by `libheif-rs`, through `pkg-config` |
| ONNX Runtime | 1.31.0, the CPU build | Needed to run `api`, which loads it at run time from `ORT_DYLIB_PATH` |
| SurrealDB and RustFS | the pinned images | Docker: `docker compose -f infra/dev.compose.yml up -d` starts both, with the ports and throwaway credentials the ignored tests expect ([infra 4](../infra/README.md#infradevcomposeyml)) |

**libheif by platform:**
- **macOS:** `brew install libheif pkg-config`. Homebrew's 1.23.1 built here ✓.
- **Ubuntu 26.04 LTS:** ships 1.21.2 ✓, so `sudo apt install libheif-dev` is enough.
- **Debian 13 (trixie):** ships 1.23.4 (✓ `1.23.4-1~deb13u1` from trixie's updates, installed by the `api` image build on 2026-10-16), which is enough.
- **Ubuntu 24.04 LTS:** ships only 1.17.6 ✓, which is too old. Use Ubuntu 26.04 instead, or build libheif 1.19 or newer from source. In WSL, pick the 26.04 image from `wsl --list --online`.
- **Fedora, Arch:** check that the packaged version is 1.19 or newer.

**ONNX Runtime 1.31.0.** Download the archive for your platform from the [release](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0) ✓:

| Platform | Archive | `ORT_DYLIB_PATH` |
|---|---|---|
| macOS, Apple silicon | `onnxruntime-osx-arm64-1.31.0.tgz` | `…/lib/libonnxruntime.dylib` |
| Linux or WSL, x86-64 | `onnxruntime-linux-x64-1.31.0.tgz` | `…/lib/libonnxruntime.so` |
| Linux, ARM64 | `onnxruntime-linux-aarch64-1.31.0.tgz` | `…/lib/libonnxruntime.so` |

There is no build for Intel Macs in this release ✓. A backend developer on an Intel Mac runs `api` in its Docker image instead. Run `api` locally with `ORT_EP=cpu` ([vision 5](../vision/README.md#5-runtimes-and-versions)). The perception worker's CUDA build is only needed on the rig (section 6).

**Checks.**
- The Rust skills' example checker: `python3 .claude/skills/rust/scripts/check_examples.py`.
  - Add `--live` to also run the SurrealDB tests against the pinned container. It needs Docker and cargo-nextest at the pinned version.
  - Add `--versions` to build every pinned crate together. That needs libheif.
- Store tests run on SurrealDB's embedded `kv-mem` engine, so `cargo nextest run` needs no server. With the dev services up, `cargo nextest run --workspace --run-ignored only` (in `backend/`) runs the two tests that need the real SurrealDB and RustFS.

## 5. Machine learning (Python)

The environment is in [training 2](../vision/training.md#2-environment). Use one virtual environment per clone, never the system Python.

```bash
python3.12 -m venv .venv && source .venv/bin/activate
pip install -r ml/requirements.lock
```

The lock file is `ml/requirements.lock`, with hashes. It doesn't exist until `ml/` lands.

| | macOS | Windows | Linux |
|---|---|---|---|
| PyTorch 2.14.1, torchvision 0.29.1 | CPU wheels, from the default index, for small checks only | In WSL 2 with an NVIDIA GPU: the cu130 index, *unverified* | With an NVIDIA GPU: `pip install torch==2.14.1 torchvision==0.29.1 --index-url https://download.pytorch.org/whl/cu130` |
| `onnxruntime-gpu` 1.31.0 | Use `onnxruntime` (CPU) instead | NVIDIA in WSL, *unverified* | NVIDIA, CUDA 13 |
| NVIDIA driver | — | 580 or newer on Windows; WSL uses it | **580.65.06 or newer** ([vision 5](../vision/README.md#5-runtimes-and-versions)) |

- **Where training runs:** on the 5070 Ti or on a Linux machine with an NVIDIA card ([architecture 3](../architecture/README.md#3-components)). Use a Mac for knowledge ingestion, label work, evaluation scripts and CPU checks of exported ONNX files.
- **vLLM** runs only in its pinned container on the rig. Don't install it in the training environment, since it pins its own torch.

## 6. The GPU rig and the control plane

Only the people who run these machines need this section. The design is in [architecture 12](../architecture/README.md#12-deployment), [backend 12](../backend/README.md#12-builds-and-deployment) and [vision 5](../vision/README.md#5-runtimes-and-versions).

| Machine | Installs |
|---|---|
| **The rig** (Linux: RTX 3070, RTX 5070 Ti, RX 9070 XT) | Ubuntu 24.04.5 with the HWE kernel, or 26.04.1. NVIDIA driver 580.65.06 or newer. [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html). AMD's ROCm 10.1 `amdgpu` driver for the 9070 XT (*unverified* until the [day-1 check](../vision/evaluation.md#7-day-1-checks)). Docker Engine with Compose v2. The Cloudflare tunnel, as architecture 12 describes |
| **The control plane** | Docker Engine with Compose v2, and the tunnel and Caddy configuration from `infra/` (architecture 12) |

The containers bring everything else: CUDA 13.0.3 and ONNX Runtime in `worker-cuda`, vLLM 0.31.0 for CUDA and ROCm, SurrealDB and RustFS. Each is pinned by digest, so nothing else is installed on the host.

## 7. Checking your setup

```bash
git --version && gh auth status
node --version            # 24 or newer
python3 --version         # 3.12 for ml/
docker compose version    # v2
claude --version

# App
eas whoami
java -version             # 17
adb version               # Android platform tools on PATH

# Backend
rustc --version           # 1.99.1 inside backend/
cargo nextest --version   # 0.9.148
pkg-config --modversion libheif   # 1.19 or newer
```

Inside `app/`, `npx expo-doctor` checks the project against SDK 57.

## 8. Sources

- [Expo: set up your environment](https://docs.expo.dev/get-started/set-up-your-environment/) · [Expo SDK 57](https://docs.expo.dev/versions/v57.0.0/) · [EAS Build](https://docs.expo.dev/build/introduction/) · [development builds](https://docs.expo.dev/develop/development-builds/introduction/)
- [react-native 0.86.3 on npm](https://www.npmjs.com/package/react-native/v/0.86.3) (its `engines` field) ✓
- [rustup](https://rustup.rs/) · [cargo-nextest installation](https://nexte.st/docs/installation/pre-built-binaries/)
- [ONNX Runtime 1.31.0 release files](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0) ✓
- libheif packages: [Ubuntu](https://launchpad.net/ubuntu/+source/libheif) ✓ · [Debian](https://tracker.debian.org/pkg/libheif) ✓
- [Install WSL](https://learn.microsoft.com/en-us/windows/wsl/install) · [Docker Desktop's WSL 2 back end](https://docs.docker.com/desktop/features/wsl/)
- [PyTorch: get started locally](https://pytorch.org/get-started/locally/)
- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html)
