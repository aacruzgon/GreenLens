# PDM Vision and ML

> **Status:** Draft v0.2.13, 2026-10-17. v0.2.13: M1's checks ran on a laptop (S1), and M2's except the embedder's golden test, which isn't written yet; M3 and M4 are still open. v0.2.12: the example entry's `tau_leaf` is the provisional 0.90, shipped with `tau_plant` 0.67 ([models 2](models.md#2-plant-present-check)). v0.2.11: the example entry's `tau_leaf` is the provisional 0.35 ([models 2](models.md#2-plant-present-check)). v0.2.10: section 6 says the perception worker reads every tensor name and threshold from the manifest and `precision` to refuse fp16 on the CPU, and the example entry has `tau_leaf` ([issue #12](https://github.com/aacruzgon/GreenLens/issues/12)). v0.2.9: section 6's delivery is built in PR #7 (`sources.toml`, `fetch.py`, `check.py`, `publish.py`, `NOTICES.md` and `licences/`, the S1 entries in the manifest, our files in `GreenLens/pdm-models`), with `--cpu` for laptops and CPU twins of the heads; open questions 11 and 14 are settled; [dev-roadmap.md](dev-roadmap.md) plans the whole ML side, models and their integration into the backend, with owners, hand-offs and phases, and [roadmap.md](roadmap.md) tracks model progress against the build plan. v0.2.8: section 6 says the compiled loader and `store` now read `ml/models/FORMATS.md`; open question 13 is settled: organisms stay in the matrix, unscored. v0.2.7: section 6 takes the head and label-matrix formats from `ml/models/FORMATS.md` (PR #6) and lists the four points where the compiled loader still differs, one of them silent. v0.2.6: section 6 keeps our own model files in a private Hugging Face model repository, as the team decided, instead of GitHub release assets; open questions 11 and 14 follow. v0.2.5: section 1 says the repository is private until the submission; section 6 says how model files reach the backend (the S1 manifest's three entries with verified hashes, what `ml/` produces for each slice, the head and label-matrix JSON formats, the private `pdm-models` release store with `sources.toml` and `fetch.py`, the read-only `/models` volume, change-overs, notices and a numbered roadmap for S1–S3); sections 10 and 11 follow. v0.2.4: the SAM 2.1-T encoder's budget is 2.0 GB, from its measured fp16 export, and the 3070 total about 5.3 GB (review of PR #6). v0.2.3: section 5 says how `ort` loads ONNX Runtime (`load-dynamic`, `api-29`, `ORT_DYLIB_PATH`, `ORT_EP`) and why the CUDA 13.0.3 image needs it; section 12 says where the findings stand now that the backend design is on `main`. v0.2.2: section 6 lists the manifest entry names the worker reads. v0.2.1: finding 27 is taken up by the backend branch (contracts 6). v0.2: `model_versions` keys follow contracts 5 (manifest entry names, `sam_encoder` and `sam_decoder`, label versions as keys); section 12 says which findings the backend branch takes up and adds finding 27, the fallback's guided-check budget. v0.1, 2026-10-15: First vision and ML design: every model the system runs, where each one runs on the three GPUs of the rig and the control plane's CPU, VRAM budgets, runtimes and version pins, how model files are pinned and versioned, licences, the build plan, open decisions, and the points where the architecture and contracts need a change to match.
> **Checked:** model cards, licences, configs, checkpoint sizes, package indexes and vLLM, xgrammar, ONNX Runtime and PyTorch sources were read on 2026-10-15 (✓). The ONNX exports of RF-DETR, SAM 2.1, MobileSAM, UperNet and BioCLIP, the PlantDoc probes, the text-embedder benchmark and YuNet were run on a Mac with ONNX Runtime 1.31 on CPU (✓ ran). Nothing has run on the rig's GPUs: every VRAM and GPU-speed figure is an *estimate* until the day-1 checks and day-2 benchmarks. Points from secondary sources are *unverified*. On 2026-10-16 the three S1 model files were downloaded at their pinned revision and commit and hashed (✓ ran), and Hugging Face's storage limits and GitHub's large-file and LFS billing pages were read (✓).

This is the design of PDM's models and the machine learning around them. It builds on the [system architecture](../architecture/README.md) (sections 2, 3, 5, 6, 8.4, 10, 13, 14 and 15) and the [data contracts](../architecture/contracts.md) (sections 5, 6, 7 and 10), which define every shape the models' outputs travel in. How the Rust code calls a model (sessions, batching, the VLM client, schema building, box conversion, face blurring, the embedder, the measurement code) is in [docs/backend](../backend/vision.md) and [the worker design](../backend/worker.md).

| Document | What it holds |
|---|---|
| [README.md](README.md) (this one) | The model inventory, placement and VRAM, runtimes, pinning and versions, licences, build plan, open decisions and findings |
| [models.md](models.md) | Every model outside the VLM: plant check, leaf detector, mask model, regions, BioCLIP 2, the disease head, image embeddings, the face detector and the text embedder |
| [vlm.md](vlm.md) | The vision-language model: choice, quantization, serving, image preparation, memory, prompts, schemas, box formats, sampling and the fallback worker's hosted model |
| [training.md](training.md) | Datasets and licences, data preparation, training, ONNX export, label lists, feedback, and the model steps of knowledge ingestion |
| [evaluation.md](evaluation.md) | The evaluation set, metrics and targets, calibration, thresholds, day-1 checks and day-2 benchmarks |
| [dev-roadmap.md](dev-roadmap.md) | The plan for the whole ML side: who builds what, the hand-offs between `ml/` and the Rust crates, the integration gaps, tasks by phase with done conditions |
| [roadmap.md](roadmap.md) | Model progress against the build plan (section 10): what is done, next and waiting, the open pull requests and the rig's state |

## Contents

1. [Goals and principles](#1-goals-and-principles)
2. [Scope and the split with docs/backend](#2-scope-and-the-split-with-docsbackend)
3. [Model inventory](#3-model-inventory)
4. [Placement and VRAM budgets](#4-placement-and-vram-budgets)
5. [Runtimes and versions](#5-runtimes-and-versions)
6. [Model files, versions and pinning](#6-model-files-versions-and-pinning)
7. [Label lists and the knowledge base](#7-label-lists-and-the-knowledge-base)
8. [The fallback path](#8-the-fallback-path)
9. [Licences and notices](#9-licences-and-notices)
10. [Build plan](#10-build-plan)
11. [Open decisions](#11-open-decisions)
12. [Findings for the architecture and contracts](#12-findings-for-the-architecture-and-contracts)
13. [Sources](#13-sources)

---

## 1. Goals and principles

- **Facts with a confidence, never a diagnosis.** Models produce observations; scoring and rules make the diagnosis ([architecture 2](../architecture/README.md#design-principles)). Every model output carries a calibrated confidence ([evaluation 5](evaluation.md#5-calibration)).
- **One backbone where one will do.** BioCLIP 2 identifies the species, scores the plant check, embeds images for similar cases and gives the disease head its features: one session, four uses ([models 6](models.md#6-species-bioclip)).
- **Permissive licences only in what ships.** The repository becomes public for the submission (it is private today) and the demo is a hosted service, so every model's weights are Apache-2.0 or MIT, and datasets with non-commercial or no-derivatives terms stay out of shipped weights ([section 9](#9-licences-and-notices)). That rules out Ultralytics YOLO (AGPL-3.0).
- **Pinned by hash.** Every model file is named in one manifest with its SHA-256, and every result records the versions that made it ([section 6](#6-model-files-versions-and-pinning)).
- **Measured, not assumed.** Each threshold is set on the evaluation set; each model has an acceptance target ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)).
- **Every model is replaceable behind its trait** (`VisionModel`, `SpeciesIdentifier`, `TextEmbedder`: [architecture 2](../architecture/README.md#design-principles)); swapping one is a manifest change plus, at most, an adapter.

---

## 2. Scope and the split with docs/backend

| Topic | docs/vision | docs/backend |
|---|---|---|
| Which model, why, licence, alternatives | ✓ | |
| Tensors, pre- and post-processing values, thresholds | ✓ (the values) | ✓ (the code) |
| Datasets, training, ONNX export, label lists | ✓ | |
| VRAM budgets per card, server flags for vLLM and llama.cpp | ✓ | Compose services |
| Prompts and per-call schemas, from the model's side | ✓ | Schema building, box conversion |
| ONNX Runtime sessions, batching, the VLM client, retries | | ✓ |
| Face blurring, depth zeroing, the blur and exposure check | Model and thresholds | ✓ |
| The text embedder's code | Model, files and recipe | ✓ |
| Depth measurement | Thresholds from the evaluation | ✓ |
| Evaluation set, metrics, calibration, benchmarks | ✓ | |
| Knowledge ingestion | Its model steps | |

---

## 3. Model inventory

| Task | Model | Licence | Runs on | Output | Section |
|---|---|---|---|---|---|
| Plant-present check | Leaf detector score OR a BioCLIP 2 plant probe | Apache-2.0 / MIT | RTX 3070 | `plant_present` | [models 2](models.md#2-plant-present-check) |
| Leaf boxes | **RF-DETR-S**, fine-tuned to one class `leaf` | Apache-2.0 ✓ | RTX 3070 | `leaf` regions | [models 3](models.md#3-leaf-detector) |
| Leaf masks, `plant` region | **SAM 2.1 Hiera-T**, prompted with boxes | Apache-2.0 ✓ | RTX 3070 | RLE masks | [models 4–5](models.md#4-mask-model) |
| Species, organisms | **BioCLIP 2** (ViT-L/14) vision tower against a precomputed label matrix | MIT ✓ | RTX 3070 | Top 3 species | [models 6](models.md#6-species-bioclip) |
| Disease per leaf | A head on BioCLIP 2 features of each leaf crop | Ours, from CC BY data | RTX 3070 (features), worker CPU (head) | Top 3 problems per crop | [models 7](models.md#7-disease-classifier) |
| Image embeddings | BioCLIP 2's image embedding, 768 dimensions | MIT ✓ | RTX 3070 | One vector per image | [models 8](models.md#8-image-embeddings) |
| Soil, organisms with boxes, damage, species cross-check, indicator answers, guided checks | **Qwen3-VL-8B-Instruct**, FP8 (primary) or **Qwen3.5-9B**, W4A16 (challenger) | Apache-2.0 ✓ | RTX 5070 Ti, and RX 9070 XT as a second VLM worker | `VlmFindings`, `VlmIndicatorAnswers`, `GuidedTexture` | [vlm.md](vlm.md) |
| Face detection for blurring | **YuNet** (`2026may`, dynamic input) | MIT ✓ | Control plane CPU | Face boxes | [models 9](models.md#9-face-detector) |
| Text embeddings | **snowflake-arctic-embed-xs**, fp32, 384 dimensions | Apache-2.0 ✓ | Control plane CPU, and the ingestion scripts | Query and document vectors | [models 10](models.md#10-text-embedder) |
| Vision fallback | `claude-opus-5-5`, configurable | Hosted API | Fallback worker | The same shapes | [vlm.md 12](vlm.md#12-the-fallback-worker) |
| Knowledge tagging, extraction | `claude-haiku-5-5` (or Jev), `claude-opus-5-5` | Hosted API | Dev machines | Seed drafts | [training 10](training.md#10-knowledge-ingestion-the-model-steps) |

Not models: the blur and exposure check (Laplacian variance and a histogram) and the measurement stage (geometry and colour thresholds), both in `api` on CPU; their thresholds come from [evaluation 6](evaluation.md#6-thresholds-set-from-the-evaluation-set).

---

## 4. Placement and VRAM budgets

### The rig

| Card | Memory | Role | Capability |
|---|---|---|---|
| RTX 3070 | 8 GB GDDR6, sm_86 | perception-worker: every ONNX model | `perception` |
| RTX 5070 Ti | 16 GB GDDR7, sm_120 | vlm-worker #1 with its vLLM sidecar; also the training card | `vlm` |
| RX 9070 XT | 16 GB GDDR6, gfx1201 (RDNA 4) | vlm-worker #2 with its own vLLM ROCm sidecar (recommended; not yet decided) | `vlm` |

**Why the 9070 XT takes the VLM.** The `vlm` job is the slow one: two calls of up to 4 images, 30–50 seconds of decoding each job *estimate*, and a `max_runtime` of 5 minutes, against about 5 GB of ONNX models that the 3070 already holds. The fallback worker takes any job still unclaimed 15 seconds after it was queued ([contracts 5](../architecture/contracts.md#leases-runs-and-attempts)), so whenever every VLM slot is busy, the overflow goes to the hosted model and costs API credits. A second VLM card doubles the scarce capacity, so far less work overflows, and gives the VLM redundancy when one card or sidecar is down. A second `--caps vlm` worker needs no contract change: the broker leases the oldest matching job to whichever worker claims first ([architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback)). Running perception on it instead would bring in AMD's own ONNX Runtime build (1.29 against 1.31 *unverified*, from the AMD stack research shared by the backend docs' author) through MIGraphX, which compiles per input shape, for capacity that isn't needed.

### RTX 3070: perception

All five sessions stay loaded. Within a job they run one after another, but ONNX Runtime's arenas keep what they grew to, so the budget is the sum. Each session gets a `gpu_mem_limit` at its budget, `arena_extend_strategy = kSameAsRequested`, and a warm-up at its largest batch when the worker starts, so the footprint is fixed and known before the first job ([docs/backend](../backend/vision.md)).

| Item | Precision | Batch cap | Weights | Budget *estimate* |
|---|---|---|---|---|
| CUDA context, cuDNN and cuBLAS workspaces | | | | 0.5 GB |
| RF-DETR-S leaf detector | fp16, `GridSample` fp32 | 8 images | 57 MB | 1.0 GB (1.7 GB at a batch of 16 in fp32 on the CPU proxy ✓ ran) |
| SAM 2.1-T encoder | fp16 | 2 images | 55 MB | 2.0 GB: its pool peaked at 1,872 MiB above the loaded session at batch 2 in fp16 on the 5070 Ti ✓ ran; the 3070 is rechecked on day 2 |
| SAM 2.1-T box decoder | fp16, fp32 logits | 16 boxes | 8 MB | 0.6 GB (38 MB per box in fp32 ✓ ran) |
| BioCLIP 2 vision tower | fp16 | 16 images | 0.61 GB | 1.2 GB |
| Heads and the plant probe | fp32 | | under 2 MB | On the worker's CPU |
| **Total** | | | | **about 5.3 GB of 8 GB** |

- At least 1 GB must stay free at peak ([evaluation 8](evaluation.md#8-day-2-benchmarks)). If the SAM encoder doesn't fit, MobileSAM replaces it (336 MB per image ✓ ran); if BioCLIP 2 doesn't, BioCLIP (0.17 GB) does, with 512-dimensional embeddings.
- `PERCEPTION_MAX_BATCH` (16) stays the job-wide cap, with the per-model caps above as deployment configuration below it ([section 12](#12-findings-for-the-architecture-and-contracts), finding 8).
- The architecture's "roughly 4 GB" for these models ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)) becomes about 5 GB with activations, still well within the card.
- The optional UperNet regions model ([models 5](models.md#option-a-scene-parsing-model)) would add about 0.5 GB with its argmax in the graph.

### RTX 5070 Ti and RX 9070 XT: the VLM

Each card runs one vLLM sidecar at `--gpu-memory-utilization 0.90` and nothing else while serving ([vlm.md 6](vlm.md#6-memory-and-vlm_slots)):

| | RTX 5070 Ti | RX 9070 XT |
|---|---|---|
| Server | vLLM v0.31.0, image built on CUDA 13.0.3 ✓ (its Dockerfile; the published tag `vllm/vllm-openai:v0.31.0` is *unverified*) | `vllm/vllm-openai-rocm:v0.31.0` (ROCm 7.2.3 userspace, built for gfx1201), tag and digest read from the registry by the AMD research |
| Checkpoint | The same checkpoint and revision on both | |
| Weights | 9.9 GiB (Qwen3-VL FP8) or 7.0 GiB (W4A16) | Same |
| KV for 2 jobs at 1 megapixel per image | About 2 GiB in BF16 (Qwen3-VL), 0.5 GiB (Qwen3.5) *estimate* | Same |
| Memory bandwidth | 896 GB/s *unverified* | 640 GB/s *unverified*: decoding about 30% slower *estimate* |
| `VLM_SLOTS` | From its own day-2 benchmark | From its own day-2 benchmark |
| Fallback server | llama.cpp (CUDA) | llama.cpp `server-vulkan` |

**One checkpoint on every VLM worker.** A scan's `vlm` job and its later `verify` and `guided` jobs can land on different cards, so both cards must serve the same weights with the same prompts, schemas and image preparation; their answers are then comparable, never bit-identical. vLLM's RDNA 4 FP8 support landed in August 2026 (vLLM #43615, from the AMD research *unverified*), and whether it covers Qwen3-VL's 128 × 128 block scales on gfx1201 is *unverified*. If it doesn't, **both cards run the W4A16 checkpoint** (`cyankiwi/Qwen3-VL-8B-Instruct-AWQ-4bit`, Marlin on NVIDIA, and on AMD the RDNA 4 W4A16 kernel of vLLM #40977, which was benchmarked on a Qwen2.5 AWQ checkpoint; whether this compressed-tensors checkpoint selects it on gfx1201 is *unverified*), and the 5070 Ti gives up FP8's small accuracy edge for consistency. Nothing has run Qwen3-VL or Qwen3.5 on ROCm yet, and Qwen3.5's Gated DeltaNet kernels there are an extra risk for the challenger. The day-1 check reads the kernel choice from vLLM's log ([evaluation 7](evaluation.md#7-day-1-checks)). If either card falls back to llama.cpp, both serve the same GGUF files, or only one VLM worker runs. A worker whose `vlm_model` differs from the others is a configuration error shown on `/status`.

**Training** uses the 5070 Ti with vLLM stopped ([training 2](training.md#2-environment)), so long training runs happen before the VLM pipeline goes live. With two VLM cards, one can be taken out for training while the other serves.

### Control plane (CPU)

| Model | Memory | Time per call *estimate* |
|---|---|---|
| YuNet, two passes per upload | 230 KB file ✓, under 100 MB working *estimate* | 0.1–0.5 s per 12-megapixel photo (38 ms at 1280 px on one Apple M3 thread ✓ ran; expect 2–5 times slower per thread on a small VM) |
| arctic-embed-xs | 90 MB file ✓, about 200 MB working *estimate* | 5–25 ms per query (4.7 ms on one M3 thread ✓ ran) |
| Blur and exposure check | | Milliseconds on a 512-pixel downscale |

Both models run on blocking threads in `api` with a small pool, so an upload burst doesn't stall chat ([docs/backend](../backend/vision.md)).

---

## 5. Runtimes and versions

| Where | Runtime | Pin |
|---|---|---|
| Perception worker (3070) | ONNX Runtime **1.31.0** with the **CUDA 13** execution provider, through `ort` 2.0.0-rc.14 | The ORT 1.31.0 GPU build is for CUDA 13.0 and cuDNN 9 and has native code for sm_86 and sm_120 ✓. The CUDA 12.8 build (1.24.1) has no sm_120 code ✓. Image `nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` ✓ |
| VLM on the 5070 Ti | vLLM **0.31.0** | Built on CUDA 13.0.3 with torch 2.13.0 and xgrammar 0.2.7 ✓ (Dockerfile and requirements at the tag); a `-cu129` image exists for older drivers ✓ (release notes) |
| VLM on the 9070 XT | vLLM **0.31.0** ROCm image | ROCm 7.2.3 userspace; the host needs ROCm 10.1's `amdgpu` 31.60 on Ubuntu 24.04.5 HWE or 26.04.1; no `HSA_OVERRIDE_GFX_VERSION` (all from the AMD research, *unverified* until the day-1 check, which also confirms the 7.2.3 userspace works on the 10.1 driver) |
| NVIDIA host | Driver **580.65.06 or later** | CUDA 13.0's minimum ✓; one driver serves the ORT and vLLM containers, since both are built for CUDA 13 |
| Training | PyTorch **2.14.1**, torchvision 0.29.1, **cu130** wheels | cu126 has no sm_120 ✓; cu130 has it in the 2.12–2.13 support matrix ✓, *unverified* for 2.14.1 until day 1 ([training 2](training.md#2-environment)) |
| Control plane | ONNX Runtime 1.31.0 CPU through `ort` | Microsoft's plain `x86_64-unknown-linux-gnu` build ✓, opened the same way (below) with `ORT_EP=cpu` |

**How `ort` loads ONNX Runtime.** `ort` is built with `load-dynamic` and `api-29`, not linked to a fixed build ([backend vision 2](../backend/vision.md#2-dependencies), the `rust-workers` skill's `ort.md`):

- At start, the binary opens the vendor's `libonnxruntime` named by `ORT_DYLIB_PATH`. On the perception worker that is Microsoft's 1.31 GPU build in the `nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04` image; on `api`, Microsoft's 1.31 CPU build.
- `ORT_EP` (`cuda`, `migraphx`, `webgpu` or `cpu`) chooses the execution provider at run time.
- `api-29` is the lowest ONNX Runtime API the binary asks for, so AMD's 1.29 build would load too, if perception ever moved to the RX 9070 XT.

The alternative, pyke's prebuilt 1.31 through `download-binaries`, needs CUDA 13.2 or newer and cuDNN 9.23 or newer (pyke's documentation, *unverified* here). So the 13.0.3 image works only with `load-dynamic`.

**ONNX Runtime settings on the 3070** (from its CUDA execution provider documentation ✓, values to confirm on day 2):

| Option | Value | Why |
|---|---|---|
| `gpu_mem_limit` | Each session's budget above | Caps that session's arena |
| `arena_extend_strategy` | `kSameAsRequested` | The default doubles allocations, which can hold up to twice what is used ✓ |
| `cudnn_conv_algo_search` | `HEURISTIC` | `EXHAUSTIVE` re-benchmarks every new input shape, and batches vary ✓ |
| `cudnn_conv_use_max_workspace` | 0 | The default can add several GB of workspace ✓ |
| `do_copy_in_default_stream` | true (default) | Recommended ✓ |
| CUDA graphs, TensorRT | Not used | CUDA graphs need fixed shapes ✓; the TensorRT execution provider's documentation stops at ORT 1.22 and CUDA 12.8, and engines take minutes to build ✓ |
| `error_on_failure` on the CUDA provider | Always | Otherwise a failed registration silently falls back to CPU ✓ |

**Correction for the architecture.** Architecture 10's day-1 check says the 5070 Ti "needs CUDA 12.8+ builds of PyTorch, vLLM and ONNX Runtime". That holds for PyTorch and vLLM, but ONNX Runtime 1.31, the version `ort` rc.14 targets, ships CUDA 13 builds only, and the current PyTorch and vLLM default to CUDA 13 too. The whole rig is on CUDA 13 with a 580 driver ([section 12](#12-findings-for-the-architecture-and-contracts), finding 1). Architecture 10 on `main` now says so.

---

## 6. Model files, versions and pinning

### The manifest

`ml/models/manifest.toml` lists every file a worker or `api` loads. Rust reads TOML with the `toml` crate and Python with `tomllib`, so one file serves the workers, `api`, the export checks and the evaluation harness.

```toml
[[model]]
name      = "leaf_detector"
version   = "rfdetr-s-leaf-2026-10-18"
file      = "leaf_detector/rfdetr-s-leaf-2026-10-18.onnx"
sha256    = "<64 hex>"
licence   = "Apache-2.0"
source    = "RF-DETR-S COCO weights (Apache-2.0), fine-tuned by ml/training/detector at <git sha>"
runs_on   = "perception"                 # perception | api
opset     = 17
precision = "fp16"                       # fp16 with GridSample in fp32

  [model.input]
  name   = "input"
  shape  = ["batch", 3, 512, 512]
  colour = "rgb"
  resize = "squash"                      # squash | shortest_side_crop | longest_side_pad
  mean   = [0.485, 0.456, 0.406]
  std    = [0.229, 0.224, 0.225]

  [model.outputs]
  boxes  = "dets"
  logits = "labels"

  [model.config]                         # thresholds and caps, set from the evaluation
  score_threshold  = 0.35
  tau_leaf         = 0.90                # τ_leaf of the plant check (provisional until fitted)
  max_batch        = 8
  max_per_image    = 40
  gpu_mem_limit_mb = 1024

  [model.check]                          # written by ml/training/export/check.py
  map50_onnx  = 0.0
  map50_torch = 0.0
```

- Entries also exist for SAM's two graphs (`sam_encoder` and `sam_decoder`), BioCLIP 2's vision tower (`bioclip`), each label matrix (`bioclip_labels_<label_set_version>`), the disease heads (`disease_head_<classifier_labels_version>`, with their temperature), the plant probe (`plant_probe`), YuNet (`yunet`) and the text embedder with its tokenizer (`text_embedder`, `text_embedder_tokenizer`): the names the worker and `api` read ([backend vision 5](../backend/vision.md#5-the-model-manifest)). The species head would be `species_head`; the worker doesn't load it until the dev split shows it helps (`SPECIES_HEAD_ENABLED`). Heads and label matrices are JSON files with base64 `f32` matrices, in the formats of [the files `ml/` produces](#what-ml-produces-and-when).
- **What Rust reads.** The `vision` crate's `ModelEntry` reads `name`, `version`, `file`, `sha256`, `licence`, `source`, `runs_on`, `[model.input]`, `[model.outputs]`, `[model.config]`, `label_set_version`, `classifier_labels_version` and `precision` ([backend vision 5](../backend/vision.md#5-the-model-manifest)). The perception worker takes every tensor name from `[model.input]` and `[model.outputs]` and checks them against each loaded ONNX file, and takes every threshold from `[model.config]`, refusing to start on a missing key or a mismatch; the environment sets only the batch caps, never above an entry's `max_batch` (`max_boxes`). It reads `precision` to refuse an fp16 model on `ORT_EP=cpu`, whose provider crashes on fp16 graphs. `opset` and `[model.check]` are for the Python export checks and the evaluation harness; serde ignores them.
- **The model store.** Upstream files come from their pinned Hugging Face revision or Git commit; our own files are in a private Hugging Face model repository, `pdm-models`, pinned by commit. `ml/models/fetch.py` downloads every file a host needs and checks its hash ([delivering model files](#delivering-model-files-to-the-backend)).
- **Every consumer verifies** each file it loads against the manifest's SHA-256 at start-up and refuses to start on a mismatch: `api`, the perception worker and the fallback worker. The VLM checkpoint is pinned the same way: by Hugging Face revision in the sidecar's command, and by the `vlm_model` key the worker reports.

### Versions in results

- `model_versions` in every `complete` and the heartbeat's `versions` ([contracts 5](../architecture/contracts.md#post-internalworkersheartbeat)) carry, for a perception worker: each model the job used under its manifest entry name, with its manifest `version` as the value (`leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, `disease_head`, `species_head`, `plant_probe`; SAM's two graphs are two entries), `ort_ep`, `worker_id`, and `perception_config`, the first 16 hex digits of the SHA-256 of every `[model.config]` table in the manifest (thresholds, temperatures, caps), so a result can be traced to the exact settings. `label_set_version` and `classifier_labels_version` are keys too: in `complete`, the ones that job used; in the heartbeat, every loaded one, comma-separated. A VLM worker reports the keys of [vlm.md 11](vlm.md#11-validation-retries-and-versions). The keys and value formats are fixed in [contracts 5](../architecture/contracts.md#post-internalworkersheartbeat) as agreed with docs/backend ([worker 17](../backend/worker.md#17-open-questions)).
- Each observation's `model_version` ([contracts 7](../architecture/contracts.md#observation)) is the version of the model that produced it: `bioclip-2@<revision>+species_head@<version>` for a species candidate, `rfdetr-s-leaf-2026-10-18` for a leaf region.
- Every stored vector carries `embedding_model`: `bioclip-2@<revision>` on `scan_image`, `snowflake-arctic-embed-xs@d8c86521` on `memory` and `knowledge_chunk` ([database 5](../database/README.md#5-reads-indexes-and-vector-search)).

### Changing a model

| Change | What follows |
|---|---|
| A new detector, SAM or head version | A manifest change; the dev split is rerun before merging ([evaluation 9](evaluation.md#9-harness-and-reports)). Old observations keep their `model_version` |
| A new BioCLIP version | New label matrices, a new `label_set_version`, retrained heads, and the garden's image embeddings re-embedded from storage |
| A new text-embedding model | Every chunk and memory re-embedded, and the HNSW index redefined if the dimension changes ([database README](../database/README.md)). A migration, never a silent swap |
| A new VLM checkpoint or prompt | New `vlm_model` or `vlm_prompts`; the dev split rerun; every VLM worker changed together |
| A new disease head or label matrix | A new `classifier_labels_version` or `label_set_version`, deployed in the order of [the change-over](#changing-a-model-file-on-the-hosts) |

### Delivering model files to the backend

Three processes load model files, and each reads one manifest:

| Consumer | Where | Reads | Entries it verifies and loads |
|---|---|---|---|
| `api` | Control plane | `{MODELS_DIR}/manifest.toml`, `MODELS_DIR` default `/models` ([`config.rs`](../../backend/bins/api/src/config.rs), [`wiring.rs`](../../backend/bins/api/src/wiring.rs)) | `text_embedder`, `text_embedder_tokenizer`, `yunet` |
| Perception worker | Rig, RTX 3070 | `MODEL_MANIFEST`, default `/models/manifest.toml` ([worker `config.rs`](../../backend/bins/worker/src/config.rs)) | `leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, every `bioclip_labels*`, every `disease_head*`, `plant_probe` if listed |
| Fallback worker | Control plane | The same `MODEL_MANIFEST` ([`fallback.rs`](../../backend/bins/worker/src/fallback.rs)) | Every `bioclip_labels*` and `disease_head*`; with none listed it starts, but can't answer a `perception` job |

Each loader calls `Manifest::verify` on only the entries it loads, and `Manifest::load` doesn't check that other files exist. So **one manifest serves every host**, and each host's `/models` holds only the files its consumers load. One manifest also gives every host the same `perception_config`, which hashes every entry's name and `[model.config]` table.

#### The S1 manifest

`ml/models/manifest.toml` starts with the three entries `api` loads, in the format of [backend vision 5](../backend/vision.md#5-the-model-manifest). Each hash was recomputed from the file downloaded at the pinned revision or commit on 2026-10-16 (✓ ran). The two LFS files also match the hash their host publishes: the Hugging Face API's LFS object id for `onnx/model.onnx`, and the Git LFS pointer for YuNet (✓).

```toml
[[model]]
name    = "text_embedder"
version = "d8c86521"                     # stored as embedding_model = "snowflake-arctic-embed-xs@d8c86521"
file    = "text_embedder/snowflake-arctic-embed-xs-d8c86521.onnx"
sha256  = "cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c"
licence = "Apache-2.0"
source  = "Snowflake/snowflake-arctic-embed-xs at revision d8c86521100d3556476a063fc2342036d45c106f, onnx/model.onnx (fp32)"
runs_on = "api"

[[model]]
name    = "text_embedder_tokenizer"
version = "d8c86521"
file    = "text_embedder/snowflake-arctic-embed-xs-d8c86521.tokenizer.json"
sha256  = "91f1def9b9391fdabe028cd3f3fcc4efd34e5d1f08c3bf2de513ebb5911a1854"
licence = "Apache-2.0"
source  = "Snowflake/snowflake-arctic-embed-xs at revision d8c86521100d3556476a063fc2342036d45c106f, tokenizer.json"
runs_on = "api"

[[model]]
name    = "yunet"
version = "yunet-2026may@26cc381e"
file    = "yunet/face_detection_yunet_2026may.onnx"
sha256  = "ebafce4e3c118d6554634be5c27ab333b4c047a9a8c3faf1d7cf93101c22f0f0"
licence = "MIT"
source  = "opencv/opencv_zoo at commit 26cc381e4d2594bb9f47a26eb8fd96c94a13660d, models/face_detection_yunet/face_detection_yunet_2026may.onnx"
runs_on = "api"
```

| Entry | Bytes | Download URL | Hash published by the source |
|---|---|---|---|
| `text_embedder` | 90,387,631 | `https://huggingface.co/Snowflake/snowflake-arctic-embed-xs/resolve/d8c86521100d3556476a063fc2342036d45c106f/onnx/model.onnx` | Yes: the LFS object id ✓ |
| `text_embedder_tokenizer` | 711,649 | `https://huggingface.co/Snowflake/snowflake-arctic-embed-xs/resolve/d8c86521100d3556476a063fc2342036d45c106f/tokenizer.json` | No: a plain Git file, so the hash is our own, from the download ✓ ran, and it matches [models 10](models.md#10-text-embedder) |
| `yunet` | 229,738 | `https://media.githubusercontent.com/media/opencv/opencv_zoo/26cc381e4d2594bb9f47a26eb8fd96c94a13660d/models/face_detection_yunet/face_detection_yunet_2026may.onnx` | Yes: the Git LFS pointer's `oid` ✓. Commit `26cc381e` (May 22, 2026) is the one that added the dynamic-input file ✓ |

- **`text_embedder`'s version stays `d8c86521`.** `ArcticEmbedder::stored_name` writes `snowflake-arctic-embed-xs@{version}` beside every vector ([versions in results](#versions-in-results)). A different string would label the stored vectors as another model's.
- **No `[model.input]`, `[model.outputs]` or `[model.config]`** on these three. Their loaders don't read them: the tokenizer recipe and the face detector's passes and thresholds are `EmbedderConfig` and `FaceConfig` in `vision`, with the values of [models 9](models.md#9-face-detector) and [models 10](models.md#10-text-embedder). Tuning those values on the control plane therefore never changes `perception_config`.
- `d8c86521` is still the head of the Hub repository's `main` (last changed 2024-12-13 ✓). The repository has no `LICENSE` file; its README states Apache-2.0 ([models 10](models.md#10-text-embedder)).
- The ingestion scripts embed knowledge chunks with the same two files ([training 10](training.md#10-knowledge-ingestion-the-model-steps)), fetched with `fetch.py --for ingest`.

#### What `ml/` produces and when

Slices are those of the [backend roadmap](../backend/roadmap.md#3-the-slices-at-a-glance): S1 on Oct 16–17, S2 on Oct 18–19, S3 on Oct 18–21.

| Manifest entry | Format | Producer | Consumers | Needed by | Store |
|---|---|---|---|---|---|
| `text_embedder` | ONNX, fp32, 90 MB | Upstream, unchanged | `api`; the ingestion scripts | S1, Oct 16 | Upstream |
| `text_embedder_tokenizer` | `tokenizer.json`, 0.7 MB | Upstream, unchanged | `api`; the ingestion scripts | S1, Oct 16 | Upstream |
| `yunet` | ONNX, fp32, 230 KB | Upstream, unchanged | `api` | S1, Oct 16 | Upstream |
| `bioclip` | ONNX, fp16, 0.61 GB: BioCLIP 2's vision tower | `ml/training/export` ([training 7](training.md#7-export-to-onnx)); `version` is the 8-hex Hugging Face revision, since the worker reports `bioclip-2@{version}` | Perception | S2, Oct 18 | `pdm-models` (Hugging Face, private) |
| `sam_encoder`, `sam_decoder` | ONNX, fp16 (decoder logits fp32), 55 MB and 8 MB | `ml/training/export`, from SAM 2.1 Hiera-T | Perception | S2, Oct 18 | `pdm-models` (Hugging Face, private) |
| `leaf_detector` | ONNX, fp16 with `GridSample` in fp32, 57 MB | `ml/training/detector` (first fine-tune Oct 17), then `ml/training/export` | Perception | S2, Oct 18 | `pdm-models` (Hugging Face, private) |
| `bioclip_labels_<label_set_version>` | Label-matrix JSON, about 0.6 MB for 200 labels | `ml/training/labels/bioclip_text.py`, from the species and organism seed files ([training 8](training.md#8-label-lists)) | Perception, fallback | S2, Oct 18 | `pdm-models` (Hugging Face, private) |
| `disease_head_<classifier_labels_version>` | Head JSON, under 1 MB | `ml/training/heads` ([training 6](training.md#6-disease-and-species-heads)): v0 on Oct 17, v1 on Oct 19 | Perception, fallback | S2, Oct 18 (v0) | `pdm-models` (Hugging Face, private) |
| `plant_probe` | Probe JSON, 3 KB | `ml/training/heads` | Perception; the worker starts without it and uses the leaf score alone ([models 2](models.md#2-plant-present-check)) | Optional in S2; S4 for the plant check | `pdm-models` (Hugging Face, private) |
| `species_head` | Head JSON | `ml/training/heads` | None yet: the code doesn't load it | Only if the dev split shows it helps | `pdm-models` (Hugging Face, private) |
| Not in the manifest: `classifier_labels.yaml` | YAML seed file ([contracts 10](../architecture/contracts.md#10-knowledge-seed-format)) | `ml/training/labels`, in the same pull request as its head | The knowledge loader, then the database, then `api`'s perception payloads | S2, with head v0 | Git, `ml/knowledge/seed/` |

S2 needs a disease head and a label matrix, not only the four ONNX files. The API never queues a `perception` job naming a `label_set_version` or `classifier_labels_version` that no live worker reports ([contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)), so a worker with no head or matrix gets no real jobs.

**The JSON files** are specified in [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md), the format of record: `pdm-linear-head/1` for the disease heads, the species head and the plant probe, and `pdm-label-matrix/1` for the label matrices, each matrix one base64 string of little-endian `f32`, row-major. `ml/training` writes them, and the files PR #6 produced follow it.

**The compiled code reads that format** (since the backend's model-formats pull request): `perception/heads.rs` loads `pdm-label-matrix/1` and `pdm-linear-head/1`, scores species over the species rows only and keeps the organism rows unscored, needs `species_id` and `problem_id` on every disease-head label, and reads the plant probe as a one-class head. The pipeline refuses a head whose `features` isn't this manifest's `bioclip` export and a file whose version differs from its manifest entry's. `store` computes `label_set_version` with the same recipe and passes the test vector. `backend/crates/vision/tests/ml_files.rs` loads files written as `headfile.py` writes them, and the committed manifest.

- Ids are slugs (`^[a-z][a-z0-9_]{0,63}$`).
- The label-matrix and head entries carry the version twice: in the manifest (`label_set_version`, `classifier_labels_version`) and in the file. The worker keys what it loads by the file's value. `python -m ml.models.check` fails unless the two are equal, the entry's name ends in the same version, a head's `features` names the same manifest's `bioclip` file, and, once the seed has it, the head's `labels` equal `classifier_labels.yaml`'s rows in index order. It also checks that every entry of both manifests has a `sources.toml` row and a `NOTICES.md` section, and that one path never names two files.
- A head's temperature lives only in the head file, as `ml/models/FORMATS.md` and the manifest say. The entry doesn't repeat it, so the two can't drift, and `check.py` has no temperature check. A refitted temperature ships as a new head file with a new `classifier_labels_version`, and results already report that version, so the change stays visible.

A head entry, for the format:

```toml
[[model]]
name                      = "disease_head_disease-head-2026-10-17"
version                   = "disease-head-2026-10-17"
file                      = "heads/disease-head-2026-10-17.json"
sha256                    = "<64 hex>"
licence                   = "Ours; trained on CC BY data (section 9)"
source                    = "ml/training/heads at <git sha>, on BioCLIP 2 <revision> features"
runs_on                   = "perception"                 # the worker's CPU, beside the GPU models
classifier_labels_version = "disease-head-2026-10-17"
```

The entry has no `[model.config]`: the head's temperature is in the file. A label matrix's entry has the same fields, with `label_set_version` instead of `classifier_labels_version`.

#### Where the files live

| What | Where | Why |
|---|---|---|
| `manifest.toml`, `sources.toml`, `fetch.py`, `check.py`, `NOTICES.md`, `licences/` | Git, in `ml/models/` | Small, reviewed, and they pin everything else |
| Upstream files (arctic-embed-xs, YuNet; later the VLM checkpoint) | Their own source, at a pinned Hugging Face revision or Git commit | They are already hosted with a fixed revision; copying them adds nothing |
| Our files (every ONNX export, the heads, the label matrices, the probe) | A private **Hugging Face model repository**, `GreenLens/pdm-models`, under the team's Hugging Face organisation. Each file sits at its manifest `file` path, every upload is a commit, and `sources.toml` pins the 40-hex commit each file was uploaded in | See below |
| Model weights in GreenLens | Never: no weights in git, and no Git LFS | |

**Why a private Hugging Face repository.** Decided by the team on 2026-10-16.
- **The same path as every other model.** arctic-embed-xs and the VLM checkpoint already come from Hugging Face by pinned revision, so every file the hosts fetch is a `resolve/<revision>/<path>` download with a hash to check. A commit revision never changes, so a pinned URL always returns the same bytes.
- **Room and cost.** A free user or organisation gets 100 GB of private storage, and the Hub bills storage, not downloads ([Hugging Face, storage limits](https://huggingface.co/docs/hub/storage-limits) ✓ read 2026-10-16). Our files are about 1 GB; the largest is BioCLIP 2's tower at 0.61 GB, far under the 500 GB file limit.
- **Its own visibility.** GreenLens is private today (✓ checked with `gh api` on 2026-10-16) and becomes public for the submission. Keeping the weights out of it means making GreenLens public can't publish them by accident. They become public only when the team accepts the licence reading of [section 9](#9-licences-and-notices), item 2: then `pdm-models` is made public. The URLs and hashes don't change.
- **Rejected:**
  - *Git LFS in GreenLens.* A Free account gets 10 GiB of LFS download bandwidth a month, and LFS is switched off for the rest of the month when it runs out ([GitHub, Git LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs) ✓ read 2026-10-16). A dozen full rig pulls would reach it. The weights would also become public with the code.
  - *GitHub release assets.* No bandwidth limit, but the files would share GreenLens's visibility and be tied to release tags rather than commits.
  - *A RustFS bucket.* It would open the image store to the rig and tie model delivery to the control plane being up.

Before `pdm-models` existed, a GitHub prerelease of GreenLens held the files for a day so development wasn't blocked. It is deleted before GreenLens goes public ([dev-roadmap](dev-roadmap.md) task M21).

**`ml/models/sources.toml`** says where each file comes from, keyed by its hash, so the manifest stays exactly the struct Rust parses. Moving a file (private to public, or to another store) changes only this file, so it never changes `perception_config` or needs a restart.

```toml
[[file]]                                  # an upstream file
sha256 = "cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c"
url    = "https://huggingface.co/Snowflake/snowflake-arctic-embed-xs/resolve/d8c86521100d3556476a063fc2342036d45c106f/onnx/model.onnx"

[[file]]                                  # one of ours: same shape, a private repository
sha256 = "<64 hex>"
url    = "https://huggingface.co/GreenLens/pdm-models/resolve/<40-hex commit>/leaf_detector/rfdetr-s-leaf-2026-10-18.onnx"
auth   = "hf"                             # send HF_TOKEN
```

**Publishing one of our files.** The producing script writes the file, its SHA-256 and the `[model.check]` values. The author runs `python -m ml.models.publish` (logged in with `hf auth login` and write access to the organisation): it uploads every file the manifests name that has no `sources.toml` row, in one commit at each file's manifest path, refuses a path already in the repository, and appends the rows pinned to that commit's 40-hex id. Then one GreenLens pull request adds the manifest entry, the `sources.toml` row, the `NOTICES.md` change and, for a head, `classifier_labels.yaml`. A path is never overwritten: a new file gets a new name, so a revision-pinned URL and the file at the head of `main` always agree.

#### How files reach `/models`

On each host, `/models` is a Docker named volume, `models`. The consumers mount it read-only (`models:/models:ro`, as in [worker 12](../backend/worker.md#12-deployment)). Only a one-shot fetch step mounts it read-write. The compose services and the image that runs the step are in the infrastructure document, `docs/infra`; this section is what the step does.

`fetch.py` uses only Python 3.12's standard library (`tomllib`, `hashlib`, `urllib`), so it runs in a `python:3.12-slim` container or on a dev machine. `--manifest` and `--sources` default to the files in `ml/models/`.

✓ Ran on a macOS arm64 laptop, 2026-10-17 (S1, items M1 and M2): `python3 -m ml.models.fetch --for api` into an empty directory fetched the three S1 files with matching hashes, a second run downloaded nothing, and a locally corrupted file was fetched again. With one hex digit of yunet's expected hash changed (in copies of `manifest.toml` and `sources.toml`), the fetch retried once, exited 2, kept no yunet file and wrote no manifest. `api` then started in about 5 s with `MODELS_DIR` at that directory and checked the manifest hashes, and an upload of OpenCV's sample portrait (`samples/data/lena.jpg`) was stored with its face found and blurred. The embedder's golden test isn't written yet ([models 10](models.md#10-text-embedder)): it needs golden values from the Python ingestion side. Not run: the BioCLIP label files are in the private `GreenLens/pdm-models` repository and need an `HF_TOKEN`, so M3 and M4 (the token check on each host and the deployed fetch step) are still open and the fallback worker waits for them.

```text
python3 -m ml.models.fetch --for api,fallback --dest /models          # control plane
python3 -m ml.models.fetch --for perception --dest /models            # rig
python3 -m ml.models.fetch --for ingest --dest ~/.cache/pdm-models    # a dev machine running ingestion
python3 -m ml.models.fetch --for perception --cpu --dest ~/.cache/pdm-models   # a laptop, ORT_EP=cpu
```

`--cpu` reads `manifest.cpu.toml`, generated by `python -m ml.models.cpu_manifest`: each fp16 ONNX file swapped for its fp32 export, which ONNX Runtime's CPU provider needs, and each head swapped for a CPU twin, `<stem>.cpu.json`, identical but for a `features` that names the fp32 BioCLIP file, since the worker refuses a head trained on another export (dev-roadmap gap G10). It is written to the destination as `manifest.toml` like any other. `fetch.py` refuses a destination that isn't empty and wasn't made by it (it leaves a `.pdm-models` marker), so step 5 never deletes files elsewhere.

| Group | Entries |
|---|---|
| `api` | `text_embedder`, `text_embedder_tokenizer`, `yunet` |
| `fallback` | every `bioclip_labels*`, every `disease_head*` |
| `perception` | `leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, every `bioclip_labels*`, every `disease_head*`, `plant_probe` |
| `ingest` | `text_embedder`, `text_embedder_tokenizer` |
| `vlm` (S3) | Not manifest entries: the VLM checkpoint's files at its pinned revision, checked in the sidecar's `hf-cache` volume (roadmap item M10) |

The groups follow the entry names the code reads ([backend vision 5](../backend/vision.md#5-the-model-manifest)); `fetch.py` holds this table and must change with that list.

1. Parse the manifest and the sources. Every entry of the chosen groups must have a source row, or the step stops before downloading anything.
2. For each entry: if `/models/<file>` exists and its SHA-256 matches, keep it. Otherwise download it to `/models/.partial/<sha256>`, hashing while it streams, then rename it to `/models/<file>`. Rows with `auth = "hf"` are fetched with `Authorization: Bearer $HF_TOKEN`, a fine-grained Hugging Face token with read access to `pdm-models` only; it lives in the host's env file and is never committed. Upstream files need no token.
3. Copy `NOTICES.md` and `licences/` into `/models`.
4. Write the manifest as `/models/manifest.toml.new` and rename it over `/models/manifest.toml`, keeping the old one as `/models/manifest.previous.toml`. The manifest moves last, so a consumer never reads a manifest whose files aren't in place.
5. Delete files that neither manifest names.

Re-running the step downloads nothing when nothing changed. It re-hashes what is there, about 1 GB on the rig, which takes seconds.

**When a hash doesn't match.**
- During a download, the step deletes the partial file and tries once more. A second mismatch stops it with exit code 2, naming the entry, the expected hash and the actual one. A download error exits with 3, and a bad manifest or a missing source row with 1.
- On any failure, `/models/manifest.toml` and its files stay as they were. The consumers start only after the step succeeds (compose's `service_completed_successfully`), so a failed fetch means a new manifest is not deployed. Already-running consumers keep their loaded models.
- If a file in the volume changes later, the consumer's own `Manifest::verify` refuses to start with `HashMismatch` naming the entry. The fix is to re-run the step, never to edit the hash.
- A mismatch on an upstream file means the source changed under a pinned revision, or the download was tampered with. Don't update the hash to make it pass: find out why, and pin a new revision through a reviewed pull request.

#### Changing a model file on the hosts

- **Every change is a manifest change in a pull request.** A new file gets a new `version` and a new file name, so the old and new files can sit side by side in `/models`. Consumers read the manifest only at start, so the step is always: merge, run the fetch step on each host that needs the file, then restart its consumers.
- **An ONNX model** (`leaf_detector`, `sam_*`, `bioclip`): fetch on the rig, restart the perception worker. A new `bioclip` revision also needs new label matrices and heads ([changing a model](#changing-a-model)).
- **A face detector:** fetch on the control plane, restart `api`.
- **The text embedder:** a migration that re-embeds every chunk and memory ([changing a model](#changing-a-model)), never a fetch and restart alone.
- **A disease head or label matrix**, a new `classifier_labels_version` or `label_set_version`:
  1. Add the new entry beside the old one. Fetch on the rig and the control plane, and restart the perception and fallback workers. Their heartbeats now report both versions, comma-separated.
  2. Load the seed that names the new version (the `seed-upload` step and the loader, [contracts 10](../architecture/contracts.md#10-knowledge-seed-format)), then restart `api`. New payloads name the new version.
  3. Once no queued or leased job names the old version, remove its entry in a later pull request and fetch again; step 5 of the fetch deletes the file.

  Reversing steps 1 and 2 would leave jobs no worker can take: the API doesn't queue them ([contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)).

#### Licence notices with the files

- `ml/models/NOTICES.md` ([section 9](#9-licences-and-notices)) has one section per manifest entry: the licence, the authors, the required citation and the training-data caveats.
- `ml/models/licences/` holds the full texts: `Apache-2.0.txt` from SPDX for arctic-embed-xs, whose repository has no licence file, and YuNet's `LICENSE` copied verbatim from commit `26cc381e` (MIT, "Copyright (c) 2020 Shiqi Yu" ✓), and one file per further upstream licence.
- The fetch step copies both into every `/models`, so the files never travel without their notices. `check.py` fails if an entry's `licence` has no section in `NOTICES.md`.

#### Roadmap

| # | Slice | Work | Exit check |
|---|---|---|---|
| M1 ✓ | S1, Oct 16 | `ml/models/` with the S1 `manifest.toml` above, `sources.toml`, `fetch.py`, `licences/` and a first `NOTICES.md` for the three files | `fetch.py --for api` into an empty directory downloads three files with matching hashes; a second run downloads nothing; one changed byte makes it exit 2 and leaves the directory's manifest unchanged. ✓ Ran on a macOS arm64 laptop, 2026-10-17: `python3 -m ml.models.fetch --for api` into an empty directory fetched the three files with matching hashes, a second run downloaded nothing, and a locally corrupted file was fetched again. With one hex digit of yunet's expected hash changed, the fetch exited 2, kept no yunet file and wrote no manifest |
| M2 (partial) | S1, Oct 16 | `api` started locally with `MODELS_DIR` at that directory | `api` starts; the embedder's golden test passes ([models 10](models.md#10-text-embedder)); `stored_name()` returns `snowflake-arctic-embed-xs@d8c86521`; a test photo's faces are found. ✓ Ran on a laptop, 2026-10-17: `api` (release, `--features heic`) started in about 5 s with `MODELS_DIR` at a directory the fetch had filled, ONNX Runtime 1.31.0 on the CPU (osx-arm64), and checked the manifest hashes; ort 2.0.0-rc.14 logs that it was designed for ORT 1.29. An upload of OpenCV's sample portrait was stored with its face found and blurred, checked by eye. `stored_name()` is `snowflake-arctic-embed-xs@` plus the manifest's `version`, `d8c86521` (read in the code, not run). Still open: the golden test, which isn't written yet; its values come from the Python ingestion side (S2) |
| M3 | S1, Oct 16 | The private Hugging Face repository `GreenLens/pdm-models` under the team's organisation, and two fine-grained read-only tokens, one for the control plane and one for the rig | Each host downloads a test file at a pinned revision with its own token, and gets `403` on an upload |
| M4 | S1, Oct 17 | The control plane's fetch step (`--for api,fallback`) before `api` and `fallback-worker`, on the `models` volume | The deployed `api` starts with the hashes checked; a photo with a face is blurred as in [architecture 11](../architecture/README.md#11-security-privacy-and-safety) (the roadmap's S1 exit check) |
| M5 | S2, Oct 17–18 | The four ONNX exports and their `[model.check]` values, uploaded to `pdm-models` with their pinned commits, entries and source rows | `ml/training/export/check.py` passes for each; `fetch.py --for perception` verifies all four |
| M6 | S2, Oct 18 | The first label matrix, disease head v0, `classifier_labels.yaml` in the seed, and `check.py` | `check.py` passes: versions agree between manifest, file and loader, and the head's labels equal the YAML's rows |
| M7 | S2, Oct 18 | The rig's fetch step (`--for perception`) before the perception worker | The worker starts; its heartbeat reports every entry's `version`, `perception_config`, `label_set_version` and `classifier_labels_version`; a real leaf photo gives the species top 3, leaf regions and masks (the roadmap's S2 exit check) |
| M8 | S2, Oct 18 | The label matrix and head on the control plane for the fallback | The fallback's heartbeat reports the same `label_set_version` and `classifier_labels_version` as the perception worker |
| M9 | S2, Oct 19 | Disease head v1 through the change-over above | During the change-over both versions are reported; no job fails with `unknown_label_version` |
| M10 | S3, Oct 18–21 | The VLM checkpoint: downloaded at its pinned revision into each sidecar's `hf-cache` volume before the sidecar starts, then served with `HF_HUB_OFFLINE=1`. `fetch.py --for vlm` reads the checkpoint's file list and LFS hashes at the revision from the Hugging Face API and checks the cache's blobs, which the Hub's cache names by hash *unverified* | A sidecar restarts with the rig's internet cut; both VLM cards report the same `vlm_model` |
| M11 | S8, Oct 25 | `NOTICES.md` final for every shipped file | `check.py` passes; the app's About screen links to it |

---

## 7. Label lists and the knowledge base

The models and the knowledge base can't drift, because the loader generates or checks every label list from the seed ([contracts 10](../architecture/contracts.md#checks-and-generated-files)):

- **The BioCLIP label matrix**: every species' and organism's `bioclip_label`, embedded offline with BioCLIP 2's text tower into an `N × 768` matrix under the loader's `label_set_version` ([training 8](training.md#8-label-lists)).
- **`classifier_labels.yaml`**: written by training into the seed directory from the disease head's classes, then validated and loaded by the loader like any seed file, which checks it against each species' `classifier_supported` ([training 8](training.md#8-label-lists)).
- **`species_options` and `organism_options`**: the enums of the VLM's schema, generated from the same seed ([contracts 5](../architecture/contracts.md#payloads)).

Each perception payload names the `label_set_version` and `classifier_labels_version` it expects. The worker loads the versions its manifest lists (the current one, and the previous one during a change-over) and fails a job that names another with a non-retryable error ([section 12](#12-findings-for-the-architecture-and-contracts), finding 23).

---

## 8. The fallback path

The fallback worker covers every job kind with a hosted model: any job left unclaimed for 15 seconds, so both when the rig is offline and when every slot of a capability is busy ([architecture 8.4](../architecture/README.md#84-job-leasing-and-fallback)):

| Stage | Fallback |
|---|---|
| Plant check, species, disease | The hosted model, with the same schemas restricted to the payload's options; confidences from certainty words ([models 7](models.md#fallback-worker)) |
| Leaf regions | One whole-image `leaf` region per close-up that shows leaves, no masks ([contracts 5](../architecture/contracts.md#results)) |
| Image embeddings | None |
| VLM calls | The same prompts and schemas, with the schema keywords the hosted API rejects stripped and enforced after decoding ([vlm.md 12](vlm.md#12-the-fallback-worker)) |
| Measurement | Only plant height and canopy width, since there are no masks ([contracts 7](../architecture/contracts.md#measurement)) |

The evaluation scores the fallback path separately ([evaluation 8](evaluation.md#fallback-worker)).

---

## 9. Licences and notices

### What ships

| Component | Licence | Notes |
|---|---|---|
| RF-DETR-S weights and code | Apache-2.0 ✓ | Only the N, S, M and L sizes; the others are PML 1.0 ✓ |
| SAM 2.1 Hiera-T | Apache-2.0 ✓ | |
| MobileSAM (if used) | Apache-2.0 ✓ | |
| BioCLIP 2 | MIT ✓ | Trained on TreeOfLife-200M, tagged CC0 as a compilation ✓. Its predecessor TreeOfLife-10M states that its images keep their own terms, some CC BY-NC-SA ✓; the same likely holds for 200M *unverified* |
| Qwen3-VL-8B-Instruct, Qwen3.5-9B | Apache-2.0 ✓ | Community quantizations inherit the base licence; checked per checkpoint |
| YuNet | MIT ✓ | Trained on WIDER FACE, CC BY-NC-ND 4.0 ✓ |
| snowflake-arctic-embed-xs | Apache-2.0 (README) ✓ | The Hub's licence tag is empty ✓ |
| Our disease and species heads, detector fine-tune | Trained on PlantDoc (CC BY 4.0), TOM2024, Tomato-Pakistan (CC BY 4.0), iBean (MIT), CC0 / CC BY iNaturalist photos, and PlantVillage (CC BY-SA 3.0) at a low weight | Attribution for each in the notices |

### Excluded

Ultralytics YOLO and YOLOv10 (AGPL-3.0), YOLOv9 (GPL-3.0), DEIMv2 (non-commercial), EdgeSAM (non-commercial), SegFormer (NVIDIA non-commercial), SCRFD's weights (non-commercial), YOLO-face variants (GPL or AGPL), DINOv3 (custom licence with a mandatory notice), ConvNeXt V2 FCMAE weights (CC BY-NC), jina-embeddings-v3 (CC BY-NC), PlantWild and Agri-Foundation-145k (NC-ND), and PlantSeg until its conflicting licences are settled ([training 3](training.md#3-datasets-and-licences)). All ✓ from their licence files or cards.

### For the team to decide

1. **The project's own licence.** The repository has no `LICENSE` file yet ✓. The shipped models' own licences (Apache-2.0 and MIT) allow an Apache-2.0 or MIT project *unverified* legal reading; items 2 and 3 are the open questions.
2. **Weights trained on restricted data.** YuNet (WIDER FACE, NC-ND), BioCLIP 2 (likely some NC images in TreeOfLife), our heads (PlantVillage's CC BY-SA 3.0, whose ShareAlike may or may not reach weights) and the optional UperNet regions model (ADE20K, non-commercial research) all carry MIT weights from their authors. Whether a dataset's terms reach a trained model is unsettled; every face detector shares the WIDER FACE question. The notices state it plainly.
3. **PlantDoc's and PlantSeg's scraped images.** CC BY 4.0 covers the annotations; the photos' owners keep their copyright *unverified*. Fine for training; the images are never redistributed.
4. **Kaggle terms** for Plant Pathology 2020/2021, before any of its images are used.

`ml/models/NOTICES.md` lists every model and dataset with its licence, its authors' required citation and these caveats, and the app's About screen links to it.

---

## 10. Build plan

The vision work, mapped onto [architecture 14](../architecture/README.md#14-build-plan). The shared plan with the integration track is [dev-roadmap.md](dev-roadmap.md), and model progress is tracked in [roadmap.md](roadmap.md):

| Dates | Vision and ML |
|---|---|
| Oct 16–17 | `ml/models/` with the S1 manifest, `fetch.py` and the `pdm-models` repository, then the control plane's fetch step (roadmap items M1–M4 of [section 6](#roadmap)). Day-1 checks on all three cards ([evaluation 7](evaluation.md#7-day-1-checks)): ORT 1.31 CUDA on the 3070 and 5070 Ti, vLLM on the 5070 Ti, vLLM ROCm on the 9070 XT. Text embedder pinned. Datasets fetched and deduplicated. BioCLIP 2, SAM 2.1-T and YuNet exported and checked. **5070 Ti:** RF-DETR-S first fine-tune on Oct 17 (1–2 hours), then vLLM back up for prompt work. Disease head v0 (CPU). Evaluation photos start (Oct 16–23). Ask a local extension office about confirming cases |
| Oct 18–19 | The four ONNX exports, the first label matrix and disease head v0 published to `pdm-models`, fetched on the rig and the control plane (M5–M8); head v1 by the change-over on Oct 19 (M9). Leaf relabelling pass (people, Oct 18). The BioCLIP label matrix from the species and organism seed files, which need only names, not reviewed problem profiles. The VLM prompts and schemas on real photos. **The benchmark the contracts call "day 2"** runs on Oct 18, once the models are on the rig: both VLM candidates on both VLM cards, and the perception batch caps ([evaluation 8](evaluation.md#8-day-2-benchmarks)). **5070 Ti, night of Oct 18:** second detector run with vLLM stopped, while the 9070 XT serves. Oct 19: crops from our detector; disease and species heads v1. Knowledge extraction and discrimination running |
| Oct 20–21 | VLM chosen; `VLM_SLOTS` per card; `VLM_MAX_PIXELS`. Heads retrained as profiles are reviewed. Fallback worker checked with one live box call. Spot colour thresholds from hand-drawn lesions |
| Oct 22–23 | Calibration on the dev split: certainty values, temperatures, BioCLIP agreement, the diagnosis margin, the measurement thresholds ([evaluation 5–6](evaluation.md#5-calibration)). Final manifests |
| Oct 24 | Test split scored once; numbers for Devpost |
| Oct 25 | Model notices and the Devpost write-up |

---

## 11. Open decisions

1. **Which VLM:** Qwen3-VL-8B FP8 or Qwen3.5-9B W4A16, from the day-2 benchmark (architecture open decision 4; [vlm.md 2](vlm.md#2-model-choice)).
2. **The 9070 XT's role and stack:** a second VLM worker (recommended) on vLLM ROCm, or llama.cpp Vulkan if vLLM fails the day-1 check; and FP8 or W4A16 on both VLM cards (section 4).
3. **SAM 2.1-T or MobileSAM**, and BioCLIP 2 or BioCLIP, from VRAM on the 3070 (section 4).
4. **The disease head or ConvNeXt-Tiny**, if the head stays below 75% top-1 ([models 7](models.md#7-disease-classifier)).
5. **The species head**: on only if it improves the dev split ([models 6](models.md#a-trained-species-head-for-the-supported-crops)).
6. **`VLM_MAX_PIXELS`, `VLM_SLOTS`, the perception batch caps** (contracts open questions 1 and 9).
7. **Text embedder:** arctic-embed-xs (recommended) or arctic-embed-s, both 384 dimensions, after 50 real question–chunk pairs; and whether French is needed, which would mean a 768-dimensional multilingual model chosen before ingestion (architecture open decision 13; [models 10](models.md#10-text-embedder)).
8. **The licence questions** of section 9.
9. **Soil and surroundings regions:** only if a stage needs them and the ADE20K question is accepted ([models 5](models.md#5-plant-soil-and-surroundings-regions)).
10. **The calibration values** (contracts open questions 2 and 3), on Oct 22–23.
11. **Settled (2026-10-16): our model files live in a private Hugging Face model repository, `GreenLens/pdm-models`,** in the team's Hugging Face organisation, which Sam created. Members with write access upload with `publish.py`; everyone else only reads.
12. **How a deploy carries `ml/models/` to the fetch step:** an image built from the commit, or a checkout on the host. Shared with the infrastructure document (`docs/infra`); either way the fetched manifest must be the deployed commit's.
13. **Settled: organisms in the label matrix.** The matrix keeps every organism row ([`FORMATS.md`](../../ml/models/FORMATS.md) gives each label its `table`), and the worker scores species over the species rows only, keeping the organism rows unscored until the organism second opinion ([models 11](models.md#11-organism-second-opinion-stretch)) uses them.
14. **Settled: token holders.** Each teammate makes their own fine-grained read-only token for `pdm-models` as an organisation member; Sam makes one for the control plane and one for the rig and puts them in the hosts' `.env.models` files. No token is committed. Still open: whether the host tokens get an expiry, so a fetch doesn't fail on demo day.
15. **The VLM cache check** (item M10): whether the Hugging Face cache keeps every LFS file as a blob named by its SHA-256 at the pinned `huggingface_hub` version, which `fetch.py --for vlm` relies on.

---

## 12. Findings for the architecture and contracts

These are points where the existing documents are wrong, out of date or ambiguous given the choices above. **This branch changes none of `contracts.md`, the architecture or `schema.surql`**: they are proposals for the team, to apply in one pull request as the source-of-truth rules require ([AGENTS.md](../../AGENTS.md#sources-of-truth)).

**Where they stand (2026-10-16).** The backend design, now on `main`, took up all but three of them: contracts v0.10.2, architecture v0.7.2 and database v1.4.2. Still open:

- 12 is [contracts open question 36](../architecture/contracts.md#12-open-questions).
- 15 is part of contracts open question 2.
- 13 isn't in contracts 12. It stays with [vlm.md 14](vlm.md#14-open-questions), question 2, to propose after the day-2 benchmark.

| # | Where | What it says | Proposed |
|---|---|---|---|
| 1 | Architecture 10, day-1 check | The 5070 Ti "needs CUDA 12.8+ builds of PyTorch, vLLM and ONNX Runtime" | ONNX Runtime 1.31 (`ort` rc.14) ships CUDA 13 builds only ✓; PyTorch 2.14 cu130 and vLLM 0.31 default to CUDA 13 ✓. The rig needs driver 580.65.06 or later (section 5) |
| 2 | Contracts 6, image preparation | Each side "rounded to a multiple of 28", vLLM's `min_pixels` and `max_pixels` | That is Qwen2.5-VL. Qwen3-VL and Qwen3.5 use 32-pixel cells and the processor's `size.longest_edge` ✓. The cell size and the server's pixel keyword are properties of the model adapter, next to `box_format` ([vlm.md 5](vlm.md#5-image-preparation-and-tokens)) |
| 3 | Contracts 6, box formats | `abs_px_prepared` for "Qwen2.5-VL on vLLM"; `norm_1000` is "divide by 1000" | The chosen models are `norm_1000` ✓, which needs no new format. State that `bbox_2d` is corners `[x1, y1, x2, y2]` and the worker converts to `[x, y, w, h]` for every format, and that `box_format` belongs to the adapter instance (model, server and preparation) |
| 4 | Contracts 6, box-format table and after decoding | `norm_1000` is "used by hosted LLMs on the fallback worker", which requests `norm_1000` boxes | Claude's documentation says it returns pixel coordinates of the image as it sees it after its own resize ✓. Send images it won't resize and use `abs_px_prepared`; settle with one live call on day 2 ([vlm.md 12](vlm.md#12-the-fallback-worker)) |
| 5 | Contracts 6, per-job schema and after decoding | "vLLM's structured-output (guided decoding) option"; `max_tokens` | `guided_json` was removed in vLLM 0.12 ✓: the option is `response_format` with a JSON schema (or `structured_outputs`), and vLLM 0.31 uses `max_completion_tokens` ✓. Add that xgrammar's length-limited strings can't contain quotes, backslashes or line breaks ✓, and that `pattern` can't be combined with `maxLength` ✓ |
| 6 | Contracts 12, open question 1 | Whether vLLM enforces `maxLength` and `maxItems` | xgrammar 0.2.7, pinned by vLLM 0.31, enforces both and integer bounds, from its source ✓. The day-2 check confirms it in practice; the worker's truncation stays |
| 7 | Contracts 5, batching | "the SAM-class model over every leaf box of an image in one prompt" | Several boxes in one SAM prompt produce one merged mask ✓. Say "in one decoder call, one prompt per box" ([models 4](models.md#4-mask-model)) |
| 8 | Contracts 5, batching; architecture 10 | One `PERCEPTION_MAX_BATCH` (16) for every model | The SAM encoder needs about 1.4 GB per image in fp32 ✓ ran, so it needs its own cap (about 2) and the decoder a box cap (16). Per-model caps as deployment configuration below the job-wide one |
| 9 | Architecture 3 (training row), 6.1 and 10; contracts 5 | "YOLO detector + SAM-class model" and "the YOLO leaf detector" (architecture 6.1, 10); "fine-tunes the disease classifier" (architecture 3, 10); a "classifier fine-tuned on PlantDoc-style field images" (contracts 5); the 3070 models "need roughly 4 GB" (architecture 10) | The detector is RF-DETR-S, because Ultralytics YOLO is AGPL-3.0 and Ultralytics states its trained models are too ✓. The disease classifier is a head on BioCLIP 2 features of each leaf crop; on BioCLIP v1 features it already matched published full fine-tunes on PlantDoc ✓ ran, and the BioCLIP 2 head is measured on day 2. About 5 GB with activations *estimate* |
| 10 | Architecture 10 and open decision 13; database open question 1 | Text embeddings "`fastembed`-style"; the model and dimension open; `DIMENSION 384` a placeholder | arctic-embed-xs, CLS pooling, 384 dimensions, fp32. The placeholder becomes the real value; `fastembed` is out because it pins `ort` rc.13 ✓ ([models 10](models.md#10-text-embedder)) |
| 11 | Database open question 2; `schema.surql` comment on `scan_image.embedding` | The image-embedding model and dimension are fixed on day 1 | BioCLIP 2's 768-dimensional embedding. The field has no fixed length, so the schema needs only its comment updated; `embedding_model` already exists ✓ |
| 12 | Contracts 5, `VlmPayload.organism_options` | Only `id` and `name` | Optionally add each organism's `recognition` line, so the findings prompt can describe every option and not only the beneficials it lists itself ([vlm.md 7](vlm.md#7-prompts)) |
| 13 | Contracts 5, photo routing | The indicator call's images are chosen by score; their send order isn't fixed | Send them in the findings order when possible, so vLLM's prefix cache reuses more of the findings call. A proposal after the day-2 benchmark ([vlm.md 14](vlm.md#14-open-questions)) |
| 14 | Contracts 5, slots table; contracts 12, open questions 1 and 9; architecture 2, 3, 8.4, 10, 12, 15 | One vlm-worker on the 5070 Ti | Each vlm-worker (5070 Ti and 9070 XT) has its own `VLM_SLOTS` from its own benchmark; every vlm-worker serves the same checkpoint; the `model_versions` keys of [vlm.md 11](vlm.md#11-validation-retries-and-versions); a third card in the diagram, components, compose files and open decisions. docs/backend covers the worker configuration and compose services |
| 15 | Contracts 7, measurement confidence | Measurements map to "the VLM's values" 0.4 / 0.65 / 0.85 | If calibration moves the VLM's values, the measurements' move too. Decide whether that coupling is intended ([evaluation 5](evaluation.md#5-calibration)) |
| 16 | Contracts 10 and architecture 5 | Chunk embeddings "computed at load time" (contracts) and "embedded in batches" by the ingestion scripts (architecture) | Say which program embeds them: the Python loader, with the same ONNX file, when it generates the migration ([training 10](training.md#embedding-step-3)) |
| 17 | Architecture 6.2, step 3 | "If both agree with a high score" | Define it: BioCLIP's top-1 in the high band and the VLM naming the same species with medium or high certainty, with the threshold raised from the evaluation if needed ([evaluation 5](evaluation.md#bioclip)) |
| 18 | Architecture 6.5 | "Qwen2.5-VL-class models do this grounding natively" | True of the chosen Qwen3-VL and Qwen3.5 too; small insects are about one 32-pixel token at 1 megapixel, so recall on tiny pests is the risk to measure, with a detail crop of the leaf underside as the lever ([vlm.md 5](vlm.md#small-organisms)) |
| 19 | Contracts 6, after decoding; contracts 12, open question 2 | The mapping exists "so a VLM finding can never show more than high (0.85)"; calibrate "on Oct 24" | Calibration keeps each word inside its band and never above 0.85; if the dev split shows a word belongs in another band, that is reported, not applied, since it changes what the app shows. Calibration runs on the dev split on Oct 22–23, and Oct 24 is the single test-split scoring ([evaluation 5](evaluation.md#5-calibration)) |
| 20 | Contracts 10, `SpeciesSeed` and `OrganismSeed` | `bioclip_label`: "the text prompt BioCLIP scores against" | Define it as the class text that pybioclip's 80 templates wrap ("a photo of {label}."), written in the seed as `<scientific name> with common name <common name>` (`Solanum lycopersicum with common name tomato`; `Coccinellidae larva with common name ladybug larva` for a life stage) ([training 8](training.md#8-label-lists)) |
| 21 | Contracts 10, files and generated files; `schema.surql` `classifier_label` | `classifier_labels.yaml` is a seed file; the loader generates `label_set_version` and `classifier_labels_version` | Agreed, and stated as a flow: training writes `classifier_labels.yaml` into `ml/knowledge/seed/` in the same pull request as the head; the loader validates it, loads the `classifier_label` rows keyed by `[model_version, index]`, and uses `model_version` as `classifier_labels_version`. To compute `label_set_version`, the loader also needs the BioCLIP revision and the template list, and it runs the text tower (or reads the matrix training produced) ([training 8](training.md#8-label-lists)) |
| 22 | Architecture 13; AGENTS.md planned layout | `ml/training`, `ml/knowledge`; `docs/` without `vision/` | Add `ml/pdm_vision/` (shared pre-processing), `ml/models/` (the manifest and notices), `ml/training/eval/`, and `docs/vision/` ([training 1](training.md#1-layout)). This branch adds `docs/vision` to AGENTS.md's document index only |
| 23 | Contracts 1, versioning; contracts 2 | Only `unsupported_version` exists for a payload the worker can't handle | Add an error code, for example `unknown_label_version`, non-retryable, for a payload whose `label_set_version` or `classifier_labels_version` the worker hasn't loaded. As a run job's non-retryable failure it would fail the scan, so the API should never queue a version no live worker reports in its heartbeat `versions` |
| 24 | Contracts 6, after decoding | The hosted structured-output feature "ignores" `maxLength` and `maxItems` | It rejects them with a 400, and `minimum` and `maximum` too ✓. The fallback adapter strips them, then truncates strings and clamps numbers and boxes after decoding ([vlm.md 12](vlm.md#12-the-fallback-worker)) |
| 25 | Architecture 6.1; contracts 5 | "Whether a plant is present comes from segmentation"; the plant check runs "over every image" | The check is the leaf detector's score **or** a plant probe on the BioCLIP 2 embedding, over the plant shots only, so a soil close-up never counts against it and an unusual plant isn't failed for lacking PlantDoc-like leaves ([models 2](models.md#2-plant-present-check)) |
| 26 | Architecture 6.1; contracts 5, batching | The disease classifier runs on "each leaf region"; BioCLIP and the image-embedding model run "over every plant image" | The classifier runs on `leaf_closeup` and `leaf_underside` regions only, since whole-plant leaves are too small at 224 pixels. BioCLIP 2 makes two passes: every plant and soil image (soil is embedded for similar cases but left out of species scoring), then the leaf crops ([models 6–8](models.md#6-species-bioclip)) |
| 27 | Contracts 6, the fallback's token budget (backend branch) | 8,000 output tokens per call, "findings and indicators alike" | The guided check on the fallback needs the same budget: its 100 tokens leave no room for the thinking the hosted model always does ([vlm.md 12](vlm.md#12-the-fallback-worker)) |

---

## 13. Sources

Read on 2026-10-15.

**Models**
- RF-DETR: [repository and README](https://github.com/roboflow/rf-detr), [ONNX export](https://raw.githubusercontent.com/roboflow/rf-detr/develop/docs/exports/onnx.md), [dataset formats](https://raw.githubusercontent.com/roboflow/rf-detr/develop/docs/learn/train/dataset-formats.md)
- D-FINE: [repository](https://github.com/Peterande/D-FINE); Ultralytics: [licence](https://www.ultralytics.com/license), [LICENSE](https://raw.githubusercontent.com/ultralytics/ultralytics/main/LICENSE)
- SAM 2: [repository](https://github.com/facebookresearch/sam2); MobileSAM: [repository](https://github.com/ChaoningZhang/MobileSAM); SAM 3: [LICENSE](https://raw.githubusercontent.com/facebookresearch/sam3/main/LICENSE); EdgeSAM: [LICENSE](https://raw.githubusercontent.com/chongzhou96/EdgeSAM/master/LICENSE)
- BioCLIP: [card](https://huggingface.co/imageomics/bioclip), [paper](https://arxiv.org/html/2311.18803); BioCLIP 2: [card](https://huggingface.co/imageomics/bioclip-2), [paper](https://arxiv.org/html/2505.23883); [pybioclip](https://github.com/Imageomics/pybioclip/blob/main/src/bioclip/predict.py)
- Qwen3-VL: [repository](https://github.com/QwenLM/Qwen3-VL), [grounding cookbook](https://github.com/QwenLM/Qwen3-VL/blob/main/cookbooks/2d_grounding.ipynb), [technical report](https://arxiv.org/html/2511.21631), [FP8 checkpoint](https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct-FP8); Qwen3.5: [repository](https://github.com/QwenLM/Qwen3.5), [card](https://huggingface.co/Qwen/Qwen3.5-9B)
- YuNet: [OpenCV Zoo](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet), [libfacedetection](https://github.com/ShiqiYu/libfacedetection); WIDER FACE: [card](https://huggingface.co/datasets/CUHK-CSE/wider_face)
- snowflake-arctic-embed-xs: [card](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs)
- UperNet ConvNeXt: [card](https://huggingface.co/openmmlab/upernet-convnext-tiny); ADE20K: [terms](https://ade20k.csail.mit.edu/terms/)

**Datasets**
- PlantDoc: [classification](https://github.com/pratikkayal/PlantDoc-Dataset), [detection](https://github.com/pratikkayal/PlantDoc-Object-Detection-Dataset), [paper](https://arxiv.org/abs/1911.10317), [Roboflow's fixes](https://roboflow.com/blog/introducing-an-improved-plantdoc-dataset-for-plant-disease-object-detection)
- [PlantVillage](https://huggingface.co/datasets/mohanty/PlantVillage), [TOM2024](https://data.mendeley.com/datasets/3d4yg89rtr/1), [Tomato Pakistan](https://data.mendeley.com/datasets/3mbnb82mxd), [iBean](https://github.com/AI-Lab-Makerere/ibean), [DiaMOS](https://zenodo.org/records/5557313), [PlantWild](https://tqwei05.github.io/PlantWild), PlantSeg ([13958858](https://zenodo.org/records/13958858), [14935094](https://zenodo.org/records/14935094)), [PhenoBench](https://www.phenobench.org/dataset.html), [iNaturalist licences](https://help.inaturalist.org/en/support/solutions/articles/151000173511)
- [Ghrieb et al. 2026, PlantDoc benchmarks](https://www.informatica.si/index.php/informatica/article/download/10277/6526); [Mohanty et al. 2016](https://pmc.ncbi.nlm.nih.gov/articles/PMC5032846/)

**Runtimes**
- ONNX Runtime 1.31: [release](https://github.com/microsoft/onnxruntime/releases/tag/v1.31.0), [kernel table](https://raw.githubusercontent.com/microsoft/onnxruntime/v1.31.0/docs/OperatorKernels.md), [CUDA execution provider](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html), [fp16](https://onnxruntime.ai/docs/performance/model-optimizations/float16.html)
- vLLM 0.31.0: [release](https://github.com/vllm-project/vllm/releases/tag/v0.31.0), [Dockerfile versions](https://github.com/vllm-project/vllm/blob/v0.31.0/docker/versions.json), [xgrammar backend](https://github.com/vllm-project/vllm/blob/v0.31.0/vllm/v1/structured_output/backend_xgrammar.py), [Qwen3.5 recipe](https://recipes.vllm.ai/Qwen/Qwen3.5-9B); xgrammar 0.2.7: [JSON Schema converter](https://github.com/mlc-ai/xgrammar/blob/v0.2.7/cpp/json_schema_converter.cc)
- llama.cpp: [server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), [multimodal](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md), [JSON schema to grammar](https://github.com/ggml-org/llama.cpp/blob/master/common/json-schema-to-grammar.cpp)
- PyTorch 2.14.1: [support matrix](https://raw.githubusercontent.com/pytorch/pytorch/v2.14.1/RELEASE.md), [ONNX export](https://docs.pytorch.org/docs/2.14/onnx_export.html); [CUDA 13.0 release notes](https://docs.nvidia.com/cuda/archive/13.0.0/cuda-toolkit-release-notes/index.html)
- ROCm: [10.1.0 release notes](https://rocm.docs.amd.com/en/latest/about/release-notes.html), [compatibility matrix](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html); vLLM RDNA 4 work: [#40977](https://github.com/vllm-project/vllm/pull/40977), [#43615](https://github.com/vllm-project/vllm/pull/43615)
- Claude: [vision coordinates](https://platform.claude.com/docs/en/build-with-claude/vision-coordinates), [structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
