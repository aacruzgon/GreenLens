# PDM Vision Roadmap

> **Status:** v0.21, 2026-10-21. v0.21: day 6 (Oct 21): #120 is merged (738265d) after Alan's review, and its branch is deleted; Sara answered on #85 that 19 classes ship, as v3c has; v3c's `classifier_labels.yaml` waits on #51's seed, and the apple, corn, grape and pepper problems on #125; #45 is out of draft and waits on our soil-photo swap and re-record; no #52 photo is uploaded yet; CI's runners haven't taken a job since 01:30. v0.20: disease head v3c, retrained with three garden-like field sets and chosen by its top 3 on the dev cases, is in the manifest (84% against v2c's 74%; Tomato-Pakistan's test falls from 0.762 to 0.688, accepted; corn's dev top-1 falls from 11 to 7 of 15); #109 is merged. v0.19: the leakage check covers the 59,821 training images on the rig and runs again on the final manifest, as part of M18. v0.18: after review of #110, the disease head's top 3 is scored as the API keeps it (73% against 52–57% for guessing), it does no better than guessing on the garden tomato leaves, and its temperature is a trade M17 chooses, its claims right 5 times in 10 without grape; #86 is merged. v0.17: the disease head stays, judged by its top 3 on the dev cases, with its dev-fitted temperature; garden-like training photos are next. v0.16: #85 is merged; the offline calibration ran, and the disease head reaches only 36–40% on the dev leaf photos, so D8 may need reopening. v0.15: the evaluation cases pass the leakage check, and their photos are in `pdm-models` for the second review. v0.14: the RTX 3070 passes ONNX Runtime's check and fits the manifest's perception caps with 3.2 GB free (#114). v0.13: D1 is decided in #86 (Qwen3.5-9B AWQ, 2 slots at 1,003,520 pixels on the 5070 Ti). v0.12: end of day 5: the detector PR (#66) is merged; the day-2 VLM benchmark ran (#86) and Qwen3.5-9B AWQ leads, though no candidate's indicator answers carry much signal on one-photo cases; the heads on our detector's crops, the species head and the trained plant probe are in review (#85); 187 evaluation cases are reviewed; the ML roadmap aims to finish by Oct 22. v0.11: [#70](https://github.com/aacruzgon/GreenLens/pull/70) and [#76](https://github.com/aacruzgon/GreenLens/pull/76) are merged: the evaluation photos come from iNaturalist and, for tomato and corn, published field datasets, and wait on review; #4's names are applied, and the splits are train 4,742, val 1,039, test 763. v0.10: day 4: #33 is merged; the third detector candidate, trained on background photos with no vegetation and a licence that allows commercial use, ships with a new plant-check pair chosen per dataset and checked on hold-out photos; #45 is re-recorded (FP8 passes all four checks, Qwen3.5 regresses on two); #48 is merged and #3 closed; the evaluation shot list is posted as #52, and still no photos exist. v0.9: #30 (`findings@2`) and #31 are merged; Sara is on the rest of P5. v0.8: end of day 1: #26 merged, #30 drafted for Sara, #33 measured a detector trained with no-plant backgrounds, YuNet pinned, the rig's state. v0.7: the day-1 plant probe is built and the provisional `τ_leaf` 0.90 and `τ_plant` 0.67 replace 0.35; #25 is merged. v0.6: `τ_leaf` is provisionally 0.35; #7, #16 and Sara's #18 are merged, so sections 1, 2 and 5 are brought up to date. v0.5: the Qwen3.5-9B challenger passed its day-1 check and the fake VLM server is built, with the fixes from its review. v0.4: decisions now live only in dev-roadmap section 9; our model files moved from the interim GitHub release to `GreenLens/pdm-models`. v0.3: PR #6 merged; PR #7 has `main` merged in and targets `main`; our model files' GitHub release is an interim until `pdm-models` (dev-roadmap M21). v0.2: the PR #6 reviews are answered: Tomato-Pakistan reworked, disease head v1d, the detector retrained on a grouped split; the whole-ML plan with the integration track moves to [dev-roadmap.md](dev-roadmap.md). v0.1: First tracker of the vision and ML work against the build plan in [README 10](README.md#10-build-plan), with what is done, what is next and what waits on whom.
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

