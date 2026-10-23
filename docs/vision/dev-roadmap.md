# PDM ML Dev Roadmap

> **Status:** v0.33, 2026-10-22. v0.33: M10's second review is done and settled: 119 cases remain (76 dev, 43 test), 21 of them `sure` (7 in test). v0.32: D3 is settled for build week: potato, strawberry, squash and bean are `label_only` species with the PlantDoc crops outside the head and six garden plants, 20 species in all, so M12's label matrix has them. v0.31: M8 is done: the text embedder's golden test has 20 strings, among them a 1,022-token text both sides cut at 512, for the Rust side's check (P4). v0.30: P5 is done: #45 merged with `findings@5` (8f8337c), which meets the narrowed condition on Qwen3.5; the condition names its photo set; FP8, the fallback, still reports the corn's blight as animal damage, in boxes it doesn't locate, and P11 notes it (Alan's review of #126); M13's remaining work follows the settled class list and #51's merge. v0.29: D5's tomato slice is signed, and cites #120 for head v3c. v0.28: D5's tomato slice has had two signing reviews and waits for the signature; the four other crops of the head are `label_only` species in #51, with their problems in #125; the `seed-upload` container is done. v0.27: D3 is decided on #4, and D5 lists the curation work of #46 with its state: the loader is merged (#48), the tomato slice is redrafted after its first review in #51 and waits for the signing review. v0.26: P5's done condition is narrowed: Qwen3.5 calls the tomato's early-blight spots `leaf_mines` with every findings wording tried, so that case moves to the new task P11, a structural fix; P5's state records the one-change-at-a-time runs and `findings@5`, which meets the narrowed condition on Qwen3.5. v0.25: M13 ships disease head v3c, retrained on garden-like photos and chosen by its top 3 on the dev cases (84% against v2c's 74%; corn's dev top-1 falls from 11 to 7 of 15), and D8's next step is done. v0.24: M18 runs the leakage check on the final manifest first; M10's check covers the 59,821 training images on the rig. v0.23: M16 and D8 follow evaluation 5 after review: the disease head's top 3 as the API keeps it (73% against 52–57% for guessing), no better than guessing on the garden tomato leaves, and the temperature's trade per species, with the claims without grape, which M17 chooses. v0.22: D8 holds, judged by the disease head's top 3 on the dev cases; M16 proposes its dev-fitted temperature. v0.21: M16's offline part ran on the dev cases; D8 gets the disease head's 36–40% on the dev leaf photos as new evidence. v0.20: M10's cases pass the leakage check against every training image. v0.19: J3's caps are measured on the 3070 (#114). v0.18: P3 is in progress, its harness being #86's `bench.rs` and `record.rs`; D1 says its numbers are for `main`'s prompts, and one bench run on #45's prompts re-checks it. v0.17: D1 is decided: Qwen3.5-9B AWQ, with `VLM_SLOTS` 2 and `VLM_MAX_PIXELS` 1,003,520 on the 5070 Ti, set in `infra/` (M14 done); it also starts within the desktop's memory budget; P5's next prompt pass targets Qwen3.5. v0.16: J2 is done: the day-2 VLM benchmark ran on the 65 one-photo dev cases through the worker's own calls, every call valid and `VLM_SLOTS` 2 fitting, with Qwen3.5-9B AWQ diagnosing 35% of the sure cases from its indicator answers and both Qwen3-VL-8B builds 12%, so D1 has its numbers; M10 has 187 reviewed cases, and #52 lists what only the team's phones can give. v0.15: M13's heads are trained on our detector's crops and in review (#85): disease head v2 at 0.84 top-1 per crop on test (0.88 with 19 classes), so D8 is decided for the head; the species head beats zero-shot on the dev cases (0.90 against 0.64) but the combination rule keeps little of it; the trained plant probe passes 0.1% of no-plant photos against 11.0%. v0.14: M10's photo sources are merged ([#70](https://github.com/aacruzgon/GreenLens/pull/70), [#76](https://github.com/aacruzgon/GreenLens/pull/76)): 118 field-dataset photos for tomato and corn and 273 iNaturalist photos wait on review; #4's names are applied in the training mappings; Sara has access to `pdm-models` (P1). v0.13: M11's run is done: the detector retrained on licence-clean background photos ships with `τ_leaf` 0.40 and `τ_plant` 0.70 (M16, provisional); P5's rest is re-recorded (FP8 passes, Qwen3.5 regresses on two); M8's golden vectors merged with #48; M10's shot list is #52. v0.12, 2026-10-19. v0.12: M21 is done and M7 and M8 have their manifest entries, so only the photo checks and more golden sentences are left; P5's rest waits on Sam's re-recording of #45; M10 is now the biggest risk; D11 is overdue. v0.11: Sara owns the knowledge seed and its ingestion (D5), with the curation work in #46. v0.10: P5's soil part is merged in #30. v0.9: P5's soil part is drafted as `findings@2`, and P5 and D1 cite the re-recorded worker calls. v0.8: the day-1 plant probe is built and `τ_leaf` 0.90 with `τ_plant` 0.67 are provisional (M13, M16). v0.7: `τ_leaf` is provisionally 0.35 (M16), and M13's plant probe must catch the plant photos the detector keeps no leaf on. v0.6: section 4 marks G4, G6 and G7 fixed and adds G11 (fp16 refused on the CPU provider), all for [issue #12](https://github.com/aacruzgon/GreenLens/issues/12); `leaf_detector` gets a `tau_leaf` key; P1 waits only for `PerceptionPipeline::load` on the real `manifest.toml`. v0.5: the fake VLM server is built (M20, section 5 says how to run it) and the Qwen3.5-9B challenger passed its day-1 check (M3, D1 evidence); after review, the fake server replays the recording whose text matches when two share a key, logs text drift and answers 404 for a model it doesn't serve. v0.4: review of PR #7: our model files are in the private Hugging Face repository `GreenLens/pdm-models` with vision 6's `fetch.py`, `check.py` and `publish.py` (M21, D9 settled); section 4 marks G1–G3, G5 and G9 fixed by #11, leaves G4 and G6 to P1 (#12), and adds G10, the CPU heads, fixed here; section 9 is the only decisions table, each row linked to its issue; J2 and P5 take the day-1 review notes. v0.3: checked against `main` after PR #6 and the backend roadmap merged: the GitHub release is an interim until our files move to the private Hugging Face repository the team chose ([vision 6](README.md#delivering-model-files-to-the-backend), new task M21 and decision D9); gap G9 (`store` computes a different `label_set_version`); the rig's services follow [infra 4](../infra/README.md#4-compose-files); task IDs of other documents are cited with their document. v0.2.1: section 5 runs the perception worker on the laptop's RTX 3060 with the fp16 files and smaller batch caps, and says how the VLM jobs are covered meanwhile; section 8 points to it. v0.2: the integration track runs on Sara's Windows laptop (WSL): model files from a GitHub release with `ml/models/fetch.py`, a CPU manifest, reference fixtures, and two ways to test the VLM worker locally (a fake server, or Qwen3-VL-2B on llama.cpp, which passed the day-1 checks on a CPU); real jobs run on Sam's machine, run by Sam. v0.1, 2026-10-16 (evening): First plan for the whole ML side, models and their integration into the backend, by phase from Oct 16 to Oct 25, with owners, dependencies and done conditions, the hand-offs between `ml/` and the Rust crates, the integration gaps found on `main`, and a getting-started guide for the integration work.
> **How to use it:** find your next task in section 6, check its "Needs" column, and when it is done, change its state and add the commit, PR or report that proves it, in the same pull request as the work. Bump this status line when you do. The model-by-model detail (datasets, exports, metrics) is in [roadmap.md](roadmap.md); this file is the plan the two tracks share.

This covers everything between a photo and a diagnosis that involves a model: training and exporting the models (`ml/`), the files and formats that carry them, the Rust code that runs them (the [`vision`](../backend/vision.md) crate, the [`worker`](../backend/worker.md) binary) and the steps of the [`pipeline`](../backend/pipeline.md) that consume their outputs. The design lives in [docs/vision](README.md) and [docs/backend](../backend/README.md); this file says who builds what, in what order, and how to tell it is done.

States: **Done**, **In progress**, **Next** (start now), **Not started**, **Waiting** (on a person, a decision or another task), **To assign**.

