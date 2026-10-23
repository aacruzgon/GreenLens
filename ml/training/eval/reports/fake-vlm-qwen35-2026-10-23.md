# The fake VLM server's recordings on Qwen3.5-9B AWQ and the shipped prompts (2026-10-23)

A follow-up to task P3 ([report](p3-2026-10-23-fake-and-2b.md), [#162](https://github.com/aacruzgon/GreenLens/pull/162)). The committed recordings were made on Oct 22 (b2f53d1) with `indicators@2`, so after `indicators@3` ([#142](https://github.com/aacruzgon/GreenLens/pull/142)) the fake server logged `text drift` on the three indicator calls. This run re-records the worker's seven calls on Qwen3.5-9B AWQ, the default VLM (D1), with the prompts `main` ships. The new file is the fake server's default: `ml/training/eval/recordings/worker-qwen35-9b-awq-2026-10-23.json`, 7 recordings, all status 200, SHA-256 `b2580f6bd4522152950c81f08db858763f5df70ca2b03a70b19ad1cea51aa3eb`.

## How it ran

On `sam-linux`, Oct 23 at 18:13–18:16 EDT (22:13–22:16 UTC), from `main` at b732321 (prompts `system@2`, `findings@5`, `indicators@3`, `verify@3`, `guided@1`; `main` at 0d86a82 has the same prompts and schemas).

- **Server:** `vllm` from `infra/rig.compose.yml` (vLLM v0.31.0 by digest), `cyankiwi/Qwen3.5-9B-AWQ-4bit` at revision `156edc4b`, thinking off, 2 slots at 0.78 of the card, maximum length 16,384, `VLM_MAX_PIXELS` 1003520: the defaults of `infra/.env.example`.
- **Card:** healthy after 156 s, at 12,049 of 16,303 MiB; 12,313 MiB after the run.
- **Record:** the fake server as a recording proxy in front of vLLM, and the worker's test through it.

```bash
python3 -m ml.training.eval.fake_vlm --port 8001 \
  --record ml/training/eval/recordings/worker-qwen35-9b-awq-2026-10-23.json \
  --upstream http://127.0.0.1:8000          # VLM_API_KEY from infra/.env in the environment
cd backend
VLM_RECORD_BASE_URL=http://127.0.0.1:8001 VLM_RECORD_IMAGES=<the 7 photos below> \
  cargo test -p worker record_against_live_server -- --ignored --nocapture
```

**Photos:** the seven of [vlm.md 4](../../../../docs/vision/vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran) since Oct 21, the same as for the Oct 22 file. They are under `ml/training/data/raw/` on the rig. The fake server keys on schema and image count, not on the images, so replays don't need them.

| Slot | Photo |
|---|---|
| Tomato plant | `tom2024/tomato_diseases/alternaria_d/1001579731_1660205986357.jpg` |
| Tomato leaf | PlantDoc test, `Tomato Early blight leaf/039b47d574bc4bb8a14259a1cd96a741.jpg` |
| Tomato soil | `tom2024/tomato_diseases/Bacterial_floundering_d/335659947_1668156145380_640x480.jpg` |
| Corn plant | `tom2024/maize_diseases/Helminthosporiosis-D/100286842_1660551837195.jpg` |
| Corn leaf | PlantDoc test, `Corn leaf blight/02c.jpg` |
| Squeeze and jar tests | `tom2024/onion_diseases/Fusarium-D/1906382600_1662021022340.jpg` (a wet-soil stand-in, twice) |

## Results

Every call decoded and validated with the worker's code on the first attempt: no retry, no answer cut at its token budget. Each call keeps its key from the Oct 22 file, so conforming and replay pick the same recordings as before.

| Call | Oct 22 file (`indicators@2`) | This run (`indicators@3`) |
|---|---|---|
| Tomato findings (3 images) | tomato, high; 3 × `leaf_mines`, 3 boxes; 8 soil readings | The same answer, token for token; 5.5 s |
| Tomato indicators (yes, no, unsure) | 5, 3, 1 | 5, 2, 2; 3.2 s |
| Tomato verify | 3, 4, 2 | 5, 2, 2; 3.2 s |
| Corn findings (2 images) | corn, high; 1 × `chewed_margins` and 3 × `other`, 4 boxes; no soil readings | The same counts, in other words; 3.4 s |
| Corn indicators | 3, 6, 0 | 3, 6, 0; 3.3 s |
| Squeeze and jar tests | crumbles; sandy | The same; 0.3 s and 0.4 s |
| Whole test | — | 23 s |

- **The tomato's spots are still `leaf_mines`.** `findings@5` didn't change, so neither did the answer. That is P11's structural fix, not a recording problem.
- **6 of the 27 indicator reasons reach the schema's 60-character cap** and stop mid-word ("…but it is unclear if it"). The cap hasn't changed; `indicators@3` raised the prompt's ask from "under 40 characters" to "at most 60", and in the Oct 22 file the longest reason was 48. Decode cuts a reason at the cap back to its last full word (`truncate` in `backend/bins/worker/src/vlm/decode.rs`), so these validate.

## Checks on the new default

On the branch rebased onto `main` at 0d86a82 (with #162's `record.rs`):

- **Replay:** `python3 -m ml.training.eval.fake_vlm` with no `--recordings`, then the same test against it. 7 of 7 calls decode and validate (3.4 s), every one an exact replay, and the server logs no `text drift`.
- **The fake server's tests:** `python3 -m pytest ml/tests/test_fake_vlm.py`, 27 passed.
