# shellcheck shell=bash
# Shared by the bootstrap scripts (sourced, not run). docs/infra/README.md section 7.
# PDM_ENV: the env directory (default /srv/pdm/env). PDM_PROJECT: the compose project (default pdm).

PDM_ENV=${PDM_ENV:-/srv/pdm/env}
PDM_PROJECT=${PDM_PROJECT:-pdm}

die() { echo "error: $*" >&2; exit 1; }

# env_value FILE KEY: the last KEY=value line of FILE, without evaluating the file, read the way
# compose's env_file reads it: matching quotes around the value are removed, and an unquoted value
# loses a trailing ` # comment`. Fails if absent.
env_value() {
  local file=$PDM_ENV/$1 key=$2 value
  [ -r "$file" ] || die "cannot read $file"
  value=$(sed -n "s/^${key}=//p" "$file" | tail -n 1)
  case $value in
    \"*\") value=${value#\"}; value=${value%\"} ;;
    \'*\') value=${value#\'}; value=${value%\'} ;;
    *) value=${value%% #*} ;;
  esac
  [ -n "$value" ] || die "$key is empty or missing in $file"
  printf '%s' "$value"
}

# container_of SERVICE: the id of the running container of a compose service.
container_of() {
  local id
  id=$(docker ps -q \
    --filter "label=com.docker.compose.project=$PDM_PROJECT" \
    --filter "label=com.docker.compose.service=$1" | head -n 1)
  [ -n "$id" ] || die "service $1 is not running in project $PDM_PROJECT (docker compose up -d surrealdb rustfs)"
  printf '%s' "$id"
}
