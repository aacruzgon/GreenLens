# PDM Vision Roadmap

> **Status:** v0.10, 2026-10-19 (evening). v0.10: day 4: #33 is merged; the third detector candidate, trained on background photos with no vegetation and a licence that allows commercial use, ships with a new plant-check pair chosen per dataset and checked on hold-out photos; #45 is re-recorded (FP8 passes all four checks, Qwen3.5 regresses on two); #48 is merged and #3 closed; the evaluation shot list is posted as #52, and still no photos exist. v0.9: #30 (`findings@2`) and #31 are merged; Sara is on the rest of P5. v0.8: end of day 1: #26 merged, #30 drafted for Sara, #33 measured a detector trained with no-plant backgrounds, YuNet pinned, the rig's state. v0.7: the day-1 plant probe is built and the provisional `τ_leaf` 0.90 and `τ_plant` 0.67 replace 0.35; #25 is merged. v0.6: `τ_leaf` is provisionally 0.35; #7, #16 and Sara's #18 are merged, so sections 1, 2 and 5 are brought up to date. v0.5: the Qwen3.5-9B challenger passed its day-1 check and the fake VLM server is built, with the fixes from its review. v0.4: decisions now live only in dev-roadmap section 9; our model files moved from the interim GitHub release to `GreenLens/pdm-models`. v0.3: PR #6 merged; PR #7 has `main` merged in and targets `main`; our model files' GitHub release is an interim until `pdm-models` (dev-roadmap M21). v0.2: the PR #6 reviews are answered: Tomato-Pakistan reworked, disease head v1d, the detector retrained on a grouped split; the whole-ML plan with the integration track moves to [dev-roadmap.md](dev-roadmap.md). v0.1: First tracker of the vision and ML work against the build plan in [README 10](README.md#10-build-plan), with what is done, what is next and what waits on whom.
> **How to use it:** read this first when picking up the ML work. Update it at the end of every work session: change the state of each item you touched, add the commit or report that proves it, and bump the status line. The plan itself stays in [README 10](README.md#10-build-plan); this file only tracks progress against it.

Owner of the vision and ML work: Sam (models, training, export, evaluation, calibration; the RTX 5070 Ti). Running the models in the backend is Sara's track ([docs/backend/vision.md](../backend/vision.md), [worker.md](../backend/worker.md)); the plan both tracks share, with owners, hand-offs and the integration gaps, is [dev-roadmap.md](dev-roadmap.md). This file tracks the models.

States: **Done**, **Done, rework** (done but a review found a problem), **In progress**, **Next**, **Not started**, **Waiting** (on a person or a decision), **Not ours** (another owner's, listed because ours depends on it).

## Contents

1. [Where we are](#1-where-we-are)
2. [Branches and pull requests](#2-branches-and-pull-requests)
3. [Tracker by build-plan period](#3-tracker-by-build-plan-period)
4. [Decisions and who they wait on](#4-decisions-and-who-they-wait-on)
5. [Next steps, in order](#5-next-steps-in-order)
6. [The rig right now](#6-the-rig-right-now)

---

## 1. Where we are

Day 4 of 10 (Oct 19, evening). **Models and serving are ahead; the evaluation set hasn't started and is the biggest risk**:

- Behind: no evaluation photos (M10, planned from Oct 16). The shot list is posted as [#52](https://github.com/aacruzgon/GreenLens/issues/52): 20 full cases by Oct 20 evening. The day-2 VLM benchmark (J2), calibration (M16), the YuNet face check and the zero-shot check on team photos all wait on them. Late October in Toronto has few diseased supported crops, so licensed photos may have to fill that group ([evaluation 2](evaluation.md#where-the-photos-come-from)).
- Detector: shipped in this PR: `rfdetr-s-leaf-2026-10-19-bglic`, trained with 400 COCO photos that have no plant pixel and a licence that allows commercial use. An earlier pick broke training.md §3's licence rule (308 of its 400 photos), so its candidate isn't shipped. The plant check moves to `τ_leaf` 0.40 and `τ_plant` 0.70: on 1,000 hold-out photos, 0.23% of plant photos fail (TOM2024 0.48%, was 2.14%) and 11.0% of no-plant photos pass (was 9.3%), 1 point over the 10% target. The leaf score alone passes 6.6% of no-plant photos, down from 52.5%.
- Plant check: chosen per dataset, because the pooled rate hid the field photos, and reported once on the test split and 1,000 COCO train2017 photos no threshold or training run had seen ([models 2](models.md#2-plant-present-check)).
- Prompts: Sara's rest of P5, [#45](https://github.com/aacruzgon/GreenLens/pull/45), is re-recorded on both candidates (18f417a). FP8 passes all four checks. Qwen3.5 fixes the tomato's `leaf_mines` and the species, but gives no soil readings on the tomato (8 before) and calls the corn's blight `leaf_mines` 10 times. It stays a draft for one more prompt pass.
- Integration: Sara's knowledge loader is merged ([#48](https://github.com/aacruzgon/GreenLens/pull/48)), with the text embedder's golden vectors for 5 inputs. M21 is done: our files come from `GreenLens/pdm-models`.

## 2. Branches and pull requests

| PR | Branch | Into | State | Holds |
|---|---|---|---|---|
| [#6](https://github.com/aacruzgon/GreenLens/pull/6) | `feat/ml-training` (deleted) | `main` | Merged (749a6b1) | Training environment, `pdm_vision`, data pipeline, exports, disease head v1d, manifest |
| [#7](https://github.com/aacruzgon/GreenLens/pull/7) | `feat/vlm-serving` (deleted) | `main` | Merged (d0db288) | `infra/` vLLM service, the day-1 check, the detector, model files from `pdm-models`, this file and dev-roadmap.md |
| [#16](https://github.com/aacruzgon/GreenLens/pull/16) | `feat/vlm-fake-and-challenger` (deleted) | `main` | Merged (59062ce) | The fake VLM server, the worker's recorded calls, the Qwen3.5-9B day-1 check |
| [#18](https://github.com/aacruzgon/GreenLens/pull/18) (Sara) | `fix/perception-manifest` | `main` | Merged (bd21eb1) | Tensor names, thresholds and precision from the manifest (G4, G6, G7, G11) |
| [#25](https://github.com/aacruzgon/GreenLens/pull/25) | `feat/provisional-tau-leaf` (deleted) | `main` | Merged (fea1808) | Provisional `τ_leaf` = 0.35, `ml.training.eval.leaf_scores` and its val report |
| [#26](https://github.com/aacruzgon/GreenLens/pull/26) | `feat/plant-probe-zero-shot` (deleted) | `main` | Merged (fd75210) | Day-1 plant probe, the `τ_leaf` and `τ_plant` pair, COCO negatives |
| [#30](https://github.com/aacruzgon/GreenLens/pull/30) (Sara) | `fix/findings-soil-shape` | `main` | Merged (1e2496a) | `findings@2`: the soil part of P5, with both models re-recorded |
| [#33](https://github.com/aacruzgon/GreenLens/pull/33) | `feat/detector-background` | `main` | Merged (a5b273d) | No-plant background images with no vegetation, the first candidate's measurements (not shipped), the per-dataset and hold-out plant check |
| [#45](https://github.com/aacruzgon/GreenLens/pull/45) (Sara) | `fix/prompts-p5` | `main` | Draft | The rest of P5, re-recorded on both candidates (18f417a); one more prompt pass for Qwen3.5's soil and corn |
| [#48](https://github.com/aacruzgon/GreenLens/pull/48) (Sara) | `feat/knowledge-loader` | `main` | Merged (b7963e2) | The knowledge loader, with the text embedder's golden vectors (M8) |
| this PR | `feat/detector-second-run` | `main` | Open | The detector retrained on licence-clean background photos and shipped with a new plant-check pair; the second candidate's measurements (not shipped); this file and dev-roadmap.md on Oct 19 |
| [#31](https://github.com/aacruzgon/GreenLens/pull/31) | `docs/roadmap-day1-wrap` | `main` | Merged (691bb40) | This file at the end of day 1 |

## 3. Tracker by build-plan period

### Oct 16–17

| Item | State | Evidence and notes |
|---|---|---|
| Training environment (`.venv`, `ml/requirements.lock`), shared pre-processing `ml/pdm_vision` | Done | e0612b8; Lanczos3 everywhere since ebd0217 (issue #2, closed) |
| PyTorch training step on the 5070 Ti | Done | Detector training ran on it |
| ONNX Runtime 1.31 CUDA on the 5070 Ti | Done | `features.cuda_session()` runs BioCLIP on CUDA and fails loudly otherwise |
| ONNX Runtime 1.31 CUDA on the RTX 3070 | Not started | Needs the 3070's host; same check as the 5070 Ti |
| Datasets fetched, pinned, mapped, deduplicated, split | Done | 3369353, 34d83d6, af4b0e2; reworked in efa6c4b: copies grouped by original photo, PlantDoc copies dropped, field visits grouped, field test splits. Train 5,102, val 1,152, test 854 |
| BioCLIP 2 vision tower exported and checked (fp16) | Done | b37e8df; manifest `bioclip` |
| SAM 2.1-T encoder and box decoder exported and checked | Done | b37e8df; manifest `sam_encoder`, `sam_decoder`; encoder budget 2.0 GB |
| YuNet face detector | Pinned | Upstream `face_detection_yunet_2026may.onnx` is in the manifest (`yunet`, MIT, M21); the check on 10 team photos waits for the photos (M10) |
| Text embedder pinned and checked (Rust vs Python to 1e-5, ≤ 20 ms) | Pinned | `text_embedder` and `text_embedder_tokenizer` in the manifest (revision d8c86521, M21); golden vectors for 5 inputs in Sara's [#48](https://github.com/aacruzgon/GreenLens/pull/48) (merged); 15 more sentences next; the Rust side is the backend's (P4) |
| RF-DETR-S leaf detector, fine-tuned and exported | Done | `rfdetr-s-leaf-2026-10-16b`: pinned COCO weights, train/valid grouped by same plant, train at ≥ 0.97 to test dropped (a5518f9). mAP50 0.937 fp16 ONNX vs 0.937 PyTorch, mAP50-95 0.729, on the 236 test images (the first run: 0.934, 0.722) |
| Disease head v0 → v1d | Done | cc884dd, c1c9c37, then v1d (3c10ae9): C = 50, T = 0.821; test top-1 0.814 without bean (PlantDoc 0.743, TOM2024 0.845, Tomato-Pakistan 0.800), top-3 0.964, ECE 0.032 |
| BioCLIP label matrix and `label_set_version` | Done (stand-in) | Format in `ml/models/FORMATS.md`, test vector `129f0395e8d89bd5` (confirmed by aacruzgon); stand-in labels until the seed files exist |
| vLLM serves Qwen3-VL-8B FP8 on the 5070 Ti, day-1 check | Done | 97b0f9a, fe1e639, 01aea25; reports in `ml/training/eval/reports/`. FP8 headless at 0.90 (84 tok/s, 2 slots fit); AWQ in desktop mode at 0.78 (115 tok/s) |
| Qwen3.5-9B AWQ challenger, day-1 check | Done | ✓ ran 2026-10-16, headless at 0.90 with 2 slots: checks a–g pass, 116.7 tok/s, 115,550 KV tokens, boxes 0–1000, no damage found in findings (FP8: one). Report `vlm-day1-2026-10-16-qwen35-9b-awq-headless` (vlm.md 4) |
| Day-1 plant probe | Done | ✓ ran 2026-10-16: zero-shot BioCLIP 2, prompt set A, `plant_probe/plant-probe-zs-2026-10-16.json` (pdm-models e00434c). Pair `τ_leaf` 0.90 and `τ_plant` 0.67: 0.8% of val plant photos fail, 11.4% of no-plant photos pass. Provisional until M16 ([report](../../ml/training/eval/reports/plant-check-val-2026-10-16.md), models.md 2) |
| COCO val2017 negatives (evaluation only) | Done | 4,758 photos without a potted plant or broccoli: `ml/training/data/negatives.py`, `splits/negatives_coco.tsv`; M13's trained probe will use them too |
| Fake VLM server (`ml.training.eval.fake_vlm`) | Done | Replays the worker's own recorded calls (dev-roadmap M20, section 5); the 27 tests in `ml/tests/test_fake_vlm.py` pass, and tomato's verify call replays its own recording |
| vLLM ROCm on the RX 9070 XT | Not ours | AMD day-1 check, owner of that card |
| BioCLIP zero-shot on 20 team photos | Not started | `labels/zero_shot_check.py` exists; needs the photos |
| Face detector on 10 team photos | Waiting | YuNet is in the manifest; needs the photos |
| Evaluation photos (Oct 16–23) | Not started | The biggest risk: `ml/training/eval/photos/` doesn't exist yet; the shot list is posted as [#52](https://github.com/aacruzgon/GreenLens/issues/52) (20 full cases first, for the benchmark); [evaluation 2](evaluation.md#2-the-evaluation-set) |
| Ask an extension office about confirming cases | Not started | |

### Oct 18–19

| Item | State | Evidence and notes |
|---|---|---|
| Day-2 VLM benchmark (Oct 18): both candidates, 20 dev cases, `VLM_SLOTS`, `VLM_MAX_PIXELS` | Waiting | Late: needs the 20 dev cases from the evaluation set; [evaluation 8](evaluation.md#8-day-2-benchmarks) |
| Perception batch caps benchmark | Not started | On the 3070, with every session loaded |
| Leaf relabelling pass (people, Oct 18) | Not started | Didn't happen; the second run trained without it |
| Real BioCLIP label matrix from the species and organism seed files | Waiting | On the knowledge-base seed files and decision #4 |
| VLM prompts and schemas on real photos | In progress | Soil: `findings@2`, merged in #30. The rest of P5 in Sara's [#45](https://github.com/aacruzgon/GreenLens/pull/45), re-recorded on Oct 19 (18f417a): FP8 passes all four checks (6 soil readings on the tomato, no `leaf_mines`, species and short English reasons); Qwen3.5 gives no soil readings on the tomato and 10 `leaf_mines` on the corn's blight, so #45 stays a draft for one more pass. Real photos still needed |
| Second detector run (night of Oct 18, vLLM stopped) | Done, this PR | `rfdetr-s-leaf-2026-10-19-bglic` (pdm-models 3306025): PlantDoc test mAP50 0.935 in PyTorch and 0.935 in fp16 ONNX, mAP50-95 0.721 (`16b`: 0.937, 0.729). Pair 0.40 / 0.70 chosen on val with 1% on every dataset; on the test split and hold-out photos 0.23% false `no_plant_found`, TOM2024 0.48%, 11.0% of no-plant photos passing ([models 2](models.md#2-plant-present-check)). The second candidate (`bgveg`) trained on a pick that broke the licence rule and isn't shipped. Trained without the relabelling pass |
| Disease and species heads on crops from our detector (Oct 19) | Next | The data rework is done and the detector is chosen; starts once this PR merges |
| Python–Rust pre-processing parity test | Waiting | dev-roadmap J1, after Sara's loader fixes (P1); `crop_box` rounding is a guess until then |

### Oct 20–25

| Item | State |
|---|---|
| VLM chosen; `VLM_SLOTS` per card; `VLM_MAX_PIXELS` (Oct 20–21) | Not started |
| Heads retrained as profiles are reviewed; fallback worker's live box call; spot colour thresholds | Not started |
| Calibration on the dev split, final manifests (Oct 22–23) | Not started |
| Test split scored once (Oct 24) | Not started |
| Model notices, Devpost write-up (Oct 25) | Not started |

## 4. Decisions and who they wait on

Decisions are tracked in one table, [dev-roadmap section 9](dev-roadmap.md#9-decisions-and-blockers), whose rows link their GitHub issues.

## 5. Next steps, in order

1. **Evaluation photos** (M10, [#52](https://github.com/aacruzgon/GreenLens/issues/52)): 20 full cases by Oct 20 evening for the benchmark. Everything in Phase 2 that is late waits on them.
2. **This PR:** Alan's review, then merge, so the worker loads the new detector and pair.
3. **Disease and species heads on the new detector's crops** (M13), once it merges.
4. **#45:** Sara's prompt pass for Qwen3.5's soil readings and the corn's `leaf_mines`, then re-record (about 8 minutes headless).
5. **#13 (D11), overdue:** how the VLM runs for the demo.
6. **Day-2 VLM benchmark** (J2) on the 20 cases.
7. **Leaf relabelling pass** (M11's first half, not done): a later detector run trains on the corrected boxes.

## 6. The rig right now

As of 2026-10-19, evening: headless and reached over SSH, vLLM stopped. Going headless killed the first desktop training run (stopping `app.slice` ends every desktop terminal); long runs now start from SSH with `setsid nohup`. The queue that was meant to run overnight ran from 15:04: the #45 re-recording, then the second detector candidate (trained 15:12–15:43) and its checks. The third candidate trained from 18:34. `infra/.env` is unchanged; the models are chosen with shell overrides.
