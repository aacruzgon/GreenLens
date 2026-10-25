#!/usr/bin/env bash
# Nightly local backup of the control plane (docs/infra/README.md section 8).
#   infra/ops/backup.sh
# 1. `surreal export` of ns pdm / db main, gzipped, to $BACKUP_DIR/surreal-<UTC timestamp>.surql.gz.
#    It runs the CLI inside the running SurrealDB container (the image is distroless: /surreal, no
#    shell), so it needs no CLI on the host and no published port. The root password reaches the
#    CLI through SURREAL_PASS (docker exec -e passes the name only), never on a command line.
#    Written to a temporary file, checked (`gzip -t`, non-empty, holds DEFINE TABLE), then renamed.
#    Only then are the older surreal-* exports beyond the newest $BACKUP_KEEP removed. The manual
#    pre-* exports are never touched.
# 2. With BACKUP_IMAGES=1, a mirror of the pdm-images bucket in $BACKUP_DIR/images, from
#    rclone/rclone with the read-only PDMBACKUP01 key. It sits on the same disk as RustFS, so it
#    guards against a damaged RustFS volume, not against losing the disk.
# Exits non-zero, with a message on stderr (the journal, under systemd), on any failure.
# Credentials: $PDM_ENV/.env.backup (SURREAL_ROOT_PASS; RUSTFS_ACCESS_KEY and RUSTFS_SECRET_KEY
# of PDMBACKUP01, needed only with BACKUP_IMAGES=1). Values are never printed.
# Environment: PDM_ENV (default /srv/pdm/env), BACKUP_DIR (default /srv/pdm/backups),
# BACKUP_KEEP (14), BACKUP_IMAGES (0), SURREAL_CONTAINER (pdm-surrealdb-1),
# SURREAL_ENDPOINT (http://127.0.0.1:8000, as seen inside the container), SURREAL_NS (pdm),
# SURREAL_DB (main), RUSTFS_ENDPOINT (http://127.0.0.1:9000, from the host network),
# RCLONE_IMAGE (the pinned digest below).
set -euo pipefail
umask 077

