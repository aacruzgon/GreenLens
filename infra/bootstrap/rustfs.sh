#!/usr/bin/env bash
# Creates the pdm-images bucket, the policies pdm-api, pdm-seed and pdm-backup and the identities
# PDMAPI01, PDMSEED01 and PDMBACKUP01 in the running rustfs service. Idempotent.
# docs/storage/README.md section 2, docs/infra/README.md section 7 step 5.
#   PDM_ENV=/srv/pdm/env infra/bootstrap/rustfs.sh
# An identity whose env file is missing is skipped with a warning (.env.seed and .env.backup
# can come after S1).
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib.sh
# shellcheck source-path=SCRIPTDIR
. "$here/lib.sh"

RC_IMAGE='rustfs/rc:v0.1.36@sha256:ab024bfebee49a750ce886b4c70963ccd9ddaa03f491704a90710641d7a26699'

cid=$(container_of rustfs)

# The secrets reach the container through the environment, never an argument.
ROOT_AK=$(env_value .env.rustfs RUSTFS_ACCESS_KEY)
ROOT_SK=$(env_value .env.rustfs RUSTFS_SECRET_KEY)
API_SK=$(env_value .env.api RUSTFS_SECRET_KEY)
export ROOT_AK ROOT_SK API_SK
SEED_SK='' BACKUP_SK=''
if [ -r "$PDM_ENV/.env.seed" ]; then SEED_SK=$(env_value .env.seed RUSTFS_SECRET_KEY); else echo "warning: no .env.seed, skipping PDMSEED01" >&2; fi
if [ -r "$PDM_ENV/.env.backup" ]; then BACKUP_SK=$(env_value .env.backup RUSTFS_SECRET_KEY); else echo "warning: no .env.backup, skipping PDMBACKUP01" >&2; fi
export SEED_SK BACKUP_SK

docker run --rm --network "container:$cid" \
  -e ROOT_AK -e ROOT_SK -e API_SK -e SEED_SK -e BACKUP_SK \
  -v "$here/policies:/w:ro" --entrypoint sh "$RC_IMAGE" -c '
set -eu
rc alias set pdm http://127.0.0.1:9000 "$ROOT_AK" "$ROOT_SK" >/dev/null
rc bucket create --ignore-existing pdm/pdm-images >/dev/null
# identity NAME POLICY SECRET: the policy, the user and the attachment; each step may already exist.
identity() {
  rc admin policy create pdm/ "$2" "/w/$2.json" >/dev/null
  rc admin user add pdm/ "$1" "$3" >/dev/null
  rc admin policy attach pdm/ "$2" --user "$1" >/dev/null
}
identity PDMAPI01 pdm-api "$API_SK"
[ -z "$SEED_SK" ] || identity PDMSEED01 pdm-seed "$SEED_SK"
[ -z "$BACKUP_SK" ] || identity PDMBACKUP01 pdm-backup "$BACKUP_SK"
echo "rustfs: bucket pdm-images, policies and identities are in place"
'
