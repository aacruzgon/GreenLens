# PDM ML Dev Roadmap

> **Status:** v0.2, 2026-10-16 (night). v0.2: the integration track runs on Sara's Windows laptop (WSL): model files from a GitHub release with `ml/models/fetch.py`, a CPU manifest, reference fixtures, and two ways to test the VLM worker locally (a fake server, or Qwen3-VL-2B on llama.cpp, which passed the day-1 checks on a CPU); real jobs run on Sam's machine, run by Sam. v0.1, 2026-10-16 (evening): First plan for the whole ML side, models and their integration into the backend, by phase from Oct 16 to Oct 25, with owners, dependencies and done conditions, the hand-offs between `ml/` and the Rust crates, the integration gaps found on `main`, and a getting-started guide for the integration work.
> **How to use it:** find your next task in section 6, check its "Needs" column, and when it is done, change its state and add the commit, PR or report that proves it, in the same pull request as the work. Bump this status line when you do. The model-by-model detail (datasets, exports, metrics) is in [roadmap.md](roadmap.md); this file is the plan the two tracks share.

This covers everything between a photo and a diagnosis that involves a model: training and exporting the models (`ml/`), the files and formats that carry them, the Rust code that runs them (the [`vision`](../backend/vision.md) crate, the [`worker`](../backend/worker.md) binary) and the steps of the [`pipeline`](../backend/pipeline.md) that consume their outputs. The design lives in [docs/vision](README.md) and [docs/backend](../backend/README.md); this file says who builds what, in what order, and how to tell it is done.