PDM_ENV=${PDM_ENV:-/srv/pdm/env}
BACKUP_DIR=${BACKUP_DIR:-/srv/pdm/backups}
BACKUP_KEEP=${BACKUP_KEEP:-14}
BACKUP_IMAGES=${BACKUP_IMAGES:-0}
SURREAL_CONTAINER=${SURREAL_CONTAINER:-pdm-surrealdb-1}
SURREAL_ENDPOINT=${SURREAL_ENDPOINT:-http://127.0.0.1:8000}
SURREAL_NS=${SURREAL_NS:-pdm}
SURREAL_DB=${SURREAL_DB:-main}
RUSTFS_ENDPOINT=${RUSTFS_ENDPOINT:-http://127.0.0.1:9000}
RCLONE_IMAGE=${RCLONE_IMAGE:-rclone/rclone:1.75.2@sha256:2687085f718d3c628f7fdfb77c52a1d344332aed543110bb88088f2c70d43eb5}

say() { echo "backup.sh: $*" >&2; }
die() { say "FAILED: $*"; exit 1; }

case $BACKUP_KEEP in '' | *[!0-9]* | 0) die "BACKUP_KEEP must be a positive integer" ;; esac
command -v docker >/dev/null || die "docker not found"
command -v gzip >/dev/null || die "gzip not found"
[ -r "$PDM_ENV/.env.backup" ] || die "cannot read $PDM_ENV/.env.backup"

# The file holds KEY=value lines and is trusted: it is mode 0600 in a 0700 directory.
set -a
# shellcheck disable=SC1091
. "$PDM_ENV/.env.backup"
set +a
[ -n "${SURREAL_ROOT_PASS:-}" ] || die "SURREAL_ROOT_PASS is not set in $PDM_ENV/.env.backup"

mkdir -p "$BACKUP_DIR"
exec 9>"$BACKUP_DIR/.backup.lock"
if command -v flock >/dev/null; then
  flock -n 9 || die "another backup is running"
fi

stamp=$(date -u +%Y%m%dT%H%M%SZ)
final=$BACKUP_DIR/surreal-$stamp.surql.gz
tmp=$BACKUP_DIR/.surreal-$stamp.surql.gz.tmp
trap 'rm -f "$tmp"' EXIT

docker inspect -f '{{.State.Running}}' "$SURREAL_CONTAINER" 2>/dev/null | grep -qx true \
  || die "container $SURREAL_CONTAINER is not running"

say "exporting $SURREAL_NS/$SURREAL_DB from $SURREAL_CONTAINER"
# --log warn keeps the CLI's "INFO ... Exporting bytes" line out of the dump: it goes to stdout
# at the default level and would be the first line of the file. pipefail makes a failed export
# fail the pipeline even though gzip succeeds.
SURREAL_PASS=$SURREAL_ROOT_PASS docker exec -e SURREAL_PASS "$SURREAL_CONTAINER" \
  /surreal --log warn export --endpoint "$SURREAL_ENDPOINT" --user root \
  --ns "$SURREAL_NS" --db "$SURREAL_DB" - | gzip -c >"$tmp" \
  || die "surreal export failed"

[ -s "$tmp" ] || die "export is empty"
gzip -t "$tmp" || die "export is not a valid gzip file"
gzip -dc "$tmp" | grep -c '^DEFINE TABLE' >/dev/null || die "export holds no DEFINE TABLE statement"
mv "$tmp" "$final"
size=$(stat -c %s "$final")
say "wrote $final ($size bytes)"

# Retention: the names sort by their UTC timestamp. Only surreal-*.surql.gz is considered.
shopt -s nullglob
exports=("$BACKUP_DIR"/surreal-*.surql.gz)
shopt -u nullglob
if [ "${#exports[@]}" -gt "$BACKUP_KEEP" ]; then
  excess=$((${#exports[@]} - BACKUP_KEEP))
  for old in "${exports[@]:0:excess}"; do
    rm -f -- "$old"
    say "removed $old"
  done
fi

if [ "$BACKUP_IMAGES" = 1 ]; then
  if [ -z "${RUSTFS_ACCESS_KEY:-}" ] || [ -z "${RUSTFS_SECRET_KEY:-}" ]; then
    die "RUSTFS_ACCESS_KEY and RUSTFS_SECRET_KEY are needed for BACKUP_IMAGES=1"
  fi
  say "mirroring pdm-images to $BACKUP_DIR/images"
  mkdir -p "$BACKUP_DIR/images"
  # The remote is configured through the environment; -e NAME passes the name, not the value.
  RCLONE_CONFIG_RUSTFS_TYPE=s3 RCLONE_CONFIG_RUSTFS_PROVIDER=Other \
    RCLONE_CONFIG_RUSTFS_ENDPOINT=$RUSTFS_ENDPOINT RCLONE_CONFIG_RUSTFS_REGION=us-east-1 \
    RCLONE_CONFIG_RUSTFS_FORCE_PATH_STYLE=true \
    RCLONE_CONFIG_RUSTFS_ACCESS_KEY_ID=$RUSTFS_ACCESS_KEY \
    RCLONE_CONFIG_RUSTFS_SECRET_ACCESS_KEY=$RUSTFS_SECRET_KEY \
    docker run --rm --network host --user "$(id -u):$(id -g)" \
    -e RCLONE_CONFIG_RUSTFS_TYPE -e RCLONE_CONFIG_RUSTFS_PROVIDER -e RCLONE_CONFIG_RUSTFS_ENDPOINT \
    -e RCLONE_CONFIG_RUSTFS_REGION -e RCLONE_CONFIG_RUSTFS_FORCE_PATH_STYLE \
    -e RCLONE_CONFIG_RUSTFS_ACCESS_KEY_ID -e RCLONE_CONFIG_RUSTFS_SECRET_ACCESS_KEY \
    -v "$BACKUP_DIR/images:/dest" "$RCLONE_IMAGE" \
    sync rustfs:pdm-images /dest --stats-one-line \
    || die "rclone sync of pdm-images failed"
fi

say "done"
