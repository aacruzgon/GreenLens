# PDM Vision Roadmap

> **Status:** v0.14, 2026-10-20. v0.14: the RTX 3070 passes ONNX Runtime's check and fits the manifest's perception caps with 3.2 GB free (#114). v0.13: D1 is decided in #86 (Qwen3.5-9B AWQ, 2 slots at 1,003,520 pixels on the 5070 Ti). v0.12: end of day 5: the detector PR (#66) is merged; the day-2 VLM benchmark ran (#86) and Qwen3.5-9B AWQ leads, though no candidate's indicator answers carry much signal on one-photo cases; the heads on our detector's crops, the species head and the trained plant probe are in review (#85); 187 evaluation cases are reviewed; the ML roadmap aims to finish by Oct 22. v0.11: [#70](https://github.com/aacruzgon/GreenLens/pull/70) and [#76](https://github.com/aacruzgon/GreenLens/pull/76) are merged: the evaluation photos come from iNaturalist and, for tomato and corn, published field datasets, and wait on review; #4's names are applied, and the splits are train 4,742, val 1,039, test 763. v0.10: day 4: #33 is merged; the third detector candidate, trained on background photos with no vegetation and a licence that allows commercial use, ships with a new plant-check pair chosen per dataset and checked on hold-out photos; #45 is re-recorded (FP8 passes all four checks, Qwen3.5 regresses on two); #48 is merged and #3 closed; the evaluation shot list is posted as #52, and still no photos exist. v0.9: #30 (`findings@2`) and #31 are merged; Sara is on the rest of P5. v0.8: end of day 1: #26 merged, #30 drafted for Sara, #33 measured a detector trained with no-plant backgrounds, YuNet pinned, the rig's state. v0.7: the day-1 plant probe is built and the provisional `τ_leaf` 0.90 and `τ_plant` 0.67 replace 0.35; #25 is merged. v0.6: `τ_leaf` is provisionally 0.35; #7, #16 and Sara's #18 are merged, so sections 1, 2 and 5 are brought up to date. v0.5: the Qwen3.5-9B challenger passed its day-1 check and the fake VLM server is built, with the fixes from its review. v0.4: decisions now live only in dev-roadmap section 9; our model files moved from the interim GitHub release to `GreenLens/pdm-models`. v0.3: PR #6 merged; PR #7 has `main` merged in and targets `main`; our model files' GitHub release is an interim until `pdm-models` (dev-roadmap M21). v0.2: the PR #6 reviews are answered: Tomato-Pakistan reworked, disease head v1d, the detector retrained on a grouped split; the whole-ML plan with the integration track moves to [dev-roadmap.md](dev-roadmap.md). v0.1: First tracker of the vision and ML work against the build plan in [README 10](README.md#10-build-plan), with what is done, what is next and what waits on whom.
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

Day 5 of 10 (Oct 20). **The models are trained and measured; the plan is to finish the ML roadmap by Thursday, Oct 22**, so Friday is free to help with the integration:

- **Evaluation (M10):** 187 reviewed cases, 115 dev and 72 test, in [#86](https://github.com/aacruzgon/GreenLens/pull/86):
  - 103 one-photo cases from four tomato and corn field datasets;
  - 84 iNaturalist observations of apple, grape, squash and tomato (two microscope shots of a mite were dropped).
  - They still need a second reviewer before the test split is scored.
  - No case has a full shot set or a ladybug. What only phones can give is listed on [#52](https://github.com/aacruzgon/GreenLens/issues/52), with a private upload folder, for Oct 21.
- **VLM benchmark (J2), done in [#86](https://github.com/aacruzgon/GreenLens/pull/86):** the 65 tomato and corn dev cases ran through the worker's own calls on three candidates.
  - Every call was valid, and two jobs at once fit.
  - Qwen3.5-9B AWQ diagnoses 35% of the sure cases from its indicator answers, both Qwen3-VL-8B builds 12%.
  - On a diseased leaf, every model says "yes" to that disease's questions barely more often than to the others', so the diagnosis rests on the disease head until the prompts and questions improve.
  - D1 is decided: Qwen3.5-9B AWQ, with Qwen3-VL-8B as the fallback.
- **Heads (M13), in review in [#85](https://github.com/aacruzgon/GreenLens/pull/85),** on crops cut exactly as the worker cuts them:
  - **Disease head:** 0.842 top-1 per crop on test with 21 classes, 0.881 with 19. The class list waits on Sara.
  - **Species head:** 0.896 against zero-shot's 0.642 on the dev cases' supported crops, but models 6's combination rule keeps little of it.
  - **Trained plant probe:** passes 0.1% of no-plant photos against the shipped pair's 11.0%, with no plant photo failing.
- **Detector:** `rfdetr-s-leaf-2026-10-19-bglic` and the plant-check pair 0.40 / 0.70 are merged ([#66](https://github.com/aacruzgon/GreenLens/pull/66)).

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
| [#70](https://github.com/aacruzgon/GreenLens/pull/70) | `feat/eval-inat` | `main` | Merged (8288546) | iNaturalist photos for the evaluation set, licence and date filters, the review file |
| [#76](https://github.com/aacruzgon/GreenLens/pull/76) | `feat/eval-field-sets` | `main` | Merged (d246e34) | Tomato and corn evaluation photos from four published field datasets, phone-condition variants, #4's names in the mappings and splits |
| [#66](https://github.com/aacruzgon/GreenLens/pull/66) | `feat/detector-second-run` | `main` | Merged (d904e4c) | The detector retrained on licence-clean background photos and shipped with a new plant-check pair; the second candidate's measurements (not shipped); this file and dev-roadmap.md on Oct 19 and 20 |
| [#85](https://github.com/aacruzgon/GreenLens/pull/85) | `feat/heads-on-crops` | `main` | Open | Disease heads v2–v2c on our detector's crops, the species head and its iNaturalist photos, the trained plant probe, their reports; this file on Oct 20 |
| [#86](https://github.com/aacruzgon/GreenLens/pull/86) | `feat/vlm-bench` | `main` | Open | The day-2 VLM benchmark (J2) and its report, the 187 evaluation cases (M10) |
| [#31](https://github.com/aacruzgon/GreenLens/pull/31) | `docs/roadmap-day1-wrap` | `main` | Merged (691bb40) | This file at the end of day 1 |

## 3. Tracker by build-plan period

### Oct 16–17

| Item | State | Evidence and notes |
|---|---|---|
| Training environment (`.venv`, `ml/requirements.lock`), shared pre-processing `ml/pdm_vision` | Done | e0612b8; Lanczos3 everywhere since ebd0217 (issue #2, closed) |
| PyTorch training step on the 5070 Ti | Done | Detector training ran on it |
| ONNX Runtime 1.31 CUDA on the 5070 Ti | Done | `features.cuda_session()` runs BioCLIP on CUDA and fails loudly otherwise |
| ONNX Runtime 1.31 CUDA on the RTX 3070 | Done ([#114](https://github.com/aacruzgon/GreenLens/pull/114)) | ✓ ran 2026-10-20 on the 3070's host: CUDA against the CPU at most 1.8e-7, every perception model opens on CUDA |
| Datasets fetched, pinned, mapped, deduplicated, split | Done | 3369353, 34d83d6, af4b0e2; reworked in efa6c4b: copies grouped by original photo, PlantDoc copies dropped, field visits grouped, field test splits. Train 5,102, val 1,152, test 854; #4's names in [#76](https://github.com/aacruzgon/GreenLens/pull/76): spider mites at genus level, TOM2024's alternaria and wilt classes dropped, the splits edited in place to train 4,742, val 1,039, test 763 |
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
| Evaluation photos (Oct 16–23) | In progress | Sources merged: iNaturalist ([#70](https://github.com/aacruzgon/GreenLens/pull/70)) and published field datasets for tomato and corn ([#76](https://github.com/aacruzgon/GreenLens/pull/76)), with phone-condition variants. 118 field-set and 273 iNaturalist photos wait on review in `ml/training/eval/photos/`; full shot sets, soil, faces, look-alikes and LiDAR still need phones ([#52](https://github.com/aacruzgon/GreenLens/issues/52)); [evaluation 2](evaluation.md#2-the-evaluation-set) |
| Ask an extension office about confirming cases | Not started | |

### Oct 18–19

| Item | State | Evidence and notes |
|---|---|---|
| Day-2 VLM benchmark (Oct 18): both candidates, 20 dev cases, `VLM_SLOTS`, `VLM_MAX_PIXELS` | Done, [#86](https://github.com/aacruzgon/GreenLens/pull/86) | Ran Oct 20 on 65 one-photo dev cases and three candidates ([evaluation 8](evaluation.md#8-day-2-benchmarks)). Qwen3.5-9B AWQ 35% of sure cases diagnosed, Qwen3-VL-8B FP8 and AWQ 12%; `VLM_SLOTS` 2 fits; size made no difference |
| Perception batch caps benchmark | Measured ([#114](https://github.com/aacruzgon/GreenLens/pull/114)) | The manifest's caps (8, 2, 16, 16) peak at 4,598 MiB with 3.2 GB free; the worker's run (P2) confirms them |
| Leaf relabelling pass (people, Oct 18) | Not started | Didn't happen; the second run trained without it |
| Real BioCLIP label matrix from the species and organism seed files | Waiting | On the knowledge-base seed files and decision #4 |
| VLM prompts and schemas on real photos | In progress | Soil: `findings@2`, merged in #30. The rest of P5 in Sara's [#45](https://github.com/aacruzgon/GreenLens/pull/45), re-recorded on Oct 19 (18f417a): FP8 passes all four checks (6 soil readings on the tomato, no `leaf_mines`, species and short English reasons); Qwen3.5 gives no soil readings on the tomato and 10 `leaf_mines` on the corn's blight, so #45 stays a draft for one more pass. Real photos still needed |
| Second detector run (night of Oct 18, vLLM stopped) | Done, [#66](https://github.com/aacruzgon/GreenLens/pull/66) | `rfdetr-s-leaf-2026-10-19-bglic` (pdm-models 3306025): PlantDoc test mAP50 0.935 in PyTorch and 0.935 in fp16 ONNX, mAP50-95 0.721 (`16b`: 0.937, 0.729). Pair 0.40 / 0.70 chosen on val with 1% on every dataset; on the test split and hold-out photos 0.23% false `no_plant_found`, TOM2024 0.48%, 11.0% of no-plant photos passing ([models 2](models.md#2-plant-present-check)). The second candidate (`bgveg`) trained on a pick that broke the licence rule and isn't shipped. Trained without the relabelling pass |
| Disease and species heads on crops from our detector (Oct 19) | In review, [#85](https://github.com/aacruzgon/GreenLens/pull/85) | Disease head 0.842 per crop on test (0.881 with 19 classes), species head 0.896 against 0.642 on the dev cases, trained plant probe 0.1% of no-plant photos passing ([training 6](training.md#6-disease-and-species-heads), [models 2](models.md#2-plant-present-check)) |
| Python–Rust pre-processing parity test | Waiting | dev-roadmap J1, after Sara's loader fixes (P1). `crop_box` now uses the worker's rounding, read from `imaging.rs`, and its test pins the Rust test vector; the tensors themselves still need comparing |

### Oct 20–25

| Item | State |
|---|---|
| VLM chosen; `VLM_SLOTS` per card; `VLM_MAX_PIXELS` (Oct 20–21) | Done ([#86](https://github.com/aacruzgon/GreenLens/pull/86)): Qwen3.5-9B AWQ, `VLM_SLOTS` 2 and `VLM_MAX_PIXELS` 1,003,520 on the 5070 Ti; the 9070 XT waits on its stack |
| Heads retrained as profiles are reviewed; fallback worker's live box call; spot colour thresholds | Not started |
| Calibration on the dev split, final manifests (Oct 22–23) | Not started; moved up to Oct 21–22 |
| Test split scored once (Oct 24) | Not started; moved up to Oct 22, after the second review of the cases |
| Model notices, Devpost write-up (Oct 25) | Not started |

## 4. Decisions and who they wait on

Decisions are tracked in one table, [dev-roadmap section 9](dev-roadmap.md#9-decisions-and-blockers), whose rows link their GitHub issues.

## 5. Next steps, in order

1. **D1** is decided in [#86](https://github.com/aacruzgon/GreenLens/pull/86): Qwen3.5-9B AWQ, 2 slots at 1,003,520 pixels on the 5070 Ti. After [#45](https://github.com/aacruzgon/GreenLens/pull/45)'s next prompt pass, one bench run on its prompts re-checks it.
2. **#85:**
   - Sara's answer on the two classes (by Oct 21 evening, else 19 classes);
   - then `classifier_labels.yaml`, the manifest entries and publishing the heads;
   - Alan's review.
3. **The species head's combination rule** (models 6): choose one on the dev cases once the real label matrix exists (M12, which needs Sara's species and organism seed files).
4. **The team's photos** (#52, Oct 21): full shot sets, a pepper plant, ladybugs, look-alikes, no-plant shots and faces. The trained probe and M7's face check need them.
5. **Calibration (M16)** on the dev cases, then the final manifests (M17) and the test split scored once (M18), by Oct 22. The test split needs a second reviewer first.
6. **The indicator questions:** J2's per-question tables show which never get a "yes" on leaves that have the problem. Raise them with Sara's prompt pass (P5, #45).

## 6. The rig right now

As of 2026-10-20, afternoon: headless and reached over SSH, vLLM stopped.
- **The night of Oct 19 ran in full:** the benchmark, the crop views and the four disease heads, the species head and the probe. Only its two plant-check reports crashed, in the markdown writer.
- **Long runs now start in the tmux session `pdm`,** each GPU job holding a shared `flock`, with every step under a timeout.
- **On Oct 20,** after the reviews, the benchmark reran (12:37–13:43) and every head retrained (13:43–14:06).
- `infra/.env` is unchanged; the models are chosen with shell overrides.
