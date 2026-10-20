#!/bin/bash
# Weekly clean-up on the CI runner machine, run by ci-prune.timer as ci-runner.
# Trims the buildx cache to 30 GB, drops images over a week old and unused volumes, and deletes
# Cargo build trees (~/ci/target/<runner>/<job>) not used for 14 days. A failed step does not stop the rest.
set -euo pipefail

step() { "$@" || echo "prune: failed: $*"; }

df -h ~

step docker buildx prune --builder pdm-ci --max-used-space 30GB -f
step docker image prune -af --filter until=168h
step docker volume prune -af

# A job touches its directory at start, so the directory's own mtime is its last use.
if [ -d ~/ci/target ]; then
  step find ~/ci/target -mindepth 2 -maxdepth 2 -type d -mtime +14 -print -exec rm -rf {} +
fi

df -h ~
du -sh ~/ci/target 2>/dev/null || true