Day 6 of 10 (Oct 21). **The models are trained and measured; the plan is to finish the ML roadmap by Thursday, Oct 22**, so Friday is free to help with the integration:

- **Evaluation (M10):** 187 reviewed cases, 115 dev and 72 test, in [#86](https://github.com/aacruzgon/GreenLens/pull/86):
  - 103 one-photo cases from four tomato and corn field datasets;
  - 84 iNaturalist observations of apple, grape, squash and tomato (two microscope shots of a mite were dropped).
  - They still need a second reviewer before the test split is scored. Their 253 photos are in `pdm-models` under `eval/photos/` for that review.
  - No case is a copy of a training image: the leakage check compared the 253 photos with all 59,821 training images on the rig by perceptual hash and BioCLIP cosine ([report](../../ml/training/eval/reports/leakage-2026-10-20.md)).
  - No case has a full shot set or a ladybug. What only phones can give is listed on [#52](https://github.com/aacruzgon/GreenLens/issues/52), with a private upload folder, for Oct 21. Sara proposed who shoots which folder on Oct 20; at 11:15 on Oct 21 no folder had a photo yet.
- **VLM benchmark (J2), done in [#86](https://github.com/aacruzgon/GreenLens/pull/86):** the 65 tomato and corn dev cases ran through the worker's own calls on three candidates.
  - Every call was valid, and two jobs at once fit.
  - Qwen3.5-9B AWQ diagnoses 35% of the sure cases from its indicator answers, both Qwen3-VL-8B builds 12%.
  - On a diseased leaf, every model says "yes" to that disease's questions barely more often than to the others', so the diagnosis rests on the disease head until the prompts and questions improve.
  - D1 is decided: Qwen3.5-9B AWQ, with Qwen3-VL-8B as the fallback.
- **Heads (M13), merged in [#85](https://github.com/aacruzgon/GreenLens/pull/85) (fc2ba3a),** on crops cut exactly as the worker cuts them:
  - **Disease head:** 0.842 top-1 per crop on test with 21 classes, 0.881 with 19. The class list is settled: Sara answered on #85 (Oct 21) that neither dropped class gets a profile by Thursday, so 19 classes ship, as v3c has.
  - **Disease head v3c, in the manifest, merged in [#120](https://github.com/aacruzgon/GreenLens/pull/120) (738265d):** retrained with PlantCity's, Pretoria's and Makerere's field photos, `C` chosen by its top 3 on the dev cases. The true problem is in its top 3 on 84% of the dev photos with a problem (v2c 74%, guessing 57%), and on 8 of the 12 iNaturalist garden tomato leaves (v2c 3). Tomato-Pakistan's test falls from 0.762 to 0.688, accepted. A review of the new data found PlantCity copies across splits; they're fixed and the head retrained. Corn's dev top-1 falls from 11 to 7 of 15, documented rather than tuned on 15 leaves. Alan's review on Oct 21 added two checks: the rule still picks C = 5 with any one dataset's val left out, and v2c scored on the new test sets gives PlantCity 0.386, Pretoria 0.382 and Makerere 0.868 (v3c 0.797, 0.713, 0.934).
  - **On the evaluation's dev leaf photos, judged by its top 3 (its job): the true problem is among them on 73% of the photos with a problem, against 52–57% for guessing; corn and tomato 66% against 38–45%.** Its top-1 is 36–40%. On the 12 iNaturalist garden tomato leaves it does no better than guessing. It stays. Its temperature is a trade M17 chooses: fitted on dev, it stops claiming on tomato and drops most of corn's right claims, and without grape, which is claimed at every value, its claims are right 5 times in 10 (v2: 10 in 16); a temperature per species keeps corn's but needs the head format changed ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)). Garden-like training photos are next.
  - **Species head:** 0.896 against zero-shot's 0.642 on the dev cases' supported crops, but models 6's combination rule keeps little of it.
  - **Trained plant probe:** passes 0.1% of no-plant photos against the shipped pair's 11.0%, with no plant photo failing.
- **Classifier labels and the seed:** v3c reaches the worker only with a `classifier_labels.yaml` whose problems are in the knowledge seed.
  - Sara's [#51](https://github.com/aacruzgon/GreenLens/pull/51) brings tomato, so tomato's 8 classes can map first. Apple, corn, grape and pepper come in as `label_only` species, and their 7 problem classes wait on [#125](https://github.com/aacruzgon/GreenLens/issues/125), which drafts those problems.
  - Until the seed, the labels and the worker agree, the API refuses real scans (`503 upstream_unavailable`), and the 3070's worker still loads v1d (Alan on #51, Oct 20).
  - Sara asks on #51 whether the worker takes a file that maps only tomato's classes until #125 lands. That answer is ours.
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
| [#45](https://github.com/aacruzgon/GreenLens/pull/45) (Sara) | `fix/prompts-p5` | `main` | In review | The rest of P5 at `findings@4` (7a07238): tighter `leaf_mines`, no repeated findings, text cut back to a full sentence or word. Waits on our swap of the tomato case's soil photo and a re-record on both models |
| [#48](https://github.com/aacruzgon/GreenLens/pull/48) (Sara) | `feat/knowledge-loader` | `main` | Merged (b7963e2) | The knowledge loader, with the text embedder's golden vectors (M8) |
| [#51](https://github.com/aacruzgon/GreenLens/pull/51) (Sara) | `feat/knowledge-tomato` | `main` | In review | The knowledge seed's tomato slice, and apple, corn, grape and pepper as `label_only` species; v3c's `classifier_labels.yaml` builds on it |
| [#70](https://github.com/aacruzgon/GreenLens/pull/70) | `feat/eval-inat` | `main` | Merged (8288546) | iNaturalist photos for the evaluation set, licence and date filters, the review file |
| [#76](https://github.com/aacruzgon/GreenLens/pull/76) | `feat/eval-field-sets` | `main` | Merged (d246e34) | Tomato and corn evaluation photos from four published field datasets, phone-condition variants, #4's names in the mappings and splits |
| [#66](https://github.com/aacruzgon/GreenLens/pull/66) | `feat/detector-second-run` | `main` | Merged (d904e4c) | The detector retrained on licence-clean background photos and shipped with a new plant-check pair; the second candidate's measurements (not shipped); this file and dev-roadmap.md on Oct 19 and 20 |
| [#85](https://github.com/aacruzgon/GreenLens/pull/85) | `feat/heads-on-crops` | `main` | Merged (fc2ba3a) | Disease heads v2–v2c on our detector's crops, the species head and its iNaturalist photos, the trained plant probe, their reports; this file on Oct 20 |
| [#86](https://github.com/aacruzgon/GreenLens/pull/86) | `feat/vlm-bench` | `main` | Merged (ca65148) | The day-2 VLM benchmark (J2) and its report, the 187 evaluation cases (M10) |
| [#109](https://github.com/aacruzgon/GreenLens/pull/109) | `feat/eval-leakage` | `main` | Merged (b77568d) | The leakage check: no evaluation photo is a copy of a training image |
| [#110](https://github.com/aacruzgon/GreenLens/pull/110) | `feat/calibration` | `feat/eval-leakage` (#109) | Merged (5f29dae) | The offline calibration on the dev cases (M16): `species_temperature`, the disease head's top 3 and its temperature's trade |
| [#114](https://github.com/aacruzgon/GreenLens/pull/114) | `feat/perception-gpu-check` | `main` | Merged (3890b38) | The perception card check and the batch caps that fit it (J3) |
| [#116](https://github.com/aacruzgon/GreenLens/pull/116) | `fix/vlm-bench-followups` | `main` | Merged (4462e39) | The post-merge review of #86 |
| [#120](https://github.com/aacruzgon/GreenLens/pull/120) | `feat/disease-garden-photos` (deleted) | `main` | Merged (738265d) | Three garden-like field sets, the copy fixes from their review, C chosen by the dev top 3, and disease head v3c in the manifest |
| [#126](https://github.com/aacruzgon/GreenLens/pull/126) | `docs/vision-tracker-120` | `main` | Open | This file on Oct 21 |
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
| Disease head v3c on garden-like photos | Done, [#120](https://github.com/aacruzgon/GreenLens/pull/120) | PlantCity, Pretoria and Makerere added; C = 5 by the dev top 3 within one standard error, best val; T = 1.82 fitted on dev; dev top 3 84% of 83 with a problem (v2c 74%); test top-1 per crop 0.850; Tomato-Pakistan test 0.688 (v2c 0.762); pdm-models 8473aad |
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
| VLM prompts and schemas on real photos | In progress | Soil: `findings@2`, merged in #30. The rest of P5 in Sara's [#45](https://github.com/aacruzgon/GreenLens/pull/45), re-recorded on Oct 19 (18f417a): FP8 passes all four checks (6 soil readings on the tomato, no `leaf_mines`, species and short English reasons); Qwen3.5 gives no soil readings on the tomato and 10 `leaf_mines` on the corn's blight, so #45 stayed a draft for one more pass. On Oct 20 the variant runs showed the tomato case's soil photo is a maize photo, so the model was right to skip it. Sara's `findings@4` (7a07238, Oct 21) tightens `leaf_mines`, drops repeated findings and cuts text back to a full sentence or word, and #45 is out of draft. Next: swap the soil photo and re-record both models, started Oct 21. Real photos still needed |
| Second detector run (night of Oct 18, vLLM stopped) | Done, [#66](https://github.com/aacruzgon/GreenLens/pull/66) | `rfdetr-s-leaf-2026-10-19-bglic` (pdm-models 3306025): PlantDoc test mAP50 0.935 in PyTorch and 0.935 in fp16 ONNX, mAP50-95 0.721 (`16b`: 0.937, 0.729). Pair 0.40 / 0.70 chosen on val with 1% on every dataset; on the test split and hold-out photos 0.23% false `no_plant_found`, TOM2024 0.48%, 11.0% of no-plant photos passing ([models 2](models.md#2-plant-present-check)). The second candidate (`bgveg`) trained on a pick that broke the licence rule and isn't shipped. Trained without the relabelling pass |
| Disease and species heads on crops from our detector (Oct 19) | Done, [#85](https://github.com/aacruzgon/GreenLens/pull/85) | Disease head 0.842 per crop on test (0.881 with 19 classes), species head 0.896 against 0.642 on the dev cases, trained plant probe 0.1% of no-plant photos passing ([training 6](training.md#6-disease-and-species-heads), [models 2](models.md#2-plant-present-check)) |
| Python–Rust pre-processing parity test | Waiting | dev-roadmap J1, after Sara's loader fixes (P1). `crop_box` now uses the worker's rounding, read from `imaging.rs`, and its test pins the Rust test vector; the tensors themselves still need comparing |

### Oct 20–25

| Item | State |
|---|---|
| VLM chosen; `VLM_SLOTS` per card; `VLM_MAX_PIXELS` (Oct 20–21) | Done ([#86](https://github.com/aacruzgon/GreenLens/pull/86)): Qwen3.5-9B AWQ, `VLM_SLOTS` 2 and `VLM_MAX_PIXELS` 1,003,520 on the 5070 Ti; the 9070 XT waits on its stack |
| Heads retrained as profiles are reviewed; fallback worker's live box call; spot colour thresholds | Waiting: 19 classes for now (Sara on #85, Oct 21), and a dropped class comes back with one retrain once its profile is signed. The box call and the colour thresholds are not started |
| Calibration on the dev split, final manifests (Oct 22–23) | In progress: the offline part is merged ([#110](https://github.com/aacruzgon/GreenLens/pull/110), 5f29dae; [evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)), and v3c carries its dev-fitted T (1.82); the VLM words, margin and weights wait on J5; moved up to Oct 21–22 |
| Test split scored once (Oct 24) | Not started; moved up to Oct 22, after the second review of the cases |
| Model notices, Devpost write-up (Oct 25) | Not started |

## 4. Decisions and who they wait on

Decisions are tracked in one table, [dev-roadmap section 9](dev-roadmap.md#9-decisions-and-blockers), whose rows link their GitHub issues.

## 5. Next steps, in order

1. **D1** is decided in [#86](https://github.com/aacruzgon/GreenLens/pull/86): Qwen3.5-9B AWQ, 2 slots at 1,003,520 pixels on the 5070 Ti. [#45](https://github.com/aacruzgon/GreenLens/pull/45) now waits on us: swap the tomato case's soil photo for a tomato-field photo with soil in it, post the 7 photos' dataset paths for vlm.md 4, and re-record `findings@4` (7a07238) on both models. FP8 needs the rig headless. Then one bench run on its prompts re-checks D1.
2. **#85 and #120 are merged, v3c is the manifest's disease head, and the class list is settled at 19** (Sara on #85, Oct 21). Left:
   - answer Sara on #51: does the worker take a `classifier_labels.yaml` that maps only tomato's classes until #125 lands?
   - v3c's `classifier_labels.yaml` once #51 is on `main`, the other four crops' classes after #125, and the species head and plant probe entries;
   - the change-over from v1d on the 3070, in README 6's order, once the seed and the labels are in.
3. **The species head's combination rule** (models 6): choose one on the dev cases once the real label matrix exists (M12, which needs Sara's species and organism seed files).
4. **The team's photos** (#52, Oct 21; Sara proposed who shoots which folder, and none had a photo at 11:15): full shot sets, a pepper plant, ladybugs, look-alikes, no-plant shots and faces. The trained probe and M7's face check need them.
5. **Garden-like training photos for the disease head:** done as v3c ([training 6](training.md#6-disease-and-species-heads)).
6. **Calibration (M16)** on the dev cases, then the final manifests (M17) and the test split scored once (M18), by Oct 22. The test split needs a second reviewer first. The leakage check passes on today's cases and runs again on the final manifest, as part of M18.
7. **The indicator questions:** J2's per-question tables show which never get a "yes" on leaves that have the problem. Raise them with Sara's prompt pass (P5, #45).

## 6. The rig right now

As of 2026-10-21, 11:15: headless and reached over SSH, vLLM stopped, the 5070 Ti idle (34 MiB used), no job holding the GPU lock.
- **The night of Oct 19 ran in full:** the benchmark, the crop views and the four disease heads, the species head and the probe. Only its two plant-check reports crashed, in the markdown writer.
- **Long runs now start in the tmux session `pdm`,** each GPU job holding a shared `flock`, with every step under a timeout.
- **On Oct 20,** after the reviews, the benchmark reran (12:37–13:43) and every head retrained (13:43–14:06).
- **On Oct 21** (around 01:00), for #120's review, v3c retrained with per-dataset val scores and came out byte-identical, and the splits rebuilt from `main`'s files came out byte-identical too. #45's re-record started at 11:09: candidates for the tomato case's soil photo, and a worker build.
- **CI:** the runners on the 3070 host haven't taken a job since 01:30 on Oct 21. `main`'s run for #120 (09:45) and #126's wait in the queue.
- `infra/.env` is unchanged; the models are chosen with shell overrides.
