#!/usr/bin/env bash
# Manual deploy by digest on the control-plane host (docs/infra/README.md section 7).
#   infra/deploy.sh <service-variable> <image@sha256:...>
#   infra/deploy.sh API_IMAGE ghcr.io/aacruzgon/greenlens/api@sha256:...
# Rolling back is the same command with the previous digest from deploy.log.
# Environment: PDM_ENV (default /srv/pdm/env), PDM_DEPLOY_LOG (default /srv/pdm/deploy.log),
# PDM_PROJECT (compose project, default pdm), DEPLOY_WAIT_S (default 180).
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PDM_ENV=${PDM_ENV:-/srv/pdm/env}
PDM_DEPLOY_LOG=${PDM_DEPLOY_LOG:-/srv/pdm/deploy.log}
PDM_PROJECT=${PDM_PROJECT:-pdm}
DEPLOY_WAIT_S=${DEPLOY_WAIT_S:-180}
compose_file=$here/control-plane.compose.yml
env_file=$PDM_ENV/.env.compose

die() { echo "error: $*" >&2; exit 1; }
[ $# -eq 2 ] || die "usage: $0 <service-variable> <image@sha256:...>"
var=$1 ref=$2

# The services that use each image variable. WORKER_CUDA_IMAGE belongs to the rig.
case $var in
  API_IMAGE) services=(api) ;;
  WORKER_IMAGE) services=(fallback-worker perception-stub-worker) ;;
  SEED_UPLOAD_IMAGE) services=(seed-upload) ;;
  WORKER_CUDA_IMAGE) die "WORKER_CUDA_IMAGE is the rig's: deploy it there with infra/rig.compose.yml" ;;
  *) die "unknown variable $var (API_IMAGE, WORKER_IMAGE or SEED_UPLOAD_IMAGE)" ;;
esac

# 1. The reference has a digest, and the host can pull it.
[[ $ref =~ ^[a-z0-9./_-]+(:[A-Za-z0-9._-]+)?@sha256:[0-9a-f]{64}$ ]] || die "$ref is not <image>@sha256:<64 hex digits>"
[ -r "$env_file" ] || die "cannot read $env_file"
docker pull --quiet "$ref" >/dev/null || die "docker pull $ref failed"

# 2. Keep the previous .env.compose, set the variable, log the change.
old=$(sed -n "s/^${var}=//p" "$env_file" | tail -n 1)
cp -p "$env_file" "$env_file.prev"
chmod 600 "$env_file.prev"
tmp=$(mktemp "$env_file.XXXXXX")
chmod --reference="$env_file" "$tmp" 2>/dev/null || chmod 600 "$tmp"
awk -v var="$var" -v val="$ref" -v done=0 '
  index($0, var "=") == 1 { print var "=" val; done = 1; next }
  { print }
  END { if (!done) print var "=" val }' "$env_file" > "$tmp"
mv "$tmp" "$env_file"
printf '%s, %s, %s, %s, %s, %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(hostname)" "$var" \
  "${old:-none}" "$ref" "$(id -un)" >> "$PDM_DEPLOY_LOG"

# 3. Recreate the services that use it, then wait.
compose() { PDM_ENV=$PDM_ENV docker compose -p "$PDM_PROJECT" -f "$compose_file" --env-file "$env_file" "$@"; }
running=()
for s in "${services[@]}"; do
  # Skip a service whose profile is off (seed-upload, the stub) or that doesn't exist here.
  if compose config --services 2>/dev/null | grep -qx "$s"; then running+=("$s"); fi
done
[ ${#running[@]} -gt 0 ] || die "none of ${services[*]} is enabled by COMPOSE_PROFILES; $var is set but nothing restarted"
compose up -d --no-deps "${running[@]}"

deadline=$((SECONDS + DEPLOY_WAIT_S))
if [ "$var" = API_IMAGE ]; then
  # Wait for the compose health check.
  while :; do
    cid=$(compose ps -q api)
    state=$([ -n "$cid" ] && docker inspect -f '{{.State.Health.Status}}' "$cid" 2>/dev/null || echo none)
    [ "$state" = healthy ] && break
    [ $SECONDS -lt $deadline ] || die "api is $state after ${DEPLOY_WAIT_S}s; roll back with: infra/deploy.sh $var ${old:-<previous digest in $PDM_DEPLOY_LOG>}"
    sleep 3
  done
elif [ "$var" = WORKER_IMAGE ]; then
  # Wait for each worker's heartbeat on /status (read from inside the api container).
  for s in "${running[@]}"; do
    wid=$(compose exec -T "$s" printenv WORKER_ID 2>/dev/null | tr -d '\r') || true
    [ -n "$wid" ] || { echo "warning: $s has no WORKER_ID, not checked" >&2; continue; }
    while :; do
      if compose exec -T api curl -fsS http://127.0.0.1:8080/status 2>/dev/null |
        python3 -c 'import json,sys; w=sys.argv[1]; sys.exit(0 if any(x["worker_id"]==w and x["healthy"] for x in json.load(sys.stdin)["workers"]) else 1)' "$wid"; then
        break
      fi
      [ $SECONDS -lt $deadline ] || die "worker $wid has no healthy heartbeat after ${DEPLOY_WAIT_S}s; roll back with: infra/deploy.sh $var ${old:-<previous digest in $PDM_DEPLOY_LOG>}"
      sleep 3
    done
  done
else
  # seed-upload is a one-shot: wait for it to exit, and it must exit 0.
  while :; do
    cid=$(compose ps -a -q seed-upload)
    status=$([ -n "$cid" ] && docker inspect -f '{{.State.Status}}' "$cid" 2>/dev/null || echo none)
    [ "$status" = exited ] && break
    [ $SECONDS -lt $deadline ] || die "seed-upload is $status after ${DEPLOY_WAIT_S}s"
    sleep 2
  done
  code=$(docker inspect -f '{{.State.ExitCode}}' "$cid")
  [ "$code" = 0 ] || die "seed-upload exited with $code"
fi

# 4. The way back.
echo "deployed $var=$ref (was ${old:-none})"
if [ -n "$old" ]; then
  echo "roll back with: infra/deploy.sh $var $old"
fi
echo "the previous env file is $env_file.prev"
