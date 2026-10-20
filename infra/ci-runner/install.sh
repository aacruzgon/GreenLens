#!/bin/bash
# Installs the CI runner hook, the weekly prune and its timer for the ci-runner user. Run it as
# ci-runner from a checkout: bash ~/runner-1/_work/GreenLens/GreenLens/infra/ci-runner/install.sh
# Idempotent, needs no sudo. Does not restart the runners, which would cancel a running job.
set -euo pipefail

if [ "$(id -un)" != "ci-runner" ]; then
  echo "run this as ci-runner, not $(id -un)" >&2
  exit 1
fi

HERE=$(cd "$(dirname "$0")" && pwd)
UNITS=$HOME/.config/systemd/user
BIN=$HOME/ci/bin

mkdir -p "$BIN" "$UNITS"
install -m 755 "$HERE/job-started.sh" "$HERE/prune.sh" "$BIN/"
install -m 644 "$HERE/gh-runner@.service" "$HERE/ci-prune.service" "$HERE/ci-prune.timer" "$UNITS/"

hook="ACTIONS_RUNNER_HOOK_JOB_STARTED=$BIN/job-started.sh"
for env in "$HOME"/runner-*/.env; do
  [ -f "$env" ] || continue
  if grep -q '^ACTIONS_RUNNER_HOOK_JOB_STARTED=' "$env"; then
    echo "hook already set in $env"
  else
    # .env may lack a final newline
    [ -z "$(tail -c1 "$env")" ] || echo >> "$env"
    echo "$hook" >> "$env"
    echo "hook added to $env"
  fi
done

systemctl --user daemon-reload
systemctl --user enable --now ci-prune.timer

echo "Done. The runners read .env at start: when no job is running, restart them with"
echo "  systemctl --user restart gh-runner@1 gh-runner@2"
