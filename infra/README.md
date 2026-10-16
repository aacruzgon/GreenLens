# infra

Compose files for the GPU rig. [`rig.compose.yml`](rig.compose.yml) runs the vLLM server (`vllm`) for the VLM ([vision/vlm.md 4](../docs/vision/vlm.md#4-serving)). The workers are added to the same file later. Deployment is described in [architecture 12](../docs/architecture/README.md#12-deployment), and the full infrastructure design, which this file grows into, in [docs/infra](../docs/infra/README.md): there the service is `vllm-nv`, beside `model-fetch`, `perception-worker` and `vlm-worker-nv` ([infra 4](../docs/infra/README.md#infrarigcomposeyml)).

## 1. Prerequisites

Fedora 44, Docker CE with the compose plugin, NVIDIA driver 580 or later (615.78.08 here). Install the NVIDIA Container Toolkit:

```bash
curl -s -L https://nvidia.github.io/libnvidia-container/stable/rpm/nvidia-container-toolkit.repo \
  | sudo tee /etc/yum.repos.d/nvidia-container-toolkit.repo
sudo dnf install -y nvidia-container-toolkit
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker
```

Check that a container sees the GPU. SELinux is enforcing, so every container that touches the GPU or a mounted path needs `label=disable`:

```bash
docker run --rm --gpus all --security-opt label=disable \
  nvidia/cuda:13.0.3-base-ubuntu24.04 nvidia-smi
```

The checkpoints must already be in `~/.cache/huggingface`. The service runs with `HF_HUB_OFFLINE=1` and never downloads.

## 2. Start, stop, check

```bash
cp infra/.env.example infra/.env     # then set VLM_API_KEY and VLM_GPU_UUID
nvidia-smi --query-gpu=uuid --format=csv,noheader   # the value for VLM_GPU_UUID
docker compose -f infra/rig.compose.yml up -d vllm
docker compose -f infra/rig.compose.yml logs -f vllm
docker compose -f infra/rig.compose.yml down
```

Start-up takes several minutes (model load, CUDA graph capture); the health check waits up to 10 minutes. Then:

```bash
curl -H "Authorization: Bearer $VLM_API_KEY" http://127.0.0.1:8000/v1/models   # lists "vlm"
curl http://127.0.0.1:8000/metrics    # KV cache usage and preemption counters
```

The port is bound to `127.0.0.1` only. The service does not restart by itself.

## 3. Desktop mode and headless mode

The 5070 Ti also draws the KDE desktop. The desktop and its apps (browser, Discord, Spotify, Zoom) held 2.3–2.9 GB on 2026-10-16. vLLM refuses to start if free memory is less than `VLM_GPU_MEM_UTIL` x total memory.

Measured on 2026-10-16:

| Mode | `VLM_GPU_MEM_UTIL` | Checkpoint | KV cache | Use for |
|---|---|---|---|---|
| Desktop | 0.78 | Qwen3-VL-8B **AWQ** only | 2.93 GiB, 21,344 tokens at 16,384 max length | Day-1 checks and prompt work, one job at a time |
| Desktop | 0.78 | Qwen3-VL-8B FP8 | **Doesn't start:** 10.37 GiB of weights leave 0.99 GiB of KV (0.59 GiB with an FP8 cache) | — |
| Headless | 0.90 | Qwen3-VL-8B FP8 (the primary) | 2.4 GiB, 17,440 tokens; 16,528 with `VLM_MAX_NUM_SEQS=2` | Full-size runs and benchmarks |

### Going headless

Work from a laptop. Every desktop program closes, so save first.

1. `ssh` into the rig (Tailscale works), start `tmux`, and run Claude Code or the commands from there. A Claude Code session in a desktop terminal ends with the desktop.
2. `sudo systemctl isolate multi-user.target` stops the login screen, so it can't take the GPU again.
3. Plasma 6 runs KWin, plasmashell and every app as **user** units, which survive both the isolate and `loginctl terminate-session`. Stop them with `systemctl --user stop graphical-session.target app.slice`. tmux, sshd, Tailscale and Docker keep running.
4. `nvidia-smi` should show no processes and about 34 MiB used. Then `VLM_GPU_MEM_UTIL=0.90 docker compose -f infra/rig.compose.yml up -d vllm`.

To return: `down` the service, then `sudo systemctl isolate graphical.target`, log in at the screen, and use 0.78 with the AWQ build again.

## 4. Switching checkpoints

Set both variables together in `infra/.env`, then `down` and `up -d vllm`. A revision belongs to one repository.

| Checkpoint | `VLM_MODEL` | `VLM_REVISION` |
|---|---|---|
| Primary, FP8 | `Qwen/Qwen3-VL-8B-Instruct-FP8` | `9cdc6310a8cb770ce18efaf4e9935334512aee45` |
| Qwen3-VL AWQ 4-bit | `cyankiwi/Qwen3-VL-8B-Instruct-AWQ-4bit` | `87196f7771efdd7022a8d8f094ac6e063bf87f5c` |
| Challenger, Qwen3.5 | `cyankiwi/Qwen3.5-9B-AWQ-4bit` | `156edc4bbeb8d1910ee7be9196bafaf1bc052156` |

Qwen3.5 also needs `VLM_EXTRA_ARGS='--default-chat-template-kwargs "{\"enable_thinking\": false}"'`. Clear it again for the Qwen3-VL checkpoints.

Every VLM worker must serve the same checkpoint, because a scan's jobs can land on either card ([vision/vlm.md 4](../docs/vision/vlm.md#4-serving), [vision README 4](../docs/vision/README.md#4-placement-and-vram-budgets)).

## 5. Changing the image size

`VLM_MAX_PIXELS` is the pixel budget per image. The `width` and `height` in `--limit-mm-per-prompt` in `rig.compose.yml` (1152 x 864 for 1003520) must change with it.
