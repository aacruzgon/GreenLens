#!/usr/bin/env bash
# M18: score the evaluation's test split once (devpost.md 10). Two stages, each run once on Sam's OK:
#   bash m18.sh leakage   # check_leakage on the final state; stops unless "0 not allowed"
#   bash m18.sh test      # plant check, face check, VLM bench (vLLM up, run, down), then the report
# Holds gpu.lock. Never commits, pushes or comments. vLLM starts from rig.compose.yml beside this file,
# a frozen copy of main's (#171 and #180 are open), with main's production defaults: Qwen3.5-9B AWQ
# at 0.78, 2 slots, 1,003,520 pixels, thinking off. The bench runs at 1,003,520 pixels, 2 at once.
set -uo pipefail

NIGHT=/home/samMichRod/Projects/GreenLens/ml/training/runs/m18-2026-10-24
REPO=/home/samMichRod/Projects/GreenLens
export CARGO_TARGET_DIR=$REPO/backend/target
export PATH=/home/samMichRod/.cargo/bin:/usr/local/bin:/usr/bin:/bin:$PATH
export HOME=/home/samMichRod
export PYTHONPATH=$REPO
PY=$REPO/.venv/bin/python
ORT=$REPO/.venv/lib/python3.12/site-packages/onnxruntime/capi/libonnxruntime.so.1.31.0
API_MODELS=/home/samMichRod/.cache/pdm-models-faces
COMPOSE=(docker compose -f "$NIGHT/rig.compose.yml" --project-directory "$REPO/infra")
STAGE=${1:?leakage or test}
D=$(date +%F)
LOG=$NIGHT/$STAGE.log
LABEL=qwen35-awq-test
READY_S=1500

exec >>"$LOG" 2>&1
log() { echo "[$(date '+%F %T')] $*"; }
stop_vllm() { "${COMPOSE[@]}" down --timeout 40 >/dev/null 2>&1 || true; }
fail() { log "FAILED: $*"; exit 1; }
trap 'rc=$?; [[ $STAGE == test ]] && { log "exit: stopping vLLM"; stop_vllm; }; echo "$(date "+%F %T") exit $rc" >"$NIGHT/$STAGE.done"' EXIT
trap 'log "signal received"; exit 130' INT TERM HUP

cd "$REPO" || exit 2
log "M18 $STAGE starts at $(git rev-parse --short HEAD) on $(git branch --show-current); waiting for the GPU lock"
exec 9>"$NIGHT/gpu.lock"
flock 9
log "GPU lock held"
[[ -z "$(git status --porcelain --untracked-files=no -- ml backend)" ]] || fail "ml/ or backend/ has uncommitted changes: the run must be on a commit"

case $STAGE in
leakage)
  "$PY" -m ml.training.eval.check_leakage --out "leakage-$D-m18" || fail "check_leakage exited $?"
  grep -q '0 not allowed' "ml/training/eval/reports/leakage-$D-m18.md" || fail "leakage-$D-m18 has a match not allowed"
  head -8 "ml/training/eval/reports/leakage-$D-m18.md"
  log "leakage done: 0 not allowed"
  ;;
test)
  [[ -f "$NIGHT/leakage.done" ]] && grep -q 'exit 0' "$NIGHT/leakage.done" || fail "run the leakage stage first"
  [[ -f "$NIGHT/test.started" ]] && fail "the test split was already scored once ($(cat "$NIGHT/test.started")); not again"
  echo "$(date '+%F %T') $(git rev-parse HEAD)" >"$NIGHT/test.started"

  log "1/5 plant check on the test cases at the shipped pair"
  "$PY" -m ml.training.eval.plant_check --cases ml/training/eval/manifest.jsonl --case-split test \
    --probe ml/models/files/plant_probe/plant-probe-v2-2026-10-23.json \
    --pair plant-probe-v2-2026-10-23:0.94:0.31 --label "plant-check-test-cases-$D" | tail -8 || fail "plant_check"

  log "2/5 face check on the test photos (CPU)"
  nice -n 5 "$PY" -m ml.training.eval.face_check run --sets test --variants original --thresholds 0.3 \
    --models-dir "$API_MODELS" --ort "$ORT" || fail "face_check"

  log "3/5 VLM bench cases"
  "$PY" -m ml.training.eval.vlm_bench cases --split test --out ml/training/runs/vlm-bench/cases-test.jsonl || fail "vlm_bench cases"
  ( cd backend && cargo test -q --release -p worker --no-run ) || fail "the worker tests don't build"

  log "4/5 vLLM up (production defaults), the bench at 2 at once, vLLM down"
  VLM_API_KEY=$(grep -h '^VLM_API_KEY=' "$REPO/infra/.env" | head -1 | cut -d= -f2-)
  VLM_GPU_UUID=$(grep -h '^VLM_GPU_UUID=' "$REPO/infra/.env" | head -1 | cut -d= -f2-)
  export VLM_API_KEY VLM_GPU_UUID
  [[ -n "${VLM_API_KEY:-}" && -n "${VLM_GPU_UUID:-}" ]] || fail "VLM_API_KEY or VLM_GPU_UUID missing in infra/.env"
  # A model's leftovers on the card would change the timing.
  used=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | head -1 | tr -d ' ')
  (( used < 1500 )) || fail "the GPU already holds $used MiB"
  stop_vllm
  "${COMPOSE[@]}" up -d vllm || fail "vLLM didn't start"
  t=$SECONDS
  until curl -sf -m 5 -H "Authorization: Bearer $VLM_API_KEY" http://127.0.0.1:8000/v1/models >/dev/null 2>&1; do
    (( SECONDS - t > READY_S )) && { "${COMPOSE[@]}" logs --tail 30 vllm; fail "vLLM not ready after $READY_S s"; }
    sleep 10
  done
  log "vLLM ready after $((SECONDS - t)) s"
  "${COMPOSE[@]}" logs vllm 2>&1 | grep -E "GPU KV cache size|Maximum concurrency" | tail -2
  "$PY" -m ml.training.eval.vlm_bench run --label "$LABEL" --cases ml/training/runs/vlm-bench/cases-test.jsonl \
    --max-pixels 1003520 --concurrency 2 --timeout 3600 || log "vlm_bench run exited $? (scored as it stands)"
  stop_vllm
  log "vLLM down: the 5070 Ti is free"
  echo "$(date '+%F %T')" >"$NIGHT/vlm.done"

  log "5/5 the report"
  "$PY" -m ml.training.eval.score_test_split --plant "ml/training/eval/reports/plant-check-test-cases-$D.json" \
    --vlm "ml/training/runs/vlm-bench/$LABEL" --faces ml/training/runs/faces/raw/test-original-0.3.jsonl \
    --out "test-split-$D" || fail "score_test_split"
  log "test done: ml/training/eval/reports/test-split-$D.md"
  ;;
*) fail "unknown stage $STAGE" ;;
esac
