# PDM Vision Roadmap

> **Status:** v0.1, 2026-10-16 (evening). v0.1: First tracker of the vision and ML work against the build plan in [README 10](README.md#10-build-plan), with what is done, what is next and what waits on whom.
> **How to use it:** read this first when picking up the ML work. Update it at the end of every work session: change the state of each item you touched, add the commit or report that proves it, and bump the status line. The plan itself stays in [README 10](README.md#10-build-plan); this file only tracks progress against it.

Owner of the vision and ML work: Sam (models, training, export, evaluation, calibration; the RTX 5070 Ti). The Rust perception pipeline that runs the models is a teammate's ([docs/backend/vision.md](../backend/vision.md), [worker.md](../backend/worker.md)).

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

Day 1 of 10 (Oct 16). The work is **ahead of plan on models and serving, behind on data quality and the evaluation set**:

- Ahead: the detector's first fine-tune (planned Oct 17) is done and exported; the disease head is at v1c (v0 was planned for Oct 16–17, v1 for Oct 19); the VLM day-1 check passed with both Qwen3-VL-8B builds.
- Rework needed: aacruzgon's follow-up review of PR #6 found that **Tomato-Pakistan is pre-augmented and partly scraped from PlantDoc**, so it leaks into validation and test. Disease head v1c's numbers are not trustworthy until that is fixed (section 5, step 1).
- Behind: no evaluation photos taken yet (planned to start Oct 16), no field test split, no Python–Rust parity test.

## 2. Branches and pull requests

| PR | Branch | Into | State | Holds |
|---|---|---|---|---|
| [#6](https://github.com/aacruzgon/GreenLens/pull/6) | `feat/ml-training` | `main` | Draft, review required; **follow-up review open** | Training environment, `pdm_vision`, data pipeline, exports, detector, disease head v1c, manifest |
| [#7](https://github.com/aacruzgon/GreenLens/pull/7) | `feat/vlm-serving` | `feat/ml-training` | Draft | `infra/` vLLM service, the day-1 check client and reports, vlm.md §4 results, this file |

## 3. Tracker by build-plan period

### Oct 16–17

| Item | State | Evidence and notes |
|---|---|---|
| Training environment (`.venv`, `ml/requirements.lock`), shared pre-processing `ml/pdm_vision` | Done | e0612b8; Lanczos3 everywhere since ebd0217 (issue #2, closed) |
| PyTorch training step on the 5070 Ti | Done | Detector training ran on it |
| ONNX Runtime 1.31 CUDA on the 5070 Ti | Done | `features.cuda_session()` runs BioCLIP on CUDA and fails loudly otherwise |
| ONNX Runtime 1.31 CUDA on the RTX 3070 | Not started | Needs the 3070's host; same check as the 5070 Ti |
| Datasets fetched, pinned, mapped, deduplicated, split | Done, rework | 3369353, 34d83d6, af4b0e2. Tomato-Pakistan must be grouped by original photo and its PlantDoc copies dropped (PR #6 follow-up) |
| BioCLIP 2 vision tower exported and checked (fp16) | Done | b37e8df; manifest `bioclip` |
| SAM 2.1-T encoder and box decoder exported and checked | Done | b37e8df; manifest `sam_encoder`, `sam_decoder`; encoder budget 2.0 GB |
| YuNet face detector export | Not started | Ran on a Mac with ORT CPU in the design phase; not in the manifest |
| Text embedder pinned and checked (Rust vs Python to 1e-5, ≤ 20 ms) | Not started | Choice recorded in architecture open decision 13; the Rust side is the backend's |
| RF-DETR-S leaf detector, first fine-tune, exported | Done | 51d052f, 97a06f3 (train deduplicated against test), 4d2da5e; mAP50 0.934 ONNX vs 0.933 PyTorch. `prepare.py` still splits train/valid by file-name hash (review leftover) |
| Disease head v0 | Done | cc884dd; superseded by v1b (c1c9c37), then v1c |
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
| VLM prompts and schemas on real photos | Waiting | The prompts are the VLM owner's; day-1 found issues to pass on (vlm.md §4) |
| Second detector run (night of Oct 18, vLLM stopped) | Not started | After relabelling |
| Disease and species heads v1 on crops from our detector (Oct 19) | Not started | After the data rework |
| Python–Rust pre-processing parity test | Waiting | On the Rust `to_tensor`; `crop_box` rounding is a guess until then |

### Oct 20–25

| Item | State |
|---|---|
| VLM chosen; `VLM_SLOTS` per card; `VLM_MAX_PIXELS` (Oct 20–21) | Not started |
| Heads retrained as profiles are reviewed; fallback worker's live box call; spot colour thresholds | Not started |
| Calibration on the dev split, final manifests (Oct 22–23) | Not started |
| Test split scored once (Oct 24) | Not started |
| Model notices, Devpost write-up (Oct 25) | Not started |

## 4. Decisions and who they wait on

| Decision | State | Waits on |
|---|---|---|
| [#2](https://github.com/aacruzgon/GreenLens/issues/2) Resize filter | Closed: Lanczos3 | — |
| [#3](https://github.com/aacruzgon/GreenLens/issues/3) Head and label-matrix JSON format, `label_set_version` | Open; all four review points done in ebc3c36 | The team agreeing to close it |
| [#4](https://github.com/aacruzgon/GreenLens/issues/4) Supported crops and class-to-slug mapping | Open | The knowledge-base owner. Bean's place decides whether iBean counts in the headline |
| [#5](https://github.com/aacruzgon/GreenLens/issues/5) Duplicate threshold and conflicting PlantDoc groups | Open | A reviewer of the sampled pairs; mostly answered by the Tomato-Pakistan finding |
| Which VLM (README 11.1) | Open | The day-2 benchmark |
| Disease head or ConvNeXt-Tiny (README 11.4) | Open | Honest numbers after the data rework (the 75% top-1 bar) |

## 5. Next steps, in order

1. **Answer the PR #6 follow-up review.** Group Tomato-Pakistan by original photo (strip `_aug_N`, `_change_N`, `.rf.<hash>`) for dedupe and split; drop its copies of PlantDoc photos; hold out a field test set (a TOM2024 collection or a slice of Tomato-Pakistan by original); report the headline without bean; widen the C grid past 500; correct the dataset's description and its `ship` flag in `sources.yaml`; fix the manifest header ("v1b"); group the detector's train/valid split by cosine. Then retrain the head as v1d.
2. **Start the Qwen3.5 challenger once** and run the day-1 check on it, so the day-2 benchmark can't be blocked by a load failure.
3. **Start the evaluation photos** and the 20 dev cases the day-2 benchmark needs (Oct 18).
4. **YuNet export** and the face-detector check; BioCLIP zero-shot on team photos.
5. Day-2 VLM benchmark on Oct 18; leaf relabelling; second detector run that night.

## 6. The rig right now

As of 2026-10-16 evening: **headless** (desktop stopped), vLLM serving Qwen3-VL-8B FP8 at 0.90 with 2 slots (`infra-vllm-1`, about 14.6 of 16.3 GB used). To get the desktop back: `docker compose -f infra/rig.compose.yml down`, `sudo systemctl isolate graphical.target`, log in, and run vLLM with the AWQ build at 0.78 ([infra README 3](../../infra/README.md#3-desktop-mode-and-headless-mode)). After the desktop is back, run `gh auth login` again so the token moves into the keyring.
