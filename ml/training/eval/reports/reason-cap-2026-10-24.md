# The indicator reason's cap above what the prompt asks for (2026-10-24)

The fix for [#183](https://github.com/aacruzgon/GreenLens/issues/183). `indicators@3` asks for each indicator's `reason` "at most 60 characters", and the schema's cap was also 60 (`Limits::CONTRACT.reason`). Qwen3.5-9B AWQ writes up to the cap, so the grammar stopped reasons mid-thought, decode then dropped their last word, and the answer that follows each reason in the same object was written after a cut one.

The cap is now **100**, and the prompt still asks for at most 60, so the prompt versions don't change. On vLLM the schema is only the grammar, not part of the prompt text, so the model writes the same text up to the old cap and only the tail changes. #137 showed that a shorter ask costs answers: under 40, Qwen3.5 diagnosed 5 of 26 `sure` cases against 8 of 26 at 60. So the issue's other option, asking for about 50 under a cap of 60, was not taken.

## How it ran

On `sam-linux`, Oct 24, from `fix/indicators-reason-length` on `main` at 0d0e089 (prompts `system@2,findings@5,indicators@3,verify@3,guided@1`).

- **Server:** the live `vllm-nv` of `infra/rig.compose.yml` (project `infra`), serving real scans: vLLM 0.31.0 by digest (`c1c9f6fd…`), `cyankiwi/Qwen3.5-9B-AWQ-4bit` at revision `156edc4b`, thinking off, 2 slots at 0.78 of the card, maximum length 16,384, `VLM_MAX_PIXELS` 1,003,520, KDE logged in. It wasn't stopped or restarted. vLLM's log has 157 requests in each run's window, the bench's 150 calls and the 7 recorded ones, so no real scan was served during the runs.
- **Cap 80** first, then **cap 100** after Sam's review of the cap-80 results: 15:38–15:46 and 16:06–16:13 EDT, each a bench run and then the recording, holding `gpu.lock`.
- **Bench:** #140's 75 bench cases, one job at a time, scored on the 72 that stay dev, against Oct 23's `qwen35-awq-i3` run of the same cases at the old cap ([vlm.md 13](../../../../docs/vision/vlm.md#re-check-on-the-shipped-prompts-2026-10-23--ran)). That run had its own vLLM at 0.90 with 2 slots.

```bash
python -m ml.training.eval.vlm_bench run --label qwen35-awq-c100 \
  --cases ml/training/runs/vlm-bench/cases-140.jsonl --max-pixels 1003520 --concurrency 1   # VLM_API_KEY from infra/.env
python -m ml.training.eval.vlm_bench score ml/training/runs/vlm-bench/qwen35-awq-i3 \
  ml/training/runs/vlm-bench/qwen35-awq-c80 ml/training/runs/vlm-bench/qwen35-awq-c100 \
  --out 2026-10-24 --cases ml/training/runs/vlm-bench/cases-102.jsonl   # 72 cases, SHA-256 ced53028111cf0d7
```

- **Scored before the rebase onto #210,** which adds a look-alike column these cases don't use. The runs' outputs were lost in that rebase: `main` now tracks `ml/training/runs` as a symlink, and checking it out removed the ignored directory. So the bench report and the figures here are the record; the Oct 23 run's outputs are still on the rig.
- **Recording:** the command and the seven photos of the [Oct 23 report](fake-vlm-qwen35-2026-10-23.md), `--record ml/training/eval/recordings/worker-qwen35-9b-awq-2026-10-24.json`. The new file is the fake server's default: 7 recordings, all status 200, SHA-256 `0fca73e71f9dfa6ab674fedf59896b007cea9a3cd46e856341a4fc35a97d55db`. It holds no key and no local path.

## Results

The bench report is [vlm-bench-2026-10-24.md](vlm-bench-2026-10-24.md). Reasons are counted as decoded, on the 72 cases' 648 indicator answers. "No closing stop" is a reason that ends without a full stop: on Qwen3.5, the sign of one the cap stopped (in the Oct 22 file, under 40, every reason had one).

| | Cap 60 (Oct 23) | Cap 80 | Cap 100 |
|---|---|---|---|
| Reasons: median / p90 / longest | 50 / 58 / 60 | 51 / 69 / 80 | 51 / 69 / 96 |
| No closing stop | 97 of 648 (15%) | 10 (1.5%) | **1** |
| Answers changed from cap 60 | — | 33 of 648 | 32 of 648 |
| Diagnosis, 7 `sure` cases | 3 | 3 | 3 |
| Healthy leaves kept clean, of 17 | 14 | 15 | 15 |
| "Yes" on healthy leaves / unsure share | 2% / 24% | 1% / 25% | 1% / 25% |
| Valid on the first try; job p50 / p95 | 72 of 72; 5.1 / 6.4 s | 72 of 72; 5.0 / 6.6 s | 72 of 72; 5.0 / 6.3 s |

- **Qwen3.5 writes past the 60 it is asked for.** With room, a quarter of its reasons are longer than 60 (159 of 648 at cap 100) and 12 longer than 80, so the cap goes to 100, not 80. The one reason still stopped is 96 characters ("…if this represents dozens per leaf on the").
- **The cap changes only the tail.** Between the cap-80 and cap-100 runs, on the same server, 617 of the 638 reasons that weren't cut at 80 are the same word for word, and 1 answer of 648 changed.
- **Against Oct 23's run, part of the change is the other server.** Its vLLM was started for the bench at 0.90, and the first reason of a call is one no cut reason before it can touch: of the 70 the old cap didn't stop, 15 read differently today, though none changed its answer. Of the 32 answers that changed, 9 follow one of the 97 reasons the old cap stopped and 23 one of the other 551.
- **The findings didn't change:** the same species, damage counts and organisms on every case, in all three runs.
- **Nothing reads a reason's length downstream.** The app doesn't show it, and `IndicatorEvidence.reason` has no cap of its own (contracts 7).

**The recording.** Every call decoded and validated on the first attempt, with no retry, in 22 s.

| Call | Oct 23 file (cap 60) | This run (cap 100) |
|---|---|---|
| Tomato findings (3 images) | tomato, high; 3 × `leaf_mines`, 3 boxes; 8 soil readings | The same, with the boxes or wording a little different; 4.1 s |
| Tomato indicators and verify (yes, no, unsure) | 5, 2, 2 each | 4, 3, 2 each; 3.3 s |
| Corn findings (2 images) | corn, high; 1 × `chewed_margins` and 3 × `other`, 4 boxes; no soil readings | The same counts; 3.4 s |
| Corn indicators | 3, 6, 0 | 3, 6, 0; 3.1 s |
| Squeeze and jar tests | crumbles; sandy | The same, word for word |
| The 27 reasons | 6 at exactly 60 (one cut mid-word), 20 with no closing stop | Longest 83, 12 over 60, every one ends with a full stop |

## Checks

- **Replay:** `python3 -m ml.training.eval.fake_vlm` on its new default, then `record_against_live_server` against it: 7 of 7 calls decode and validate (3.4 s), and the server logs no `conformed`, `synthesised` or `text drift`, so every call is an exact replay.
- **The worker:** a new test, `the_reason_cap_is_well_above_what_the_prompt_asks_for`, reads the prompt's "at most N" and fails unless the cap is at least N + 40. `cargo test -p worker -p domain`, `cargo clippy -p worker -p domain --all-targets -- -D warnings` and `cargo fmt --check` pass.
- **`ml/`:** `vlm_check.py` (the day-1 check) has the same cap as the worker. `vlm_bench score` reports the reasons' lengths and the answers that differ from the first run, with a test. `python3 -m pytest ml/tests` passes, except `test_chunk_reads_a_two_column_pdf_by_type_size`, which needs `pdfminer`, missing from the rig's venv and unrelated to this change.
