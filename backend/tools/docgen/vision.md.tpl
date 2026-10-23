# PDM Backend: `vision` crate

> **Status:** Draft v0.2.8, 2026-10-23. v0.2.8: `rle::to_png` encodes a worker's mask as the 8-bit PNG that `pipeline` stores when a perception job completes and `GET /scans/{id}/masks/{mask_id}` streams; section 11 said `api` decoded a mask on each read, but the route streams the stored object (section 11). v0.2.7: open question 7: a HEIC's sensor frame is lost on decode, so a sensor-frame depth map sent with a HEIC is refused (found on the rig with `smoke.py --depth-check`). v0.2.6: the perception adapter takes every tensor name from the manifest (`TensorNames`, gap G4) and checks them against each loaded session at start-up (`check_names`), reads every threshold from `[model.config]` inside `PerceptionPipeline::load` with `PerceptionConfig::from_manifest` and has no default, and refuses an environment batch cap above the manifest's (G6; `tau_leaf` is a new `leaf_detector` key, `tau_plant` comes from `plant_probe`), and refuses fp16 models on the CPU provider (`check_precision`, G11), all from [issue #12](https://github.com/aacruzgon/GreenLens/issues/12). v0.2.5: open question 4 is settled: xgrammar enforces `anyOf` with `null`. v0.2.4: the head and label-matrix loaders read `ml/models/FORMATS.md` (`pdm-label-matrix/1`, `pdm-linear-head/1`), refuse a head trained on another export or a file whose version differs from its entry, and are tested on files written as `ml/` writes them; open question 5 is resolved. v0.2.3: open question 5 is open again: `ml/models/FORMATS.md` is the format of record, and the loader doesn't read it yet. v0.2.2: open question 5 is resolved: vision 6 now fixes the head and label-matrix JSON. v0.2.1: the section 1 note says the `rust-workers` skill follows this crate: `ort` with `load-dynamic` and `api-29`, providers as cargo features chosen by `ORT_EP`, one inference thread and the session pool. v0.2: the text embedder takes `domain::ports::EmbedMode` on each `embed` call instead of offering two pre-set instances (`queries()` and `documents()` are gone). v0.1: ONNX Runtime sessions with the execution provider chosen at run time, the perception models as one `VisionModel`, the CPU upload pipeline (decode, quality, two-pass face blurring, WebP, depth), LiDAR measurement as deterministic geometry, the text embedder and COCO RLE, compiled on Rust 1.99.1 with `ort` 2.0.0-rc.14.

