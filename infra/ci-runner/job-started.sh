#!/bin/bash
# Runner job-started hook (ACTIONS_RUNNER_HOOK_JOB_STARTED): removes what cancelled jobs left behind.
# Removes every container created over 2 hours ago and unused networks over 2 hours old. No job runs
# longer than 90 minutes, so nothing live is touched, even with two runners. The buildx builder's
# container (buildx_buildkit_pdm-ci0) is meant to live on and is kept. Never fails: a hook that
# exits non-zero fails the job.
set -euo pipefail

cutoff=$(( $(date +%s) - 2 * 3600 ))

# Created is RFC 3339, which date -d parses.
{
  ids=$(docker ps -aq 2>/dev/null) || ids=""
  if [ -n "$ids" ]; then
    # shellcheck disable=SC2086  # one argument per container ID
    docker inspect --format '{{.Id}} {{.Created}} {{.Name}}' $ids 2>/dev/null |
      while read -r id created name; do
        case $name in /buildx_buildkit_*) continue ;; esac
        ts=$(date -d "$created" +%s 2>/dev/null) || continue
        if [ "$ts" -lt "$cutoff" ]; then
          echo "job-started: removing container ${id:0:12} (created $created)"
          docker rm -f -v "$id" >/dev/null 2>&1 || echo "job-started: could not remove ${id:0:12}"
        fi
      done
  fi
  docker network prune -f --filter until=2h 2>&1 | sed 's/^/job-started: /'
} || echo "job-started: clean-up failed, continuing"

exit 0
