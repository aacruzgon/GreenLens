# infra

The control plane (section 6) and the compose files for the GPU rig. [`rig.compose.yml`](rig.compose.yml) is `sam-linux`'s: the vLLM server for the VLM on the RTX 5070 Ti (`vllm-nv`, [vision/vlm.md 4](../docs/vision/vlm.md#4-serving)) and the VLM worker that claims `vlm` jobs from the control plane's `api` and calls it (`vlm-worker-nv`). [`rig-amd.compose.yml`](rig-amd.compose.yml) is the same pair for the RX 9070 XT on Mark's host (`vllm-amd`, `vlm-worker-amd`), a draft until the card's day-1 check (dev-roadmap P7); it is set up as section 2 says for `sam-linux`, with the worker id `rig-9070xt-vlm-1` and the AMD lines of `infra/.env.example`. The 3070's perception worker runs on the control-plane host, in `control-plane.compose.yml`. Deployment is described in [architecture 12](../docs/architecture/README.md#12-deployment), and the full infrastructure design in [docs/infra](../docs/infra/README.md) ([infra 4](../docs/infra/README.md#infrarigcomposeyml)).

## 1. Prerequisites

Fedora 44, Docker CE with the compose plugin, NVIDIA driver 580 or later (615.78.08 here). Install the NVIDIA Container Toolkit:

```bash
curl -s -L https://nvidia.github.io/libnvidia-container/stable/rpm/nvidia-container-toolkit.repo \
  | sudo tee /etc/yum.repos.d/nvidia-container-toolkit.repo
sudo dnf install -y nvidia-container-toolkit
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker
```

Check that a container sees the GPU. SELinux is enforcing, but Docker CE runs here without `selinux-enabled`, so it doesn't label containers: this works with or without `--security-opt label=disable`, and so does reading a bind-mounted file under the home directory (✓ ran 2026-10-23). `rig.compose.yml`'s `vllm-nv` keeps the option for the day dockerd's SELinux support is turned on; there it turns off SELinux confinement for that container only.

```bash
docker run --rm --gpus all --security-opt label=disable \
  nvidia/cuda:13.0.3-base-ubuntu24.04 nvidia-smi
```

The checkpoints must already be in `~/.cache/huggingface`. The service runs with `HF_HUB_OFFLINE=1` and never downloads.