Companion to the [backend architecture](README.md), the [`domain` crate](domain.md) whose ports this crate implements, and the [computer-vision design](../vision/README.md), whose model choices it follows. Every listing below is copied from the compiled source of `crates/vision` by the docgen tool. `cargo check -p vision` (also with `--features cuda`, `migraphx`, `webgpu`), `cargo clippy -p vision --all-targets` and `cargo test -p vision` pass: 61 unit tests and 19 tests that load `ml/`'s committed manifest and files written as `ml/` writes them, among them an end-to-end measurement of a synthetic leaf and byte-for-byte RLE fixtures from pycocotools. ONNX Runtime itself never runs in the tests: the default `load-dynamic` feature links nothing, and no test builds a session.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [ONNX Runtime: environment, execution providers, session pool](#4-onnx-runtime-environment-execution-providers-session-pool)
5. [The model manifest](#5-the-model-manifest)
6. [Perception](#6-perception)
7. [The upload pipeline](#7-the-upload-pipeline)
8. [Depth](#8-depth)
9. [Measurement](#9-measurement)
10. [The text embedder](#10-the-text-embedder)
11. [Masks and COCO RLE](#11-masks-and-coco-rle)
12. [Concurrency and lifetimes](#12-concurrency-and-lifetimes)
13. [Errors](#13-errors)
14. [Testing](#14-testing)
15. [Open questions](#15-open-questions)
16. [Sources](#16-sources)

---

## 1. Purpose and boundaries

**What it owns.**

| Area | Module | Implements | Called by |
|---|---|---|---|
| ONNX Runtime environment, providers, session pool | `runtime` | | everything below |
| Model files, hashes, configuration | `manifest` | | `api`, the perception and fallback workers |
| Perception stages on the RTX 3070 | `perception` | `VisionModel::perceive` | the `perception` worker |
| Decode, quality gate, faces, resize, WebP | `imaging`, `upload` | | `api` (`POST /scans/{id}/images`, chat images) |
| Depth validation, rotation, intrinsics, face zeroing | `depth` | | `upload`, the measurement stage |
| LiDAR measurement | `measure` | `SpotSegmenter` (the colour default) | `pipeline`, inside `api` |
| Text embeddings | `embed` | `TextEmbedder` | `api` (`agent`, `knowledge`) |
| Masks, COCO RLE | `mask`, `rle` | | perception (encode), `pipeline` (decode to the stored PNG), measurement |

**What it never does.** No network, no database, no object storage, no job protocol: callers hand in bytes and get bytes and records back. The VLM adapters are not here: they are HTTP clients and live in the [worker](worker.md). Answering measurable indicators from measurements belongs to `pipeline` ([pipeline](pipeline.md)); this crate only measures.

**Synchronous core, async edges.** Every CPU-heavy function is synchronous and borrows its buffers (`&[u8]`, `ImageView<'a>`, `DepthMap<'a>`); the caller runs it on a blocking thread. Only the port implementations (`PerceptionPipeline`, `ArcticEmbedder`) are async, and they move owned data onto a blocking thread or the inference thread themselves (section 12).

**The project skills.** The `rust-workers` skill (`ort.md`) follows this crate: one inference thread owns the perception models on the worker (section 6), `api`'s CPU models sit in a session pool (section 4), and `ort` is built with `load-dynamic` and `api-29`, not a fixed `cuda` build: the execution providers' bindings are cargo features (`cuda`, `migraphx`, `webgpu`), `ORT_EP` picks one at run time, and `ORT_DYLIB_PATH` names the vendor's ONNX Runtime, so one source builds the NVIDIA, AMD and CPU binaries ([AMD research](#16-sources)).

---

## 2. Dependencies

@@file ../Cargo.toml@@

| Crate | Version | Why |
|---|---|---|
| `ort` | =2.0.0-rc.14, `std`, `ndarray`, `tracing` (+ `load-dynamic`, `api-29`) | ONNX Runtime 1.31 bindings. A release candidate, pinned exactly. `load-dynamic` opens the vendor's `libonnxruntime` at start (`ORT_DYLIB_PATH`), so one binary serves Microsoft's 1.31 build and AMD's 1.29 build; `api-29` is the lowest API level asked for ✓ (AMD research 2.2) |
| `ndarray` | 0.17.2 | Tensors; must be the minor `ort` uses |
| `image` | 0.25.10, `jpeg`, `png`, `webp` | Decode with EXIF orientation, JPEG encode for the VLM, crops and blurs |
| `imageproc` | 0.27.0, no default features | Laplacian and histogram for the quality gate |
| `fast_image_resize` | 6.1.0 | SIMD Lanczos3 resizing of 12-megapixel photos |
| `webp` | 0.3.1 | Lossy WebP through libwebp (`image` writes only lossless) ✓ |
| `png` | 0.18.1 | Exact 16-bit and 8-bit grayscale PNGs for depth and confidence ✓ |
| `libheif-rs` | 3.0.0, `v1_19` (feature `heic`) | HEIC uploads; needs a system libheif ≥ 1.19 ✓ |
| `nalgebra` | 0.35.0 | 3×3 SVD for plane fits, vectors |
| `chacha20`, `rand` | 0.10.2 (`rng`), 0.10.3 | A seeded RNG whose stream is specified, for reproducible RANSAC ✓ |
| `tokenizers` | 0.23.2, `fancy-regex` | The embedder's WordPiece tokenizer; `fancy-regex` instead of the default `onig`, so no C toolchain for oniguruma ✓ |
| `toml`, `sha2`, `data-encoding` | 1.1.8, 0.11.1, 2.11.1 | The manifest, its SHA-256 checks, base64 head matrices |
| `tokio`, `tokio-util`, `async-trait` | 1.53.3, 0.7.20, 0.1.92 | `spawn_blocking`, the inference thread's channels, the async ports |

The `fastembed` crate is not used: 7.1.1 pins `ort =2.0.0-rc.13`, and `ort-sys`'s `links = "onnxruntime"` forbids two versions in one build ✓ ([vision models 10](../vision/models.md#10-text-embedder)).

---

## 3. Module layout

```
crates/vision/src/
├─ lib.rs                  modules; the crate-wide cast allowance for pixel arithmetic
├─ error.rs                VisionError and its mapping to ModelError
├─ runtime.rs              OrtEp, RuntimeConfig, OrtRuntime: the environment and session building
├─ runtime/pool.rs         Pool<T> / SessionPool and the Pooled<'p, T> guard
├─ manifest.rs             ml/models/manifest.toml: entries, hashes, perception_config
├─ imaging.rs              ImageView<'a>, format sniffing, decode with EXIF orientation, resize, WebP, JPEG
├─ upload.rs               UploadProcessor: the scan and chat upload pipeline
├─ upload/quality.rs       blur score and exposure, the quality verdict
├─ upload/faces.rs         YuNet two-pass detection, union merge, growth, blurring
├─ depth.rs                depth and confidence maps, PNG, rotation, intrinsics, face zeroing, process_depth
├─ mask.rs                 MaskBuf, resampling, erosion, components, holes, outlines
├─ rle.rs                  COCO RLE (port of maskApi.c)
├─ measure.rs              Measurer: inputs, quantities, aggregation, organism length
├─ measure/config.rs       MeasurementConfig: every constant, contract defaults
├─ measure/camera.rs       Camera: rays through K⁻¹, undistortion; stored_fx
├─ measure/plane.rs        planes, seeded RANSAC, nearest-rank percentiles
├─ measure/scene.rs        DepthScene<'a>: confident depth points
├─ measure/leaf.rs         leaf plane, length, area, spots, holes
├─ measure/plant.rs        ground plane, plant points, height, canopy width
├─ measure/confidence.rs   the criteria table
├─ measure/spots.rs        ColourSpotSegmenter (SpotSegmenter)
├─ perception.rs           PerceptionPipeline (VisionModel): stages, batching, events
├─ perception/inference.rs the inference thread; TerminateOnDrop
├─ perception/preprocess.rs image → tensor from the manifest recipe; batches
├─ perception/detector.rs  RF-DETR post-processing
├─ perception/sam.rs       SAM 2.1 prompts and mask upsampling
├─ perception/heads.rs     label matrix, disease head, plant probe, species combination
└─ embed.rs                ArcticEmbedder (TextEmbedder)
```

---

## 4. ONNX Runtime: environment, execution providers, session pool

**One environment per process.** `ort` rc.14 has no implicit global environment: `Session::builder(&Environment)` takes one, and a second `build()` in the process is an error ✓. `OrtRuntime::init` builds it once with one shared intra-op pool (spinning off), so four or five sessions don't each start a core-sized pool. Every session keeps a clone of the environment alive.

**The provider is chosen at run time.** `ORT_EP=cuda|migraphx|webgpu|cpu` picks it; each GPU provider exists only when its cargo feature is on (`cuda`, `migraphx`, `webgpu`), so the CUDA and MIGraphX bindings never have to link into one binary. With `download-binaries` they can't: pyke publishes no dist with both, and the build script sets a link-error cfg ✓ (AMD research 2.2). With the default `load-dynamic`, the image ships its vendor's `libonnxruntime` and `ORT_DYLIB_PATH` names it. Asking for a provider that isn't compiled in fails `OrtRuntime::init`.

@@items runtime.rs OrtEp RuntimeConfig SessionSettings OrtRuntime@@

**`error_on_failure` on every provider.** Without it a provider that fails to register is logged and skipped, and the session runs on the CPU: a silent slowdown on the rig ✓ (ort source, `ep/mod.rs`). The CUDA provider gets the settings of [vision 5](../vision/README.md#5-runtimes-and-versions): `kSameAsRequested` arena growth, `HEURISTIC` cuDNN search, no maximum workspace, and each session's `gpu_mem_limit` from its manifest entry.

@@impl runtime.rs impl OrtRuntime@@

The builder chain goes through `ort::Result` before `VisionError`: each builder step returns `ort::Error<SessionBuilder>`, which carries the builder back and isn't `Send + Sync`; `?` into `ort::Result` converts it to the plain `ort::Error` ✓ (compile error E0277, then fixed).

@@items runtime.rs commit@@

**The session pool.** `Session::run` takes `&mut self` in rc.14 ✓, so a session serves one caller at a time; `Session` is `Send + Sync` ✓. `api`'s CPU models (the face detector and the text embedder) each get a `SessionPool` of N sessions: N blocking threads run the model at once, and the next waits on a `Condvar`. Waiting on a `std` lock is fine on a blocking thread, which is the only place the pool is used. The pool is generic over its value, so its checkout logic is tested without ONNX Runtime.

@@items runtime/pool.rs Pool SessionPool Pooled@@
@@impl runtime/pool.rs impl<T> Pool<T>@@
@@impl runtime/pool.rs impl<T> Drop for Pooled<'_, T>@@

`Pooled<'p, T>` borrows the pool for `'p`: it can't outlive the pool, and it returns its value in `Drop` however the caller's scope ends (an early `?`, a panic). How it meets `spawn_blocking`'s `'static` bound is in section 12.

---

## 5. The model manifest

`ml/models/manifest.toml` lists every file a worker or `api` loads, with its SHA-256, input recipe and `[model.config]` thresholds ([vision 6](../vision/README.md#6-model-files-versions-and-pinning)). Each loader calls `Manifest::verify` on the entries it loads and refuses to start on a mismatch. `perception_config` is the first 16 hex digits of the SHA-256 over every entry's configuration table, reported as the `perception_config` key of `model_versions` ([contracts 5](../architecture/contracts.md#post-internaljobsidcomplete)).

@@items manifest.rs ModelEntry ModelInput ResizeRule Colour RunsOn Manifest@@
@@impl manifest.rs impl Manifest@@

**What the perception adapter reads from each entry.** Every tensor name: inputs from `[model.input] name` and outputs from `[model.outputs]` by their logical names (`boxes`, `logits`, `high_res_feats_0`, `high_res_feats_1`, `image_embed`, `mask_single`, `iou_single`, `embeddings`); only the SAM decoder's four inputs, which its entry doesn't list, are a constant (section 6, `TensorNames`). Once each session is built, `check_names` compares the file's own input and output names with these and refuses to start on a difference, so a manifest that disagrees with its export fails at start-up, not on the first job. Every threshold, from the entries in `PerceptionConfig::from_manifest`'s table, which `PerceptionPipeline::load` calls itself; a missing key refuses to start, so the settings in use are always the ones `perception_config` hashes. The environment sets only the batch caps, and each may lower a model's `max_batch` (`max_boxes` for the SAM decoder) but not raise it, since that is the batch its memory budget was measured at. `precision`: with `ORT_EP=cpu`, an fp16 GPU model refuses to start, because ONNX Runtime's CPU provider segfaults on fp16 graphs instead of returning an error; the CPU manifest (`ml.models.fetch --cpu`) has the fp32 files.

Entry names the code reads: `leaf_detector`, `sam_encoder`, `sam_decoder`, `bioclip`, every `bioclip_labels*` (one label matrix per `label_set_version`), every `disease_head*` (one per `classifier_labels_version`), `plant_probe`, `yunet`, `text_embedder` and `text_embedder_tokenizer`. The heads and label matrices are JSON files with their matrices as base64 little-endian `f32` (section 6); the format is ours, since [vision 6](../vision/README.md#6-model-files-versions-and-pinning) fixes the manifest but not these files.

---

## 6. Perception

`PerceptionPipeline` is the perception worker's `VisionModel` ([contracts 5, results](../architecture/contracts.md#results); [vision models](../vision/models.md)). It answers `perceive` and returns `ModelError::Unsupported` for the other kinds.

### The inference thread

One OS thread owns every GPU session for the life of the process; jobs send it owned tensors over a bounded channel (`mpsc::channel(1)`) and await a `oneshot`. The worker holds one job (`slots = 1`) and runs the models one after another, so one owner fits, `Session::run(&mut self)` needs no lock, and ONNX Runtime's threads stay off tokio's workers, so the heartbeat keeps ticking during a long batch (the `rust-async` skill, `blocking.md` section 4). Sessions load on the thread, with an optional warm-up at the largest batch, before the worker claims anything.

@@items perception/inference.rs ModelSlot SessionSpec Inputs Inference TerminateOnDrop check_names@@
@@impl perception/inference.rs impl Inference@@

**Cancellation by drop.** The `VisionModel` port cancels by dropping the job's future ([domain 9](domain.md#models)). A dropped future would leave a batch running on the thread, so `Inference::run` arms a `TerminateOnDrop` guard holding the run's `Arc<RunOptions>`: dropped before the reply arrives, it calls `RunOptions::terminate`, and ONNX Runtime ends the run with an error. How quickly a CUDA run notices the flag is *unverified* (the ort example is `no_run` for that reason).

### Batching and caps

Each model runs once per job over all its inputs, split at its own cap, never above the job-wide `PERCEPTION_MAX_BATCH` ([contracts 5, batching](../architecture/contracts.md#results); [vision 4](../vision/README.md#4-placement-and-vram-budgets)).

@@items perception.rs BatchCaps PerceptionConfig@@
@@impl perception.rs impl PerceptionConfig@@
@@items perception.rs TensorNames check_precision@@
@@items perception/preprocess.rs to_tensor batches effective_cap@@

### Stages

| Stage | Models | Output | Rule |
|---|---|---|---|
| `segmentation` | RF-DETR over the plant shots (≤ 8 a batch); SAM 2.1 encoder (≤ 2) and box decoder (≤ 16 boxes a call, one prompt per box) | `leaf` regions with masks as COCO RLE; a `plant` region from the leaves on a whole-plant shot with ≥ 3 leaf boxes | Region ids `<image_id>.<n>` from 1 per image; a mask below `τ_mask` (0.6) is dropped and the box kept |
| `plant_check` | The detector's top leaf score and the plant probe on the `BioCLIP` embedding | `plant_present` | Present when any plant shot has a leaf score ≥ `τ_leaf` or a plant probability ≥ `τ_plant`; soil shots never count |
| `species` | `BioCLIP` 2 pass 1 over every image, the label matrix of the payload's `label_set_version` | top 3 | Zero-shot softmax of `(100/T)·e·tᵀ` per plant shot, averaged with weights 2 (whole plant, flower or fruit) and 1 (leaves) |
| `disease` | `BioCLIP` 2 pass 2 over the leaf crops (boxes grown 10%), the disease head of the payload's `classifier_labels_version` | per crop top 3 | Restricted to the working species' classes before the softmax; whole close-ups when no leaf region; `species_not_supported` and `no_leaves_found` skips |
| `image_embedding` | pass 1's vectors | 768-value vectors, `model = bioclip-2@<revision>` | No extra GPU time |

The order of work differs from the stage list: segmentation runs first because the plant check uses its scores, and `BioCLIP`'s first pass serves the probe, species and embeddings at once. A version the worker hasn't loaded fails the job before any GPU time with `VisionError::UnknownLabelVersion`, which the worker reports as `unknown_label_version`, not retryable ([worker 6](worker.md#6-leases-heartbeats-and-completion)).

@@items perception.rs PerceptionPipeline@@
@@items perception/detector.rs DetectorConfig Detection decode@@
@@items perception/sam.rs SamConfig prompt upsample enclosing@@
@@items perception/heads.rs LabelMatrix combine_species shot_weight DiseaseHead PlantProbe plant_present@@

**Partial results.** A `StageClock` sends `stage_started` when a stage begins and the stage's `partial_result` followed by `stage_completed` when it ends (or `stage_completed` with `skipped`). A failed send is logged and ignored: `complete` repeats every stage, and a `409 lease_lost` is handled by the worker's sink, which cancels the job ([worker 7](worker.md#7-cancellation)).

@@items perception.rs StageClock@@

**The species head** of [vision models 6](../vision/models.md#6-species-bioclip) (`SPECIES_HEAD_ENABLED`) is not wired: it is turned on only if the day-2 evaluation shows it helps. The `LabelMatrix::probabilities` output is the place it would merge.

---

## 7. The upload pipeline

`api` runs `UploadProcessor::process` inside `spawn_blocking`, behind its own semaphore, for every `POST /scans/{id}/images` ([contracts 3](../architecture/contracts.md#post-scansidimages)), and `process_chat` for chat images. The order is fixed:

1. **Sniff and decode.** The format comes from the first bytes, never from the file name or content type; HEIC is an ISO-BMFF `ftyp` box with a HEIF brand, which `image` doesn't recognise ✓. JPEG, PNG and WebP decode with `image`, which reads the EXIF orientation and bakes it into the pixels; HEIC decodes with libheif (feature `heic`), which applies `irot` itself. Decoding is bounded (12,000 pixels a side, 512 MiB). Re-encoding later drops every metadata chunk ✓.
2. **Quality gate**, every scan kind: blur (variance of the 3×3 Laplacian) and exposure (luma histogram) on a 512-pixel downscale, so the scores don't depend on resolution. A failing shot is `422 image_quality` with the problem's hint. Chat images skip it.
3. **Faces**, fail closed: `YuNet` 2026may in two passes, the whole image at 1280 pixels and overlapping 1280-pixel tiles of the full-resolution image (stride 960), score 0.3, NMS 0.3 within a pass, boxes from both passes **united** when they overlap by more than IoU 0.2 or one contains the other, each grown by 25% on every side, then a Gaussian blur of sigma a quarter of the box's longer side ([vision models 9](../vision/models.md#9-face-detector)). Any error fails the upload: no photo is stored unblurred.
4. **Depth** (section 8), validated against the photo's sensor-frame size before the resize, so the stored size knows whether a depth map is kept.
5. **Resize**: a long side of `default_long_side` (1600), or at least 2048 pixels when the depth survives validation ([contracts 3](../architecture/contracts.md#post-scansidimages)); never upscaled.
6. **Lossy WebP** at quality 82 through libwebp.

@@items upload.rs UploadConfig UploadInput ProcessedImage ProcessedUpload UploadError UploadProcessor@@
@@impl upload.rs impl UploadProcessor@@

@@items imaging.rs ImageView SourceFormat sniff ExifOrientation Rotation DecodeLimits Decoded decode_upload fit_long_side resize encode_webp encode_jpeg@@

@@items upload/quality.rs QualityConfig QualityScores verdict@@

@@items upload/faces.rs FaceConfig FaceBox union_merge decode_stride tile_origins blur_faces@@

The `YuNet` decode follows OpenCV's `face_detect.cpp` (score = √(cls·obj), centre and size from the stride grid); it was reproduced in Python and found 29 of 29 faces on a test photo ([vision models 9](../vision/models.md#9-face-detector)). A parity test against OpenCV's `FaceDetectorYN` on 10 photos needs the model file and a real runtime, so it runs in the rig's integration tests, not here.

---

## 8. Depth

The depth path of an upload ([contracts 3, depth](../architecture/contracts.md#post-scansidimages); [architecture 6.7](../architecture/README.md#67-depth-and-measurement-lidar)). Depth problems never fail the upload: every step returns a `DepthProblem`, and the photo is stored without its depth.

| Step | Code | Problem on failure |
|---|---|---|
| Sent with a guided check | `process_depth` | `not_for_this_kind` |
| Over 4 MB | `process_depth` | `too_large` |
| Not exactly a 16-bit grayscale PNG | `decode_depth_png` | `unsupported_format` |
| Mirrored orientation, aspect off by > 1% | `domain::rules::uploads::check_depth` | `invalid_meta`, `aspect_mismatch` |
| Under 20% coverage | `check_depth_coverage` | `too_sparse` |
| `depth_meta` unparseable; focal lengths, principal point, reference aspect or gravity length wrong | `validate_meta` | `invalid_meta` |
| Confidence map of another size or format | `decode_confidence_png` | `invalid_meta`, `unsupported_format` |
| Storing it | `api` | `storage_error` |

Then the maps turn with the photo by exact pixel moves (no interpolation, so 0 stays "no depth"); the intrinsics, distortion centre and gravity turn with them (a quarter turn swaps `fx` with `fy` and the reference sizes); the intrinsics are scaled to the stored image, so the stored `DepthMeta`'s `ref_width`/`ref_height` **are** the stored image's size; depth and confidence are zeroed inside every blur box grown a further 10% and rounded outwards on the map's grid; and both maps are encoded as PNG.

@@items depth.rs DepthConfidence DepthMapBuf DepthMap ConfidenceMapBuf ConfidenceMap@@
@@items depth.rs rotate_grid rotate_point rotate_gravity rotate_intrinsics scale_intrinsics transform_meta validate_meta zero_faces@@
@@items depth.rs DepthInput DepthContext ProcessedDepth process_depth@@

**Coordinate convention.** Points use continuous pixel coordinates (pixel `i` covers `[i, i+1)`), so a clockwise turn maps `(u, v)` to `(H − v, u)`. Whether Apple's principal point is corner- or centre-based is *unverified* (the LiDAR day-1 check); the difference is half a pixel, about 0.1 mm at 30 cm.

---

## 9. Measurement

The measurement stage ([contracts 7, measurement](../architecture/contracts.md#measurement); [architecture 6.7](../architecture/README.md#67-depth-and-measurement-lidar)): deterministic geometry over the stored photo, its stored depth map and its segmentation regions, called by `pipeline` on a blocking thread in `api`. An error never fails the scan: `pipeline` completes the stage as skipped.

@@items measure.rs MeasureRegion MeasureInput MeasuredQuantity MeasureError Measurer@@
@@impl measure.rs impl Measurer@@

### Constants

Every constant is in `MeasurementConfig`, with the contract's value as the default; the ones the contract doesn't give are marked "ours" and are starting values for the evaluation set's LiDAR cases ([contracts 12](../architecture/contracts.md#12-open-questions), question 10).

@@items measure/config.rs MeasurementConfig@@

### Pixels to a plane

A stored-image pixel `(u, v)` is the ray `K⁻¹ · (u + ½, v + ½, 1)` after undistortion, with `K` scaled to the stored image; a depth pixel is that ray scaled by its z-depth on the depth map's own grid. The area a pixel covers on a plane `n · p + d = 0` is `t² / (fx · fy · |n · dir|)`, where the ray meets the plane at `t · dir`: a fronto-parallel pixel at 30 cm with `fx = 1000` is 0.3 mm square, and a 45° tilt multiplies it by √2 (both tested).

@@items measure/camera.rs Undistort Camera stored_fx@@
@@items measure/leaf.rs pixel_area@@

### Deterministic RANSAC

`rand`'s `StdRng` is documented as non-portable (a future version may change the algorithm) ✓, so measurements use `ChaCha8Rng` from `chacha20`, seeded with FNV-1a 64 of the image id and the region number, and draw indices by a fixed multiply-shift rule on `next_u64`. A test pins the stream (`seed 7 → 157, 167, 704, 726, 601`), so a dependency bump that would change every measurement fails loudly. Percentiles are nearest-rank, and the contract's medians are the nearest-rank 50th percentile.

@@items measure/plane.rs Plane PlaneFit fit_plane seeded below seed_for ransac percentile median@@

### Leaves, spots and holes

For each `leaf` region with a mask on a leaf close-up or underside: the mask is resampled to the depth grid and eroded by two depth pixels; a RANSAC plane (4 mm inliers) is fitted to the confident depth inside it; its residual is the inliers' RMS distance. Leaf length is the longest distance between outline points projected onto the plane (sampled to 1,500 points); area is the sum of the mask pixels' footprints. Spots are 4-connected components of the pixels the `SpotSegmenter` marks necrotic, sized by the circle of their projected area and sized only from 4 pixels across; halo pixels count toward `affected_area_pct` only. Holes are the outside components inside the leaf's bounding box that don't reach its edge.

@@items measure/leaf.rs SizedItem LeafGeometry measure_leaf@@
@@items measure/spots.rs SpotThresholds ColourSpotSegmenter@@

### Height and canopy width

@@items measure/plant.rs PlantHint PlantGeometry GroundCandidate ground_candidates choose_ground connected_layer measure_plant@@

Surface normals for the ground candidates come from each depth pixel's right and lower neighbours. Width runs along `gravity × line of sight` (horizontal, across the view). The edge rules use the plant points' depth pixels after the ground is removed: a point on the top or bottom row cancels the height, on a side column the width.

### Confidence and aggregation

@@items measure/confidence.rs PlantRegion Criteria band@@
@@items measure.rs aggregate@@

For a quantity over several leaves, contracts 7 doesn't say how per-leaf criteria combine; here coverage is the leaves' mean, the residual their median RMS, and the distance the quantity's `distance_m` (section 15). `MeasuredQuantity.certainty` is the band; `confidence` is `certainty.confidence()` (0.4, 0.65, 0.85).

**What `pipeline` needs** to answer measurable indicators: the measurements with their bands (it picks the best per quantity: highest confidence, then largest `n`), `distance_m` in each, and `fx` in stored-image pixels, which is `meta.intrinsics.fx` of the stored meta (`stored_fx` rescales if a caller ever passes another width).

---

## 10. The text embedder

snowflake-arctic-embed-xs in fp32 at revision `d8c86521`, with the recipe of [vision models 10](../vision/models.md#10-text-embedder): `tokenizer.json` with truncation at 512 and padding to the longest with id 0, `token_type_ids` all zeros, CLS pooling (`last_hidden_state[:, 0, :]`), L2 to unit length dividing by `max(‖v‖, 1e-12)`, 384 dimensions. The query prefix goes before chat questions and search queries, nothing before knowledge chunks and memories.

@@items embed.rs QUERY_PREFIX EmbedderConfig ArcticEmbedder model_inputs cls_pool_normalise@@
@@impl embed.rs impl TextEmbedder for ArcticEmbedder@@

**Two modes, one session pool.** The prefix depends on whether the texts are queries or documents, so `TextEmbedder::embed(mode, texts)` takes a `domain::ports::EmbedMode` (re-exported here) and `model_inputs` puts `QUERY_PREFIX` before queries only. One `ArcticEmbedder` (one `Arc` of sessions and tokenizer) serves chat, search and memory writes; each caller names the mode. Vectors are stored with `embedding_model = stored_name()` (`snowflake-arctic-embed-xs@d8c86521`).

The golden test of [vision models 10](../vision/models.md#10-text-embedder) (5 fixed strings, the first 8 values within 1e-4, norms within 1e-5) needs the model file and a runtime; it runs with the Python ingestion's fixtures in the integration tests.

---

## 11. Masks and COCO RLE

Masks cross the worker API as COCO RLE ([contracts 5, `RleMask`](../architecture/contracts.md#results)). No maintained Rust crate does this, and the algorithm is short, so `rle` ports `rleToString`, `rleFrString` and the encode and decode loops of the COCO API's `maskApi.c`. Masks are flattened column-major, `size` is `[height, width]`, and from the third count each is stored as the difference to the count two places earlier. Decoding checks every character, truncated varints, negative runs, the 64-megapixel cap and that the runs cover exactly `height × width`. Six fixtures written by pycocotools decode to the same pixels and re-encode byte for byte ✓.

@@items rle.rs RleError counts_from_string string_from_counts encode decode MaskPngError to_png@@
@@items mask.rs MaskBuf resample erode Component components holes outline@@

When a perception job completes, `pipeline` turns each region's mask into the object `GET /scans/{id}/masks/{mask_id}` serves with `rle::to_png`: `rle::decode`, then an 8-bit grayscale PNG at the mask's own size (the stored image's), 255 inside and 0 outside, through the `png` encoder the depth maps use. It is stored before its `mask` record ([pipeline 4](pipeline.md#4-ports-and-construction)), and the route streams that object without decoding anything.

---

## 12. Concurrency and lifetimes

| What | Runs on | Why |
|---|---|---|
| Upload pipeline, measurement, RLE | `api`'s blocking pool, called inside `spawn_blocking` behind a semaphore | CPU-bound for 0.1–1 s; tokio's blocking pool grows without bound otherwise |
| Face detector, text embedder | the caller's blocking thread, one pooled session each | `Session::run(&mut self)`; the pool lets N calls run at once |
| Perception models | the inference thread, for the life of the process | one owner, no locks, ONNX Runtime's threads off tokio's workers |
| Perception decoding and pre-processing | the blocking pool, behind `PerceptionPipeline.cpu` | owned images in, owned tensors out |

**Named lifetimes.**

| Lifetime | Where | What it borrows |
|---|---|---|
| `'a` | `ImageView<'a>`, `DepthMap<'a>`, `ConfidenceMap<'a>`, `domain::ports::MaskView<'a>` | the pixel or sample buffer; views are `Copy` and can't outlive their buffer, so one decoded map is read by many functions without copying |
| `'a` | `UploadInput<'a>`, `DepthInput<'a>`, `MeasureInput<'a>` | the request's bytes and records for one blocking call: `api` moves the owned `Bytes` into the closure and borrows them inside it, so nothing is copied to cross onto the thread |
| `'f` | `DepthContext<'f>` | the upload's face list, read by the depth step |
| `'p` | `Pooled<'p, T>` | the pool, so a checked-out session can't outlive it |
| `'a` | `DepthScene<'a>` | the decoded depth and confidence buffers, owned by `Measurer::measure`'s stack frame for the whole measurement |
| `'i`, `'s` | the private `Context<'i, 's>` of `measure` | the input (`'i`) and the scene and image (`'s`), kept apart because the scene is built inside the call while the input comes from the caller |
| `'m` | `Manifest::entries_with_prefix<'m>` | the manifest and the prefix; the returned iterator captures both (the 2024 edition captures every in-scope lifetime in `impl Trait`, which is what this iterator needs) |

**`'static` and `Send` across threads.** `spawn_blocking` takes a `FnOnce + Send + 'static` closure, because the blocking task may outlive the caller's future (dropping the `JoinHandle` doesn't stop it) ✓ (tokio docs). So no closure here borrows the caller: `ArcticEmbedder::embed` moves a clone of itself (an `Arc` and a mode) and an owned copy of the texts in; `PerceptionPipeline::decode` and `tensors` move owned `Bytes` clones, `RgbImage`s and the recipe in, plus the semaphore's `OwnedSemaphorePermit`, so the permit is held until the work ends even if the job is dropped. Inside such a closure a `Pooled<'p, _>` guard borrows the pool through the `Arc` the closure owns, so `'p` is bounded by the closure's own lifetime and the `'static` rule holds. Tensors sent to the inference thread are owned (`Inputs = Vec<(String, ArrayD<f32>)>`): a channel's item type must be `'static` and `Send` to cross threads.

**Shared state.** `UploadProcessor`, `Measurer`, `ArcticEmbedder` and `PerceptionPipeline` are `Send + Sync` and shared in `Arc`s; `Measurer` holds its segmenter as `Arc<dyn SpotSegmenter>` (dyn compatible: no generic methods, the lifetime is on the method). Pure geometry (planes, RANSAC, rotations) uses generics and static dispatch: `rotate_grid<T: Copy>` serves `u16` depth and `u8` confidence with one implementation; `below<R: Rng>` takes any RNG.

---

## 13. Errors

@@items error.rs VisionError@@
@@impl error.rs impl VisionError@@

| Error | Becomes |
|---|---|
| `VisionError` from a perception job | `ModelError::Failed` → `complete` with `model_error`, retryable; `UnknownLabelVersion` → `unknown_label_version`, not retryable (the worker downcasts the source) |
| `VisionError` from the embedder | `ModelError::Failed` → `api`'s `503 upstream_unavailable` path for chat |
| `UploadError::Quality` | `422 image_quality` with `problem.hint()` |
| `UploadError::UnsupportedMedia` | `415 unsupported_media_type` |
| `UploadError::FaceBlur`, `Encode` | `503 processing_unavailable`; the image isn't stored |
| `DepthProblem` | `UploadImageResponse.depth = { stored: false, problem }`; the photo is stored |
| `MeasureError` | the measurement stage is skipped (`stage_completed` with `skipped: error`) |
| `ManifestError` | start-up fails |

---

## 14. Testing

| Area | Tests |
|---|---|
| Pool | never more checkouts than values (8 threads, 2 values); a panicking caller still returns its value |
| Manifest | parsing an entry and its config; `perception_config` follows the values; `ml/models/manifest.toml` as committed parses (`tests/ml_files.rs`) |
| Heads and label matrices (`tests/ml_files.rs`) | files written as `ml/pdm_vision/headfile.py` writes them load: a matrix scores its species rows and keeps its organisms; a disease head reads every `problem_id`; the plant probe is a one-class head. A missing `problem_id`, another format, kind, index order or class count, and a matrix whose counts disagree are refused. A head must name this export (`features`) and carry its entry's version |
| Imaging | EXIF orientation 6 turns 40×20 into 20×40 and is gone after WebP; HEIC is sniffed by its brand; `fit_long_side` never upscales; boxes round outwards |
| Quality | a sharp checkerboard passes, its blur fails; a black frame is too dark |
| Faces | union merge of overlaps and containment; growth clamped; 12 tiles cover 12 megapixels; YuNet decode places an anchor on its cell; blur changes only the box |
| Depth | quarter-turn intrinsics and inverse; gravity turns with the pixels; grid rotation matches image rotation; PNG round trip and format checks; face zeroing with the extra 10%; `process_depth` maps each failure to its problem |
| Measurement | pixel footprints (fronto-parallel and 45°); the longest extent; RANSAC finds a 15° plane among outliers and is deterministic; the pinned stream; percentiles; the criteria table; ground choice (raised bed over path, bench above the base); the colour classes; **a synthetic 18 cm leaf at 30 cm with a 30 mm spot measures within 2 mm, 3 cm² and 0.5 cm, twice identically**; organism length |
| Perception | tensors are CHW and normalised for every resize rule; batches split at the effective cap; detector decode keeps confident large boxes and drops duplicates; SAM prompts and upsampling; species scores follow the closest row and shot weights; the disease head is restricted to the working species; soil shots never count for the plant check |
| Embedder | CLS pooling and normalisation |
| RLE and masks | pycocotools fixtures byte for byte; garbage rejected; holes, erosion, resampling, outlines; `to_png` gives an 8-bit grayscale PNG of the mask's size, 255 exactly inside |

Nothing here runs ONNX Runtime: `ort` with `cuda` can't link on macOS, and the default `load-dynamic` links nothing but would need a runtime at the first session (the `rust-workers` skill, `ort.md`). Model-level checks (YuNet parity with OpenCV, the embedder's golden vectors, warm-up at the largest batch, VRAM budgets) run on the rig on day 1–2 ([vision evaluation](../vision/evaluation.md)).

---

## 15. Open questions

Real ones, for [contracts 12](../architecture/contracts.md#12-open-questions) and [vision 11](../vision/README.md#11-open-decisions):

1. **Pooling criteria across leaves.** Contracts 7's confidence table is per measurement, but leaf quantities pool several leaves. Here: mean coverage, median residual. Alternatives: the lowest leaf's band, or one measurement per leaf.
2. **The stored size of a photo without depth.** Contracts 3 fixes only "at least 2048" with depth; this crate uses 1600 (100–500 KB of WebP, as storage 9 estimates). And "uploaded with depth" is read as "whose depth survived validation" (agreed with `api`): a dropped depth map stores the photo at the ordinary size.
3. **Resolved: `TextEmbedder` takes an `EmbedMode`** ([domain 9](domain.md#9-boundary-traits)). The two-mode wrapper is gone; every call names its mode.
4. **Nullable schema shapes** (`anyOf` with `null`) are used for soil readings and `other_name` in the worker's VLM schemas. Settled on 2026-10-16 ([contracts open question 31](../architecture/contracts.md#12-open-questions), [vision vlm 4](../vision/vlm.md#4-serving)): xgrammar in vLLM v0.31.0 produces both branches and blocks values outside the schema. The non-null soil branch needs the prompt to show a reading's shape.
5. **Resolved: the head and label-matrix files** follow [`ml/models/FORMATS.md`](../../ml/models/FORMATS.md). `perception/heads.rs` reads `pdm-label-matrix/1` (species rows scored, organism rows kept unscored) and `pdm-linear-head/1` (the disease head, whose labels each need `species_id` and `problem_id`, and the plant probe as a one-class head). The pipeline refuses a head or probe whose `features` isn't this manifest's `bioclip` export (`bioclip-2@<revision>+<first 16 hex of its SHA-256>`), and a file whose version differs from its manifest entry's. A `bioclip` `version` may be the revision alone or already `bioclip-2@`-prefixed.
6. **Apple's principal-point convention** and the inverse-distortion table's exact use (section 8, [camera](#pixels-to-a-plane)) are *unverified* until the LiDAR day-1 check ([contracts 12](../architecture/contracts.md#12-open-questions), question 11).
7. **A HEIC's sensor frame is lost** (found on the rig, 2026-10-20). `decode_heic` (`imaging.rs`) lets libheif apply the file's `irot`/`imir`, then reports the rotated size as `sensor_size`, with `ExifOrientation::UPRIGHT`. An iPhone HEIC stores its pixels in the sensor frame (`docs/photos/IMG_1178.HEIC`: `ispe` 4032×3024 with `irot` 3, decoded upright as 3024×4032), and contracts 3 puts the depth map in that frame. So a real LiDAR depth map sent with a HEIC fails `aspect_mismatch` and is dropped, while the same depth map rotated to the upright photo is accepted. `smoke.py --depth-check` showed both on the deployed `api`: a portrait 192×256 depth map was stored with a 1536×2048 photo, and the sensor-frame 256×192 one was refused. JPEG is right, since its EXIF orientation is applied separately. **Proposed fix:** decode with `DecodingOptions::set_ignore_transformations(true)`, take `sensor_size` from those pixels, read the primary item's `irot` and `imir` from the `ipco` and `ipma` boxes in plain Rust (libheif-rs 3.0 has no safe getter, and the workspace denies `unsafe`), and map them to the equivalent `ExifOrientation`, so the photo and its depth go through the JPEG path's code. `clap` (a crop) is refused or applied after, decided with the LiDAR day-1 check, as for question 6. This matters only if the LiDAR module writes HEIC; [frontend 4](../frontend/README.md#4-libraries) uploads its files unchanged.

---

## 16. Sources

**Rust language**
- The Book: [16.4 `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html) · [10.3 lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) · [15.3 `Drop`](https://doc.rust-lang.org/book/ch15-03-drop.html) · [18.2 trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
- The Reference: [dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility) · [lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html)
- [Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html) · std [`Condvar`](https://doc.rust-lang.org/std/sync/struct.Condvar.html) · [`PoisonError::into_inner`](https://doc.rust-lang.org/std/sync/struct.PoisonError.html#method.into_inner)
- tokio 1.53.3: [`spawn_blocking`](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html) · [`Semaphore::acquire_owned`](https://docs.rs/tokio/1.53.3/tokio/sync/struct.Semaphore.html#method.acquire_owned) · [`mpsc::Receiver::blocking_recv`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Receiver.html#method.blocking_recv)

**Crates at the pinned versions**
- [ort 2.0.0-rc.14](https://docs.rs/ort/2.0.0-rc.14/ort/) ([`Session::run`](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.Session.html#method.run), [`RunOptions::terminate`](https://docs.rs/ort/2.0.0-rc.14/ort/session/struct.RunOptions.html#method.terminate), [execution providers](https://ort.pyke.io/perf/execution-providers)) and its source (`ep/`, `environment.rs`, `ort-sys` `dist.tsv`)
- [image 0.25.10](https://docs.rs/image/0.25.10/image/) · [imageproc 0.27.0](https://docs.rs/imageproc/0.27.0/imageproc/) · [fast_image_resize 6.1.0](https://docs.rs/fast_image_resize/6.1.0/fast_image_resize/) · [webp 0.3.1](https://docs.rs/webp/0.3.1/webp/) · [png 0.18.1](https://docs.rs/png/0.18.1/png/) · [libheif-rs 3.0.0](https://docs.rs/libheif-rs/3.0.0/libheif_rs/)
- [nalgebra 0.35.0 `SVD`](https://docs.rs/nalgebra/0.35.0/nalgebra/linalg/struct.SVD.html) · [rand 0.10.3](https://docs.rs/rand/0.10.3/rand/) · [chacha20 0.10.2](https://docs.rs/chacha20/0.10.2/chacha20/) · [tokenizers 0.23.2](https://docs.rs/tokenizers/0.23.2/tokenizers/) · [ndarray 0.17.2](https://docs.rs/ndarray/0.17.2/ndarray/)

**Models and algorithms**
- [COCO API `maskApi.c`](https://github.com/cocodataset/cocoapi/blob/master/common/maskApi.c) · [OpenCV Zoo YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) · [ONNX Runtime CUDA execution provider](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html) · [MIGraphX execution provider](https://onnxruntime.ai/docs/execution-providers/MIGraphX-ExecutionProvider.html) · [WebGPU execution provider](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html)

**Project documents, research and skills**
- [Data contracts](../architecture/contracts.md) · [architecture](../architecture/README.md) · [storage](../storage/README.md) · [domain](domain.md) · [pipeline](pipeline.md) · [api](api.md) · [worker](worker.md)
- [Computer-vision design](../vision/README.md): [models](../vision/models.md), [VLM](../vision/vlm.md), [evaluation](../vision/evaluation.md)
- The backend research notes (vision, AMD RX 9070 XT, model decisions), with the vision and AMD probe crates; ✓ marks a fact compiled or run there or here
- Skills in the main checkout's `.claude/skills/`: `rust` (`versions.md`), `rust-workers` (`ort.md`), `rust-async` (`blocking.md`), `rust-errors`, `rust-testing`

External links were taken from the research notes, which opened them on 2026-10-15; the crate facts marked ✓ were compiled against the pinned versions in the scratch workspace.
