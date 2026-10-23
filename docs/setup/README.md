# PDM Developer Setup

> **Status:** v0.1.9, 2026-10-23. v0.1.9: on Ubuntu 26.04, libheif's HEVC plugin `libheif-plugin-libde265` is needed besides `libheif-dev`, or iPhone HEIC photos don't decode. v0.1.8: section 6 has a row per rig host; `sam-linux`, the 5070 Ti's, runs Fedora 44 with SELinux, where Docker CE doesn't label containers, and keeps its desktop, since vLLM serves at 0.78 of the card (D11, #13). v0.1.7: section 5 adds `unar`, which unpacks the Makerere maize set's RAR archives. v0.1.6: a third CI runner, `rig-3`, for the light jobs, and step 13 marked as run. v0.1.5: section 6 has a CI runner subsection, with the commands that set up the two self-hosted runners on the rig's 3070 host. v0.1.4: backend developers working on connectors start Era's simulators with the compose file's `era` profile. v0.1.3: the control plane reaches the internet through Tailscale Funnel, run as a container, instead of a Cloudflare tunnel. v0.1.2: backend developers start SurrealDB and RustFS with `infra/dev.compose.yml`. v0.1.1: Debian 13's libheif is 1.23.4, as the `api` image build installed it. v0.1: first version: what each team member installs on macOS, Windows and Linux, by role, with the versions the other documents pin.
> **Checked:** on 2026-10-16. The release files of ONNX Runtime 1.31.0 on GitHub, libheif's versions in Ubuntu and Debian, and the Node versions that React Native 0.86.3 and EAS CLI accept were read from their sources (✓). Items marked *unverified* weren't tried on that platform. On 2026-10-19 the steps of [CI runner](#ci-runner) in section 6 were run on the rig's 3070 host (✓ run); the install script's step and the first job on the runners were not.

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
| Whoever looks after the CI runners (the rig's owner) | 2, 6, [CI runner](#ci-runner) | `infra/ci-runner/`, `.github/` ([infra 2](../infra/README.md#runners-and-budget)) |

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
| Era's Slack and Google Drive simulators | the pinned images | Docker, for connector work: `docker compose -f infra/dev.compose.yml --profile era up -d era-slack era-drive` starts them on `127.0.0.1:8091` and `:8092` ([infra 4](../infra/README.md#infradevcomposeyml), [EON 2](../EON/README.md#2-before-you-start)) |

**libheif by platform:**
- **macOS:** `brew install libheif pkg-config`. Homebrew's 1.23.1 built here ✓.
- **Ubuntu 26.04 LTS:** ships 1.21.2 ✓. Install `sudo apt install libheif-dev libheif-plugin-libde265`. `libheif-dev` alone brings only the AV1 plugins, and an iPhone's HEIC, which is HEVC, then fails to decode with `DecoderPluginError(Unspecified)` (✓ ran 2026-10-23 in WSL, `docs/photos/IMG_1178.HEIC`).
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
| `unar`, to unpack Makerere's RAR archives ([training 4](../vision/training.md#4-data-preparation)) | `brew install unar` | `sudo apt install unar` in WSL | `sudo apt install unar` or `sudo dnf install unar`; 1.10.8 on Fedora 44 ✓ |

- **Where training runs:** on the 5070 Ti or on a Linux machine with an NVIDIA card ([architecture 3](../architecture/README.md#3-components)). Use a Mac for knowledge ingestion, label work, evaluation scripts and CPU checks of exported ONNX files.
- **vLLM** runs only in its pinned container on the rig. Don't install it in the training environment, since it pins its own torch.

## 6. The GPU rig and the control plane

Only the people who run these machines need this section. The design is in [architecture 12](../architecture/README.md#12-deployment), [backend 12](../backend/README.md#12-builds-and-deployment) and [vision 5](../vision/README.md#5-runtimes-and-versions).

| Machine | Installs |
|---|---|
| **The rig: the RTX 3070's host** (Linux) | Ubuntu 24.04.5 with the HWE kernel, or 26.04.1, or Debian 13 as on the CI runners' host (below). NVIDIA driver 580.65.06 or newer. [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html). Docker Engine with Compose v2. The control plane runs here too, with Tailscale Funnel as a container ([infra 5](../infra/README.md#5-ingress)), so the host itself needs nothing more for ingress |
| **The rig: `sam-linux`, the RTX 5070 Ti** | Fedora 44 with SELinux enforcing. Docker CE runs here without `selinux-enabled`, so SELinux doesn't label or confine its containers: one reads a bind-mounted file under the home directory and sees the GPU with or without `--security-opt label=disable` (✓ ran 2026-10-23). `infra/rig.compose.yml`'s `vllm` sets that option anyway, for the day dockerd's SELinux support is turned on; there it turns off SELinux confinement for that container only, and `:z` on its bind mounts with the `container_use_devices` boolean or CDI for the GPU would be stricter (*unverified*, not tried). NVIDIA driver 615.78.08, the NVIDIA Container Toolkit from its RPM repository, and Docker CE with the compose plugin ([infra README 1](../../infra/README.md#1-prerequisites)). The card also draws the KDE desktop: vLLM serves Qwen3.5-9B AWQ at 0.78 of the card beside it, so the demo needs no headless step, with nothing else that draws on the card running while it serves ([infra README 3](../../infra/README.md#3-desktop-mode-and-headless-mode), demo day); only the fallback checkpoint needs the card headless ([infra README 3](../../infra/README.md#3-desktop-mode-and-headless-mode); D11, [#13](https://github.com/aacruzgon/GreenLens/issues/13)) |
| **The rig: the RX 9070 XT's host** (a teammate's Linux machine) | AMD's ROCm 10.1 `amdgpu` driver (*unverified* until the [day-1 check](../vision/evaluation.md#7-day-1-checks)) and Docker Engine with Compose v2; it runs `infra/rig-amd.compose.yml` ([architecture 12](../architecture/README.md#12-deployment)) |
| **The control plane** | Docker Engine with Compose v2, and the Caddy and ingress configuration from `infra/` ([infra 5](../infra/README.md#5-ingress)): a Tailscale account with Funnel turned on for the rig, or a domain on Cloudflare or a VM for the other profiles |

The containers bring everything else: CUDA 13.0.3 and ONNX Runtime in `worker-cuda`, vLLM 0.31.0 for CUDA and ROCm, SurrealDB and RustFS. Each is pinned by digest, so nothing else is installed on the host.

### CI runner

CI runs on three self-hosted runners on the rig's 3070 host: `rig-1` and `rig-2` for the builds, and `rig-3` for the light jobs, (Debian 13, 16 threads, 15 GB of RAM, 16 GB of swap). The design, the guards in `ci.yml` and the risks are in [infra 2](../infra/README.md#runners-and-budget). Pull-request code runs on this machine, so the runners run as their own user with no `sudo` and no `docker` group, in their own rootless Docker daemon.

✓ Run on 2026-10-19, in this order; step 13 after the first job had checked the repository out, and step 14 later the same night. Replace `<uid>` with the output of `id -u ci-runner`.

**In your own terminal, with `sudo`:**

1. Create the user, with no password and no `sudo` or `docker` membership:

   ```bash
   sudo adduser --disabled-password --comment "GitHub Actions runner" ci-runner
   ```

2. Check that it has subordinate ID ranges, which rootless Docker needs. If the command prints nothing for either file, add a range of 65,536 that starts after the highest one already in `/etc/subuid` (`sort -t: -k2 -n /etc/subuid | tail -1`), so no two users' containers share IDs: `sudo usermod --add-subuids <start>-<start+65535> --add-subgids <start>-<start+65535> ci-runner`. On the rig it got 362144:

   ```bash
   grep ci-runner /etc/subuid /etc/subgid
   ```

3. Install what rootless Docker needs, and keep the user's services running without a login:

   ```bash
   sudo apt install uidmap dbus-user-session rootlesskit slirp4netns systemd-container
   sudo loginctl enable-linger ci-runner
   ```

4. Cap the user's memory, so your desktop and builds survive two Rust links at once:

   ```bash
   sudo systemctl set-property user-$(id -u ci-runner).slice MemoryHigh=9G MemoryMax=11G
   ```

5. Check that your home is closed to it (mode 700), so CI can't read your files or the rig's env files:

   ```bash
   stat -c '%a %n' /home/acruzgon
   ```

6. Open a shell as `ci-runner`. Use `machinectl`: `sudo -iu ci-runner` has no user bus, so `systemctl --user` fails. Don't paste multi-line blocks into this shell.

   ```bash
   sudo machinectl shell ci-runner@
   ```

**In the `ci-runner` shell:**

7. Rootless Docker, from the Debian package's script. The script expects `dockerd-rootless.sh` and `dockerd` on the `PATH`, and Debian keeps them in `/usr/share/docker.io/contrib` and `/usr/sbin`, so put both on it first (with `~/.cargo/bin`, for step 8). Then start the daemon as a user service and check it:

   ```bash
   echo 'export PATH=$HOME/.cargo/bin:/usr/share/docker.io/contrib:/usr/sbin:$PATH' >> ~/.bashrc
   echo 'export DOCKER_HOST=unix:///run/user/$(id -u)/docker.sock' >> ~/.bashrc
   source ~/.bashrc
   dockerd-rootless-setuptool.sh install
   systemctl --user enable --now docker
   systemctl --user is-enabled docker
   docker run --rm hello-world
   ```

8. Rust 1.99.1, and the buildx plugin v0.38.0 with its checksum. The `pdm-ci` builder keeps image layers between jobs. The `docker` driver can't export a registry cache, so a `docker-container` builder is needed:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.99.1 -c rustfmt -c clippy
   . ~/.cargo/env
   rustc --version
   mkdir -p ~/.docker/cli-plugins
   curl -fsSL -o ~/.docker/cli-plugins/docker-buildx https://github.com/docker/buildx/releases/download/v0.38.0/buildx-v0.38.0.linux-amd64
   echo '4fe4cc38adf48169132749b6ca22a990928db0118e3407584ee553723115d287  /home/ci-runner/.docker/cli-plugins/docker-buildx' | sha256sum -c -
   chmod +x ~/.docker/cli-plugins/docker-buildx
   docker buildx version
   docker buildx create --name pdm-ci --driver docker-container --bootstrap
   ```

9. The runner software, actions/runner v2.338.0, unpacked twice with its checksum checked:

   ```bash
   curl -fsSL -o ~/actions-runner.tar.gz https://github.com/actions/runner/releases/download/v2.338.0/actions-runner-linux-x64-2.338.0.tar.gz
   echo 'af4b794c1bc41d73d40535e3fe092a39f9679cd8d965954c2aca25a05ca41d32  /home/ci-runner/actions-runner.tar.gz' | sha256sum -c -
   mkdir -p ~/runner-1 ~/runner-2
   tar xzf ~/actions-runner.tar.gz -C ~/runner-1
   tar xzf ~/actions-runner.tar.gz -C ~/runner-2
   ```

**On GitHub, in your own terminal** (`ci-runner` isn't logged in to `gh`):

10. Get a registration token. It lasts an hour and registers both runners. You can instead copy it from Settings, Actions, Runners, New self-hosted runner.

    ```bash
    gh api -X POST repos/aacruzgon/GreenLens/actions/runners/registration-token --jq .token
    ```

**Back in the `ci-runner` shell:**

11. Register the runners, set each one's environment, and install the service unit. Run `config.sh` where `rustc` works, so the runner records `~/.cargo/bin` in its `PATH`. Two jobs at a time fit in 15 GB; three don't, because linking a test binary takes 1–2 GB. The unit file comes from the repository, which `ci-runner` can't read, so copy it first from your own terminal: `sudo install -D -o ci-runner -g ci-runner -m 644 infra/ci-runner/gh-runner@.service /home/ci-runner/.config/systemd/user/gh-runner@.service`.

    ```bash
    cd ~/runner-1 && ./config.sh --unattended --url https://github.com/aacruzgon/GreenLens --token <token> --name rig-1 --labels pdm --work _work --replace
    cd ~/runner-2 && ./config.sh --unattended --url https://github.com/aacruzgon/GreenLens --token <token> --name rig-2 --labels pdm --work _work --replace
    for n in 1 2; do printf '%s\n' DOCKER_HOST=unix:///run/user/<uid>/docker.sock CARGO_BUILD_JOBS=6 NEXTEST_TEST_THREADS=8 >> ~/runner-$n/.env; cp ~/runner-$n/bin/runsvc.sh ~/runner-$n/runsvc.sh; done
    systemctl --user daemon-reload
    systemctl --user enable --now gh-runner@1 gh-runner@2
    systemctl --user status gh-runner@1 gh-runner@2 --no-pager
    ```

**On GitHub, in your own terminal:**

12. Send every job to the runners, and check that both are `online`. Deleting the variable sends the jobs back to GitHub's `ubuntu-26.04`:

    ```bash
    gh variable set RUNNER_LABELS --body '["self-hosted","linux","x64","pdm"]'
    gh api repos/aacruzgon/GreenLens/actions/runners --jq '.runners[] | [.name, .status] | @tsv'
    ```

**After the first job has checked the repository out**, in the `ci-runner` shell:

13. Install the job-started hook, which removes containers and networks older than 2 hours that cancelled jobs left behind, and the weekly `ci-prune.timer`, which trims the buildx cache to 30 GB and removes old images, unused volumes and build trees unused for 14 days. The script is idempotent and needs no `sudo`. Then restart the runners when no job is running, because they read `.env` at start. Expect under 100 GB of disk in use.

    ```bash
    bash ~/runner-1/_work/GreenLens/GreenLens/infra/ci-runner/install.sh
    systemctl --user restart gh-runner@1 gh-runner@2
    systemctl --user list-timers ci-prune.timer --no-pager
    ```

14. A third runner, `rig-3`, for the light jobs (`changes`, `docs`, `infra`, `security`, `ci`, `release`), so they don't queue behind Rust builds ([infra 2](../infra/README.md#runners-and-budget)). It gets only the label `pdm-light`, so it never takes a build. In the `ci-runner` shell, with a registration token from step 10:

    ```bash
    curl -fL -# -o /tmp/runner.tgz https://github.com/actions/runner/releases/download/v2.338.0/actions-runner-linux-x64-2.338.0.tar.gz
    echo "af4b794c1bc41d73d40535e3fe092a39f9679cd8d965954c2aca25a05ca41d32  /tmp/runner.tgz" | sha256sum -c
    mkdir -p ~/runner-3 && tar xzf /tmp/runner.tgz -C ~/runner-3 && rm /tmp/runner.tgz
    cd ~/runner-3 && ./config.sh --unattended --url https://github.com/aacruzgon/GreenLens --token <token> --name rig-3 --labels pdm-light --work _work --replace
    cp bin/runsvc.sh . && cd ~
    printf '%s\n' "DOCKER_HOST=unix:///run/user/$(id -u)/docker.sock" 'CARGO_BUILD_JOBS=2' 'NEXTEST_TEST_THREADS=2' "ACTIONS_RUNNER_HOOK_JOB_STARTED=$HOME/ci/bin/job-started.sh" >> ~/runner-3/.env
    systemctl --user enable --now gh-runner@3
    ```

    Then, in your own terminal, give `rig-1` and `rig-2` the light label too, so light jobs can also use them when idle, and set the variable the light jobs read:

    ```bash
    for r in $(gh api repos/aacruzgon/GreenLens/actions/runners --jq '.runners[] | select(.name=="rig-1" or .name=="rig-2") | .id'); do gh api -X POST repos/aacruzgon/GreenLens/actions/runners/$r/labels -f 'labels[]=pdm-light'; done
    gh variable set LIGHT_RUNNER_LABELS --body '["self-hosted","linux","x64","pdm-light"]'
    ```

**To remove a runner:** disable its unit (with `Restart=always`, a removed runner would otherwise restart every 10 seconds), get a removal token from `gh api -X POST repos/aacruzgon/GreenLens/actions/runners/remove-token --jq .token` in your own terminal, then in the `ci-runner` shell:

```bash
systemctl --user disable --now gh-runner@2
cd ~/runner-2 && ./config.sh remove --token <token>
```

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
