# PDM Vision Roadmap

> **Status:** v0.4, 2026-10-16 (night). v0.4: decisions now live only in dev-roadmap section 9; our model files moved from the interim GitHub release to `GreenLens/pdm-models`. v0.3: PR #6 merged; PR #7 has `main` merged in and targets `main`; our model files' GitHub release is an interim until `pdm-models` (dev-roadmap M21). v0.2: the PR #6 reviews are answered: Tomato-Pakistan reworked, disease head v1d, the detector retrained on a grouped split; the whole-ML plan with the integration track moves to [dev-roadmap.md](dev-roadmap.md). v0.1: First tracker of the vision and ML work against the build plan in [README 10](README.md#10-build-plan), with what is done, what is next and what waits on whom.
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

Day 1 of 10 (Oct 16). The work is **ahead of plan on models and serving, behind on the evaluation set**:

- Ahead: the detector is on its second clean fine-tune (planned Oct 17); the disease head is at v1d (v1 was planned for Oct 19); the VLM day-1 check passed with both Qwen3-VL-8B builds.
- Fixed tonight (PR #6 reviews): Tomato-Pakistan's 7,200 files are copies of about 934 photos, 302 of them PlantDoc's web photos. Copies are now grouped by original photo, the PlantDoc copies dropped, TOM2024 grouped by field visit, and the field sets have their own test split. Validation images with a near-copy in train fell from 925 of 1,792 to 287 of 1,152. Disease head v1d: test top-1 0.814 without bean (val 0.819), ECE 0.032.
- Behind: no evaluation photos yet (planned from Oct 16), no Python–Rust parity test.
- Found tonight: the Rust loaders on `main` don't read our head, label-matrix and manifest formats yet (dev-roadmap gaps G1–G9). Sara's first task.

## 2. Branches and pull requests

| PR | Branch | Into | State | Holds |
|---|---|---|---|---|
| [#6](https://github.com/aacruzgon/GreenLens/pull/6) | `feat/ml-training` (deleted) | `main` | Merged (749a6b1) | Training environment, `pdm_vision`, data pipeline, exports, disease head v1d, manifest |
| [#7](https://github.com/aacruzgon/GreenLens/pull/7) | `feat/vlm-serving` | `main` | Ready for review; `main` merged in | `infra/` vLLM service, the day-1 check client and reports, vlm.md §4 results, the detector, this file and dev-roadmap.md |

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
| YuNet face detector export | Not started | Ran on a Mac with ORT CPU in the design phase; not in the manifest |
| Text embedder pinned and checked (Rust vs Python to 1e-5, ≤ 20 ms) | Not started | Choice recorded in architecture open decision 13; the Rust side is the backend's |
| RF-DETR-S leaf detector, fine-tuned and exported | Done | `rfdetr-s-leaf-2026-10-16b`: pinned COCO weights, train/valid grouped by same plant, train at ≥ 0.97 to test dropped (a5518f9). mAP50 0.937 fp16 ONNX vs 0.937 PyTorch, mAP50-95 0.729, on the 236 test images (the first run: 0.934, 0.722) |
| Disease head v0 → v1d | Done | cc884dd, c1c9c37, then v1d (3c10ae9): C = 50, T = 0.821; test top-1 0.814 without bean (PlantDoc 0.743, TOM2024 0.845, Tomato-Pakistan 0.800), top-3 0.964, ECE 0.032 |
| BioCLIP label matrix and `label_set_version` | Done (stand-in) | Format in `ml/models/FORMATS.md`, test vector `129f0395e8d89bd5` (confirmed by aacruzgon); stand-in labels until the seed files exist |
| vLLM serves Qwen3-VL-8B FP8 on the 5070 Ti, day-1 check | Done | 97b0f9a, fe1e639, 01aea25; reports in `ml/training/eval/reports/`. FP8 headless at 0.90 (84 tok/s, 2 slots fit); AWQ in desktop mode at 0.78 (115 tok/s) |
| Qwen3.5-9B AWQ challenger loads | Next | Weights downloaded and pinned (infra README 4); not yet started once |
| vLLM ROCm on the RX 9070 XT | Not ours | AMD day-1 check, owner of that card |
| BioCLIP zero-shot on 20 team photos | Not started | `labels/zero_shot_check.py` exists; needs the photos |
| Face detector on 10 team photos | Not started | Needs YuNet and the photos |
| Evaluation photos (Oct 16–23) | Not started | `ml/training/eval/photos/` doesn't exist yet; [evaluation 2](evaluation.md#2-the-evaluation-set) |
| Ask an extension office about confirming cases | Not started | |

### Oct 18–19

| Item | State | Evidence and notes |
|---|---|---|
| Day-2 VLM benchmark (Oct 18): both candidates, 20 dev cases, `VLM_SLOTS`, `VLM_MAX_PIXELS` | Not started | Needs the 20 dev cases from the evaluation set; [evaluation 8](evaluation.md#8-day-2-benchmarks) |
| Perception batch caps benchmark | Not started | On the 3070, with every session loaded |
| Leaf relabelling pass (people, Oct 18) | Not started | |
| Real BioCLIP label matrix from the species and organism seed files | Waiting | On the knowledge-base seed files and decision #4 |
| VLM prompts and schemas on real photos | Waiting | The prompts are in Sara's worker code (dev-roadmap P5); day-1 found three issues (vlm.md §4) |
| Second detector run (night of Oct 18, vLLM stopped) | Not started | After relabelling |
| Disease and species heads on crops from our detector (Oct 19) | Not started | The data rework is done; needs the second detector run |
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

1. **Merge PR #7** once reviewed, so Sara builds against `main`; invite Sara and Alan to the `GreenLens` organisation; make the control plane's and the rig's read tokens; delete the interim GitHub release (dev-roadmap M21).
2. **Unblock Sara:** `pdm-models`, `fetch.py --for perception --cpu`, the CPU manifest with its head twins and the Rust fixtures are done (dev-roadmap M6, M9, M21); the formats are settled by #11. Next, the fake VLM server (M20).
3. **Start the Qwen3.5 challenger once** and run the day-1 check on it.
4. **Start the evaluation photos** and the 20 dev cases the day-2 benchmark needs (Oct 18).
5. **YuNet and the text embedder pinned** (dev-roadmap M7, M8); BioCLIP zero-shot on team photos.
6. Day-2 VLM benchmark on Oct 18 with Sara's worker; leaf relabelling; second detector run that night.

## 6. The rig right now

As of 2026-10-16 night: **headless** (desktop stopped), vLLM serving Qwen3-VL-8B FP8 at 0.90 with 2 slots (`infra-vllm-1`, about 14.6 of 16.3 GB used); it was stopped for the detector retrain and started again with `docker start infra-vllm-1`. To get the desktop back: `docker compose -f infra/rig.compose.yml down`, `sudo systemctl isolate graphical.target`, log in, and run vLLM with the AWQ build at 0.78 ([infra README 3](../../infra/README.md#3-desktop-mode-and-headless-mode)). After the desktop is back, run `gh auth login` again so the token moves into the keyring.
