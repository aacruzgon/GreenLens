---
name: stop-and-relaunch
description: "Stop the whole PDM backend (the control plane on the 3070 host and the rig VLM workers) and bring it back later, for example between build week and Demo Day: what survives, what must be redone (the Era farm), what never to run while it's down, and the checks that prove it's back. Use before stopping, restarting or relaunching the deployed backend."
---

# Stop and relaunch the backend

This is the whole-backend version of the runbook in [infra 8](../../../docs/infra/README.md#runbook). Where they disagree, the runbook wins; fix this skill.

**Agents don't run these commands.** They stop or start the live deployment that the app and the demo use. Print the steps for the operator, who runs them on the control-plane host in their own terminal (or with `!` in Claude Code).

## What runs where

| Host | What | Compose |
|---|---|---|
| Control plane (the 3070 host) | project `pdm`: `surrealdb`, `rustfs`, `api`, `caddy`, `tailscale`, `fallback-worker`, `perception-worker`, `era-slack`, `era-drive`, plus the one-shots `model-fetch`, `seed-upload`, `era-seed` | `infra/control-plane.compose.yml` ([infra 4](../../../docs/infra/README.md#infracontrol-planecomposeyml)) |
| Control plane | `pdm-registry` on `127.0.0.1:5000`, for branch images; not in the `pdm` project | none |
| sam-linux (RTX 5070 Ti) | `vllm-nv`, `vlm-worker-nv` (`rig-5070ti-vlm-1`) | `infra/rig.compose.yml` |
| The RX 9070 XT's host | `vllm-amd`, `vlm-worker-amd` (`rig-9070xt-vlm-1`) | `infra/rig-amd.compose.yml`, still on PR #171, not on `main` |

The control plane works alone: when no VLM worker heartbeats, `fallback-worker` claims `vlm` jobs after 15 s. The rig workers make the VLM step faster and better.

Every command on the control plane runs from `~/Github/GreenLens`, with

```sh
dc() { PDM_ENV=/srv/pdm/env docker compose -p pdm -f infra/control-plane.compose.yml --env-file /srv/pdm/env/.env.compose "$@"; }
```

That checkout is the live configuration: the Caddyfile, `ml/models`, `infra/era-seed` and the Era fixture are mounted from it. Check it's on `main` (`git branch --show-current`) before the relaunch. Images are pinned by digest in `.env.compose`, so the branch doesn't change them.

## What survives a stop

- **The data.** The database (`pdm_surrealdb-data`, RocksDB), the photos (`pdm_rustfs-data`), the model files (`pdm_models`), Caddy's state and the Tailscale node (`pdm_tailscale-state`) are named volumes. `stop` and `down` both keep them; only `down -v` or a volume prune deletes them.
- **The public URL.** The tailscale sidecar keeps its node in its state volume, so no auth key is needed again.
- **A reboot.** Every long-running service has `restart: unless-stopped`, so a container you stopped stays stopped after a reboot. Docker itself still starts.
- **Jobs.** `stop` sends SIGTERM and waits for the grace period (`api` 190 s, the workers 30 s) while they drain. A job whose lease runs out while everything is down goes back to the queue through the scheduler's sweep once `api` is up.
- **The model files offline.** `model-fetch` runs on every `up` but keeps each file whose hash matches, so the relaunch needs no Hugging Face access when the volume is intact.

## What is lost or breaks while it's down

- **The Era farm.** `era-slack` and `era-drive` keep their state in memory. It's gone when they stop. Seed it again on the relaunch (step 3). Records already synced into the database stay.
- **The nightly backup** (`pdm-backup.timer`, 03:30) fails each night, since it needs the running `pdm-surrealdb-1`. A failed run deletes no older export ([infra 8](../../../docs/infra/README.md#8-operations-hooks)).
- **Clerk's `user.deleted` webhook** gets no answer. Svix retries it, and after enough failures it can disable the endpoint. Check it on the relaunch (step 6).
- **The rig workers** retry their heartbeats and claims (claims back off up to 30 s) and log warnings. They reconnect by themselves when `api` is back. Their vLLM servers keep holding the GPUs unless their owners stop them.

## Never, while it's stopped

- `dc down -v`, `docker volume prune -a` or `docker system prune --volumes -a`: these delete the database and the photos.
- `docker system prune` or `docker container prune`: they remove the stopped containers, after which `docker image prune -a` can take their images. If that happens, `dc up -d` pulls the digests from GHCR again, which needs the host's `docker login ghcr.io`.
- `--help` on a CLI inside a container that holds secrets: clap prints the environment defaults, passwords included.

`dc stop` is the right command. `dc down` (without `-v`) is safe for the data but also removes the containers and the networks.

## Stop

1. **Nothing in flight.** `curl -s http://127.0.0.1:8082/status`: `queue` should show `"queued":0,"leased":0`. If a scan is running, wait for it.
2. **A fresh backup.** `BACKUP_IMAGES=1 /srv/pdm/bin/backup.sh`, then `ls -l /srv/pdm/backups | tail -3` shows a new `surreal-<UTC>.surql.gz` and a non-empty file.
3. **The rig workers** (optional; the owners run these on their hosts): `docker compose -f infra/rig.compose.yml stop` on sam-linux, and the same with `infra/rig-amd.compose.yml` on the 9070 XT's host. This frees the GPUs. Left running, they only log warnings.
4. **The control plane.** `dc stop`. It stops in reverse dependency order, `api` before `surrealdb` and `rustfs`. Then `dc ps -a` shows every service `Exited`, and `docker volume ls | grep pdm_` still lists the volumes.
5. **The registry** (optional): `docker stop pdm-registry`.

Leave the CI runners alone: they are separate services and need the host, not the backend.

## Relaunch

Do it at least a day before it's needed, so there is time to fix what comes up.

1. **The checkout.** In `~/Github/GreenLens`: `git branch --show-current` is `main`, and `git pull` if `main` has moved.
2. **Start.** `dc up -d`, and `docker start pdm-registry` if you stopped it. `api` waits for `model-fetch`, a healthy `surrealdb` and `rustfs`, and the optional `seed-upload` and `era-seed`. Then `dc ps -a`: the long-running services are `Up`, and `api` is `(healthy)` within a minute. `api` applies the migrations on every start. They are idempotent, so the same image changes nothing.
3. **The Era farm.** `dc up -d era-seed`, then `dc logs era-seed` prints the counts it injected and read back, and it exits 0 (`dc ps -a era-seed`). Then `dc restart api`, whose start-up seeding sets the demo garden's two `era` connectors active again. This is safe even if step 2 already seeded the farm, because `era-seed` resets before it injects.
4. **The checks on the host.**
   - `curl -fsS http://127.0.0.1:8081/healthz` answers 200.
   - `curl -s http://127.0.0.1:8082/status`: `cp-fallback-1` and `rig-3070-perception-1` are `"healthy":true`. Workers that no longer run (such as `dev-stub-perception-1`) stay listed as unhealthy, and that's expected.
   - If `rig-3070-perception-1` is missing or stale, run `dc up -d --no-deps perception-worker` (a `--no-deps` recreate of `api` stops it and leaves it stopped). `dc logs perception-worker | grep -c CUDAExecutionProvider` is above 0 when its models loaded on the GPU.
5. **The public URL.** From outside the tailnet (a phone on mobile data): `curl https://pdm-cp.<tailnet>.ts.net/healthz`. The first requests can fail with a TLS error for a few seconds while the node gets its certificate ([infra 5](../../../docs/infra/README.md#5-ingress)). The public `/status` answers 404 on purpose; read it on `127.0.0.1:8082`.
6. **Clerk.** In the Clerk dashboard (development instance), Webhooks: the endpoint for `/webhooks/auth` is enabled. Svix shows the failed deliveries from the downtime; re-enable the endpoint if it was disabled.
7. **The rig workers.** The owners run `docker compose -f infra/rig.compose.yml up -d` on sam-linux (and `rig-amd.compose.yml` on the 9070 XT's host). `vllm-nv` has `restart: "no"`, so it doesn't come back by itself after a reboot, and `vlm-worker-nv` waits for it to be healthy (up to 600 s while the model loads). Their `WORKER_IMAGE` should be the digest `fallback-worker` runs: `docker inspect -f '{{.Image}}' pdm-fallback-worker-1`. Then `rig-5070ti-vlm-1` (and `rig-9070xt-vlm-1`) are `"healthy":true` in `/status`.
8. **A smoke scan** through the public URL, from `backend/`:

   ```sh
   python3 tools/smoke/smoke.py --base-url https://pdm-cp.<tailnet>.ts.net \
     --env-file /srv/pdm/env/.env.api --photo <a plant photo>
   ```

   It reads only `CLERK_SECRET_KEY` from the env file and never prints it. It passes when the scan reaches `plan_ready` and the resume check holds ([smoke](../../../backend/tools/smoke/README.md)).
9. **The demo garden's connectors.** Within about 15 minutes, `dc logs --since 20m api | grep '"farm sync"'` shows `"end":"Synced"` for both `slack` and `google_drive`. A `source_missing` there means step 3 didn't take: run it again. Then open Maple Row Farm in the app as the demo account.

## If something fails

| Symptom | Look at | Usual cause |
|---|---|---|
| `api` restarts in a loop | `dc logs --tail 50 api` | A failed migration names its file; [database 7](../../../docs/database/README.md#7-operations). Also a missing env file in `/srv/pdm/env` |
| `api` never starts, `model-fetch` exited non-zero | `dc logs model-fetch` | A model file failed its hash and needed a download: check `HF_TOKEN` in `.env.models` and the network |
| `perception-worker` up but no `CUDAExecutionProvider` in its log | `dc logs perception-worker`, `nvidia-smi` | The NVIDIA driver or the container toolkit after a host update |
| The public URL never answers | `docker exec pdm-tailscale-1 tailscale status` | Tailscale on the host or the sidecar is offline; [infra 5](../../../docs/infra/README.md#5-ingress) |
| `farm sync` logs `source_missing` | `dc ps -a era-seed`, `dc logs era-seed` | The farm wasn't seeded after the simulators restarted: step 3 |
| A rig worker never turns healthy | its `docker compose logs vlm-worker-nv` | `vllm-nv` still loading, or a `WORKER_IMAGE` the control plane doesn't match |

The health check and restart for each service are in the [runbook's table](../../../docs/infra/README.md#runbook). For a lost database, restore from `/srv/pdm/backups` as the runbook describes.