States: **Done**, **In progress**, **Next** (start now), **Not started**, **Waiting** (on a person, a decision or another task), **To assign**.

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
| Not ML, but on our path | Other teammates | The knowledge seed (species, problems, organisms; issue #4), the `api`, the app, LiDAR measurement, the RTX 3070 and RX 9070 XT hosts | `ml/knowledge`, `backend/bins/api`, `app/` |

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
| The model manifest: entry names, files, SHA-256, input recipes, output names, `[model.config]` | Sam → `vision::manifest` | [backend vision 5](../backend/vision.md#5-the-model-manifest), [vision README 6](README.md#6-model-files-versions-and-pinning), [`ml/models/manifest.toml`](../../ml/models/manifest.toml) | Entries for `bioclip`, `sam_encoder`, `sam_decoder`, `leaf_detector`, a stand-in label matrix and `disease_head_v1d`. Thresholds aren't read yet (gap G6) |
| ONNX files: tensor names, shapes, dtypes | Sam → perception-worker | [models 3–6](models.md#3-leaf-detector), the manifest | Exported in fp16 with fp32 I/O and checked on the 5070 Ti. BioCLIP's names differ (gap G4) |
| Head and label-matrix JSON | Sam → `vision::perception::heads` | [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md), [issue #3](https://github.com/aacruzgon/GreenLens/issues/3) | **Doesn't match the Rust loader** (gaps G1–G3) |
| Pre-processing (resize, crop, normalise, crop boxes) | Both | [backend vision 6](../backend/vision.md#6-perception) `to_tensor`, [`ml/pdm_vision/preprocess.py`](../../ml/pdm_vision/preprocess.py) | Written to match; never compared on real images (gap G8) |
| Model files on a machine | Sam → every worker host and laptop | [`ml/models/fetch.py`](../../ml/models/fetch.py) downloads a manifest's files from the [`models-2026-10-16`](https://github.com/aacruzgon/GreenLens/releases/tag/models-2026-10-16) release, checks each SHA-256 and puts the manifest beside them; `MODEL_MANIFEST` points at that copy, since the worker resolves paths against the manifest's folder. [`manifest.cpu.toml`](../../ml/models/manifest.cpu.toml) names the fp32 files for `ORT_EP=cpu` | Done (tasks M5, M9) |
| The VLM server: URL, key, served name, checkpoint, flags | Sam → vlm-worker | [vlm.md 4](vlm.md#4-serving), [`infra/README.md`](../../infra/README.md) | Live on the 5070 Ti: Qwen3-VL-8B FP8, served as `vlm`, `VLM_MAX_PIXELS` 1003520, 2 slots headless; used for real jobs, run by Sam. For testing: a fake server (M20) or Qwen3-VL-2B on llama.cpp on the laptop (section 5) |
| VLM requests: image preparation, schemas, prompts, decoding | Sara (worker code) | [contracts 6](../architecture/contracts.md#6-vlm-structured-output), [vlm.md 5–11](vlm.md#5-image-preparation-and-tokens) | Worker code on `main`; a Python reference with passing day-1 results in [`ml/training/eval/vlm_check.py`](../../ml/training/eval/vlm_check.py) |
| Thresholds, temperatures, certainty values | Sam (values) → workers and `api` (reading them) | [evaluation 5–6](evaluation.md#5-calibration) | Values are the docs' starting points until Oct 22–23 |
| Text embedder file and reference vectors | Sam → `vision::embed` in `api` | [models 10](models.md#10-text-embedder) | Not pinned yet (task M8) |
| YuNet face detector | Sam → `vision::upload::faces` in `api` | [models 9](models.md#9-face-detector) | Not pinned yet (task M7) |
| The evaluation harness's access to the API | Sara and backend → Sam | [evaluation 9](evaluation.md#9-harness-and-reports) | Not started (task J4) |

---

## 4. Integration gaps found on main

Found by reading `backend/` on `main` against what `ml/` writes, on 2026-10-16. Each one breaks the perception worker at start-up or, worse, gives wrong results silently. They are the first integration tasks (P1 in section 6). The proposed fix keeps FORMATS.md and the manifest as the spec and changes the Rust side, since the manifest already carries the names and values; settle the formats in issue #3 before coding if anyone prefers the other direction.

| # | Where | What `ml/` writes | What Rust reads | Effect | Proposed fix |
|---|---|---|---|---|---|
| **G1** | `vision/src/perception/heads.rs`, `HeadFile` | `labels[i].problem_id` (a slug or `"healthy"`), `version` | `labels[i].problem` (optional, defaults to `Healthy`), `classifier_labels_version` (optional, defaults to `""`) | **Silent:** every disease class loads as `healthy`. The head is keyed by `""`, so every job fails with `unknown_label_version` | Read `problem_id` and `version`; refuse a file whose `format` isn't `pdm-linear-head/1`; no defaults |
| **G2** | `heads.rs`, `LabelFile` | `labels: [{id, table, label}]` with species and organism rows, `count`, `dim` | `species: [{id, label}]`, species rows only | The label matrix fails to load | Read `labels`; keep the rows whose `table` is `species` (organism rows stay for later) |
| G3 | `heads.rs`, `ProbeFile` | FORMATS.md: `bias` is base64 `f32` like every head | `bias` is a JSON number | The plant probe would fail to load once it exists | Read FORMATS.md's shape (one class, base64 bias), or change FORMATS.md in #3 |
| **G4** | `vision/src/perception.rs`, the BioCLIP call | Input `images`, output `embeddings` (also in the manifest) | Hard-coded `image` and `embedding` | BioCLIP fails on the first job | Take every model's tensor names from the manifest's `[model.input] name` and `[model.outputs]` instead of string literals. The detector and SAM names match today, but only by agreement |
| G5 | `perception.rs`, `bioclip_version` | Manifest `version = "bioclip-2@2957b322"` | `format!("bioclip-2@{}", version)` | Every image embedding's `model` reads `bioclip-2@bioclip-2@2957b322` | Use the manifest version as it is |
| **G6** | `worker/src/main.rs`, `PerceptionConfig` | `[model.config]`: `score_threshold`, `min_side_px`, `max_per_image`, `nms_iou`, `min_iou`, `plant_min_leaves`, `plant_grow`, `plant_min_cover`, `species_temperature`, `crop_grow` | `PerceptionConfig::default()` plus the batch caps from the environment | **Silent:** the manifest's thresholds are never used, yet `perception_config` hashes them as if they were. Calibration on Oct 22–23 would change nothing | `PerceptionConfig::from_manifest`, failing on a missing key; the environment keeps only the batch caps |
| G7 | `worker/src/config.rs` | The compose service is `vllm` | `VLM_BASE_URL` defaults to `http://vlm:8000` | The VLM worker can't reach the server in the compose network | Set `VLM_BASE_URL=http://vllm:8000` in the worker's env file (or rename the service) |
| G8 | `vision/src/perception/preprocess.rs` vs `ml/pdm_vision/preprocess.py` | Pillow Lanczos3, `rust_round`, `crop_box` rounding *guessed* | `fast_image_resize` Lanczos3, Rust rounding | Unknown: the heads were trained on Python's pixels | The parity test (task J1) |

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
2. **In WSL**, follow [setup 2](../setup/README.md#2-everyone) (git, SSH signing) and [setup 4](../setup/README.md#4-backend-rust): Rust 1.99.1, cargo-nextest, the build tools, libheif, and ONNX Runtime 1.31.0's `onnxruntime-linux-x64-1.31.0.tgz`, with `ORT_DYLIB_PATH` pointing at its `lib/libonnxruntime.so`. Add the GitHub CLI (`sudo apt install gh`, then `gh auth login`); Python 3.11 or newer is already there.
3. **Model files.** From the repository root:

   ```bash
   python3 -m ml.models.fetch --cpu
   ```

   It downloads the fp32 files from the `models-2026-10-16` release (about 1.5 GB), checks each SHA-256, and puts `manifest.cpu.toml` beside them. The worker resolves file paths against the manifest's own folder, so point it at that copy:

   ```bash
   export MODEL_MANIFEST=$PWD/ml/models/files/manifest.cpu.toml
   export ORT_EP=cpu
   export ORT_DYLIB_PATH=$HOME/onnxruntime-linux-x64-1.31.0/lib/libonnxruntime.so
   ```

   `manifest.cpu.toml` is `manifest.toml` with each fp16 ONNX file swapped for its fp32 export, because ONNX Runtime's CPU provider runs fp32 graphs. Every fp32 graph runs on the CPU (BioCLIP: about 0.3 s per image), and its embeddings match the rig's fp16 ones to cosine 0.9997. Use it for development, not for measuring accuracy. Without `--cpu`, `fetch` gets the fp16 set the GPU workers load.
4. **Reference outputs for your tests:** [`ml/tests/fixtures/perception_reference.json`](../../ml/tests/fixtures/perception_reference.json) holds six photos' BioCLIP embeddings with the species top 3 and the disease head's top 3 that Python computes. A Rust test that loads the same head and label-matrix files and feeds it these embeddings should get the same ids in the same order, with probabilities within 1e-4. `python -m ml.tests.make_fixtures` regenerates it.

### Testing the VLM worker

Two ways, both on your laptop. Neither needs Sam's machine.

**Option 1: the fake VLM server.** A small OpenAI-compatible server that answers with the real Qwen3-VL-8B responses recorded in the day-1 check (`ml/training/eval/reports/`). It tests request building, schemas, decoding, validation and box conversion against real model output, offline and instantly, but always gives the same answers, so it can't test a prompt change. Sam adds it (task M20).

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
2. Fix G1, G2, G4 and G5 (they are small) and add a test that loads `disease-head-v1d-2026-10-16.json` and the stand-in label matrix and checks them against `perception_reference.json`.
3. Fix G6 so the thresholds come from the manifest.
4. Start the harness for P2: load `PerceptionPipeline` from `manifest.cpu.toml` with `ORT_EP=cpu` and run it on a few of the fixture photos.

---

## 6. Roadmap by phase

IDs: **M** = models track (Sam), **P** = pipeline and integration track (Sara), **J** = joint. A task's "Done when" is the check that closes it.

### Phase 1, Oct 16–17: the seams, and a first real perception job

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M1 | Disease head v1d on the corrected splits (Tomato-Pakistan grouped by original photo, field test splits) | Sam | — | Manifest entry and report committed; test top-1 0.814 without bean, ECE 0.032 | Done (PR #6, 3c10ae9) |
| M2 | Leaf detector retrained on the grouped train/valid split, exported, manifest entry updated | Sam | GPU (vLLM stopped) | New ONNX within 0.5 points of PyTorch mAP50 on the test split; manifest hash updated | Done: `rfdetr-s-leaf-2026-10-16b`, mAP50 0.937 (PR #7) |
| M3 | Qwen3.5-9B challenger starts and passes the day-1 check | Sam | M2 finished (GPU) | A report in `ml/training/eval/reports/` | Next |
| M4 | Answer PR #6's reviews; merge PRs #6 and #7 | Sam | Reviews | Both merged into `main` | In progress: reviews answered (efa6c4b–a2ebaa9), merges wait on approval |
| M5 | Model files for other machines: `ml/models/fetch.py` and the `models-2026-10-16` release (fp16 and fp32 sets) | Sam | M2 | Sara gets the files on a fresh machine with one command | Done (PR #7, a3fa16a): tested into an empty folder |
| M6 | Fixtures for the Rust tests: a few saved BioCLIP embeddings with the Python head and zero-shot outputs (JSON) | Sam | M1 | Committed under `ml/tests/fixtures/` | Done (PR #7, 9ab7368): `perception_reference.json`, 6 photos |
| M7 | YuNet pinned (file, SHA-256, manifest entry `yunet`) and the face check on 10 team photos ([evaluation 7](evaluation.md#7-day-1-checks)) | Sam | 10 photos with people | Every face found; manifest entry committed | Not started |
| M8 | Text embedder pinned: arctic-embed-xs ONNX and tokenizer in the manifest (`text_embedder`, `text_embedder_tokenizer`), plus reference vectors from Python for 20 sentences | Sam | — | Files and vectors committed; P5 can compare | Not started |
| M9 | A CPU development manifest (`manifest.cpu.toml`) pointing at the fp32 files, so the perception code runs on a laptop without a GPU | Sam | M2 | Sara runs `ORT_EP=cpu` locally | Done (PR #7, a3fa16a): every fp32 graph runs on the CPU provider |
| M20 | The fake VLM server: OpenAI-compatible, answers with the recorded day-1 responses of Qwen3-VL-8B (option 1 in section 5) | Sam | — | Sara's VLM harness (P3) runs against it offline | Next |
| P0 | Set up the laptop: WSL with Ubuntu 26.04, the backend tools, `fetch --cpu`, llama-server with Qwen3-VL-2B (section 5) | Sara | M5, M9 | `cargo nextest run` passes in `backend/`, and `curl` on `127.0.0.1:8080/v1/models` lists `vlm` | Next |
| P1 | Fix gaps G1–G6 in `vision` and `worker` (section 4) | Sara | P0; M6 for the tests | `PerceptionPipeline::load` succeeds on the real manifest; tests load the real head and matrix and match the Python fixtures | Next |
| J1 | Pre-processing parity (G8): Rust writes the tensors for 10 photos × 3 recipes and 5 leaf crops; Python compares | Sara (Rust dump), Sam (comparison) | P1 | Max absolute difference per tensor recorded; under 0.02 after normalisation, or the gap explained and fixed. `crop_box`'s rounding confirmed | Not started |
| P2 | A harness that runs one perception job without the API (a job payload and its images in, the result JSON out), first on the laptop with `manifest.cpu.toml`, then on a GPU by Sam with `manifest.toml` | Sara (harness, CPU run), Sam (GPU run) | P1 | On the CPU: leaf regions with masks, species top 3, disease top 3 per crop and embeddings for 6 shots. On the GPU: the same, with time per job and peak VRAM | Not started |
| P3 | The same harness for a `vlm` job: the worker's real prompts and schemas through `VlmAdapter`, against the fake server and against Qwen3-VL-2B on the laptop; then Sam runs it against Qwen3-VL-8B | Sara (harness, local runs), Sam (real run) | P0; M20 for the fake server | Findings and indicators decode and validate on all three servers; compared with `vlm_check`'s day-1 report | Not started |
| P4 | Decide where the text embedder and YuNet run in `api` and who wires them | Sara and the `api` owner | — | Owner named here | To assign |

### Phase 2, Oct 18–19: real models, the day-2 benchmark

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M10 | Evaluation photos: the 20 dev cases with full shot sets the day-2 benchmark needs ([evaluation 2](evaluation.md#2-the-evaluation-set)) | Sam, with the team taking photos | Plants to photograph | 20 cases in `ml/training/eval/manifest.jsonl` (photos stay out of git) | Not started |
| J2 | **Day-2 VLM benchmark**: both candidates, `VLM_SLOTS` 1 and 2, `VLM_MAX_PIXELS` at 4 sizes, on the 20 dev cases, through the worker's real prompts and schemas ([evaluation 8](evaluation.md#8-day-2-benchmarks)) | Sam (runs and scores), Sara (worker build from P3) | M10, P3, M3 | A benchmark report; the numbers for decision D1 | Not started |
| P5 | Prompt fixes from day 1 ([vlm.md 4](vlm.md#4-serving)): species answered `other` although it was in the enum; a fungal spot called `leaf_mines`; a switch to Chinese mid-word under a `maxLength` cap | Sara (prompt code), Sam (reruns the check) | P3 | The three cases answered correctly on the day-1 photos; prompt versions bumped | Not started |
| J3 | Perception batch caps on the RTX 3070 with every session loaded ([evaluation 8](evaluation.md#perception-batch-size)) | Sam (runs), Sara (worker), the 3070's owner (host) | P2, the 3070 host | Caps chosen with 1 GB free at peak; written to the worker's environment | Not started |
| M11 | Leaf relabelling pass (people, Oct 18), then the second detector run the night of Oct 18 | Sam and the team | GPU night slot | New detector exported and in the manifest | Not started |
| M12 | The real BioCLIP label matrix from the species and organism seed files | Sam | The knowledge seed's `species/` and `organisms/` files; issue #4 | Manifest entry `bioclip_labels_<label_set_version>`; zero-shot check on team photos | Waiting |
| M13 | Heads on our detector's crops (Oct 19): disease head v2, the species head, the plant probe (needs non-plant negatives: soil and surroundings photos) | Sam | M11, M12 | Manifest entries; the species head kept only if it improves the dev split | Not started |
| P6 | The worker services in `infra/rig.compose.yml` (Sara writes them, Sam runs them): `perception-worker` and `vlm-worker`, each with its env file (`WORKER_TOKEN`, GPU by UUID, `ORT_DYLIB_PATH`, `MODEL_MANIFEST` mounted read-only, `VLM_BASE_URL=http://vllm:8000`) | Sara (Sam reviews; it's his machine) | P2, P3 | `docker compose -f infra/rig.compose.yml up` starts all three and the workers' heartbeats reach the API | Not started |
| P7 | Second VLM worker on the RX 9070 XT (AMD day-1 check, then the same checkpoint and flags) | The 9070 XT's owner | The card's host | Findings and indicators calls pass with our schema | To assign |

### Phase 3, Oct 20–21: end-to-end scans

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M14 | Choose the VLM and set `VLM_SLOTS` per card and `VLM_MAX_PIXELS` (decision D1) | Sam | J2 | Values in `infra/.env.example` and the worker's env; vlm.md updated | Not started |
| P8 | `pipeline` retrieve and score on real perception and VLM results: candidates from the disease head, indicators from the VLM, the `verify` job for an organism outside the set | Sara | P2, P3, the knowledge seed | A real scan reaches a diagnosis or a question with the right candidates | Not started |
| **J4** | **First full scan end to end on the rig**: upload → perception → retrieve → vlm → score → plan, with `model_versions` traced on every result | Sara (pipeline), Sam (models), the `api` owner | P6, P8 | A scan from the app (or a script) ends in a plan; its event log saved as a fixture | Not started |
| J5 | The evaluation harness runs dev cases through the API and scores them per stage ([evaluation 9](evaluation.md#9-harness-and-reports)) | Sam (harness), Sara (a worker token and a test garden) | J4 | One command scores all dev cases | Not started |
| P9 | Fallback worker: one live box call settles its box format; whether 8,000-token calls fit its doubled `max_runtime` (contracts open question 26); cost per scan | Sara (runs), Sam (scores) | An LLM API key | Numbers recorded in vlm.md 12 | Not started |
| M15 | Heads retrained as problem profiles are reviewed and the label list grows; spot colour thresholds from hand-drawn lesions | Sam | M13, reviewed profiles | New manifest entries | Not started |

### Phase 4, Oct 22–23: calibration, then freeze the models

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M16 | Calibration on the dev split: `T_species`, `T_disease`, `τ_det`, `τ_leaf`, `τ_plant`, `τ_mask`, the VLM certainty values, BioCLIP agreement, the diagnosis margin, indicator weights (contracts open questions 2–4; [evaluation 5–6](evaluation.md#5-calibration)) | Sam | J5 | Values in the manifest and wherever `api` reads them | Not started |
| P10 | The calibrated values reach the code: the workers read the manifest (P1's G6 fix); `api` reads the diagnosis margin, certainty values and indicator weights from their configuration | Sara | M16 | A changed value in the manifest changes a scan's result, and `perception_config` changes with it | Not started |
| M17 | Final manifests: every hash, licence and notice ([vision README 9](README.md#9-licences-and-notices)) | Sam | M16 | Committed; the rig runs exactly these files | Not started |
| J6 | Hand-over test: kill the rig in the middle of a scan; the fallback worker takes over within about 45 seconds | Sara, with the backend team | P9, J4 | Passes twice | Not started |

### Phase 5, Oct 24–25: score once, ship

| ID | Task | Owner | Needs | Done when | State |
|---|---|---|---|---|---|
| M18 | Score the test split once; the numbers for Devpost | Sam | M17 | A report committed; no model changes after it | Not started |
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

The perception worker and vLLM don't fit together on this card (vLLM leaves about 1.5 GB free at 0.90), so Sam stops vLLM for a perception run (`docker stop infra-vllm-1`, then `docker start infra-vllm-1`). To ask for a real run, push the branch and say in the pull request what to run; Sam posts the output there. Once the RTX 3070 and RX 9070 XT hosts are up, perception moves to the 3070 and this conflict ends ([vision README 4](README.md#4-placement-and-vram-budgets)).

---

## 9. Decisions and blockers

| # | Decision or blocker | Blocks | Who decides | State |
|---|---|---|---|---|
| D1 | Which VLM: Qwen3-VL-8B FP8 or Qwen3.5-9B AWQ; `VLM_SLOTS`, `VLM_MAX_PIXELS` | M14, the 9070 XT's checkpoint | Sam, from J2 | Waiting on J2 |
| D2 | [Issue #3](https://github.com/aacruzgon/GreenLens/issues/3): the head and label-matrix JSON, now with gaps G1–G3 | P1 | Sam and Sara | Open |
| D3 | [Issue #4](https://github.com/aacruzgon/GreenLens/issues/4): supported crops and slugs; whether bean stays | M12, M13, P8 | The knowledge-base owner | Open |
| D4 | [Issue #5](https://github.com/aacruzgon/GreenLens/issues/5): the duplicate threshold and the conflicting PlantDoc groups | Heads retraining | Whoever checks the sampled pairs | Open |
| D5 | Who owns the knowledge ingestion (`ml/knowledge`, its model steps in [training 10](training.md#10-knowledge-ingestion-the-model-steps)) | M12, P8 | Team | To assign |
| D6 | The 9070 XT's server: vLLM ROCm or llama.cpp Vulkan | P7 | The card's owner, from its day-1 check | Open |
| D7 | Where the text embedder and YuNet are wired in `api`, and by whom | M7, M8 | Sara and the `api` owner (P4) | Open |
| D8 | Disease head or ConvNeXt-Tiny, if the head stays under 75% top-1 on detector crops | M13 | Sam | v1d is at 0.814 on whole images; decided on crops |
