# PDM Perception and CPU Models

> **Status:** Draft v0.2.7, 2026-10-21. v0.2.7: section 7 names the shipped disease head, v3c, trained on field and garden photos too, with its loss on corn and its five crops. v0.2.6: section 2 records the trained plant probe (with `τ_leaf` 0.99 and `τ_plant` 0.71 it passes 0.1% of the hold-out's no-plant photos against 11.0%, with no plant photo failing), not shipped yet; section 6 measures the species head (0.90 against zero-shot's 0.64 on the dev cases' supported crops) and finds that the combination rule keeps little of it; section 7 notes that the training crops are cut with the worker's rounding. v0.2.5: section 2 ships `τ_leaf` 0.40 and `τ_plant` 0.70 with the detector retrained on licence-clean background photos, chosen with the 1% limit on every dataset and measured on test and hold-out photos. v0.2.4: section 2 records the built day-1 plant probe, the shipped `τ_leaf` 0.90 and `τ_plant` 0.67, and the COCO negatives they were chosen on. v0.2.3: section 2 records the provisional `τ_leaf` = 0.35 and the plant photos the detector keeps no leaf on. v0.2.2: BioCLIP's input is resized with Lanczos3, as the worker does, not bicubic (issue #2). v0.2.1: the text embedder is the choice architecture open decision 13 records. v0.2: the fallback worker loads the label files named by the payload's versions from the manifest. v0.1, 2026-10-15: First model design for every model outside the VLM: the plant-present check, the leaf detector, the mask model, region classes, BioCLIP 2 for species and organisms, the disease head on BioCLIP features, image embeddings, the face detector and the text embedder, each with its choice, alternatives, licence, tensors, pre- and post-processing, export and failure behaviour.
> **Checked:** licences, model cards, configs and file hashes were read on 2026-10-15 (✓). The BioCLIP v1 export, the linear probes on PlantDoc, the text-embedder benchmark and the YuNet runs were executed on a Mac with ONNX Runtime 1.31 on CPU (✓ ran). Nothing has run on the rig's GPUs; VRAM figures are *estimates*.

This is the model side of the perception worker and of the two models `api` runs on CPU. How the worker opens sessions, batches and calls them is in [docs/backend](../backend/vision.md); which card each model sits on is in [README 4](README.md#4-placement-and-vram-budgets); how each is trained and exported is in [training.md](training.md).

## Contents

1. [Conventions for every model](#1-conventions-for-every-model)
2. [Plant-present check](#2-plant-present-check)
3. [Leaf detector](#3-leaf-detector)
4. [Mask model](#4-mask-model)
5. [Plant, soil and surroundings regions](#5-plant-soil-and-surroundings-regions)
6. [Species (BioCLIP)](#6-species-bioclip)
7. [Disease classifier](#7-disease-classifier)
8. [Image embeddings](#8-image-embeddings)
9. [Face detector](#9-face-detector)
10. [Text embedder](#10-text-embedder)
11. [Organism second opinion (stretch)](#11-organism-second-opinion-stretch)
12. [Open questions](#12-open-questions)

---

## 1. Conventions for every model

- **ONNX, one file per model**, run with ONNX Runtime 1.31 through `ort` ([README 5](README.md#5-runtimes-and-versions)). Opset 17, the one every model here was exported and checked at ([training 7](training.md#7-export-to-onnx)).
- **Dynamic batch** on the first axis of every GPU model, as [contracts 5](../architecture/contracts.md#results) requires. A batch above `PERCEPTION_MAX_BATCH` is split by the worker.
- **No pre-processing inside the graph** except what is cheap and fixed (L2 normalisation of an embedding). Resizing, cropping and normalisation are done in Rust from the values in the model's manifest entry ([README 6](README.md#6-model-files-versions-and-pinning)), so one tested code path feeds every model and the Python evaluation uses the same numbers.
- **No post-processing that changes output size inside the graph.** NMS, box decoding and top-k run in the worker: data-dependent shapes export badly and hide thresholds that belong in configuration.
- **fp32 inputs and outputs.** A model converted to fp16 keeps fp32 at its boundaries (`keep_io_types`), so the worker never handles half floats. fp16 is used only when the export check passes ([training 7](training.md#7-export-to-onnx)).
- **Every threshold is configuration** with its value in the manifest, set from the evaluation set ([evaluation 6](evaluation.md#6-thresholds-set-from-the-evaluation-set)), and reported in `model_versions`.
- **Image space.** Every model sees the stored image (upright and metadata stripped, [contracts 1](../architecture/contracts.md#wire-format); faces blurred, [architecture 11](../architecture/README.md#11-security-privacy-and-safety)), fetched through the job's signed URL. Boxes and masks are returned in the stored image's coordinates.

---

## 2. Plant-present check

`plant_present: false` fails the scan with `no_plant_found` ([contracts 5](../architecture/contracts.md#results)), so **a false negative kills a valid scan**. The check leans towards "present" and uses two signals the worker already has, with no model of its own:

```
plant_present = any plant image of the scan has
                  (top leaf score ≥ τ_leaf)                 leaf detector, section 3
               OR (plant probability ≥ τ_plant)             BioCLIP 2 embedding, section 6
```

| Signal | Strength | Weakness |
|---|---|---|
| The leaf detector's highest score | Tied directly to the regions later stages use | Trained on PlantDoc's 13 crops with incomplete labels (section 3): it will miss succulents, conifers, grasses, seedlings and distant plants *unverified* |
| A plant / not-plant score on the BioCLIP 2 image embedding | Broad: BioCLIP 2 was trained on 952,000 taxa across all of life ✓, plants among them | Its training data is organisms, so "not a plant" prompts are outside its distribution *unverified* |

- **Day 1 (built, 2026-10-16, ✓ ran):** the second signal is zero-shot and is exactly the worker's `pdm-linear-head/1` form, so it needs no Rust change. For an L2-normalised image embedding `e`, `p = σ(s · e · (t_plant − t_not))`, with `s` = 100 (BioCLIP 2's logit scale) and `t` the mean of the normalised text embeddings of one side's labels over the 80 templates. That is `w = s (t_plant − t_not)`, `b = 0`, `T = 1`. It differs from this section's earlier wording ("the softmax mass on the plant prompts"): there is one prototype per side (prompt ensembling), not one score per prompt. Prompt set A is the one above (plant: plant, leaves, garden plant; not: wall, person, pet, room, hand, car). Set B, which adds wilted plant, dying plant, leaf and tree to the plant side, was no better. Python and Rust outputs agree to 6e-7, and `PerceptionPipeline::load` loads the probe on the CPU set and the fp16 CUDA set. The file is `plant_probe/plant-probe-zs-2026-10-16.json` in `GreenLens/pdm-models` at commit e00434c (`python -m ml.training.heads.plant_probe --set A`).
- **The trained probe (built 2026-10-20, ✓ ran, not shipped yet).** A logistic-regression probe (a 768-value weight vector and a bias, the same file form), trained on the BioCLIP 2 embeddings of the training split's plant photos and 2,012 iNaturalist photos of the five crops, cacti, succulents, aroids, mints and gourds, against half of COCO val2017's photos without a potted plant or broccoli ([training 6](training.md#trained-plant-probe)). The other half chose the pair, by the rule below: at most 1% false `no_plant_found` on every dataset, then the fewest no-plant photos passing ([val report](../../ml/training/eval/reports/plant-check-val-2026-10-20-probe-v1.md)). It was then measured once on the test split and the 1,000 COCO train2017 hold-out photos ([report](../../ml/training/eval/reports/plant-check-test-holdout-2026-10-20-probe-v1.md)):

  | Pair | Photos | False `no_plant_found` | TOM2024 (field) | No-plant passing (indoor / outdoor / people and pets) |
  |---|---|---|---|---|
  | Trained probe, `τ_leaf` 0.99, `τ_plant` 0.71 | Val, COCO val2017 tune half | 0.10% | 0.30% | 0.08% |
  | Trained probe, 0.99 / 0.71 | Test, hold-out | 0% | 0% | 0.1% (0 / 0 / 0.2) |
  | Shipped zero-shot pair A, 0.40 / 0.70 | Test, hold-out | 0% | 0% | 11.0% (9.4 / 18.6 / 8.1) |

  - **The probe does the work alone:** at `τ_leaf` 0.99 the leaf score almost never decides.
  - **Plant photos it fails.** At 0.71 it fails 1.8% of iNaturalist's val photos (9 of 503). Six show nothing a scan would send: a scarecrow, a drain, a pumpkin cart, a sculpture, an apple crate and a dry corn husk. Three are real misses: a pumpkin among its leaves, bare branches, and a flower with a butterfly on it. On the evaluation set it failed 2 of 189 cases. Both were tomato spider-mite cases whose only photo showed one mite under a microscope or an extreme macro lens, with no plant to see, so they were dropped from the set; it passes every remaining case.
  - **Caveats.** The negatives are COCO photos, not the app's mistaken shots, and the plant photos are datasets' and iNaturalist's. Before it ships (a manifest change, M16–M17), it should be checked on the team's own no-plant and plant photos ([#52](https://github.com/aacruzgon/GreenLens/issues/52)).
- **Thresholds:** `τ_leaf` and `τ_plant` are set on the evaluation set so false `no_plant_found` stays under 1% while no-plant cases are still caught 90% of the time ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)).
- **Shipped pair, provisional (2026-10-19, ✓ ran): `τ_leaf` 0.40 and `τ_plant` 0.70 (prompt set A), with the detector `rfdetr-s-leaf-2026-10-19-bglic`.** That detector also trained on 400 COCO train2017 photos with no plant pixel and a licence that allows commercial use, shown with no box ([training 5](training.md#5-leaf-detector)), because the first one fired on non-plants (below). The pair was chosen on the 1,152 validation plant photos and 4,758 COCO val2017 photos without a potted plant or broccoli, by the rule fixed before measuring: of the pairs that keep false `no_plant_found` at or under 1% **on every dataset**, the one that lets the fewest no-plant photos through ([report](../../ml/training/eval/reports/plant-check-val-2026-10-19-detector-bglic.md)). It was then measured once on the 854 test plant photos and 1,000 COCO train2017 photos without a plant that no threshold or training run had seen ([report](../../ml/training/eval/reports/plant-check-test-holdout-2026-10-19-detector-bglic.md), `python -m ml.training.eval.plant_check --split test --negatives ml/training/data/splits/negatives_coco_train_holdout.tsv --pair A:0.40:0.70`):

  | Detector and pair | Photos | False `no_plant_found` | TOM2024 (field) | No-plant passing (indoor / outdoor / people and pets) |
  |---|---|---|---|---|
  | Shipped, 0.40 / 0.70 | Val, COCO val2017 | 0.35% | 0.90% | 12.5% (10.9 / 19.1 / 9.6) |
  | Shipped, 0.40 / 0.70 | Test, hold-out | 0.23% | 0.48% | 11.0% (9.4 / 18.6 / 8.1) |
  | First, 0.90 / 0.67 | Test, hold-out | 1.05% | 2.14% | 9.3% (10.0 / 15.7 / 5.2) |

  - **It misses the no-plant target by 1 to 2.5 points.** On val no pair with prompt set A meets both limits: TOM2024 has 444 photos, so 1% allows 4 failures, and under 10% of no-plant photos passing needs about 6 (1.35%). The rule puts the user with a real plant first; M16 refits both thresholds on the evaluation set.
  - **Per dataset, not pooled.** The pooled optimum (0.55 / 0.90) is under 1% overall only because three datasets score 0%; it fails 2.48% of TOM2024 on val and 1.67% on test. TOM2024's field photos are the closest we have to the app's.
  - **Half the no-plant photos that pass show vegetation.** The hold-out photos have no potted plant or broccoli, but 58 of the 110 that pass have COCO-Stuff plant pixels (53 over 5% of the frame): trees, hedges and grass, which the check may rightly call a plant. 5.2% pass with no plant pixel at all (✓ ran).
  - The leaf score alone at 0.35 now passes 6.2% of the val no-plant photos, down from 51.9%.
  - **A second candidate wasn't shipped.** `rfdetr-s-leaf-2026-10-19-bgveg` trained on an earlier pick that ignored the photos' licences (292 of its 400 non-commercial). It met both limits with 0.50 / 0.88 (val 0.43%, TOM2024 0.90%, 6.6%; hold-out 0.35%, 0.71%, 6.5%), so the shipped detector lets about 5 points more no-plant photos through at the same false `no_plant_found`. Whether that comes from the pick or from chance between training runs isn't measured ([report](../../ml/training/eval/reports/plant-check-test-holdout-2026-10-19-detector-bgveg.md)).
- **The first pair (2026-10-16, ✓ ran): `τ_leaf` 0.90 and `τ_plant` 0.67, with the first detector `rfdetr-s-leaf-2026-10-16b`.** Chosen together on the same val photos, pooled ([report](../../ml/training/eval/reports/plant-check-val-2026-10-16.md)):
  - Result: 0.8% of the plant photos fail, and 11.4% of the no-plant photos pass (indoor 11.7%, people and pets 8.1%, outdoor 16.3%), just short of the 90% catch target.
  - **Finding:** the leaf score alone at `τ_leaf` 0.35 passed 51.9% of the no-plant photos (outdoor 67%, indoor 55%): trained only on PlantDoc's leaves, the detector had never seen an image with nothing to find. The probe alone was the better signal: at `τ_plant` 0.7 it failed 2.1% of the plant photos and passed 9.0% of the no-plant photos. The plan's first rule (keep `τ_leaf` 0.35, set `τ_plant` where the probe passes 5% of negatives, 0.85) gave 0.5% and 53.7%, so `τ_leaf` had to rise.
  - Before that, on the plant photos alone, 2.9% scored under 0.35 and 5.6% under 0.5, nearly all TOM2024 field photos ([report](../../ml/training/eval/reports/tau-leaf-val-2026-10-16.md), `python -m ml.training.eval.leaf_scores`). Those photos have no kept leaf at all (score 0): wilted plants, frame-filling or blurred leaf close-ups, plants seen from a distance. Only the probe can pass them: at `τ_plant` 0.67 it passed 30 of those 34 photos (88.2%).
- **Caveats:** the positives are training-set photos and the negatives are COCO, not the app's mistaken shots. A scan has several shots and passes if any shot does, so per-scan rates differ from the per-photo rates here. Both thresholds are provisional and are fitted on the evaluation set on Oct 22–23 (dev-roadmap M16). M13's trained probe should use COCO negatives and add wilted plants, frame-filling leaf close-ups and succulents as positives (a teammate grows hundreds, and the leaf detector likely misses them).
- The check uses the plant shots only (`whole_plant`, `leaf_closeup`, `leaf_underside`, `flower_fruit`); a soil close-up has no plant by design.
- A photo that is unreadable but has a plant (blurred, far away) is not this check's job: the readable-plant rule ends it in `photos_unusable` ([architecture 8.1](../architecture/README.md#81-scan--diagnosis--plan)).

---

## 3. Leaf detector

### Choice

**RF-DETR-S** (`RFDETRSmall`, Roboflow): code and the N, S, M and L weights Apache-2.0 ✓ (the Atto, Femto, Pico, XL and 2XL sizes are under Roboflow's PML 1.0 licence ✓ and are not used). 32 M parameters, 512 × 512 input, COCO AP 53.0 at 3.5 ms on a T4 ✓, and 60.2 on RF100-VL, a 100-dataset fine-tuning benchmark ✓, which is likely the better guide to transfer to a small dataset like PlantDoc *unverified*. D-FINE-S scores 60.3 there ✓, so on fine-tuning the two are level; RF-DETR-S wins on COCO AP at the same speed and on the maintained, pip-installable trainer. Maintained: version 1.11.2 on 2026-10-04 ✓. Its ONNX export has a dynamic batch and needs no NMS ✓ ran. Fine-tuned to **one class, `leaf`**, on PlantDoc's boxes ([training 5](training.md#5-leaf-detector)).

| Family | Code / weights licence | COCO AP (S size) | Verdict |
|---|---|---|---|
| **RF-DETR-S** | Apache-2.0 / Apache-2.0 ✓ | 53.0 ✓ | **Use** |
| D-FINE-S | Apache-2.0 / Apache-2.0 (Objects365 checkpoints carry that dataset's terms) ✓ | 48.5 ✓ | **Runner-up**: 10 M parameters, post-processing inside its graph |
| RF-DETR-N | Apache-2.0 ✓ | 48.4 at 384 px ✓ | The same model family if VRAM is short |
| RT-DETRv2, YOLOX, LW-DETR | Apache-2.0 ✓ | 40–52 | Older or slower to adapt; YOLOX needs NMS |
| Ultralytics YOLO11 / YOLO26 | **AGPL-3.0**, and Ultralytics states that trained models are AGPL too ✓ | 47.7–48.6 | **Excluded**: it would make the whole project AGPL, and a hosted service without published source would need an Enterprise licence ✓ ([ultralytics.com/license](https://www.ultralytics.com/license)) |
| YOLOv10, YOLOv9 | AGPL-3.0, GPL-3.0 ✓ | | Excluded: copyleft |
| DEIMv2 | Non-commercial, bans hosted services ✓ | | Excluded |

The architecture calls it "a YOLO detector"; it means a fast single-stage detector, and an Apache-2.0 detector of the same class serves it without the copyleft ([README 12](README.md#12-findings-for-the-architecture-and-contracts)).

### Tensors

| | Name | Shape | Notes |
|---|---|---|---|
| Input | `input` | `[batch, 3, 512, 512]` float32 | Plain resize to 512 × 512 (squashed, no letterbox), RGB, ImageNet mean (0.485, 0.456, 0.406) and standard deviation (0.229, 0.224, 0.225) ✓. The side must be divisible by 32 ✓ |
| Output | `dets` | `[batch, 300, 4]` | Normalised centre x, centre y, width, height ✓ ran |
| Output | `labels` | `[batch, 300, C + 1]` | Raw logits; for a fine-tuned checkpoint the background is the last slot ✓. **Match outputs by name**: with few classes the widths can coincide ✓ |

### Post-processing (worker)

1. Sigmoid on the `leaf` logit of each of the 300 queries; keep queries at or above `τ_det` (start 0.35, set on the evaluation set).
2. No NMS: the model is trained one-to-one ✓. A final class-agnostic NMS at IoU 0.8 removes the rare exact duplicates.
3. Convert to the stored image's normalised `[x, y, w, h]`, which the squashed resize makes a direct scaling.
4. Drop boxes under 24 pixels on their shorter side in the stored image: too small to classify or measure.
5. Keep at most 40 leaves per image, by score, which bounds the mask model's work.
6. `confidence` is the sigmoid score, calibrated against the evaluation set's leaf boxes before it is shown.

### Shots and resolution

The detector runs on every plant shot. At 512 pixels a whole-plant shot's leaves are small, which is acceptable: whole-plant leaf boxes feed the `plant` region (section 5) and the plant check, while the leaves that are classified and measured come from close-ups, where they are large.

### Known limits

- **PlantDoc labels only the leaves that mattered to its annotators.** In 3 of 8 training images drawn by hand, only the diseased or prominent leaves were boxed ✓ ran. A detector trained on that learns to skip ordinary leaves, which caps recall; [training 5](training.md#5-leaf-detector) adds a relabelling pass.
- The deformable attention uses `GridSample`, which the CUDA execution provider runs only in fp32 ✓, so an fp16 conversion keeps those nodes in fp32 ([training 7](training.md#7-export-to-onnx)).
- RF-DETR's INT8 export costs about 2.7 AP ✓; not used.

---

## 4. Mask model

### Choice

**SAM 2.1 Hiera-T** (`facebook/sam2.1-hiera-tiny`): Apache-2.0 ✓, 39 M parameters (image encoder 27 M, prompt encoder and mask decoder 4 M) ✓, with high-resolution features that give sharper leaf edges, which matters for measuring spots and holes ([architecture 6.7](../architecture/README.md#67-depth-and-measurement-lidar)). There is no official ONNX export; our own split export (encoder and box decoder) has a dynamic batch and matched PyTorch within 7e-5, and produced clean masks on real PlantDoc leaves (predicted IoU 0.82–0.96) ✓ ran.

| Model | Weights licence | Params | Verdict |
|---|---|---|---|
| **SAM 2.1 Hiera-T** | Apache-2.0 ✓ | 39 M ✓ | **Use** |
| MobileSAM | Apache-2.0 ✓ | 10 M ✓ | **Runner-up**: a quarter of the encoder memory, lower mask quality (COCO box-prompt AP 39.4 against SAM-H's 46.1 ✓), the same decoder interface. The switch is configuration |
| SAM 2.1 Hiera-S | Apache-2.0 ✓ | 46 M ✓ | Only if T's masks are too coarse and memory allows |
| EfficientSAM | Apache-2.0 ✓ | 10–26 M | Unmaintained since 2024 ✓ |
| EdgeSAM | S-Lab licence, non-commercial ✓ | | Excluded |
| SAM 3 / 3.1 | Meta's SAM License (commercial use allowed, with obligations) ✓ | 848 M ✓, gated weights ✓ | Too big for a shared 8 GB card |

### Two graphs

| Graph | Inputs | Outputs |
|---|---|---|
| Encoder | `images` `[B, 3, 1024, 1024]` | `high_res_feats_0` `[B, 32, 256, 256]`, `high_res_feats_1` `[B, 64, 128, 128]`, `image_embed` `[B, 256, 64, 64]` ✓ ran |
| Box decoder | One image's three feature tensors (`[1, …]`) and `boxes` `[N, 4]` (corners, in the 1024 × 1024 frame) | `mask_single` `[N, 1, 256, 256]` and `iou_single` `[N, 1]`; `masks` `[N, 3, 256, 256]` and `iou_predictions` `[N, 3]` (the multi-mask tokens); `object_score_logits` `[N, 1]` ✓ ran |

### How a scan maps onto them

- **Images onto the encoder's batch.** SAM 2 **squashes** every image to 1024 × 1024, ignoring the aspect ratio ✓ (`SAM2Transforms`), and box coordinates are scaled by the same rule. (SAM 1 and MobileSAM resize the long side and pad instead ✓; a switch to MobileSAM changes this rule, and the worker takes it from the manifest.)
- **One image's boxes onto the decoder's batch axis N.** Each box is its own prompt, with corner labels 2 and 3 ✓. Putting several boxes into a single prompt returns one merged mask ✓, so "every leaf box of an image in one prompt" ([contracts 5](../architecture/contracts.md#results)) is implemented as **one decoder call with N boxes**, not one prompt ([README 12](README.md#12-findings-for-the-architecture-and-contracts)).
- **One decoder call per image**, in chunks of `SAM_MAX_BOXES` (16) boxes.
- **Single-mask output.** A box isn't ambiguous, so the worker uses `mask_single` *unverified* (SAM's guidance for box prompts; both outputs exist, so it is a configuration choice checked on the evaluation masks).
- **Upsampling.** The 256 × 256 logits are resized bilinearly straight to the stored image's size (correct for SAM 2's squash) and thresholded at 0 ✓, in the worker, then encoded as COCO RLE, which runs **column by column** ✓. A mask's RLE was a median of 383 bytes on PlantDoc leaves ✓ ran.
- **`confidence`** of a masked region is the detector's score; the mask's `iou_single` below `τ_mask` (start 0.6) drops the mask and keeps the box, since masks are optional ([contracts 5](../architecture/contracts.md#results)).

### Memory

Measured as CPU peak memory in fp32, a proxy for activation memory on the GPU ✓ ran: the encoder needs about **1.4 GB per image** (5.6 GB at a batch of 4), and the decoder about **38 MB per box**. The encoder therefore gets its own batch cap, `SAM_ENCODER_MAX_BATCH` (start 2), well below `PERCEPTION_MAX_BATCH`. fp16 and ONNX Runtime's fusions should lower it; the day-2 benchmark measures it on the 3070 ([evaluation 8](evaluation.md#8-day-2-benchmarks)). If the encoder doesn't fit at fp16 with a batch of 2 next to the other sessions, the worker switches to MobileSAM (336 MB per image ✓ ran).

---

## 5. Plant, soil and surroundings regions

The contract makes these regions optional ([contracts 5](../architecture/contracts.md#results)). Their main consumer is the measurement stage: a `plant` mask gives plant height and canopy width high rather than medium confidence and keeps a pot out of the height ([contracts 7](../architecture/contracts.md#measurement)).

### Version 1: a `plant` region from the leaves, no new model

On each `whole_plant` shot with at least 3 leaf boxes:

1. Take the box enclosing every leaf box, grown by 5%.
2. Prompt SAM 2.1 with it, in the same decoder call as the leaves.
3. Keep the mask as the `plant` region when its predicted IoU is at least `τ_mask` and it covers at least 60% of the leaf boxes' area; otherwise report the box alone, without a mask.

It costs one more box in a decoder call. Its risks: a pot or a stake inside the box may be included in the mask, and a sprawling plant's outer leaves may be missed. The evaluation set's LiDAR cases check the height error with and without it ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)) before the measurement stage trusts it.

`soil` and `surroundings` regions are not produced in version 1: no stage needs them, since the VLM reads soil from the photo itself and the ground plane comes from depth.

### Option: a scene-parsing model

UperNet with a ConvNeXt-T backbone trained on ADE20K (`openmmlab/upernet-convnext-tiny`) has `plant`, `tree`, `grass`, `flower`, `earth`, `sand`, `field` and `pot` among its 150 classes ✓, and its weights are MIT on Hugging Face ✓. It exports with a dynamic batch at 576 × 576 (the side must be a multiple of 192) and matched PyTorch with full argmax agreement ✓ ran; it found soil and mulch sensibly on 8 PlantDoc photos ✓ ran. It would give `soil` (its earth group, when it covers at least 2% of the image) and `surroundings`, and a second opinion for `plant`.

It is not in version 1 because **ADE20K's images are for non-commercial research only** ✓ ([terms](https://ade20k.csail.mit.edu/terms/)), the same kind of question as the face detector's training data (section 9), and because it adds 237 MB and a fifth session to the 3070. If the team accepts the licence reading and a use for `soil` regions appears, it goes in with `ArgMax` inside the graph and a class map at a quarter of the resolution, which cuts its memory from about 660 MB to well under 200 MB per image *estimate*. SegFormer is excluded (NVIDIA's non-commercial licence ✓), and SAM 3 with text prompts is too big for the card.

---

## 6. Species (BioCLIP)

### Choice

**BioCLIP 2** (`imageomics/bioclip-2`): weights and code MIT ✓, a CLIP ViT-L/14 at 224 × 224 pixels trained on TreeOfLife-200M (about 214 million images of 952,000 taxa ✓), with a **768-dimensional** shared embedding ✓.

| | BioCLIP | **BioCLIP 2** | BioCLIP 2.5 Huge |
|---|---|---|---|
| Vision tower | ViT-B/16, 86 M params ✓ | ViT-L/14, about 304 M *unverified* | ViT-H/14, about 632 M *unverified* |
| Embedding | 512 ✓ | 768 ✓ | 1024 ✓ |
| fp16 vision weights | about 0.17 GB | about 0.61 GB | about 1.3 GB |
| PlantNet zero-shot top-1 | 88–91 ✓ | **96.8** ✓ | 96.9 ✓ |
| Insects (Meta-Album) zero-shot | 34.9 ✓ | **55.3** ✓ | 68.2 ✓ |
| Fungi zero-shot | 40.9 ✓ | **83.8** ✓ | 84.9 ✓ |
| Verdict | Fallback if VRAM is short | **Use** | Too big to share the 3070 |

Sources: [BioCLIP paper](https://arxiv.org/html/2311.18803) table 4, [BioCLIP 2 paper](https://arxiv.org/html/2505.23883) tables 1–2, the three model cards.

Why: it is far better than BioCLIP on plants and on insects, which matters because the same model scores organism crops later (section 11), and its image embedding doubles as the similar-case embedding (section 8) and as the disease classifier's features (section 7). **One backbone, four uses**, one session on the 3070.

Alternatives considered: an external identification API (Pl@ntNet, Kindwise/plant.id) behind `SpeciesIdentifier` ([architecture 6.2](../architecture/README.md#62-species-identification)), kept as a later backup; a plain CLIP or SigLIP 2, which has no plant advantage and is another model to host.

### How it scores species

1. **Text side, offline.** For every `bioclip_label` in the knowledge base (every species, `full` and `label_only`, and every organism: [contracts 10](../architecture/contracts.md#checks-and-generated-files)), embed `a photo of {scientific_name} with common name {common_name}.` with BioCLIP 2's text tower, averaged over the 80 OpenAI ImageNet templates and re-normalised, as `pybioclip` does ✓. The result is an `N × 768` fp32 matrix shipped with the label list under its `label_set_version` ([training 8](training.md#8-label-lists)). **Only the vision tower runs on the GPU.**
2. **Image side.** Every plant and soil image of the scan is embedded in **pass 1** (the leaf crops of section 7 are **pass 2**). Each is resized on its shortest side to 224 with **Lanczos3** filtering (the worker's `imaging::resize`, for every model) and centre-cropped to 224 × 224, normalised with CLIP's mean (0.48145466, 0.4578275, 0.40821073) and standard deviation (0.26862954, 0.26130258, 0.27577711) ✓, and embedded in one batch. The graph returns L2-normalised vectors. This is open_clip's own transform except for the filter, which is bicubic there ✓; on 225 PlantDoc test photos the two filters gave zero-shot top-1 0.764 (Lanczos3) and 0.773 (bicubic) with identical top-3 ✓ ran, so the worker's filter was kept ([issue #2](https://github.com/aacruzgon/GreenLens/issues/2)). `pybioclip` instead squashes images to 224 × 224 without a crop ✓. Either works, but **the same one is used in training, calibration, the evaluation and serving**, from `pdm_vision` ([training 1](training.md#1-layout)), so the heads and thresholds see what the worker sends.
3. **Scores.** For each plant shot (`whole_plant`, `leaf_closeup`, `leaf_underside`, `flower_fruit`; a `soil_surface` shot is embedded for section 8 but not scored), `logits = (100 / T_species) · image · textᵀ` over the **species** rows only, softmax. 100 is the model's logit scale ✓; `T_species` is fitted on the dev cases ([evaluation 5](evaluation.md#5-calibration)).
4. **Combine shots.** Average the per-image probabilities with weights by kind: `whole_plant` 2, `flower_fruit` 2, `leaf_closeup` and `leaf_underside` 1. Several views identify a plant better than one ([architecture 6.2](../architecture/README.md#62-species-identification)), and a single leaf close-up is the weakest view: BioCLIP v1 zero-shot on PlantDoc's leaf photos scored 54–58% top-1 and 81–83% top-3 over 19 labels, 13 crops and 6 distractors (56–62% top-1 over the 13 crops alone) ✓ ran.
5. **Top 3** go out as `species.candidates` with `species_id` from the label list ([contracts 5](../architecture/contracts.md#results)).

### A trained species head for the supported crops

Zero-shot is weak on leaves, while a logistic-regression head on the same BioCLIP v1 features reached **0.89 top-1 and 0.99 top-3** for PlantDoc's 13 crops (✓ ran). So for the species that have `classifier_supported: true`, a species head trained on BioCLIP 2 features of our training images ([training 6](training.md#6-disease-and-species-heads)) gives a second distribution over those crops. The worker uses it as follows:

- The head's probabilities (temperature-scaled) and the zero-shot probabilities restricted to supported crops are averaged, and the result replaces the zero-shot probabilities for those crops, scaled so the supported crops keep their zero-shot share of the total.
- `label_only` species keep their zero-shot scores, so an unsupported plant (basil) is still recognised rather than forced into the nearest crop ([architecture 6.2](../architecture/README.md#62-species-identification), step 6).

The head is a matrix multiplication on the image embedding the worker already has, so it adds no model and no GPU time. Whether it improves the dev split's top-1 on whole-plant shots, where zero-shot is expected to be much stronger, is checked before it is turned on (`SPECIES_HEAD_ENABLED`).

**Measured (2026-10-20, ✓ ran,** `species-head-v1-2026-10-20`, [training 6](training.md#species-head)): on the 134 plant shots of the evaluation's dev cases whose species is supported, top-1 is 0.642 for zero-shot, 0.896 for the head and 0.664 for the rule above.
- **The head improves the dev split,** so it passes the condition for turning it on.
- **The rule keeps little of that.** Averaging inside the supported crops can't move a photo whose zero-shot mass sits mostly on another species, since that share is kept.
- **The zero-shot side is still the 20-species stand-in matrix.** With the seed's real list (M12) the shares change, so the rule is decided on the real matrix.
- **Options to measure on the dev cases then:**
  - let the head's confidence raise the supported crops' share;
  - gate on whether zero-shot's supported share passes a threshold fitted on the dev cases;
  - use the head alone when the photo's working species is a supported crop.

### Confidence and agreement

`score` is the combined probability. The "both agree with a high score" rule of architecture 6.2 uses BioCLIP's top-1 in the high band and the VLM naming the same species with medium or high certainty, with the threshold raised from the evaluation if needed ([evaluation 5](evaluation.md#5-calibration)).

### Failure behaviour

A BioCLIP failure fails the `species` stage. The worker reports the job failed with `retryable: true`; a species stage can't be skipped silently, because the run then has no working species unless the plant's species is confirmed. A job whose payload has `species_id` or `skip_species_identification` doesn't run this stage ([contracts 5](../architecture/contracts.md#payloads)).

---

## 7. Disease classifier

### Choice

A **trained head on frozen BioCLIP 2 image features of each leaf crop**, with its output restricted to the working species' labels ([contracts 5](../architecture/contracts.md#results)).

| Option | PlantDoc top-1 | Extra GPU cost | Verdict |
|---|---|---|---|
| **Logistic or 2-layer MLP head on BioCLIP features** | 0.742 top-1, 0.919 top-3 on BioCLIP v1 features, 27 classes; 0.801 restricted to the true species (✓ ran) | None: one more batched pass of the vision tower over leaf crops | **Use**, re-measured on BioCLIP 2 features on day 2 |
| ConvNeXt-Tiny `fb_in22k_ft_in1k` fine-tuned at 288 px (Apache-2.0 ✓) | Published full fine-tunes on PlantDoc: ConvNeXt-T 71.96%, Swin-B 73.81% ✓ ([Ghrieb et al. 2026](https://www.informatica.si/index.php/informatica/article/download/10277/6526)) | Its own session, about 0.3 GB *estimate* | **Runner-up**, if the head plateaus below 75% top-1 on the deduplicated field validation set |
| DINOv2 ViT-S/14 features + head (Apache-2.0 ✓) | 0.712 top-1, 0.949 top-3 (✓ ran) | Its own session | Similar accuracy, one more model |
| DINOv3 | | | Custom licence with a "Built with DINOv3" notice and gated downloads ✓; small gain |
| ConvNeXt V2 FCMAE weights | | | CC BY-NC ✓: excluded |

A head on frozen features matches published full fine-tunes on PlantDoc (70–77% top-1 is typical; higher numbers usually mean a different split or leakage). It trains in minutes, so it can be retrained whenever `classifier_labels.yaml` changes, and it costs no extra model on the 3070. Its weakness is resolution: a small lesion on a leaf crop resized to 224 pixels can vanish, which is where a CNN at 288–384 pixels would win.

**Shipped: v3c (Oct 20),** 19 classes, trained on our detector's crops of field and garden photos as well as PlantDoc's and TOM2024's. On the evaluation's dev leaf photos with a problem, the true problem is among its top 3 on 84%, against 57% for guessing, and on 8 of the 12 iNaturalist garden tomato leaves ([training 6](training.md#6-disease-and-species-heads)). Corn is worse at top-1, 7 of 15 dev leaves against v2c's 11. It covers five crops (apple, corn, grape, pepper, tomato), where v1d covered 14 with 34 classes.

### Inputs

- **Crops:** each `leaf` region's box from segmentation, grown by 10% on each side and clamped to the image, then resized and cropped as in section 6. With a mask, pixels outside the mask are **not** blanked: the head is trained on box crops, so serving matches training. Training cuts its crops the same way, from the shipped detector's boxes with `PixelRect::from_normalised`'s rounding ([training 6](training.md#features)).
- **No leaf region:** the whole `leaf_closeup` image instead, with `region_id` absent ([contracts 5](../architecture/contracts.md#results)).
- **Which shots:** `leaf_closeup` and `leaf_underside` regions. Whole-plant leaves are too small to classify at 224 pixels and are left to the VLM.
- Only when `species.classifier_supported` is true; otherwise the stage is skipped with `species_not_supported` ([architecture 6.2](../architecture/README.md#62-species-identification), step 6).

### Output

- The head outputs one logit per `classifier_labels.yaml` entry (index → species, problem or `healthy`: [contracts 10](../architecture/contracts.md#other-entity-shapes)).
- The worker keeps the working species' indices, divides by the head's temperature `T_disease`, applies softmax over those, and returns each crop's top 3 mapped to `problem_id` or `healthy`.
- Classes with too few training images are left out of the label file rather than trained badly: PlantDoc has 2 training images of tomato spider mites ✓. Several crops (squash, raspberry, blueberry, cherry, peach, soybean) have only healthy images or one disease in PlantDoc ✓, so for them the classifier can at best say healthy or that one problem, and the VLM's indicators do the rest ([training 3](training.md#3-datasets-and-licences)).

### Fallback worker

The fallback worker classifies each `leaf_closeup` with the hosted model and the same output shape, restricted by its schema to the working species' `classifier_labels` problems plus `healthy` ([contracts 5](../architecture/contracts.md#results)). The payload carries only the label versions, so the fallback worker loads the same label files from the manifest as the perception worker. Its `model_versions` say so. Its confidences are the certainty words mapped as for the VLM, never a softmax.

---

## 8. Image embeddings

**Reuse BioCLIP 2's whole-image embedding**: 768 dimensions, fp32, L2-normalised, from pass 1 of section 6, which covers every plant and soil image. It costs nothing extra, is MIT, and its space is organised by organism and trait, which suits "a past case that looked like this" ✓ (BioCLIP 2 paper on intra-species structure).

| Option | Dimension | Extra cost | Licence | Verdict |
|---|---|---|---|---|
| **BioCLIP 2 image embedding** | 768 ✓ | None | MIT ✓ | **Use** |
| DINOv2 ViT-S/14 | 384 ✓ | One more session | Apache-2.0 ✓ | Better for near-duplicates of the same plant; the alternative if similarity search proves too semantic |
| SigLIP 2 B/16 | 768 ✓ | One more session | Apache-2.0 ✓ | No plant advantage |

- `image_embeddings` holds one vector per plant image of the scan, with `model` set to `bioclip-2@<revision>` ([contracts 5](../architecture/contracts.md#results)). `soil_surface` shots are included, so a similar soil can be found.
- This fixes `scan_image.embedding` at **768** ([database open question 2](../database/README.md#9-open-questions)). The field has no index, so the dimension isn't in the schema; `embedding_model` on each row keeps vectors from different models apart, and a model change re-embeds the garden's images from storage.
- If BioCLIP falls back to v1 (section 6), the dimension is 512, and the same rule applies.
- The fallback worker returns no embeddings ([contracts 5](../architecture/contracts.md#results)).

---

## 9. Face detector

Runs in `api` on the control plane's CPU, on every upload, before the image is stored ([architecture 11](../architecture/README.md#11-security-privacy-and-safety)). It fails closed: an image is not stored until blurring succeeds. The blurring, the depth-map zeroing and the upload flow are backend code ([docs/backend](../backend/vision.md)); this section is the model.

### Choice

**YuNet**, `face_detection_yunet_2026may.onnx` from OpenCV Zoo: MIT ✓, 230 KB, **dynamic input size** ✓, about 0.88 / 0.87 / 0.75 AP on WIDER FACE easy, medium and hard ✓ (OpenCV Zoo's evaluation table). It detects faces from about 10 to 300 pixels across at its input size ✓.

| Model | Licence | Why not |
|---|---|---|
| YuNet `2023mar` (fixed 640 × 640) | MIT ✓ | Same network with a fixed input; the fallback if a runtime can't take dynamic sizes |
| UltraFace RFB-320 | MIT ✓ | Fixed 240 × 320 input, weaker on small faces *unverified* |
| SCRFD | Code MIT, pretrained weights non-commercial ✓ | Licence |
| YOLO-face variants | GPL-3.0 or AGPL-3.0 ✓ | Licence |
| RetinaFace MobileNet-0.25 | Code MIT ✓, weights of unclear provenance | No gain, weak provenance |
| MediaPipe BlazeFace | Apache-2.0 *unverified* | Built for selfie-distance faces |

**Training-data caveat.** YuNet, like every alternative, is trained on WIDER FACE, whose dataset card says CC BY-NC-ND 4.0 ✓. The weights are MIT-licensed by their author. Whether a dataset's terms reach trained weights is a legal question this document can't settle; it is listed for the team to decide ([README 9](README.md#9-licences-and-notices)).

### Tensors and decoding

- Input `input`, `[1, 3, H, W]` float32, **BGR**, 0–255, no normalisation; H and W padded with zeros at the bottom and right to multiples of 32 ✓.
- 12 outputs: `cls_{8,16,32}`, `obj_{8,16,32}` (`[1, A, 1]`), `bbox_{8,16,32}` (`[1, A, 4]`) and `kps_{8,16,32}` (`[1, A, 10]`), with `A = (H/s)(W/s)` ✓ (ran).
- Decoding per stride `s`, anchor `i` at row `i / cols`, column `i % cols`: score = √(clamp(cls) · clamp(obj)); centre = ((col + b0) · s, (row + b1) · s); size = (e^b2 · s, e^b3 · s) ✓ (from OpenCV's `face_detect.cpp`; reproduced in Python and found 29 of 29 faces on a test photo, ✓ ran). A parity test against OpenCV's `FaceDetectorYN` on 10 photos is part of the backend's tests.

### Passes for a 12-megapixel photo

At 1280 pixels on the long side, YuNet found all 29 faces of a 3000 × 2171 group photo at every threshold, in 38 ms on one Apple M3 thread (13 ms on 4) ✓ ran. Its reliable floor is about 20 pixels across at its input *unverified*, so at 1280 the smallest face caught in a 4032-pixel photo is about 64 pixels. The blur runs on the upload before it is resized for storage, and a LiDAR shot is stored with a long side of 2048 pixels or more ([contracts 3](../architecture/contracts.md#post-scansidimages)), so faces of 20–64 pixels need a pass at full resolution.

1. **Pass A:** the whole image at 1280 pixels on the long side.
2. **Pass B:** overlapping 1280 × 1280 tiles of the full-resolution image, with a stride of 960 pixels (25% overlap), so 12 tiles for 12 megapixels. Each tile is at YuNet's native scale, so faces down to about 20 pixels in the original are caught *estimate*. About 0.5 s on one Apple M3 thread (12 × 38 ms) *estimate*; expect 2–5 times slower per thread on a small x86 VM, to be measured, and run the tiles on the pool's threads in parallel. Pass A covers the faces too large for a tile.
3. **Merge** both passes' boxes in original coordinates: union boxes that overlap with IoU above 0.2 or contain one another, rather than suppressing them.

| Setting | Value | Why |
|---|---|---|
| Score threshold | **0.3** | Recall first: a false blur over a leaf costs a little evidence; a missed face is a privacy failure. 0.3 added no false positives at 1280 on the test photo ✓ |
| NMS IoU within a pass | 0.3 | OpenCV's default ✓ |
| Box growth before blurring | 25% on each side | A tight box leaves hair, ears and chin; the backend owns the exact rule. The depth map is zeroed inside this grown blur box, grown a further 10% when scaled to the depth map ([contracts 3](../architecture/contracts.md#post-scansidimages)) |
| Recall target | 98% of faces 20 px or more in the stored image, a design target set here (the research proposed 95% of faces over 32 px) | [evaluation 4](evaluation.md#4-metrics-and-acceptance-targets) |

The thresholds are `api` configuration and are tuned on the evaluation set's face photos.

---

## 10. Text embedder

Runs in `api` on the control plane's CPU for chat queries, memories and knowledge search ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)), and in the ingestion scripts for knowledge chunks, with the same file ([training 10](training.md#10-knowledge-ingestion-the-model-steps)). The backend writes its own embedder with `ort` and `tokenizers`, because `fastembed` 7.1.1 pins `ort` rc.13 and can't share a build with rc.14 ✓ ([docs/backend](../backend/vision.md)).

### Choice

**`Snowflake/snowflake-arctic-embed-xs`**, fp32 `onnx/model.onnx`: Apache-2.0 (stated in its README; the Hub's licence tag is empty ✓), 22 M parameters, 6 layers, **384 dimensions**, 90 MB ✓. This is the choice recorded in architecture open decision 13, confirmed by the day-1 check ([evaluation 7](evaluation.md#7-day-1-checks)), and it keeps the schema's `DIMENSION 384` ([database 5](../database/README.md#5-reads-indexes-and-vector-search)).

| Model | Licence | Dim | Retrieval (14 MTEB v1 tasks) ✓ | Query p50, 1 thread / 4 threads ✓ ran | Top-1, 12 plant questions ✓ ran |
|---|---|---|---|---|---|
| **arctic-embed-xs** | Apache-2.0 | 384 | 50.91 | 4.7 / 1.7 ms | 12 / 12 |
| arctic-embed-s (runner-up) | Apache-2.0 | 384 | 52.85 | 9.7 / 3.4 ms | 10 / 12 |
| bge-small-en-v1.5 | MIT | 384 | 52.46 | 9.9 / 6.9 ms | 11 / 12 |
| all-MiniLM-L6-v2 | Apache-2.0 | 384 | not published on its card | 2.9 / 1.3 ms | 11 / 12 |
| nomic-embed-text-v1.5 | Apache-2.0 | 768 | 53.76 | | |
| arctic-embed-m-v2.0 | Apache-2.0 | 768, multilingual | 56.19 | | |
| EmbeddingGemma-300m | Gemma terms | 768 | | | Not OSI-licensed; 3 times bigger than needed |
| Qwen3-Embedding-0.6B | Apache-2.0 | 1024 | | | Too big for a 2–4 vCPU request path |
| jina-embeddings-v3 | CC BY-NC 4.0 ✓ | | | | Licence |

The retrieval column is a mean over 14 of MTEB v1's 15 retrieval tasks computed from each card's own results (MSMARCO is missing from the cards), comparable only within the table. Latencies are from an Apple M3 Pro with ONNX Runtime 1.31 in Python; the control-plane VM is likely 2–5 times slower per thread *unverified*, which still keeps a query around 10–25 ms. The 12-question check is too small to rank models; it shows that none of them is broken on plant-health language.

Why xs over s: the same tokenizer, prefix, pooling and dimension at half the latency, and the retrieval gap (1.9 points) is smaller than what better chunking or more knowledge gives. **Switching to arctic-embed-s before ingestion costs nothing in the schema**, so the day-1 check ([evaluation 7](evaluation.md#7-day-1-checks)) can still move to it after measuring both on 50 real question–chunk pairs. French would need arctic-embed-m-v2.0 at 768 dimensions; decide that before ingestion, because changing the dimension later re-embeds every chunk and memory.

### The pinned recipe

| Item | Value |
|---|---|
| Files | `onnx/model.onnx` sha256 `cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c`; `tokenizer.json` sha256 `91f1def9b9391fdabe028cd3f3fcc4efd34e5d1f08c3bf2de513ebb5911a1854`; both at revision `d8c86521100d3556476a063fc2342036d45c106f` ✓ |
| Inputs | `input_ids`, `attention_mask`, `token_type_ids` (all zeros), int64, `[batch, seq]` ✓ |
| Output | `last_hidden_state`, `[batch, seq, 384]` ✓ |
| Tokenizer | `tokenizer.json` loaded from the file; BERT uncased WordPiece, `[CLS] … [SEP]` added; truncation at 512 tokens, right; padding to the longest in the batch with id 0 `[PAD]` ✓ |
| Pooling | **CLS**: `last_hidden_state[:, 0, :]` ✓ (`1_Pooling/config.json`). Not mean pooling |
| Normalisation | L2 to unit length in fp32, dividing by max(‖v‖, 1e-12). Cosine equals the dot product |
| Query prefix | `Represent this sentence for searching relevant passages: ` before chat questions and search queries ✓ |
| Document prefix | None, for knowledge chunks and memories ✓ |
| Precision | **fp32 on both sides.** The int8 files quantise activations per batch, so a text's vector depends on what it is batched with: cosine 0.9976–0.9983 for arctic-embed-xs between the same text alone and in a batch of 64 ✓ ran. fp32 gives 1.00000 ✓ ran |
| Recorded as | `embedding_model = "snowflake-arctic-embed-xs@d8c86521"` on every vector |

A **golden test** shared by the Rust embedder and the Python ingestion embeds 5 fixed strings (queries and documents) and checks the first 8 values within 1e-4 and a norm of 1 within 1e-5.

---

## 11. Organism second opinion (stretch)

BioCLIP 2 scores insects much better than BioCLIP (55.3 against 34.9 zero-shot on Meta-Album insects ✓), and the label list already includes every organism's `bioclip_label`. So once the VLM's organism boxes are stored, the same vision tower could score each box's crop against the **organism** rows of the label matrix and report agreement or disagreement with the VLM's label.

- It would need the crops after the VLM job, so either a second small `perception` job or the vlm-worker running BioCLIP itself; both are changes to the job flow and stay a stretch goal ([architecture 3](../architecture/README.md#3-components)).
- Its value is highest for the beneficial look-alikes (ladybug larvae against pests): a disagreement there could lower the finding's confidence instead of letting a plan target a beneficial.
- Accuracy on small insect crops will be noticeably lower than on whole photos *unverified*; measure it on the evaluation set's organism boxes before using it for anything.

---

## 12. Open questions

1. **BioCLIP 2 against BioCLIP on the 3070.** BioCLIP 2's vision tower is about 0.6 GB in fp16 against 0.17 GB, and it runs two passes per scan (every plant and soil image, then the leaf crops). Confirm the VRAM and the time per job on day 2 ([evaluation 8](evaluation.md#8-day-2-benchmarks)).
2. **The disease head on BioCLIP 2 features** was measured only on BioCLIP v1 features (0.742 top-1). Re-measure before committing; switch to ConvNeXt-Tiny if it stays below 75% on the deduplicated field validation set.
3. **The species head** (section 6): on only if it improves the dev split's top-1.
4. **The face detector's training data** licence (section 9), for the team to decide.
