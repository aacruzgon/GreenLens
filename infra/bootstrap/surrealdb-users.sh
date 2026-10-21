#!/usr/bin/env bash
# Creates the pdm namespace, the main database and the `api` database user (EDITOR) in the
# running surrealdb service. Idempotent (IF NOT EXISTS). docs/infra/README.md section 7 step 4,
# docs/database/README.md section 1.
#   PDM_ENV=/srv/pdm/env infra/bootstrap/surrealdb-users.sh
# Rotating the api password needs `DEFINE USER OVERWRITE` as root by hand (infra section 6).
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib.sh
# shellcheck source-path=SCRIPTDIR
. "$here/lib.sh"

cid=$(container_of surrealdb)
image=$(docker inspect -f '{{.Config.Image}}' "$cid")   # the pinned digest the service runs

# The secrets reach the container through the environment, never an argument.
SURREAL_USER=$(env_value .env.surrealdb SURREAL_USER)
SURREAL_PASS=$(env_value .env.surrealdb SURREAL_PASS)
export SURREAL_USER SURREAL_PASS
api_pass=$(env_value .env.api SURREAL_PASS)

# PASSWORD can't take a parameter (database 1), so the password is written into the statement as
# a single-quoted string with \ and ' escaped.
esc=${api_pass//\\/\\\\}
esc=${esc//\'/\\\'}

out=$(printf '%s\n' \
  "DEFINE NAMESPACE IF NOT EXISTS pdm;" \
  "USE NS pdm;" \
  "DEFINE DATABASE IF NOT EXISTS main;" \
  "USE DB main;" \
  "DEFINE USER IF NOT EXISTS api ON DATABASE PASSWORD '$esc' ROLES EDITOR;" |
  docker run --rm -i --network "container:$cid" -e SURREAL_USER -e SURREAL_PASS "$image" \
    sql --endpoint http://127.0.0.1:8000 --hide-welcome --json 2>&1) || die "surreal sql failed"

# surreal sql can exit 0 with a statement error in its output; the JSON lines carry "status":"ERR"
# or an "error" key then. Print nothing from it, since it may echo the statement.
if printf '%s' "$out" | grep -Eqi '"status" *: *"ERR"|"error"|IAM error|[Pp]arse error|ERROR'; then
  die "a statement failed (output withheld: it can contain the password)"
fi
echo "surrealdb: namespace pdm, database main and user api are in place"