For the worker, log in to the registry once with a classic personal access token that has only `read:packages` ([docs/infra 3](../docs/infra/README.md#registry-tags-and-who-pushes)): `docker login ghcr.io -u <your GitHub user>`.

## 2. Start, stop, check

`infra/.env` holds both services' settings, and the worker's token is in its own file ([docs/infra 6](../docs/infra/README.md#6-secrets-and-configuration)):

```bash
cp infra/.env.example infra/.env     # then set VLM_API_KEY, VLM_GPU_UUID and WORKER_IMAGE
nvidia-smi --query-gpu=uuid --format=csv,noheader   # the value for VLM_GPU_UUID
sudo install -d -m 0700 -o "$USER" /srv/pdm/env
( umask 077; printf 'WORKER_TOKEN=%s\n' "$(openssl rand -hex 32)" > /srv/pdm/env/.env.rig-5070ti-vlm-1 )
```

- **`WORKER_IMAGE`** is the worker image by digest, from the latest `images` run on `main` (its summary has a `worker ghcr.io/aacruzgon/greenlens/worker@sha256:…` line), the same digest as `WORKER_IMAGE` in the control plane's `.env.compose`.
- **`API_BASE_URL`** is the control plane's public URL, `https://pdm-cp.tailb3ce07.ts.net` ([docs/infra 5](../docs/infra/README.md#5-ingress)). The worker reaches `api` through it like any client.
- **Until both are set,** `up -d vllm-nv` still starts the server alone; `up -d` fails on the worker's image (`worker:unset`).
- **The token** goes to the control plane's owner the way the team shares secrets, never in a chat message or a commit, and becomes `rig-5070ti-vlm-1=<token>` in `api`'s `WORKER_TOKENS` there. `api` refuses a worker whose token it doesn't know.

```bash
docker compose -f infra/rig.compose.yml up -d                 # vllm-nv, then vlm-worker-nv once it is healthy
docker compose -f infra/rig.compose.yml logs -f vllm-nv vlm-worker-nv
docker compose -f infra/rig.compose.yml down
```

Start-up takes several minutes (model load, CUDA graph capture); the health check waits up to 10 minutes, and the worker starts only after it passes. Then:

```bash
curl -H "Authorization: Bearer $VLM_API_KEY" http://127.0.0.1:8000/v1/models   # lists "vlm"
curl http://127.0.0.1:8000/metrics    # KV cache usage and preemption counters
```

The worker checks at start that the server serves `vlm` and exits if not, then logs `worker starting` with `slots=2` and heartbeats every 10 s. Its heartbeats reaching `api` is P6's check ([dev-roadmap](../docs/vision/dev-roadmap.md#phase-2-oct-1819-real-models-the-day-2-benchmark)):

```bash
curl -s https://pdm-cp.tailb3ce07.ts.net/status | jq '.workers[] | select(.worker_id == "rig-5070ti-vlm-1")'   # "healthy": true
```

The port is bound to `127.0.0.1` only. `vllm-nv` does not restart by itself; `vlm-worker-nv` does (`unless-stopped`).

**Until P6 the service was called `vllm`.** A container started under that name keeps the card and port 8000 after the rename, so the first `up` after pulling removes it: `docker compose -f infra/rig.compose.yml up -d --remove-orphans`.

## 3. Desktop mode and headless mode

The 5070 Ti also draws the KDE desktop. The desktop and its apps (browser, Discord, Spotify, Zoom) held 2.3–2.9 GB on 2026-10-16. vLLM refuses to start if free memory is less than `VLM_GPU_MEM_UTIL` x total memory.

Measured on 2026-10-16:

| Mode | `VLM_GPU_MEM_UTIL` | Checkpoint | KV cache | Use for |
|---|---|---|---|---|
| Desktop | 0.78 | Qwen3-VL-8B **AWQ** | 2.93 GiB, 21,344 tokens at 16,384 max length | Day-1 checks and prompt work, one job at a time |
| Desktop budget | 0.78 | **Qwen3.5-9B AWQ (chosen)**, `VLM_MAX_NUM_SEQS=2` ✓ ran 2026-10-20 | 2.45 GiB, 65,536 tokens (4 times the concurrency at 16,384); 8.41 GiB of weights; 12,500 MiB peak; healthy after 151 s | Everything, with 2 slots: the 65 benchmark cases ran as fast as at 0.90, with no preemption. Measured headless, so the desktop's 2.3–2.9 GB on top makes about 14,800–15,400 of 16,303 MiB |
| Desktop, logged in | 0.78 | Qwen3.5-9B AWQ (chosen), `VLM_MAX_NUM_SEQS=2` ✓ ran 2026-10-22 | The same 2.45 GiB and 65,536 tokens; the desktop 330 MiB (a fresh login, no apps open); 13,058 of 16,303 MiB at the start-up peak, 12,796 during the run; healthy after 151 s | The demo: the 65 benchmark cases in 204 s at 2 at once (214 s headless), no preemption ([report](../ml/training/eval/reports/vlm-bench-2026-10-22-qwen35-awq-desktop.md)). vLLM's own peak, 13,058 − 330 = 12,728 MiB, plus the desktop's 2.3–2.9 GB with its apps open makes about 15,000–15,600 of 16,303 MiB, by arithmetic |
| Desktop | 0.78 | Qwen3-VL-8B FP8 | **Doesn't start:** 10.37 GiB of weights leave 0.99 GiB of KV (0.59 GiB with an FP8 cache) | — |
| Headless | 0.90 | Qwen3-VL-8B FP8 (the fallback) | 2.4 GiB, 17,440 tokens; 16,528 with `VLM_MAX_NUM_SEQS=2` | Full-size runs and benchmarks |
| Desktop, logged in | 0.90 | Qwen3-VL-8B FP8 and Qwen3.5-9B AWQ, `VLM_MAX_NUM_SEQS=2` ✓ ran 2026-10-23 | Not read from these runs' logs: vLLM sizes it from the share, so as headless (*unverified*). The desktop held 329 MiB before vLLM started; the card peaked at 14,729 MiB (FP8) and 14,633 MiB (Qwen3.5) of 16,303 | D1's re-check, both models to the end, with KDE logged in ([vlm.md 13](../docs/vision/vlm.md#re-check-on-the-shipped-prompts-2026-10-23--ran)); the FP8 fallback beside the desktop, with nothing else open ([below](#beside-the-desktop-on-demo-day)) |
| Headless | 0.90 | Qwen3.5-9B AWQ (chosen), `VLM_MAX_NUM_SEQS=2` ✓ ran | 115,550 tokens (7.05 times the concurrency at 16,384); 8.41 GiB of weights; 13,686 of 16,303 MiB used; healthy after 150 s | Full-size runs and benchmarks |

### Beside the desktop, on demo day

vLLM checks at start that 0.78 of the card is free, then takes its weights, KV cache and CUDA graphs. Its own use still moves with the jobs: on Oct 22 it held 11,730 MiB when ready and 12,458 MiB after the run, and about 12,728 MiB at its start-up profiling peak, which is what it plans for. That leaves the desktop 3,575 MiB.

- **If the desktop holds more than 3,587 MiB (0.22 of the card) when vLLM starts,** vLLM refuses to start; a little less in practice, since vLLM checks after creating its own CUDA context, which counts as used (`request_memory` in `v1/worker/utils.py`, called from `gpu_worker.py`; ✓ read 2026-10-23 in the pinned image).
- **At 0.90, the FP8 fallback's share,** vLLM needs 14,673 MiB free at start, so the desktop must hold well under 1,630 MiB, less vLLM's CUDA context (unmeasured). On Oct 23 KDE was logged in at 329 MiB, both models started, and the card peaked at 14,729 MiB (FP8) and 14,633 MiB (Qwen3.5). That leaves about 1.5 GB while it serves, which one browser window or Discord could take. Close every app before `up -d vllm`, and open nothing until it's down. With the apps open (2.3–2.9 GB) vLLM wouldn't start (derived from these figures, not run).
- **If the desktop grows while vLLM serves,** whichever allocates next when the card is full fails. That can be the desktop's apps (a browser falls back to software rendering or loses its GPU process, windows go black) or vLLM, on a job that needs back the memory it freed after profiling, which ends its engine. Neither has been provoked (*unverified*).
- **Before serving:** close what draws on the card and isn't needed (games, browsers with hardware acceleration, Discord, Spotify, Zoom), and check with `nvidia-smi` that the desktop holds well under 3,575 MiB before `up -d`. Open nothing new on `sam-linux` while it serves.
- **If vLLM stops while serving:** stop the VLM worker too (`docker compose -f infra/rig.compose.yml stop vlm-worker-nv`), so the hosted fallback worker takes the `vlm` jobs 15 seconds after they're queued ([architecture 8.4](../docs/architecture/README.md#84-job-leasing-and-fallback)). Then `docker compose -f infra/rig.compose.yml up -d`, which starts `vllm-nv` and, once `/health` answers (about 2.5 minutes), the worker again.

### Going headless

Work from a laptop. Every desktop program closes, so save first.

1. `ssh` into the rig (Tailscale works), start `tmux`, and run Claude Code or the commands from there. A Claude Code session in a desktop terminal ends with the desktop.
2. `sudo systemctl isolate multi-user.target` stops the login screen, so it can't take the GPU again.
3. Plasma 6 runs KWin, plasmashell and every app as **user** units, which survive both the isolate and `loginctl terminate-session`. Stop them with `systemctl --user stop graphical-session.target app.slice`. tmux, sshd, Tailscale and Docker keep running.
4. `nvidia-smi` should show no processes and about 34 MiB used. Then `VLM_GPU_MEM_UTIL=0.90 docker compose -f infra/rig.compose.yml up -d`.

To return: `down` the services, then `sudo systemctl isolate graphical.target`, log in at the screen, and use 0.78 again. If the login stays on a black screen, the user's D-Bus socket closed with the last desktop session while SSH kept the user manager up: `systemctl --user start dbus.socket`, end the hung `startplasma-wayland`, and log in again (✓ seen 2026-10-22). Qwen3.5-9B AWQ, the chosen model, runs at 0.78 with 2 slots; only Qwen3-VL FP8 needs 0.90, which runs beside the desktop only with nothing else open, before and while it serves ([at 0.90](#beside-the-desktop-on-demo-day)); otherwise go headless.

## 4. Switching checkpoints

**Qwen3.5-9B AWQ was chosen on 2026-10-20** (decision D1, from the day-2 benchmark: [evaluation 8](../docs/vision/evaluation.md#vlm-results-2026-10-20--ran), [vlm.md 2](../docs/vision/vlm.md#2-model-choice)). It is the default in `infra/.env.example` and in `rig.compose.yml`, with thinking off and 2 slots. The Qwen3-VL checkpoints stay as the fallback.

To switch, set both variables together in `infra/.env`, with `VLM_QUANT` (`awq_w4a16` for both AWQ checkpoints, `fp8_block` for FP8), then `down` and `up -d`. The worker reports `VLM_MODEL@VLM_REVISION` and `VLM_QUANT` with every result, so they must match what the server loads. A revision belongs to one repository.

| Checkpoint | `VLM_MODEL` | `VLM_REVISION` | `VLM_EXTRA_ARGS` |
|---|---|---|---|
| **Chosen**, Qwen3.5-9B AWQ 4-bit | `cyankiwi/Qwen3.5-9B-AWQ-4bit` | `156edc4bbeb8d1910ee7be9196bafaf1bc052156` | `'--default-chat-template-kwargs "{\"enable_thinking\": false}"'`, or leave the line out |
| Fallback, Qwen3-VL-8B FP8 (0.90: nothing open on the desktop while it serves, or headless) | `Qwen/Qwen3-VL-8B-Instruct-FP8` | `9cdc6310a8cb770ce18efaf4e9935334512aee45` | Empty: `VLM_EXTRA_ARGS=` |
| Fallback, Qwen3-VL-8B AWQ 4-bit | `cyankiwi/Qwen3-VL-8B-Instruct-AWQ-4bit` | `87196f7771efdd7022a8d8f094ac6e063bf87f5c` | Empty: `VLM_EXTRA_ARGS=` |

Qwen3.5 thinks before it answers by default, and the worker expects the JSON answer alone, so thinking must be off. The compose file reads `VLM_EXTRA_ARGS` with `${VLM_EXTRA_ARGS-…}`, so a missing line gives thinking off and an empty one gives no extra flag.

To try a checkpoint without editing `infra/.env`, pass the variables from the shell (they win over the file):

```bash
VLM_MODEL=Qwen/Qwen3-VL-8B-Instruct-FP8 VLM_REVISION=9cdc6310a8cb770ce18efaf4e9935334512aee45 \
VLM_QUANT=fp8_block VLM_EXTRA_ARGS= VLM_GPU_MEM_UTIL=0.90 VLM_MAX_NUM_SEQS=2 docker compose -f infra/rig.compose.yml up -d
```

Every VLM worker must serve the same checkpoint, because a scan's jobs can land on either card ([vision/vlm.md 4](../docs/vision/vlm.md#4-serving), [vision README 4](../docs/vision/README.md#4-placement-and-vram-budgets)).

## 5. Changing the image size

`VLM_MAX_PIXELS` is the pixel budget per image. The `width` and `height` in `--limit-mm-per-prompt` in `rig.compose.yml` (1152 x 864 for 1003520) must change with it.

## 6. The control plane

[`control-plane.compose.yml`](control-plane.compose.yml) runs SurrealDB, RustFS, `api`, the fallback worker, Caddy and an ingress sidecar ([docs/infra 4](../docs/infra/README.md#infracontrol-planecomposeyml)). It reads secrets from `/srv/pdm/env/.env.<service>` (override with `PDM_ENV`); [`env/.env.example`](env/.env.example) lists every file, one `# --- .env.X ---` section each. Never load that example whole.

First time on a host ([docs/infra 7](../docs/infra/README.md#first-time-bootstrap)):

```bash
export PDM_ENV=/srv/pdm/env
docker compose -f infra/control-plane.compose.yml --env-file $PDM_ENV/.env.compose up -d surrealdb rustfs
infra/bootstrap/surrealdb-users.sh      # namespace, database and the `api` user (EDITOR)
infra/bootstrap/rustfs.sh               # pdm-images, the policies and PDMAPI01, PDMSEED01, PDMBACKUP01
docker compose -f infra/control-plane.compose.yml --env-file $PDM_ENV/.env.compose up -d
```

Both scripts are idempotent and read their secrets from the env files, never from arguments. `COMPOSE_PROFILES` in `.env.compose` picks the ingress (`tailscale` or `cloudflare`; empty on a VM) and the optional services: `stub` (the S1 perception stub worker, turned off in S2) and `seed` (`seed-upload`, from S2).

Deploy a new image by digest with `infra/deploy.sh API_IMAGE ghcr.io/aacruzgon/greenlens/api@sha256:...`; it logs to `/srv/pdm/deploy.log` and prints the rollback command. The Caddyfile is in [`caddy/`](caddy/Caddyfile) and the ingress configuration in [`ingress/`](ingress/).