Task IDs (M, P, J), gaps (G) and decisions (D) are this file's own. [Vision 6's delivery roadmap](README.md#roadmap) and [store's migration items](../backend/store.md) also number their items M1, M2, …, so cite those with their document: "vision 6 M3", "store M5". The backend's slices are S0–S8 ([backend roadmap](../backend/roadmap.md)).

## Contents

1. [Who does what](#1-who-does-what)
2. [How the pieces fit](#2-how-the-pieces-fit)
3. [Hand-offs between the two tracks](#3-hand-offs-between-the-two-tracks)
4. [Integration gaps found on main](#4-integration-gaps-found-on-main)
5. [Getting started (integration track)](#5-getting-started-integration-track)
6. [Roadmap by phase](#6-roadmap-by-phase)
7. [Milestones](#7-milestones)
8. [GPU time on the rig](#8-gpu-time-on-the-rig)
9. [Decisions and blockers](#9-decisions-and-blockers)

---

## 1. Who does what

| Track | Owner | Owns | Code |
|---|---|---|---|
| **Models** | Sam | Datasets, training, ONNX export, the heads and label matrices, the model manifest, the VLM server (vLLM on the 5070 Ti), evaluation, benchmarks and calibration | `ml/`, `infra/rig.compose.yml` (the `vllm` service), `docs/vision` |
| **Pipeline and integration** | Sara | Running the models in the backend: the `vision` crate's perception code, the perception and VLM workers, the VLM prompts and schemas in the worker, the worker services for the rig (Sam runs them there), and wiring their results through the `pipeline` steps. Works on her own laptop (section 5) | `backend/crates/vision`, `backend/bins/worker`, `backend/crates/pipeline`, the worker services in `infra/` |
| Shared | Sam and Sara | The hand-offs of section 3: file formats, pre-processing parity, the day-2 benchmark, the end-to-end scan, calibration values reaching the code | both |
| **Knowledge base** | Sara | The knowledge seed (species, problems, organisms, sources; issues [#4](https://github.com/aacruzgon/GreenLens/issues/4) and [#46](https://github.com/aacruzgon/GreenLens/issues/46)) and its ingestion, with each profile reviewed by a teammate | `ml/knowledge` |
| Not ML, but on our path | Other teammates | The `api`, the app, LiDAR measurement, the RTX 3070 and RX 9070 XT hosts | `backend/bins/api`, `app/` |

The backend skeleton on `main` already implements every crate from the design (no `todo!()` left in `vision`, `worker`, `pipeline` or `knowledge`). The integration work is therefore mostly **making that code run against the real model files**, not writing it from scratch. Section 4 lists what doesn't match yet.

---

## 2. How the pieces fit

```
 Sam: ml/ (Python)                              Sara: backend/ (Rust), infra/
 ─────────────────                              ──────────────────────────────
 datasets ─► train ─► export ─┐
                              ├─► ml/models/files/*.onnx, *.json ──► vision::manifest (hash check)
 ml/models/manifest.toml ─────┘    (entries, recipes, thresholds)        │
                                                                         ▼
 pdm_vision.preprocess ◄──── parity test ────► vision::perception::preprocess
                                                                         │
                                               perception-worker (RTX 3070): detector, SAM,
                                               BioCLIP species, disease head, embeddings
                                                                         │ job result
 vLLM server (5070 Ti) ◄──── OpenAI-compatible HTTP ──── vlm-worker: prompts, schemas,
 infra/rig.compose.yml                                   image preparation, decoding
                                                                         │ job result
                                                                         ▼
 evaluation harness ──── scans through the API ────► api: pipeline retrieve → score → plan
 (ml/training/eval)                                       (knowledge, fusion, plan writer)
         │
         └─► calibration values ─► manifest [model.config] ─► read by the workers and api
```

A scan's model work, in order ([architecture 6.1](../architecture/README.md#61-stages), [backend vision 6](../backend/vision.md#6-perception)):

1. **Upload** (`api`, CPU): blur and exposure check, faces blurred by YuNet.
2. **`perception` job** (perception-worker): leaf boxes (RF-DETR-S) → masks (SAM 2.1-T, one prompt per box) → BioCLIP 2 pass 1 over every image (plant probe, species top 3, image embeddings) → BioCLIP 2 pass 2 over the leaf crops → disease head top 3 per crop, restricted to the working species.
3. **`retrieve`** (`api`, `pipeline` + `knowledge`): measurement, weather, the candidate set and the indicators to ask.
4. **`vlm` job** (vlm-worker): the findings call (soil, organisms with boxes, damage, species cross-check) and the indicators call (yes, no or unsure per indicator).
5. **`score`** (`api`): deterministic scoring, an optional `verify` job, a question or the diagnosis, fusion.
6. **`plan`** (`api`, agent): the plan writer.

---

## 3. Hand-offs between the two tracks

Every arrow in section 2 that crosses from `ml/` to `backend/` is a contract. Each has one written spec; a change to it changes the spec and both sides in the same pull request.

| Hand-off | Producer → consumer | Spec | State |
|---|---|---|---|
| The model manifest: entry names, files, SHA-256, input recipes, output names, `[model.config]` | Sam → `vision::manifest` | [backend vision 5](../backend/vision.md#5-the-model-manifest), [vision README 6](README.md#6-model-files-versions-and-pinning), [`ml/models/manifest.toml`](../../ml/models/manifest.toml) | Entries for `text_embedder`, `text_embedder_tokenizer` and `yunet` (upstream, for `api`), and `bioclip`, `sam_encoder`, `sam_decoder`, `leaf_detector`, a stand-in label matrix and `disease_head_disease-head-v3c-2026-10-20`. Thresholds aren't read yet (gap G6) |
| ONNX files: tensor names, shapes, dtypes | Sam → perception-worker | [models 3–6](models.md#3-leaf-detector), the manifest | Exported in fp16 with fp32 I/O and checked on the 5070 Ti. BioCLIP's names differ (gap G4) |
| Head and label-matrix JSON | Sam → `vision::perception::heads` | [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md), [issue #3](https://github.com/aacruzgon/GreenLens/issues/3) | Matches the Rust loader since [PR #11](https://github.com/aacruzgon/GreenLens/pull/11) (G1–G3, G9 fixed, bb60549); the test vector `129f0395e8d89bd5` passes |
| Pre-processing (resize, crop, normalise, crop boxes) | Both | [backend vision 6](../backend/vision.md#6-perception) `to_tensor`, [`ml/pdm_vision/preprocess.py`](../../ml/pdm_vision/preprocess.py) | Written to match; never compared on real images (gap G8) |
| Model files on a machine | Sam → every worker host and laptop | [vision 6](README.md#delivering-model-files-to-the-backend): our files are in the private Hugging Face repository `GreenLens/pdm-models`, pinned by commit in [`sources.toml`](../../ml/models/sources.toml); [`ml/models/fetch.py`](../../ml/models/fetch.py) `--for <group> --dest DIR` downloads a host's files with `HF_TOKEN`, checks each SHA-256 and writes `DIR/manifest.toml`, which `MODEL_MANIFEST` points at. `--cpu` takes [`manifest.cpu.toml`](../../ml/models/manifest.cpu.toml): the fp32 ONNX files and CPU twins of the heads, for `ORT_EP=cpu`. [`check.py`](../../ml/models/check.py) and [`publish.py`](../../ml/models/publish.py) keep the manifests, sources and notices in step | Done (M5, M9, M21) |
| The VLM server: URL, key, served name, checkpoint, flags | Sam → vlm-worker | [vlm.md 4](vlm.md#4-serving), [`infra/README.md`](../../infra/README.md) | Live on the 5070 Ti: Qwen3-VL-8B FP8, served as `vlm`, `VLM_MAX_PIXELS` 1003520, 2 slots headless; used for real jobs, run by Sam. For testing: a fake server (M20) or Qwen3-VL-2B on llama.cpp on the laptop (section 5) |
| VLM requests: image preparation, schemas, prompts, decoding | Sara (worker code) | [contracts 6](../architecture/contracts.md#6-vlm-structured-output), [vlm.md 5–11](vlm.md#5-image-preparation-and-tokens) | Worker code on `main`; a Python reference with passing day-1 results in [`ml/training/eval/vlm_check.py`](../../ml/training/eval/vlm_check.py) |
| Thresholds, temperatures, certainty values | Sam (values) → workers and `api` (reading them) | [evaluation 5–6](evaluation.md#5-calibration) | Values are the docs' starting points until Oct 22–23 |
| Text embedder file and reference vectors | Sam → `vision::embed` in `api` | [models 10](models.md#10-text-embedder) | In `manifest.toml` (M21, revision d8c86521); golden vectors for 20 strings in `ml/tests/fixtures/text_embedder_golden.json` (M8), for the Rust embedder to check (P4) |
| YuNet face detector | Sam → `vision::upload::faces` in `api` | [models 9](models.md#9-face-detector) | Pinned with a checked hash in vision 6's S1 manifest; not in `manifest.toml` yet, face check not run (task M7) |
| The evaluation harness's access to the API | Sara and backend → Sam | [evaluation 9](evaluation.md#9-harness-and-reports) | Not started (task J4) |

---

## 4. Integration gaps found on main

Found by reading `backend/` on `main` against what `ml/` writes, on 2026-10-16. Each one breaks the perception worker at start-up or, worse, gives wrong results silently. They are the first integration tasks (P1 in section 6). [PR #11](https://github.com/aacruzgon/GreenLens/pull/11) (merged as bb60549) fixed G1, G2, G3, G5 and G9 on the Rust side, keeping FORMATS.md and the manifest as the spec. G4, G6, G7 and G11 are fixed for [issue #12](https://github.com/aacruzgon/GreenLens/issues/12); it closes once `PerceptionPipeline::load` succeeds on the real `manifest.toml`. The service rename that goes with G7 stays with task P6. G8 waits for J1. G10 was found and fixed in PR #7. The State column says which is which.

| # | Where | What `ml/` writes | What Rust reads | Effect | Proposed fix | State |
|---|---|---|---|---|---|---|
| **G1** | `vision/src/perception/heads.rs`, `HeadFile` | `labels[i].problem_id` (a slug or `"healthy"`), `version` | `labels[i].problem` (optional, defaults to `Healthy`), `classifier_labels_version` (optional, defaults to `""`) | **Silent:** every disease class loads as `healthy`. The head is keyed by `""`, so every job fails with `unknown_label_version` | Read `problem_id` and `version`; refuse a file whose `format` isn't `pdm-linear-head/1`; no defaults | Fixed in #11 (bb60549) |
| **G2** | `heads.rs`, `LabelFile` | `labels: [{id, table, label}]` with species and organism rows, `count`, `dim` | `species: [{id, label}]`, species rows only | The label matrix fails to load | Read `labels`; keep the rows whose `table` is `species` (organism rows stay for later) | Fixed in #11 (bb60549) |
| G3 | `heads.rs`, `ProbeFile` | FORMATS.md: `bias` is base64 `f32` like every head | `bias` is a JSON number | The plant probe would fail to load once it exists | Read FORMATS.md's shape (one class, base64 bias), or change FORMATS.md in #3 | Fixed in #11 (bb60549) |
| **G4** | `vision/src/perception.rs`, the BioCLIP call | Input `images`, output `embeddings` (also in the manifest) | Hard-coded `image` and `embedding` | BioCLIP fails on the first job | Take every model's tensor names from the manifest's `[model.input] name` and `[model.outputs]` instead of string literals. The detector and SAM names match today, but only by agreement | Fixed for [#12](https://github.com/aacruzgon/GreenLens/issues/12): `TensorNames` reads every model's input and output names from the manifest, and `load` checks them against each loaded session, so a manifest that disagrees with its export fails at start-up; tests load both committed manifests |
| G5 | `perception.rs`, `bioclip_version` | Manifest `version = "bioclip-2@2957b322"` | `format!("bioclip-2@{}", version)` | Every image embedding's `model` reads `bioclip-2@bioclip-2@2957b322` | Use the manifest version as it is | Fixed in #11 (bb60549) |
| **G6** | `worker/src/main.rs`, `PerceptionConfig` | `[model.config]`: `score_threshold`, `min_side_px`, `max_per_image`, `nms_iou`, `min_iou`, `plant_min_leaves`, `plant_grow`, `plant_min_cover`, `species_temperature`, `crop_grow` | `PerceptionConfig::default()` plus the batch caps from the environment | **Silent:** the manifest's thresholds are never used, yet `perception_config` hashes them as if they were. Calibration on Oct 22–23 would change nothing | `PerceptionConfig::from_manifest`, failing on a missing key; the environment keeps only the batch caps | Fixed for [#12](https://github.com/aacruzgon/GreenLens/issues/12): `PerceptionConfig::from_manifest` reads every threshold and has no default; a missing key refuses to start, and so does an environment batch cap above the manifest's `max_batch` or `max_boxes`. `tau_leaf` is a new `leaf_detector` key, 0.5 as a placeholder until M16 fits it and `tau_plant` is read from `plant_probe` when it exists |
| G7 | `worker/src/config.rs` | The compose service is `vllm` today; [infra 4](../infra/README.md#infrarigcomposeyml) names it `vllm-nv` | `VLM_BASE_URL` defaults to `http://vlm:8000` | The VLM worker can't reach the server in the compose network | Rename the service to `vllm-nv` with P6 and set `VLM_BASE_URL=http://vllm-nv:8000` in `vlm-worker-nv`'s environment | Fixed for [#12](https://github.com/aacruzgon/GreenLens/issues/12): the default is `http://vllm-nv:8000`. The service rename stays with P6 |
| **G9** | `store/src/repo/knowledge.rs`, `label_set_version` | The first 16 hex digits of a SHA-256 over the BioCLIP revision, the templates and the sorted labels, with a test vector (FORMATS.md) | An FNV-1a hash of the seed's ids and labels | The API queues jobs under a version no worker reports, so submit answers `503` | Read the version `ml/` writes instead of hashing (store M5 plans this through `knowledge_meta`), or compute FORMATS.md's hash; test it on FORMATS.md's vector | Fixed in #11 (bb60549) |
| **G10** | `ml/models/manifest.cpu.toml` vs the heads' `features` | `manifest.cpu.toml` swaps BioCLIP to its fp32 file, but a head's `features` names the fp16 file ([FORMATS.md](../../ml/models/FORMATS.md)) | The worker checks `features` against the loaded BioCLIP file | Since #11 the worker refused the head on the CPU (confirmed with `PerceptionPipeline::load`) | `ml/models/cpu_manifest.py` writes a CPU twin of each head (`<stem>.cpu.json`, same version and weights, `features` naming the fp32 file) | Fixed in PR #7: `PerceptionPipeline::load` now succeeds on `manifest.cpu.toml` with ONNX Runtime 1.31 on the CPU ✓ ran 2026-10-16. **Warning:** the fp16 `manifest.toml` can't be loaded with `ORT_EP=cpu`; the process segfaults in ONNX Runtime's CPU provider |
| **G11** | `vision/src/manifest.rs`, `ModelEntry`; `runtime.rs`, `OrtEp::Cpu` | `precision = "fp16"` on every GPU entry of `manifest.toml` | `precision` wasn't read, and the CPU provider was accepted with any manifest | `ORT_EP=cpu` with `manifest.toml` segfaults in ONNX Runtime instead of stopping with an error (found by Sam on PR #7) | Read `precision`; refuse fp16 on the CPU at start-up, naming the entry and `--cpu` | Fixed for [#12](https://github.com/aacruzgon/GreenLens/issues/12): `check_precision` runs first in `PerceptionPipeline::load` |
| G8 | `vision/src/perception/preprocess.rs` vs `ml/pdm_vision/preprocess.py` | Pillow Lanczos3, `rust_round`, `crop_box` rounding *guessed* | `fast_image_resize` Lanczos3, Rust rounding | Unknown: the heads were trained on Python's pixels | The parity test (task J1) | Open, task J1 |

---

## 5. Getting started (integration track)

### Read first, in this order

1. [AGENTS.md](../../AGENTS.md) (repository rules: branches, signed commits, no pushes to `main`) and this file.
2. [backend README 5, 8 and 13](../backend/README.md#5-crate-graph-and-dependency-rules): the crate graph, how the binaries are composed, the backend build plan.
3. [backend vision 4–6](../backend/vision.md#4-onnx-runtime-environment-execution-providers-session-pool): ONNX Runtime, the manifest, perception.
4. [worker 1, 4, 8 and 9](../backend/worker.md#1-purpose-and-boundaries): roles, configuration, the VLM adapters, schemas and decoding.
5. [vision README 3–6](README.md#3-model-inventory) and [vlm.md 4–11](vlm.md#4-serving): which model runs where, serving, image preparation, prompts.
6. [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md) and [`ml/models/manifest.toml`](../../ml/models/manifest.toml): the files you load.
7. In Claude Code, start Rust work with `/rust`; it routes to `/rust-workers` (leases, `ort`) and the other area skills.

### Python references for every Rust step

When a Rust result looks wrong, run the same step in Python on the same file. These are the implementations the models were trained and checked with.

| Step | Python reference |
|---|---|
| Image → tensor, every recipe; leaf crops | `ml/pdm_vision/preprocess.py` (`to_tensor`, `crop_box`) |
| BioCLIP embedding with the shipped ONNX file | `ml/pdm_vision/features.py` (`Embedder`) |
| Reading heads and label matrices | `ml/pdm_vision/headfile.py` |
| Disease head scoring, restricted softmax, temperature | `ml/training/heads/train_disease.py` (`restricted_logits`, `softmax`) |
| Species zero-shot against the label matrix | `ml/training/labels/zero_shot_check.py` |
| VLM image preparation, schemas, a findings and an indicators call | `ml/training/eval/vlm_check.py` and its reports in `ml/training/eval/reports/` |

### Your machine: Windows with WSL

Everything in the integration track runs on your laptop. You never log in to Sam's machine: real GPU runs happen there, run by Sam (see [Real jobs](#real-jobs-on-the-rig) below).

1. **WSL with Ubuntu 26.04** (in PowerShell as administrator: `wsl --list --online`, then `wsl --install -d <the 26.04 name>`). 26.04 because the backend needs libheif 1.19 or newer ([setup 4](../setup/README.md#4-backend-rust)). Clone the repository inside WSL (`~/…`), not under `/mnt/c`, which is slow for builds.
2. **In WSL**, follow [setup 2](../setup/README.md#2-everyone) (git, SSH signing) and [setup 4](../setup/README.md#4-backend-rust): Rust 1.99.1, cargo-nextest, the build tools, libheif, and ONNX Runtime 1.31.0's `onnxruntime-linux-x64-1.31.0.tgz`, with `ORT_DYLIB_PATH` pointing at its `lib/libonnxruntime.so`. Python 3.11 or newer is already there.
3. **Model files.** Our files are in the private Hugging Face repository `GreenLens/pdm-models` ([vision 6](README.md#delivering-model-files-to-the-backend)). Accept Sam's invitation to the `GreenLens` organisation, then make your own **fine-grained, read-only** token for that repository (huggingface.co → Settings → Access Tokens → Fine-grained: *Read access to contents of selected repos*, repository `GreenLens/pdm-models`). Keep it in your shell profile or an ignored `.env`, never in git. From the repository root:

   ```bash
   export HF_TOKEN=hf_…                     # your read-only token
   python3 -m ml.models.fetch --for perception --cpu --dest ~/.cache/pdm-models
   ```

   It downloads the fp32 set (about 1.5 GB) from `pdm-models`, at the commits pinned in `sources.toml`, checks each SHA-256, and writes the CPU manifest as `~/.cache/pdm-models/manifest.toml`. Rerunning it downloads only what changed. The worker resolves file paths against the manifest's own folder, so point it there:

   ```bash
   export MODEL_MANIFEST=$HOME/.cache/pdm-models/manifest.toml
   export ORT_EP=cpu
   export ORT_DYLIB_PATH=$HOME/onnxruntime-linux-x64-1.31.0/lib/libonnxruntime.so
   ```

   `manifest.cpu.toml` is `manifest.toml` with each fp16 ONNX file swapped for its fp32 export, because ONNX Runtime's CPU provider runs fp32 graphs (the fp16 manifest crashes it), and each head swapped for a CPU twin whose `features` names the fp32 BioCLIP file, which the worker checks (gap G10). `PerceptionPipeline::load` succeeds on it with ONNX Runtime 1.31 on the CPU ✓ ran 2026-10-16. Every fp32 graph runs on the CPU (BioCLIP: about 0.3 s per image), and its embeddings match the rig's fp16 ones to cosine 0.9997. Use it for development, not for measuring accuracy. Without `--cpu`, `fetch` gets the fp16 set the GPU workers load. `--for api` adds the text embedder and YuNet that `api` loads.
4. **Reference outputs for your tests:** [`ml/tests/fixtures/perception_reference.json`](../../ml/tests/fixtures/perception_reference.json) holds six photos' BioCLIP embeddings with the species top 3 and the disease head's top 3 that Python computes. A Rust test that loads the same head and label-matrix files and feeds it these embeddings should get the same ids in the same order, with probabilities within 1e-4. `python -m ml.tests.make_fixtures` regenerates it.

### The laptop's RTX 3060

Your laptop's RTX 3060 is an Ampere card (sm_86), the same architecture as the RTX 3070 that runs the perception worker, and ONNX Runtime 1.31's CUDA 13 build has native code for it ([vision README 5](README.md#5-runtimes-and-versions)). So the perception worker can run on a GPU with the real fp16 files on your laptop, without the rig. Use it to check that the GPU path works and that its outputs match the CPU run; speed, memory and benchmark figures still come from the 3070 (J3). Nobody has run the worker on this card yet.

1. **Driver.** NVIDIA's Windows driver 580.65.06 or newer, CUDA 13.0's minimum. WSL gets CUDA from the Windows driver, so don't install a Linux NVIDIA driver inside WSL; `nvidia-smi` in WSL should list the card.
2. **ONNX Runtime's GPU build.** `onnxruntime-linux-x64-gpu-1.31.0.tgz` instead of the CPU tarball, with the CUDA 13.0 runtime and cuDNN 9 libraries in WSL: the same pieces as the perception image, `nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04`. The package names in NVIDIA's WSL-Ubuntu repository are *unverified* here.
3. **The fp16 files and smaller batches.**

   ```bash
   python3 -m ml.models.fetch --for perception --dest ~/.cache/pdm-models-gpu   # the fp16 set, as on the rig
   export MODEL_MANIFEST=$HOME/.cache/pdm-models-gpu/manifest.toml
   export ORT_EP=cuda
   export ORT_DYLIB_PATH=$HOME/onnxruntime-linux-x64-gpu-1.31.0/lib/libonnxruntime.so
   export SAM_ENCODER_MAX_BATCH=1 DETECTOR_MAX_BATCH=4 BIOCLIP_MAX_BATCH=8
   ```

   Most RTX 3060 laptops have 6 GB, and the 3070's budget is about 5.3 GB at the planned caps ([vision README 4](README.md#4-placement-and-vram-budgets)). These caps should bring it to roughly 3.5–4 GB *estimate*; the SAM encoder at a batch of 1 saves the most. Caps change speed, not results: every export check confirms that per-image outputs agree across batch sizes. Check with `nvidia-smi` how much Windows' desktop holds on the card. On laptops whose integrated GPU draws the screen, the 3060 is nearly empty.
4. **The VLM doesn't fit next to it.** Qwen3-VL-8B's weights alone are 7.5 GiB in the AWQ build (vlm.md 4). Run Qwen3-VL-2B (option 2 below) on the CPU while perception has the card, or on the card when perception isn't running. Once the full local stack runs (J4), there is a third way: the fallback worker (`--caps fallback` with `LLM_API_KEY`, [worker 4](../backend/worker.md#4-configuration)) takes any job left unclaimed for 15 seconds, so with no vlm-worker running, the `vlm` jobs go to the hosted model. That runs a whole scan end to end without a VLM card, but it costs API credits.

### Testing the VLM worker

Two ways, both on your laptop. Neither needs Sam's machine.

**Option 1: the fake VLM server.** A small OpenAI-compatible server that answers with real Qwen3-VL-8B replies to the worker's own requests. It tests request building, schemas, decoding, validation and box conversion against real model output, offline and instantly, but always gives the same answers, so it can't test a prompt change. Built as task M20 (`ml/training/eval/fake_vlm.py`, standard library only). Its default replay source is `ml/training/eval/recordings/worker-qwen3vl8b-fp8-2026-10-16.json`, recorded 2026-10-16 by capturing the worker's own requests (the Rust test `record_against_live_server`, which drives the real adapter) against Qwen3-VL-8B FP8.

```bash
python3 -m ml.training.eval.fake_vlm            # port 8001, served name vlm
VLM_BASE_URL=http://127.0.0.1:8001 VLM_ADAPTER=vllm VLM_SERVED_NAME=vlm   # the worker's env; add VLM_API_KEY if you pass --api-key
```

Each reply says how it was made in the `x-fake-vlm` header. `replay` is an exact hit on the request's key: the recorded reply. `conformed` is the closest recording of the same call kind, fitted to the request's schema. `synthesised` is built from the schema alone, when nothing of that kind was recorded. When two recordings share a key (tomato's analyse `indicators` call and its `verify` call send the same schema), the one whose text matches the request's is replayed. Only `replay` is real model output. A schema change shows up as `conformed` or `synthesised`; a prompt change that keeps the schema still replays the old answer, and the server logs a `text drift` line. Either way it tells you the request changed, not whether the new prompt works. A request whose `model` isn't the served name gets 404, as from vLLM, so a wrong `VLM_SERVED_NAME` shows offline. It also can't test real latency or real image understanding. For retries, `--fault length|http500|http429|bad-json|slow` with `--fault-every N` breaks every Nth chat call. `--log-requests DIR` writes each request to a folder, and `--record FILE --upstream URL` turns it into a proxy that records a real server's replies. See `--help` for all flags.

**Option 2: Qwen3-VL-2B on llama.cpp.** The same model family as production, with the same 0–1000 box format, small enough for a laptop's CPU. Its answers are weaker and slower, so use it to test the wiring and see a prompt's effect, not to judge accuracy. On 2026-10-16 it passed all five day-1 checks on a CPU ✓ ran (llama.cpp b11528, 2 PlantDoc photos, `ml/training/eval/vlm_check.py`): findings valid against the schema in 16 s, indicators, the `maxLength` cap, nullable soil readings, and 25 tokens per second of decoding. A laptop CPU will be slower. In WSL:

```bash
mkdir -p ~/vlm && cd ~/vlm
curl -LO https://github.com/ggml-org/llama.cpp/releases/download/b11528/llama-b11528-bin-ubuntu-x64.tar.gz
tar xzf llama-b11528-bin-ubuntu-x64.tar.gz          # unpacks llama-b11528/
R=https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct-GGUF/resolve/52d6c8ffea26cc873ac5ad116f8631268d7eb503
curl -LO $R/Qwen3VL-2B-Instruct-Q8_0.gguf           # 1.83 GB
curl -LO $R/mmproj-Qwen3VL-2B-Instruct-F16.gguf     # 0.82 GB
./llama-b11528/llama-server \
  -m Qwen3VL-2B-Instruct-Q8_0.gguf --mmproj mmproj-Qwen3VL-2B-Instruct-F16.gguf \
  -c 12288 -np 1 --image-min-tokens 256 --image-max-tokens 1024 --temp 0 --jinja \
  --alias vlm --host 127.0.0.1 --port 8080 --api-key dev-key
```

Then the worker's VLM settings ([worker 4](../backend/worker.md#4-configuration)):

```bash
export VLM_BASE_URL=http://127.0.0.1:8080 VLM_API_KEY=dev-key VLM_SERVED_NAME=vlm
export VLM_ADAPTER=llamacpp VLM_SERVER=llama.cpp@b11528 VLM_QUANT=gguf_q8_0
export VLM_MODEL=Qwen/Qwen3-VL-2B-Instruct-GGUF@52d6c8ffea26cc873ac5ad116f8631268d7eb503
export VLM_BOX_FORMAT=norm1000 VLM_CELL_PX=32 VLM_MAX_PIXELS=1003520 VLM_SLOTS=1
```

The SHA-256 of the two files are in `ml/models/dev-vlm.sha256`. If your laptop has an NVIDIA GPU, `llama-b11528-bin-ubuntu-cuda-12.8-x64.tar.gz` runs the same command on it through WSL's CUDA support.

**Running one job without the API.** The worker only takes jobs from the API (`worker` claims, then `complete`s), so a test job needs either the whole local stack (API, SurrealDB, RustFS in Docker, a worker token and a queued scan) or a small harness that hands one job payload and its image files straight to the `VisionModel` (`PerceptionPipeline`, or `VlmAdapter` with the server above) and prints the result. The harness is part of tasks P2 and P3, and is the quicker route; the full stack comes with J4.

### Real jobs on the rig

Anything that needs the real models at full size runs on Sam's machine, and Sam runs it: the perception worker on a GPU, the VLM worker against Qwen3-VL-8B, the day-2 benchmark, the end-to-end scans. Push a branch and say what to run; Sam runs it and posts the output in the pull request. In deployment the workers need no access from outside either: they claim jobs from the API over outbound HTTPS and reach vLLM inside the same compose network ([worker 1](../backend/worker.md#1-purpose-and-boundaries)).

### A first useful session

1. Set up WSL, fetch the CPU model files and start llama-server (above).
2. Check that a test loads `disease-head-v3c-2026-10-20.json` and the stand-in label matrix against `perception_reference.json` (G1, G2, G4, G5 and G6 are fixed).
3. Start the harness for P2: load `PerceptionPipeline` from `manifest.cpu.toml` with `ORT_EP=cpu` and run it on a few of the fixture photos.

---

## 6. Roadmap by phase

IDs: **M** = models track (Sam), **P** = pipeline and integration track (Sara), **J** = joint. A task's "Done when" is the check that closes it.

### Phase 1, Oct 16–17: the seams, and a first real perception job

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M1 | Disease head v1d on the corrected splits (Tomato-Pakistan grouped by original photo, field test splits) | Sam | — | Manifest entry and report committed; test top-1 0.814 without bean, ECE 0.032 | Done (PR #6, 3c10ae9) |
| M2 | Leaf detector retrained on the grouped train/valid split, exported, manifest entry updated | Sam | GPU (vLLM stopped) | New ONNX within 0.5 points of PyTorch mAP50 on the test split; manifest hash updated | Done: `rfdetr-s-leaf-2026-10-16b`, mAP50 0.937 (PR #7) |
| M3 | Qwen3.5-9B challenger starts and passes the day-1 check | Sam | M2 finished (GPU) | A report in `ml/training/eval/reports/` | Done 2026-10-16 ✓ ran: `vlm-day1-2026-10-16-qwen35-9b-awq-headless`, checks a–g all pass, 116.7 tokens/s (FP8: 84), 115,550 KV tokens (FP8: 17,440), box scale 0–1000, no damage found in findings (FP8: one) |
| M4 | Answer PR #6's reviews; merge PRs #6 and #7 | Sam | Reviews | Both merged into `main` | In progress: #6 merged (749a6b1); #7 has `main` merged in and waits on review |
| M5 | Model files for other machines: a first `ml/models/fetch.py` and a GitHub release (fp16 and fp32 sets) | Sam | M2 | Sara gets the files on a fresh machine with one command | Done (PR #7, a3fa16a), then replaced by M21 |
| M6 | Fixtures for the Rust tests: a few saved BioCLIP embeddings with the Python head and zero-shot outputs (JSON) | Sam | M1 | Committed under `ml/tests/fixtures/` | Done (PR #7, 9ab7368): `perception_reference.json`, 6 photos |
| M7 | YuNet in `manifest.toml` (entry `yunet`; file and hash already pinned in vision 6's S1 manifest, vision 6 M1) and the face check on 10 team photos ([evaluation 7](evaluation.md#7-day-1-checks)) | Sam | 10 photos with people | Every face found; manifest entry committed | Manifest entry done (M21: `yunet`, MIT); the face check on 10 team photos waits on the photos (M10) |
| M8 | Text embedder in `manifest.toml`: arctic-embed-xs ONNX and tokenizer (`text_embedder`, `text_embedder_tokenizer`; pinned in vision 6's S1 manifest, vision 6 M1), plus reference vectors from Python for 20 sentences | Sam | — | Files and vectors committed; P5 can compare | Done. Manifest entries (M21: `text_embedder` and `text_embedder_tokenizer`, revision d8c86521). Golden vectors for 20 strings in `ml/tests/fixtures/text_embedder_golden.json`, written by `python -m ml.tests.make_text_golden`: the 5 merged with Sara's loader ([#48](https://github.com/aacruzgon/GreenLens/pull/48)) and 15 more covering case, accents, symbols, an emoji, whitespace, a seed chunk and a 1,022-token text cut at 512 ([models 10](models.md#the-pinned-recipe)). The Rust-vs-Python check is P4's |
| M9 | A CPU development manifest (`manifest.cpu.toml`) pointing at the fp32 files, so the perception code runs on a laptop without a GPU | Sam | M2 | Sara runs `ORT_EP=cpu` locally | Done (PR #7, a3fa16a): every fp32 graph runs on the CPU provider |
| M21 | Our model files in the private Hugging Face repository `GreenLens/pdm-models`, as vision 6 designs: `sources.toml` pinned by commit, `fetch.py` with `--for` groups (and `--cpu` for laptops), `check.py`, `publish.py`, `NOTICES.md` and `licences/`; the S1 entries (`text_embedder`, `text_embedder_tokenizer`, `yunet`) in the manifest; the interim GitHub release deleted | Sam | D9 | `fetch.py --for perception` verifies every file into an empty folder; `check.py` passes; the release is gone before GreenLens goes public | Done (b4ebadf): our files are in `GreenLens/pdm-models`, pinned by commit in `ml/models/sources.toml`; the interim release is deleted; `fetch.py --for api` ✓ ran on a fresh laptop in [#40](https://github.com/aacruzgon/GreenLens/pull/40) |
| M20 | The fake VLM server: OpenAI-compatible, replays real Qwen3-VL-8B FP8 replies to the worker's own calls, recorded through it in record mode, and conforms a recording to any other schema (option 1 in section 5) | Sam | — | Sara's VLM harness (P3) runs against it offline | Done ([#16](https://github.com/aacruzgon/GreenLens/pull/16)) ✓ ran: the worker's real calls replay 7 of 7 and validate; a changed payload gets a conformed answer that validates; 23 tests |
| P0 | Set up the laptop: WSL with Ubuntu 26.04, the backend tools, the `GreenLens` invitation and a read token, `fetch --for perception --cpu`, llama-server with Qwen3-VL-2B (section 5) | Sara | M21, M9 | `cargo nextest run` passes in `backend/`, and `curl` on `127.0.0.1:8080/v1/models` lists `vlm` | Next |
| P1 | Fix gaps G4, G6, G7 and G11 in `vision` and `worker` (section 4; [issue #12](https://github.com/aacruzgon/GreenLens/issues/12)). G1–G3, G5 and G9 were fixed in #11 | Sara | P0; M6 for the tests | G4, G6, G7 and G11 fixed with tests; `PerceptionPipeline::load` succeeds on the real `manifest.toml` | In progress: the fixes and their tests are done; Sara has access to `pdm-models` (as `Sara-LHP`), and the load on the real files is still to run |
| J1 | Pre-processing parity (G8): Rust writes the tensors for 10 photos × 3 recipes and 5 leaf crops; Python compares | Sara (Rust dump), Sam (comparison) | P1 | Max absolute difference per tensor recorded; under 0.02 after normalisation, or the gap explained and fixed. `crop_box`'s rounding confirmed | Not started |
| P2 | A harness that runs one perception job without the API (a job payload and its images in, the result JSON out), first on the laptop with `manifest.cpu.toml`, then on a GPU by Sam with `manifest.toml` | Sara (harness, CPU run), Sam (GPU run) | P1 | On the CPU: leaf regions with masks, species top 3, disease top 3 per crop and embeddings for 6 shots. On the GPU: the same, with time per job and peak VRAM | Not started |
| P3 | The same harness for a `vlm` job: the worker's real prompts and schemas through `VlmAdapter`, against the fake server (seed: the worker test `record_against_live_server`, which records the real calls) and against Qwen3-VL-2B on the laptop; then Sam runs it against Qwen3-VL-8B | Sara (harness, local runs), Sam (real run) | P0; M20 for the fake server | Findings and indicators decode and validate on all three servers; compared with `vlm_check`'s day-1 report | In progress: the harness is `bench.rs` and `record.rs` ([#86](https://github.com/aacruzgon/GreenLens/pull/86)); left: Sara's runs against the fake server and Qwen3-VL-2B |
| P4 | Decide where the text embedder and YuNet run in `api` and who wires them | Sara and the `api` owner | — | Owner named here | To assign |

### Phase 2, Oct 18–19: real models, the day-2 benchmark

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M10 | Evaluation photos: the 20 dev cases with full shot sets the day-2 benchmark needs ([evaluation 2](evaluation.md#2-the-evaluation-set)) | Sam, with the team taking photos | Sam's review of the fetched photos; phones for what no dataset has | 20 cases in `ml/training/eval/manifest.jsonl` (photos stay out of git) | In progress: 187 cases in `ml/training/eval/manifest.jsonl` (115 dev, 72 test) at first: 103 from four published tomato and corn field datasets ([#76](https://github.com/aacruzgon/GreenLens/pull/76)) and 84 iNaturalist observations ([#70](https://github.com/aacruzgon/GreenLens/pull/70): apple, grape, squash and tomato), reviewed by Sam on Oct 19 and 20, less two spider-mite cases dropped as microscope or extreme-macro shots with no plant to see. Alan's second review of all 187 on Oct 22 ([#52](https://github.com/aacruzgon/GreenLens/issues/52#issuecomment-6346712013)), once settled and reviewed again on [#135](https://github.com/aacruzgon/GreenLens/pull/135), dropped 68: 40 iNaturalist cases on another host, 8 with no readable plant, 17 repeats of one leaf and 3 whose photos can't settle his call ([evaluation 3](evaluation.md#second-review-2026-10-22)). 119 cases remain (76 dev, 43 test), all with `reviewed_by`, and 21 are `sure` (7 in test). No case has a full shot set or a ladybird (none of the 21 iNaturalist ladybird larvae named its plant). What only phones can give (full shot sets, a pepper plant, ladybugs, look-alikes, no-plant shots, faces) is listed on [#52](https://github.com/aacruzgon/GreenLens/issues/52) with a private upload folder, for Oct 21. The leakage check passes: no case is a copy of any of the 59,821 training images on the rig ([report](../../ml/training/eval/reports/leakage-2026-10-20.md)) |
| J2 | **Day-2 VLM benchmark**: both candidates, `VLM_SLOTS` 1 and 2, `VLM_MAX_PIXELS` at 4 sizes, on the 20 dev cases, through the worker's real prompts and schemas ([evaluation 8](evaluation.md#8-day-2-benchmarks)). Use the evaluation set's phone photos: the day-1 images were PlantDoc web photos (a wallpaper, an isolated stock leaf), fine for a day-1 check but not for timing or box conclusions | Sam (runs and scores), Sara (worker build from P3) | M10, P3, M3 | A benchmark report; the numbers for decision D1 | Done ([#86](https://github.com/aacruzgon/GreenLens/pull/86), ✓ ran 2026-10-20): the 65 one-photo tomato and corn dev cases through the worker's own `analyse` on three candidates, 0.6 and 1.0 MP, 1 and 2 at once ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)). Every call valid; two jobs at once fit (p95 under 10 s). Diagnosis from the indicator answers on 26 sure cases: Qwen3.5-9B AWQ 35%, Qwen3-VL-8B FP8 and AWQ 12%; the questions carry little signal on these photos. Organisms, soil, beneficial-as-pest and the 9070 XT wait on full cases (M10) |
| P5 | Prompt fixes from day 1 ([vlm.md 4](vlm.md#4-serving)): species answered `other` although it was in the enum; a fungal spot called `leaf_mines` (seen again on the worker's own `findings@2` prompt: Qwen3.5 calls the tomato's early-blight spots `leaf_mines` three times, [vlm.md 4](vlm.md#the-workers-own-prompts-on-both-candidates-2026-10-16--ran)); a switch to Chinese mid-word under a `maxLength` cap (grammar truncation, not a language fault: also ask for English, and show the shape of every nullable object in the prompt, or the model falls back to `null` (check f, [vlm.md 4](vlm.md#4-serving)); keep each string's expected length well under its `maxLength` so the cap is a guard the model rarely hits); and, confirmed on the worker's own findings prompt with both candidates, **no soil readings at all** from a soil shot until the prompt shows a reading's shape ([vlm.md 4](vlm.md#the-workers-own-prompts-on-both-candidates-2026-10-16--ran)) | Sara (prompt code), Sam (reruns the check) | P3 | On P5's photo set, the day-1 photos with the tomato case's soil photo swapped for a tomato-field one (Oct 21): species answered by id and no switch of language at a cap; on Qwen3.5, soil readings kept and no disease called `leaf_mines` on the corn; prompt versions bumped. The tomato's early-blight spots as `leaf_mines` moved to P11 (Oct 22) | Done ([#45](https://github.com/aacruzgon/GreenLens/pull/45), merged Oct 22, 8f8337c; the soil part as `findings@2` in [#30](https://github.com/aacruzgon/GreenLens/pull/30)). `findings@4` (da5be61, Oct 21) failed on Qwen3.5. Run one change at a time, every species line asking for the id silenced the soil, and the damage section moved the corn's blight into `leaf_mines` ([runs](../../ml/training/eval/reports/p5-qwen35-2026-10-21.md)). `findings@5` is `main`'s findings prompt with the organisms line and the `notes` budget, under `system@2` and `indicators@2`. Decode maps an `other` that names a listed species or organism to its id, the day-1 apple case, which these photos can't exercise. Re-recorded on Oct 21 at 22:25 (b2f53d1): on Qwen3.5, species by id, 8 soil readings, no `leaf_mines` on the corn, English, no reason at its cap. The tomato's 3 `leaf_mines` are P11's. FP8 reads soil and has no `leaf_mines`, but it still calls the corn's blight `chewed_margins` and `shot_holes`, boxed as the whole image, and gives the tomato's aphid and `shot_holes` one box, so it isn't a clean fallback. The tomato's soil photo shows a plant wilting from bacterial wilt; a plain soil shot from [#52](https://github.com/aacruzgon/GreenLens/issues/52) replaces it at P11's first re-record |
| J3 | Perception batch caps on the RTX 3070 with every session loaded ([evaluation 8](evaluation.md#perception-batch-size)) | Sam (runs), Sara (worker), the 3070's owner (host) | P2, the 3070 host | Caps chosen with 1 GB free at peak; written to the worker's environment | Measured with `perception_gpu` ([#114](https://github.com/aacruzgon/GreenLens/pull/114)): the manifest's caps peak at 4,598 MiB, 3.2 GB free, so they stay. Left: the worker's run (P2) and its environment |
| M11 | Leaf relabelling pass (people, Oct 18), then the second detector run the night of Oct 18 | Sam and the team | GPU night slot | New detector exported and in the manifest | Done ([#66](https://github.com/aacruzgon/GreenLens/pull/66)), without the relabelling pass: `rfdetr-s-leaf-2026-10-19-bglic` in both manifests with `τ_leaf` 0.40 and `τ_plant` 0.70, trained with 400 no-plant background photos whose licence allows commercial use ([models 2](models.md#2-plant-present-check)). A later run trains on the corrected boxes |
| M12 | The real BioCLIP label matrix from the species and organism seed files | Sam | The knowledge seed's `species/` and `organisms/` files; issue #4 | Manifest entry `bioclip_labels_<label_set_version>`; zero-shot check on team photos | Waiting |
| M13 | Heads on our detector's crops (Oct 19): disease head v2, the species head, the plant probe (the day-1 zero-shot probe is built; the trained one uses the COCO val2017 negatives, already fetched, and its plant photos must add wilted plants, frame-filling or blurred leaf close-ups and succulents, where the detector keeps no leaf: 2.9% of val photos, [models 2](models.md#2-plant-present-check)) | Sam | M11, M12 | Manifest entries; the species head kept only if it improves the dev split | In progress: [#85](https://github.com/aacruzgon/GreenLens/pull/85) merged (✓ ran 2026-10-20), on `rfdetr-s-leaf-2026-10-19-bglic`'s crops cut as the worker cuts them, C chosen per crop ([training 6](training.md#6-disease-and-species-heads)): disease head v2 (21 classes) at 0.842 top-1 per crop on test, v2c (19 classes, without mosaic virus and curvularia, which have no profile) at 0.881; the species head at 0.896 against zero-shot's 0.642 on the dev cases' supported crops, though models 6's combination rule keeps little of it (0.664); the trained plant probe passes 0.1% of the hold-out's no-plant photos against the shipped pair's 11.0%. Disease head v3c, retrained with three garden-like field sets and C chosen by its top 3 on the dev cases, is in the manifest and published (pdm-models 8473aad; [training 6](training.md#6-disease-and-species-heads)): the true problem in its top 3 on 84% of the dev photos with a problem against v2c's 74%, while Tomato-Pakistan's test falls from 0.762 to 0.688, accepted, and corn's dev top-1 from 11 to 7 of 15. The class list is settled at 19 (Sara on #85, Oct 21), as v3c has. Still to do: v3c's `classifier_labels.yaml`, with tomato's 8 classes now that [#51](https://github.com/aacruzgon/GreenLens/pull/51) is on `main` (44cf80f), and the 7 apple, corn, grape and pepper problem classes after [#125](https://github.com/aacruzgon/GreenLens/issues/125), and the species head and plant probe entries |
| P6 | The worker services in `infra/rig.compose.yml` as [infra 4](../infra/README.md#infrarigcomposeyml) designs them (Sara writes them, Sam runs them): `model-fetch`, `perception-worker` and `vlm-worker-nv`, with the running `vllm` service renamed `vllm-nv`; tokens in env files, GPUs by UUID, `ORT_DYLIB_PATH`, `/models` mounted read-only, `VLM_BASE_URL=http://vllm-nv:8000` | Sara (Sam reviews; it's his machine) | P2, P3 | `docker compose -f infra/rig.compose.yml up` starts all three and the workers' heartbeats reach the API | Not started |
| P7 | Second VLM worker on the RX 9070 XT (AMD day-1 check, then the same checkpoint and flags) | The 9070 XT's owner | The card's host | Findings and indicators calls pass with our schema | To assign |

### Phase 3, Oct 20–21: end-to-end scans

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M14 | Choose the VLM and set `VLM_SLOTS` per card and `VLM_MAX_PIXELS` (decision D1) | Sam | J2 | Values in `infra/.env.example` and the worker's env; vlm.md updated | Done for the 5070 Ti ([#86](https://github.com/aacruzgon/GreenLens/pull/86)): Qwen3.5-9B AWQ with thinking off, `VLM_MAX_NUM_SEQS` 2 and `VLM_MAX_PIXELS` 1,003,520 are the defaults of `infra/.env.example` and `infra/rig.compose.yml`; [vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran). The vlm-worker's `VLM_SLOTS` is 2 to match, set where its compose service is added. The 9070 XT's value waits on its own run |
| P8 | `pipeline` retrieve and score on real perception and VLM results: candidates from the disease head, indicators from the VLM, the `verify` job for an organism outside the set | Sara | P2, P3, the knowledge seed | A real scan reaches a diagnosis or a question with the right candidates | Not started |
| **J4** | **First full scan end to end on the rig**: upload → perception → retrieve → vlm → score → plan, with `model_versions` traced on every result | Sara (pipeline), Sam (models), the `api` owner | P6, P8 | A scan from the app (or a script) ends in a plan; its event log saved as a fixture | Not started |
| J5 | The evaluation harness runs dev cases through the API and scores them per stage ([evaluation 9](evaluation.md#9-harness-and-reports)) | Sam (harness), Sara (a worker token and a test garden) | J4 | One command scores all dev cases | Not started |
| P9 | Fallback worker: one live box call settles its box format; whether 8,000-token calls fit its doubled `max_runtime` (contracts open question 26); cost per scan | Sara (runs), Sam (scores) | An LLM API key | Numbers recorded in vlm.md 12 | Not started |
| M15 | Heads retrained as problem profiles are reviewed and the label list grows; spot colour thresholds from hand-drawn lesions | Sam | M13, reviewed profiles | New manifest entries | Not started |

### Phase 4, Oct 22–23: calibration, then freeze the models

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M16 | Calibration on the dev split: `T_species`, `T_disease`, `τ_det`, `τ_leaf`, `τ_plant`, `τ_mask`, the VLM certainty values, BioCLIP agreement, the diagnosis margin, indicator weights (contracts open questions 2–4; [evaluation 5–6](evaluation.md#5-calibration)) | Sam | J5 | Values in the manifest and wherever `api` reads them | In progress: the offline part ran on the dev cases ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)). `species_temperature` 1.88 is proposed on the stand-in matrix, which makes the agreement rule hold, out of fold too. For the disease head, evaluation 5 gives the temperature's trade. Fitted on dev (3.66 for v2c, 2.02 for v2), 13 of the head's 18 medium-confidence claims are right (v2: 18 of 24), but grape is claimed and right at every value; without it, 5 of 10 (v2: 10 of 16). The fit stops claiming on tomato, where 24 of its 29 claims at the file's value were wrong, and drops 7 of corn's 9 right claims. A temperature per species keeps corn's claims but needs the head format changed. No healthy tomato leaf gives `healthy` at medium at any value. M17 chooses. Left: the VLM words, the margin and the weights, with J5; the values go into the manifest with M17. `τ_leaf` 0.40 and `τ_plant` 0.70 are provisional, chosen on the val split with at most 1% false `no_plant_found` on every dataset and checked on hold-out photos ([models 2](models.md#2-plant-present-check)) |
| P10 | The calibrated values reach the code: the workers read the manifest (P1's G6 fix); `api` reads the diagnosis margin, certainty values and indicator weights from their configuration | Sara | M16 | A changed value in the manifest changes a scan's result, and `perception_config` changes with it | Not started |
| P11 | Disease reported as animal damage by Qwen3.5: the tomato's early-blight spots as `leaf_mines` with every findings wording tried, and disease lesions as `other` damage ([vlm.md 4](vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran)). FP8, the fallback, does it too: it calls the corn's blight `chewed_margins` in every run ([runs](../../ml/training/eval/reports/p5-qwen35-2026-10-21.md#the-findings5-runs)). Wording doesn't move it, so look for a structural fix, such as each damage finding naming the animal or trace that made it, with decode dropping one that names none: a contract change (contracts 6) | Sara (prompt and contract), Sam (reruns) | P5 | On the day-1 photos and the dev cases, Qwen3.5 reports no damage finding for disease signs, and P5's checks still hold | Not started |
| M17 | Final manifests: every hash, licence and notice ([vision README 9](README.md#9-licences-and-notices)) | Sam | M16 | Committed; the rig runs exactly these files | Not started |
| J6 | Hand-over test: kill the rig in the middle of a scan; the fallback worker takes over within about 45 seconds | Sara, with the backend team | P9, J4 | Passes twice | Not started |

### Phase 5, Oct 24–25: score once, ship

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M18 | Score the test split once; the numbers for Devpost | Sam | M17 | `check_leakage` passes on the final manifest first ([evaluation 2](evaluation.md#where-the-photos-come-from)); a report committed; no model changes after it | Not started |
| J7 | Feature freeze: the demo scan rehearsed on the rig, with the fallback tested | Team | J6 | The demo scan works three times in a row | Not started |
| M19 | Model notices and the ML part of the Devpost write-up | Sam | M18 | Committed | Not started |

---

## 7. Milestones

| Milestone | Date | Proves |
|---|---|---|
| **MS1: Perception on a GPU** | Oct 17, evening | P1 and P2: the real model files load in Rust and one perception job finishes |
| **MS2: VLM through the worker** | Oct 18 | P3 and J2: the worker's prompts and schemas run against vLLM, and the day-2 benchmark has numbers |
| **MS3: First full scan** | Oct 21 | J4: a photo becomes a plan, through every model |
| **MS4: Calibrated and safe** | Oct 23 | M16, P10 and J6: calibrated values are live and the fallback takes over |
| **MS5: Frozen** | Oct 24 | M18: the test split is scored once |

If a milestone slips, the tasks that need it slip with it; say so in the team chat the same day.

---

## 8. GPU time on the rig

Only Sam uses the rig (`sam-linux`, one RTX 5070 Ti, 16 GB, which also draws Sam's desktop). Everything else runs on laptops (section 5). Three things want the card:

| Use | VRAM | When |
|---|---|---|
| vLLM (Qwen3-VL-8B) | 0.78 of the card with the desktop on (AWQ build), 0.90 headless (FP8, 2 slots) | Most of the day: the real VLM runs of P3, P5, J2 and J4 |
| Detector training | Most of the card for about 30 minutes per run | M11, the night of Oct 18 |
| The perception worker's GPU runs, until the RTX 3070's host is up | About 5.3 GB at the planned batch caps | P2's GPU run, J1, J3 |

The perception worker and vLLM don't fit together on this card (vLLM leaves about 1.5 GB free at 0.90), so Sam stops vLLM for a perception run (`docker stop infra-vllm-1`, then `docker start infra-vllm-1`). To ask for a real run, push the branch and say in the pull request what to run; Sam posts the output there. A perception run that only checks the GPU path, not speed or memory, can run on Sara's RTX 3060 instead ([section 5](#the-laptops-rtx-3060)). Once the RTX 3070 and RX 9070 XT hosts are up, perception moves to the 3070 and this conflict ends ([vision README 4](README.md#4-placement-and-vram-budgets)).

---

## 9. Decisions and blockers

| # | Decision or blocker | Blocks | Who decides | State |
|---|---|---|---|---|
| D1 | Which VLM: Qwen3-VL-8B FP8 or Qwen3.5-9B AWQ; `VLM_SLOTS`, `VLM_MAX_PIXELS`; where it runs for the demo, since the 5070 Ti also draws a desktop ([#13](https://github.com/aacruzgon/GreenLens/issues/13)). On the worker's own prompts both validate 7 of 7; on the tomato FP8 finds aphids and shot holes, while Qwen3.5 found no damage with `findings@1` and calls three spots `leaf_mines` with `findings@2` ([vlm.md 4](vlm.md#the-workers-own-prompts-on-both-candidates-2026-10-16--ran)) | M14, the 9070 XT's checkpoint | Sam, from J2 | **Decided on Oct 20 (Sam): Qwen3.5-9B AWQ**, Qwen3-VL-8B the fallback; `VLM_SLOTS` 2, `VLM_MAX_PIXELS` 1,003,520 ([vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran)). Reopened only if it fails the beneficial-as-pest rule on the team's full cases. It also runs at 0.78 of the card with 2 slots, the desktop's budget, as fast as at 0.90 (evidence for [#13](https://github.com/aacruzgon/GreenLens/issues/13), which stays the team's). Measured on 65 one-photo cases with `main`'s prompts, `findings@2` and `indicators@1` ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)): Qwen3.5-9B AWQ leads on indicator answers (35% of sure cases diagnosed against 12%, 25% `unsure` against 32%) and is as fast (p50 5.3 s); FP8 reports chewed margins and leaf mines these leaves don't show, Qwen3.5 puts its false damage under `other` and reports no organisms. `VLM_SLOTS` 2 fits on the 5070 Ti at 1.0 MP; size made no difference from 0.6 to 2.0 MP. Not yet measured: beneficial-as-pest and organism recall. After [#45](https://github.com/aacruzgon/GreenLens/pull/45)'s next prompt pass, one bench run of the 65 cases on its prompts re-checks the choice |
| D2 | [#3](https://github.com/aacruzgon/GreenLens/issues/3): the head and label-matrix JSON and the `label_set_version` recipe. Settled by [PR #11](https://github.com/aacruzgon/GreenLens/pull/11): the Rust loaders read FORMATS.md, and the test vector `129f0395e8d89bd5` passes. The issue stays open only for the two owners' confirmations | P1 | Sam and Sara | Settled by #11; Sara's loader ([#48](https://github.com/aacruzgon/GreenLens/pull/48)) computes the same test vector. Close [#3](https://github.com/aacruzgon/GreenLens/issues/3) |
| D3 | [#4](https://github.com/aacruzgon/GreenLens/issues/4): supported crops and slugs; whether bean stays | M12, M13, P8 | Sara (the knowledge-base owner) | **Settled 2026-10-22** ([#4](https://github.com/aacruzgon/GreenLens/issues/4#issuecomment-6344093645), after the [Oct 17 decision](https://github.com/aacruzgon/GreenLens/issues/4#issuecomment-6244498119)): the head covers tomato, apple, grape, pepper and corn, with v3c's 19 classes. Potato, strawberry, squash and bean are `label_only` for build week, their full profiles after; potato joins the head with a healthy-potato source. Blueberry, cherry, peach, raspberry, soybean, basil, lettuce, rose, hosta, cucumber and onion are `label_only`. The seed holds the 20 species, so M12 builds from them and the organisms. Slugs `pepper`, `corn_northern_leaf_blight`, `apple_cedar_apple_rust`, `tomato_spider_mites`; `tomato_bacterial_wilt` dropped; the training mappings are signed. #4 closes when training writes `classifier_labels.yaml` |
| D4 | [#5](https://github.com/aacruzgon/GreenLens/issues/5): the duplicate threshold and the conflicting PlantDoc groups | Heads retraining | Whoever checks the sampled pairs | Open |
| D5 | Who owns the knowledge ingestion ([#15](https://github.com/aacruzgon/GreenLens/issues/15); `ml/knowledge`, its model steps in [training 10](training.md#10-knowledge-ingestion-the-model-steps)) | M12, P8 | Team | Settled 2026-10-17: Sara; the curation work is [#46](https://github.com/aacruzgon/GreenLens/issues/46). Its tasks, in order: (1) the loader and its checks, merged in [#48](https://github.com/aacruzgon/GreenLens/pull/48); (2) the tomato slice in [#51](https://github.com/aacruzgon/GreenLens/pull/51), redrafted after its first review, fixed after two signing reviews and signed by Sam (tomato mosaic virus has no source, so head v3c drops it, [#120](https://github.com/aacruzgon/GreenLens/pull/120)); (3) the other supported crops: apple, grape, pepper and corn are in #51 as `label_only` species, their problems for the head's classes follow in [#125](https://github.com/aacruzgon/GreenLens/issues/125), then potato, strawberry and squash; (4) Ontario's invasive species with their reporting pages; (5) soil conditions; (6) reference photos, up to 3 per species; (7) the `seed-upload` container, done (deployed 2026-10-21, [backend roadmap S2](../backend/roadmap.md#6-s2-perception-site-data-and-knowledge)). Each crop is drafted with [training 10](training.md#running-it)'s scripts and loads only once reviewed |
| D6 | The 9070 XT's server: vLLM ROCm or llama.cpp Vulkan | P7 | The card's owner, from its day-1 check | Open |
| D7 | Where the text embedder and YuNet are wired in `api`, and by whom ([#15](https://github.com/aacruzgon/GreenLens/issues/15)) | M7, M8 | Sara and the `api` owner (P4) | Open |
| D8 | Disease head or ConvNeXt-Tiny, if the head stays under 75% top-1 on detector crops | M13 | Sam | Decided 2026-10-20: the head stays. On our detector's crops it reaches 0.842 top-1 per crop on test with 21 classes and 0.881 with 19 ([training 6](training.md#6-disease-and-species-heads)). Reviewed on Oct 20 against the dev cases ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)): judged by its job, the true problem among its top 3 on the photos with a problem, it reaches 73% against 52–57% for guessing (corn and tomato 66% against 38–45%), while its top-1 is 36–40%. On the 12 iNaturalist garden tomato leaves, the photos most like the app's, it does no better than guessing (3–4 of 12, against 33–38%). It stays (Sam); its temperature is M16's trade, chosen at M17. Retrained with garden-like photos as v3c and shipped: 84% at top 3 on the dev photos with a problem, the iNaturalist garden tomato leaves 8 of 12 ([training 6](training.md#6-disease-and-species-heads)) |
| D9 | [#15](https://github.com/aacruzgon/GreenLens/issues/15): where `pdm-models` lives and who holds the read tokens. **Settled:** `pdm-models` lives in the Hugging Face organisation `GreenLens`, which Sam created (`GreenLens/pdm-models`). Teammates join the organisation and use their own fine-grained read-only token scoped to `pdm-models`; each host (control plane, rig) gets its own read token; tokens are never committed ([vision 11](README.md#11-open-decisions), items 11 and 14) | M21, vision 6 M3 | Sam | Settled (organisation name to fill in) |
| D10 | [#9](https://github.com/aacruzgon/GreenLens/issues/9): send the indicator call's images in the findings order, so vLLM's prefix cache is reused ([vlm.md 14](vlm.md#14-open-questions) question 2) | The backend change after J2 | Sam and Sara, from the day-2 benchmark | Open |
| D11 | [#13](https://github.com/aacruzgon/GreenLens/issues/13): how the VLM runs for the demo (headless FP8, AWQ as the primary, the display moved off the card, or a dedicated host) | J2, M14, D1 | The team, with Sam (owns the card) and Sara | Open, overdue (it was due before Oct 18) |
| D12 | [#14](https://github.com/aacruzgon/GreenLens/issues/14): licences of our trained weights (PlantDoc, Tomato-Pakistan, restricted data, the project licence) | M21, going public | The team | Open |
