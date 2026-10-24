# PDM Devpost Write-up: the ML Part

> **Status:** Draft v0.2.5, 2026-10-23. v0.2.5: M18 scored the test split once on Oct 23 and filled section 3's T rows: species zero-shot top 1 41 of 56 (missed), the disease head's top 3 36 of 43 against 60% guessing (undecided) and its ECE 0.210 (undecided: a perfectly calibrated head with the same confidences averages 0.165 on 51 photos), the plant check 58 of 58, the VLM 41 of 41 jobs valid at 8.8 s p95, a face box on 38 of 73 test photos, and T11–T16 not measured for the submission without J5's harness; every row has its figure without the 7 former dev cases, and the disease head's top 3 is where test falls below dev; the leakage check on the final state covers plant probe v2's photos. v0.2.4: the trained plant probe v2 ships at 0.94 / 0.31 (M17), so iNaturalist, iBean and 2,372 licence-clean COCO photos are in shipped weights, each photo credited in `ml/models/credits/`; COCO's photo licences are version 2.0. v0.2.3: section 3's dev results are M16's re-runs on the 102 dev cases (#156), with 95% intervals where a target depends on them: species zero-shot 74.3% top 1 and 90.1% top 3, both missing; v3c's top 3 93% against 62% guessing, undecided against 85%, with its temperature pending M17 (#167); and the VLM on the shipped prompts 3 of 7 `sure` cases against FP8's 1. Sections 4 to 7 and 10 follow. v0.2.2: the split is #147's, 102 dev and 58 test with 9 `sure` test cases, since neighbouring field-dataset frames now share a plant key; the 7 test cases that were used on dev are named, and M18 reports with and without them. v0.2.1: the split is #140's after Alan's second review of #135, 104 dev and 56 test, since a same-plant squash pair moved to test. v0.2: after Alan's review, the 12 garden tomato leaves are out (the second review keeps at most 3 of them), the VLM's 35% sits beside the 33% a random pick gets, prior work says what existed before build week, the species result is the seed's matrix (61%, as the stand-in), case counts are #140's and marked pending, T5 and T11–T14 name their cases, section 10's checklist covers the re-runs, and the duplicates between fields are cut. v0.1: first draft of the ML part of each Devpost field (dev-roadmap M19): what the models do on a scan, how they were built and measured, challenges, accomplishments, lessons, next steps, built with and prior work, with the test split's numbers left as placeholders for M18.

This file holds the machine-learning part of each field on PDM's Devpost page, ready to paste; each section names its field. The app, the backend and the farm connectors are written up elsewhere. Every number links its source: a section of the vision documents, a committed report, or an open pull request where the fact isn't on `main` yet. Each number carries one of these labels:

- **Dev:** the evaluation set's dev cases. We chose and tuned the models on them, so they flatter the models.
- **Held-out:** photos set aside from the training sources: the training datasets' own test photos, and for the plant check 1,000 COCO photos no threshold was chosen on. They come from the same cameras and fields as the training photos, and they are not the evaluation set.
- **Bench:** speed and memory, measured on the machine named beside it.
- **Test:** the evaluation set's test cases, scored once by task M18 on Oct 23 ([report](../../ml/training/eval/reports/test-split-2026-10-23.md); [dev-roadmap, phase 5](dev-roadmap.md#phase-5-oct-2425-score-once-ship)). A number M18 couldn't measure says "not measured" and why, never an estimate.

## Contents

1. [What the models do on a scan](#1-what-the-models-do-on-a-scan)
2. [How we built it](#2-how-we-built-it)
3. [How we measured it](#3-how-we-measured-it)
4. [Challenges we ran into](#4-challenges-we-ran-into)
5. [Accomplishments](#5-accomplishments)
6. [What we learned](#6-what-we-learned)
7. [What's next](#7-whats-next)
8. [Built with](#8-built-with)
9. [Prior work](#9-prior-work)
10. [For M18](#10-for-m18)

---

## 1. What the models do on a scan

*Devpost field: What it does (the ML part).*

A scan is a few photos of one plant: the whole plant, leaf close-ups, a leaf's underside, the soil. The photos pass through these models:

1. **Face blurring, on upload.** Before a photo is stored, YuNet looks for faces on the control plane's CPU, on the whole photo at 1,280 pixels and on full-resolution tiles, and the API blurs what it finds. The upload fails closed: a photo isn't stored until blurring succeeds ([models 9](models.md#9-face-detector)).
2. **Leaf detector.** On the RTX 3070, RF-DETR-S, which we fine-tuned to one class, `leaf`, boxes the leaves it finds. It misses some ordinary leaves, because PlantDoc's annotators often boxed only the diseased or prominent ones ([models 3](models.md#3-leaf-detector)).
3. **Plant-present check.** A scan goes on if any plant photo has a leaf box scoring at least `τ_leaf`, or a BioCLIP 2 plant probe scoring at least `τ_plant`; otherwise it ends with `no_plant_found`. The check leans towards "present", because a false "no plant" kills a real scan ([models 2](models.md#2-plant-present-check)).
4. **Masks.** SAM 2.1 Hiera-T, prompted with the leaf boxes, outlines each leaf and the plant ([models 4](models.md#4-mask-model)).
5. **Species.** BioCLIP 2 embeds every plant photo and compares it, zero-shot, with a label matrix: its own text embedding of each of the knowledge seed's 20 species and 5 organisms, averaged over 80 prompt templates ([models 6](models.md#6-species-bioclip); the seed's matrix is in [#134](https://github.com/aacruzgon/GreenLens/pull/134)). The photos' scores are combined, and the top 3 species go to the app. We also trained a species head on the same embeddings for five crops. It is measured, but the worker doesn't run it yet.
6. **Disease head.** BioCLIP 2 embeds each leaf crop, and our disease head, a logistic regression on that embedding, scores 19 problem-or-healthy classes on five crops: apple, corn, grape, pepper and tomato. Its scores are restricted to the plant's species, and each leaf's top 3 become candidate diagnoses ([models 7](models.md#7-disease-classifier)).
7. **The VLM's findings.** On the RTX 5070 Ti, Qwen3.5-9B (4-bit AWQ, served by vLLM, thinking off) reads the photos and returns JSON held to a schema: soil readings, organisms with boxes, damage patterns, a species cross-check, and a yes, no or unsure answer to each candidate diagnosis's indicator questions ([vlm.md](vlm.md)). When no GPU worker is free, a hosted model does the same job in the same shapes ([README 3](README.md#3-model-inventory)).
8. **Diagnosis.** The API checks the candidates against curated disease profiles, using the VLM's answers. It confirms one only when it leads the next by a margin, and otherwise reports it as uncertain and asks a question ([architecture 6.3](../architecture/README.md#63-retrieve-then-verify)). No single model makes the call.
9. **Text embedder for chat and search.** snowflake-arctic-embed-xs, on the control plane's CPU, embeds chat questions, memories and knowledge-base passages, so the chat can find and cite its sources ([models 10](models.md#10-text-embedder)).

BioCLIP 2 does four jobs on the 3070 from one session: species, the plant probe, the disease head's features, and image embeddings for finding similar past scans ([models 6](models.md#6-species-bioclip)).

---

## 2. How we built it

*Devpost field: How we built it (the ML part).*

| Model | Job | Runs on | What we did |
|---|---|---|---|
| RF-DETR-S (Roboflow) | Leaf boxes; one signal of the plant check | RTX 3070, fp16 ONNX | Fine-tuned to one class, `leaf` |
| SAM 2.1 Hiera-T (Meta) | Leaf and plant masks | RTX 3070, fp16 ONNX | Used as is; our own export to two ONNX graphs |
| BioCLIP 2 (Imageomics) | Species, plant probe, features for our heads, image embeddings | RTX 3070, fp16 ONNX vision tower | Used as is; its text tower builds our label matrix |
| Disease head (ours) | Problem or healthy, per leaf crop | The perception worker's CPU | Trained: logistic regression on BioCLIP 2 features, 19 classes |
| Plant probe (ours) | Plant or not, per photo | The perception worker's CPU | Logistic regression on the BioCLIP 2 embedding, trained on leaf-dataset and iNaturalist plant photos against licence-clean COCO photos without a plant |
| Species head (ours) | Species of the five crops | Not in the worker yet | Trained: logistic regression on BioCLIP 2 features |
| Qwen3.5-9B, as `cyankiwi/Qwen3.5-9B-AWQ-4bit` | Findings and indicator answers | RTX 5070 Ti, vLLM 0.31.0 | Used as is, a community 4-bit quantization; our prompts and JSON schemas |
| Qwen3-VL-8B-Instruct | The VLM's fallback, by configuration | RTX 5070 Ti, vLLM 0.31.0 | Used as is |
| YuNet `2026may` (OpenCV Zoo) | Faces to blur | Control plane CPU | Used as is |
| snowflake-arctic-embed-xs | Text embeddings, 384 dimensions | Control plane CPU | Used as is, in fp32 |
| `claude-opus-5-5` | Vision fallback when no GPU worker is free | Fallback worker | Used as is, with the same output shapes |
| `claude-haiku-5-5`, `claude-opus-5-5` | Tagging and drafting knowledge-base entries, which a teammate reviews | Developers' machines | Used as is |

Sources: [README 3](README.md#3-model-inventory) (inventory) and [4](README.md#4-placement-and-vram-budgets) (placement). Authors and licences are in section 9.

**Placement and pinning.** The four perception sessions share the 8 GB RTX 3070, and the VLM has the 16 GB RTX 5070 Ti to itself, where two jobs at once fit ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)). A second VLM worker on the rig's RX 9070 XT is planned; nothing has run Qwen3.5 on it yet ([README 4](README.md#4-placement-and-vram-budgets)). Our own files live in a private Hugging Face model repository and upstream files come from their pinned revision; both reach the hosts pinned by SHA-256 in [`manifest.toml`](../../ml/models/manifest.toml), with a [notices](../../ml/models/NOTICES.md) entry for each. The VLM is pinned by its Hugging Face revision in the sidecar's command. The hosted model and the species head aren't in the manifest ([README 6](README.md#6-model-files-versions-and-pinning)).

**Leaf detector.** We chose RF-DETR-S because the YOLO models of the same class are AGPL-3.0, and Ultralytics states that models trained with them are too; RF-DETR-S is Apache-2.0 and scores 53.0 COCO AP at 3.5 ms on a T4 ([models 3](models.md#3-leaf-detector)). We fine-tuned it on PlantDoc's detection boxes with every class collapsed to `leaf`, after clipping 4 out-of-bounds boxes and dropping 4 empty ones ([training 5](training.md#5-leaf-detector)). Trained on PlantDoc alone, it had never seen a photo with nothing to find: its leaf score alone, at 0.35, passed 51.9% of photos without a plant. Retrained with 400 COCO photos with no plant pixel, shown with no box, it passed 6.2% ([models 2](models.md#2-plant-present-check)). Held-out: mAP50 0.935 on PlantDoc's 236 test photos, the same in PyTorch and in the fp16 ONNX file the worker loads ([roadmap 3](roadmap.md#oct-1819)).

**Heads on frozen features.** A logistic regression on BioCLIP 2's embedding trains in seconds on a CPU and adds no model to the GPU; the worker applies it in Rust ([training 6](training.md#6-disease-and-species-heads)). The disease head trains on the crops the worker will see: our detector's boxes, grown and rounded exactly as the worker cuts them.
- **Disease head v3c,** the one shipped, has 19 classes on five crops. It trained on 5,331 photos from PlantDoc, TOM2024, Tomato-Pakistan and three sets of handheld field photos: PlantCity, Pretoria's maize and Makerere's maize. Its regularisation was chosen on the dev cases, by how often the true problem is in its top 3 (its job in the pipeline), and only then by top-1 on its validation photos ([training 6](training.md#disease-head)).
- **Held-out:** top-1 0.850 per crop on the training sources' test photos, with two caveats. By Oct 20, PlantDoc's, TOM2024's and Tomato-Pakistan's test photos had been read three times, by v3c and two heads dropped before it. Makerere's test photos come from the same fields and week as its training photos, so they aren't held out the way the others are ([training 6](training.md#disease-head)). Section 3 shows the number is optimistic for garden photos.
- **Its classes map to the knowledge base** through `classifier_labels.yaml`. Tomato's 8 classes map now; apple's, corn's, grape's and pepper's wait on their problems' profiles in [#125](https://github.com/aacruzgon/GreenLens/issues/125) ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
- **The species head** trained on whole-photo embeddings of the training photos, plus 1,115 CC0 and CC BY iNaturalist photos of the five crops' whole plants, flowers and fruit ([training 3](training.md#3-datasets-and-licences), [training 6](training.md#species-head)).

**The VLM.** Qwen3-VL-8B and Qwen3.5-9B share an image processor and return boxes as `bbox_2d` on a 0–1000 scale, so switching between them is configuration, not code. Qwen3-VL documents that scale; Qwen3.5's card doesn't, so we measured it. Both are Apache-2.0. Qwen2.5-VL-7B also writes `bbox_2d`, but in pixels and with 31% more tokens per image, so it was a baseline only ([vlm.md 2](vlm.md#2-model-choice)). We ran three builds through the worker's own calls (its prompts, schemas, image preparation and validation) on the 65 tomato and corn dev cases of Oct 20. We chose Qwen3.5-9B AWQ: its indicator answers were the only ones with any gap between the true problem's questions and the other candidates'. That lead is thin (section 3), and it was a little slower than Qwen3-VL-8B AWQ, at 5.3 against 4.4 s per job at the median, far inside the limit ([vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran)).

**The text embedder.** arctic-embed-xs over arctic-embed-s: the same tokenizer and dimension at half the latency (bench: 4.7 ms per query on one thread of an Apple M3 Pro), in fp32 for the reason in section 4 ([models 10](models.md#10-text-embedder)).

**The evaluation set.** We built 187 cases: 103 one-photo cases from four published tomato and corn field datasets that none of our models trained on, and 84 iNaturalist observations of apple, grape, squash and tomato from 2026, newer than BioCLIP 2's training data. A VLM may have seen the 2023 and 2025 field sets (*unverified*) ([evaluation 2](evaluation.md#where-the-photos-come-from)). A second review and replacement cases leave 160, pending (section 3).

---

## 3. How we measured it

*Devpost field: How we built it (continued), or Accomplishments.*

### The evaluation set

- **A case is one scan** as the app would send it: its photos, each with its kind, and the truth for that plant ([evaluation 2](evaluation.md#2-the-evaluation-set)).
- **Split by plant** into dev and test, so two photos of one plant are always on the same side. Where a dataset's photos can't show which plant a leaf came from, neighbouring frames count as one plant. The test cases are scored once, and no threshold is tuned on them ([evaluation 2](evaluation.md#splits)).
- **Labelled** from the knowledge base's reviewed disease profiles, as `sure` or `probable`. Only `sure` cases count toward diagnosis accuracy, so a doubtful label can't punish a right answer ([evaluation 3](evaluation.md#how-cases-are-labelled)).
- **Reviewed twice.** A second teammate reviewed all 187 cases on Oct 22 against each case's photos and the profile's signs. Settling the review dropped 68: 40 iNaturalist cases whose host wasn't the case's plant, 8 with no readable plant, 17 repeats of a leaf already in the set, and 3 whose photos couldn't settle the reviewer's call. 60 more became `probable` ([#135](https://github.com/aacruzgon/GreenLens/pull/135)). 41 replacement cases, screened for the host, the signs and repeats and reviewed the same way, bring the set to 160: 104 dev and 56 test ([#140](https://github.com/aacruzgon/GreenLens/pull/140)). Neighbouring field-dataset frames then took one plant key, which moved 4 cases and left 102 dev and 58 test, with 18 dev and 9 test cases `sure` ([#147](https://github.com/aacruzgon/GreenLens/pull/147)). Pending.
- **Repeats across splits.** 4 of the 8 repeated leaves had cases in both dev and test while v3c's regularisation and temperature were chosen on dev ([#135](https://github.com/aacruzgon/GreenLens/pull/135)). One case per leaf was kept:
  - Two kept their test case: an early-blight leaf with 3 cases in dev, and a late-blight leaf with 5. So dev photos of these two test leaves helped choose v3c. Both test cases are `probable`, so they count toward species and the disease head's top 3, not diagnosis accuracy. M18 reports the disease head's top 3 with and without them.
  - Two kept their dev case, an early-blight leaf and a bacterial-spot leaf, so no test case shows them.
- **Dev cases moved to test.** Grouping photos by plant moved 7 dev cases to test after they had helped choose v3c, the VLM or the calibration: 4 when one observer's observations took one plant key ([#135](https://github.com/aacruzgon/GreenLens/pull/135)), and 3 when neighbouring frames did ([#147](https://github.com/aacruzgon/GreenLens/pull/147)). One is `sure`. M18 reports each test number with and without them.
- **Kept apart from training.** A leakage check compared the 160 cases' 203 photos with all 61,591 training images, the trained plant probe's 1,770 COCO photos among them, by perceptual hash (within 4 bits over the photo's 8 rotations and flips) and by BioCLIP 2 cosine (0.98 or more). The one match, at 0.983, is two different flowering tomato plants; the next closest pair is at 0.950 (M18's run on the final state, [report](../../ml/training/eval/reports/leakage-2026-10-23-m18.md); the Oct 20 run on the first 187 cases found the same, [report](../../ml/training/eval/reports/leakage-2026-10-20.md)). Edited copies of training images (halved, recompressed, rotated, mirrored, brightened) match in 98–100% of cases, but a copy with 5% cropped off each edge matches only 26% of the time, so a cropped copy could slip through ([evaluation 2](evaluation.md#where-the-photos-come-from)).
- **What it lacks.** Every case is a published photo. None has the full set of shots the app asks for, and none shows a ladybug, a face or a scene without a plant. So the VLM's organisms, soil and beneficial insects aren't measured on the set ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)), and neither is face blurring.

### Results so far: dev cases, held-out photos and benches

Each result says where it was measured. The dev numbers are M16's re-runs on the 102 dev cases after the second review and #147's regrouping ([#156](https://github.com/aacruzgon/GreenLens/pull/156)); the VLM's are on 72 of the 73 dev cases with a tomato or corn bench fixture (the 73rd came from test after the run, [vlm.md 13](vlm.md#re-check-on-the-shipped-prompts-2026-10-23--ran)). Targets are from [evaluation 4](evaluation.md#4-metrics-and-acceptance-targets): where a result is near one, it has its 95% Wilson interval, and a target counts as met or missed only when the whole interval is on one side.

- **Species, BioCLIP 2 zero-shot on the seed's matrix (dev):** the right species first on 75 of the 101 dev cases of the head's crops (apple, corn, grape and tomato; dev has no pepper case), 74.3% (65.0–81.8%), and in the top 3 on 91, 90.1% (82.7–94.5%). Both miss their targets, 85% and 95%, across the whole interval. The seed's 20 species are the stand-in matrix's, so it scores as the stand-in did. Tomato is the weak crop, 34 of 55, with 7 of the misses called potato and 3 each pepper and soybean; the one squash case is called cucumber. At `species_temperature` 1.3747, fitted on the 102, the 56 cases zero-shot puts at 0.8 or more are 98% right, so the agreement threshold stays 0.8 ([report](../../ml/training/eval/reports/species-check-2026-10-23.md), [calibration](../../ml/training/eval/reports/calibration-2026-10-23.md)).
- **Species head (dev):** 99.0% on the same cases. Averaged with zero-shot, the rule in [models 6](models.md#6-species-bioclip), it gets 77.2%, since 15 of the seed's 20 species are outside its crops. A better rule is still to choose, and the worker doesn't load a species head yet ([report](../../ml/training/eval/reports/species-check-2026-10-23.md)).
- **Disease head v3c (dev):** on the 72 dev leaf photos with a problem (not the same 72 as the VLM's cases), the true problem is in its top 3 on 67, 93% (84.8–97.0%), against 62% for guessing 3 of the species' classes. That is above the 85% target, but the interval crosses it, so the target is undecided at this count. On the 50 photos whose crop has more than 3 classes, where it can miss, it gets 90%, against 46%. Its top-1 is 48 of 89 photos, 54% ([evaluation 5](evaluation.md#results-on-the-102-dev-cases-2026-10-23--ran-offline-part), [report](../../ml/training/eval/reports/calibration-2026-10-23.md)). On the 115 cases before the review its top 3 was 84%, against 61 of 83 for v2c, the head before it ([report](../../ml/training/eval/reports/calibration-2026-10-20-garden.md)).
  - Tomato, with 8 classes, has the true problem in the top 3 on 35 of 39 photos, against 38% for guessing; 37 of the 39 are field-set photos (33 in the top 3). None of the iNaturalist garden tomato leaves that gave half of tomato's gain over v2c on Oct 20 is among these photos ([training 6](training.md#disease-head), [#135](https://github.com/aacruzgon/GreenLens/pull/135)).
  - The dev cases (the 115 before the review) chose its regularisation, so these numbers are partly in-sample; the value chosen wasn't that set's best, which keeps the bias small ([training 6](training.md#disease-head)).
  - The numbers assume the species is right, and send each one-photo case's photo as a leaf close-up, as 76 of the 89 photos are ([evaluation 5](evaluation.md#results-on-the-102-dev-cases-2026-10-23--ran-offline-part)).
  - Its temperature, which sets how often it claims a problem at medium confidence, is the head file's 1.82, which M17 ([#167](https://github.com/aacruzgon/GreenLens/pull/167)) kept over the dev fit, 1.29. At 1.82 no healthy leaf is blocked by a problem, and the tomato and corn photos claim a problem at medium confidence 4 times, 1 of them rightly; the apple and grape photos claim one at every temperature tried (22 claims, 20 right).
- **Plant check (held-out, not the evaluation set):** with the trained probe at `τ_leaf` 0.94 and `τ_plant` 0.31, none of 1,276 held-out plant photos fail and 3 of 1,000 COCO photos without a plant pass (0.3%), against targets of 1% and 10%; the zero-shot probe it replaced let 11.0% through. On the validation split it fails 0.99% of iNaturalist's whole-plant photos (the zero-shot probe 2.19%), and on the 102 dev cases every case passes ([models 2](models.md#2-plant-present-check)).
- **VLM (dev):** 72 of the 73 tomato and corn dev cases, one photo each, on the shipped prompts (`findings@5`, `indicators@3`), one job at a time ([vlm.md 13](vlm.md#re-check-on-the-shipped-prompts-2026-10-23--ran), [report](../../ml/training/eval/reports/vlm-bench-2026-10-23.md)):
  - both builds returned valid output on every call, on the first try;
  - on the 5070 Ti at 1 megapixel, Qwen3.5-9B AWQ took 5.1 s per job at the median and 6.4 s at p95 (Qwen3-VL-8B FP8: 6.5 and 7.6 s). With two at once, on Oct 20's bench, every build's p95 stayed at or under 9.6 s, against a 2.5-minute limit ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran));
  - from its indicator answers alone, it named the right problem on 3 of the 7 `sure` cases (16–75%), against 1 of 7 for FP8 (3–51%). Each case chooses among its fixture's 3 candidates, so a random pick is right about a third of the time; both intervals contain a third, so 7 cases can't tell either model from chance. It said "yes" to the true problem's questions on 4 of 21 and to the other candidates' on 5 of 132, so the diagnosis rests on the disease head's candidates until the questions improve. On Oct 20's labels and prompts it got 9 of 26 (35%), against 3 for both Qwen3-VL-8B builds;
  - on 14 of the 17 healthy leaves, its answers pointed to no disease (FP8: 16);
  - both report damage these disease leaves don't show: 84 damage findings from Qwen3.5 and 67 from FP8, none labelled (P11).
- **Text embedder (bench):** 4.7 ms per query on one thread of an Apple M3 Pro, against a 20 ms target on the control plane, whose CPU is likely 2–5 times slower per thread (*unverified*) ([models 10](models.md#10-text-embedder)). Its recall target, on 50 labelled question and passage pairs, isn't measured yet ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)).

### Test split, scored once by M18

The test cases: 58, of which 9 are `sure`: 5 corn and 4 tomato, 6 sick and 3 healthy ([#147](https://github.com/aacruzgon/GreenLens/pull/147)). M18 scored them once on Oct 23 with the final manifest's files at the values they ship, fitting and changing nothing ([report](../../ml/training/eval/reports/test-split-2026-10-23.md)). Each rate is k of n with its 95% Wilson interval. A target counts as met or missed only when the whole interval is on one side of it; otherwise it is undecided at that count ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)). The calibration error has no such interval, so it is set against a perfectly calibrated head with the same confidences instead (the same section). Each row names the cases it rests on. "Without the 7" leaves out the test cases that were dev cases when choices were made (section 3, above).

**Offline, from the scripts that score the dev cases:**

| ID | Stage | Metric | Target | Cases | Test result |
|---|---|---|---|---|---|
| T1 | Plant check | Recall on cases with a plant | 99% or more | Every test case | 58 of 58 (93.8–100%): undecided. Without the 7: 51 of 51 |
| T2 | Species, BioCLIP 2 | Top-1 on the five crops | 85% or more | Test cases of the five crops | 41 of 56, 73.2% (60.4–83.0%): missed. Without the 7: 36 of 50, 72.0% |
| T3 | Species, BioCLIP 2 | Top-3 on the five crops | 95% or more | As T2 | 52 of 56, 92.9% (83.0–97.2%): undecided. Without the 7: 46 of 50, 92.0% |
| T4 | Species, with the VLM | Cases where BioCLIP 2 (0.8 or more) and the VLM agree on a wrong species | 3% or less | Tomato and corn test cases | 0 of 41 (0–8.6%): undecided. Without the 7: 0 of 38 |
| T5 | Disease head | True problem in the top 3, per leaf, restricted to the species | 85% or more | Test leaf photos with a problem, `sure` and `probable`, as on dev; the `sure` ones beside them | 36 of 43, 83.7% (70.0–91.9%), against 60% guessing: undecided. Without the 7 and the 2 repeat leaves: 32 of 38, 84.2%. The `sure` photos: 5 of 5 (56.6–100%) |
| T6 | Disease head | Macro F1 of the top-1, per crop | Report | Test leaf photos | Apple 0.29 (5 photos), corn 0.46 (12), grape 1.00 (6), tomato 0.34 (28); top-1 26 of 51. Without the 7: apple 0.33 (4), corn 0.41 (10), grape 1.00 (6), tomato 0.33 (27) |
| T7 | Disease head | Expected calibration error at the temperature that ships | 0.05 or less | Test leaf photos | 0.210 on 51 photos at 1.82: undecided, since a perfectly calibrated head with the same confidences scores 0.165 on average and 0.210 or more in 12% of draws (dev 0.087). Without the 7: 0.204 on 47 |
| T8 | VLM | Calls that decode and validate on the first try | 99% or more; 100% after one retry | Tomato and corn test cases | 82 of 82 calls (95.5–100%): undecided; 41 of 41 jobs finished with no retry (91.4–100%): no failures, a check rather than a verdict. Without the 7: 76 of 76 calls, 38 of 38 jobs |
| T9 | VLM | Species guess, top-1 | Report beside BioCLIP 2's | Tomato and corn test cases | 40 of 41, 97.6% (87.4–99.6%); BioCLIP 2's top-1 on the same 41: 27. Without the 7: 37 of 38 |
| T10 | VLM | Job time at p95, two jobs at once | Under 2.5 minutes | Tomato and corn test cases | 8.8 s (median 6.3 s) on the 5070 Ti: met. Without the 7: 8.3 s |

**Through the API, and at upload:**

| ID | Stage | Metric | Target | Cases | Test result |
|---|---|---|---|---|---|
| T11 | Diagnosis | Precision of `confirmed`: the primary problem is a true one | 90% or more; 3 `confirmed` of 3 is 44–100% | The 9 `sure` test cases | Not measured: J5's end-to-end harness wasn't built |
| T12 | Diagnosis | Diseased cases that end `confirmed` | Report | The 6 diseased `sure` test cases | Not measured: as T11 |
| T13 | Diagnosis | Healthy plants diagnosed with a problem | 10% or less; one miss is 33%, and even 0 of 3 is 0–56% | The 3 healthy `sure` test cases | Not measured: as T11 |
| T14 | Diagnosis | Sick plants diagnosed `healthy` | 5% or less; one miss is 17%, and even 0 of 6 is 0–39% | The 6 diseased `sure` test cases | Not measured: as T11 |
| T15 | Timing | Submit to `diagnosed`, p95, GPU path | 90 s or less | Every test case | Not measured: as T11 |
| T16 | Timing | The same on the fallback worker | Report | Every test case | Not measured: as T11 |
| T17 | Upload | Good photos rejected by the blur and exposure check | 5% or less | Every test photo | 3 of 73, 4.1% (1.4–11.4%): undecided; 2 too blurry, 1 too bright. Without the 7: 3 of 65 |
| T18 | Upload | Plant photos with a false face blurred over them | 2% or less | Every test photo | 38 of 73, 52.1% (40.8–63.1%): missed (dev 67 of 130). Without the 7: 33 of 65 |

T17 and T18 come from the API's own quality gate and face detector at its settings (`vision`'s `face_check` example, YuNet at 0.3, on each photo as uploaded), not from a scan. T11–T16 need cases through the API, and J5's end-to-end harness wasn't built, so the rows above are each stage on its own, not the diagnosis a scan ends with.

What the test split says, against the dev cases:

- **It matches dev on species, the plant check, the VLM and the face boxes; the disease head's top 3 is lower.** Species zero-shot 73% top 1 and 93% top 3 (dev 74% and 90%), the plant check passing every case, the VLM valid on every call, and a face box on about half the photos (dev 52%). The disease head's top 3 is 84% against dev's 93%, which is above the test interval (70.0–91.9%); guessing is 60% here and 62% there. Leaving out the 7 former dev cases moves no rate by more than 1.3 points.
- **Tomato is the weak species again:** 14 of the 15 wrong top-1 species are tomato leaves, called potato 7 times and pepper 3. On the 41 tomato and corn cases the VLM names the species on 40, against BioCLIP 2's 27, and the two never agree on a wrong one.
- **The disease head's calibration can't be judged on 51 photos.** Its ECE is 0.210, against dev's 0.087, but a perfectly calibrated head with the same confidences scores 0.165 on average and 0.210 or more in 12% of draws, so this count can't tell the two apart. Its mean confidence matches its accuracy (0.50 against 26 of 51 right), though the 14 photos it scores 0.4–0.6 are right 4 times. The temperature stays the file's 1.82: the test split chooses nothing.
- **The face blur would cover half the plant photos.** At the shipped threshold, YuNet finds a face on 38 of the 73 test photos, all of them false, as on dev; the choice of a stricter setting is D13 ([#159](https://github.com/aacruzgon/GreenLens/issues/159)).

**Not measurable on this test split.** The rest of [evaluation 4](evaluation.md#4-metrics-and-acceptance-targets) needs cases or labels the set doesn't have. The manifest's truth holds each case's outcome, species, problems, organisms and damage, and every case's organisms and damage are empty ([manifest](../../ml/training/eval/manifest.jsonl)).

| Metric | Why not |
|---|---|
| Plant check specificity; no-plant and unusable cases ending correctly; blurred or dark photos caught | No no-plant, blurred or dark cases |
| Face blur recall | No face photos; they would be dev only ([evaluation 2](evaluation.md#splits)) |
| Leaf detector recall | No leaf boxes in the truth. Held-out mAP50 on PlantDoc is 0.935 ([roadmap 3](roadmap.md#oct-1819)) |
| Mask IoU | No hand-drawn masks |
| Species top 3 on `label_only` plants; such plants diagnosed with a crop's disease | Two test cases, both squash; no basil, lettuce, beans, roses or hostas |
| Same-plant retrieval from image embeddings | No repeat scans |
| VLM organisms, beneficials reported as pests, damage, soil, indicator answers, guided soil checks | No organism, damage, soil or indicator truth, and no soil or guided-check photos |
| LiDAR measurements | No LiDAR cases |
| Text embedder recall and latency on the control plane | Not part of the evaluation set; see the results above |

---

## 4. Challenges we ran into

*Devpost field: Challenges we ran into (the ML part).*

- **The VLM reads disease as insect damage.** Qwen3.5-9B calls a tomato's early-blight spots `leaf_mines`, the tunnels of a larva, under every findings prompt we tried, and returns other disease lesions as `other` damage. Changing one prompt line at a time showed how fragile the wording is: one damage section turned a corn leaf's blight into 3 `leaf_mines`, and asking for the species by its id took the soil readings from 8 to 0. The shipped prompt, `findings@5`, keeps only the changes that held, and the early-blight case moved to a structural fix (task P11), such as making each damage finding name the animal or trace that made it ([vlm.md 4](vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran), [dev-roadmap phase 4](dev-roadmap.md#phase-4-oct-2223-calibration-then-freeze-the-models)).
- **The disease head scored well on its own test photos and poorly on garden-like ones.** Its held-out top-1 per crop was 0.84–0.88, but on the dev leaf photos before the second review its top-1 was 36–40% ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)). Retraining with three sets of handheld field photos (v3c) raised its top 3 on those dev photos with a problem from 61 to 70 of 83. It cost Tomato-Pakistan's held-out top-1 (0.762 to 0.688), which we accepted because the app's photos are garden photos, and corn's dev top-1 on them (11 to 7 of 15; 9 of 17 on today's 102 dev cases), and 15 or 17 leaves are too few to tune on ([training 6](training.md#disease-head)).
- **Published datasets hide copies.** PlantCity ships each photo with flipped and turned copies under unrelated names, and its test archive repeats its training archive ([training 3](training.md#3-datasets-and-licences)). A review found 34 of PlantCity's 169 test photos in our first split were copies or other shots of training photos, so those heads were dropped and the copies grouped by perceptual hash, with turned copies compared in 1° steps ([training 4](training.md#4-data-preparation), [training 6](training.md#disease-head)). PlantDoc's own splits leak too: 13 of its 236 test images have a training image at cosine 0.98 or more, and only 4 of those pairs share a label ([training 3](training.md#3-datasets-and-licences)). The evaluation set's second review found 17 more cases repeating 8 leaves, turned, flipped or shot from below, 4 of those leaves in both dev and test (section 3).
- **iNaturalist files a disease under its pathogen.** So an observation of grape black rot or squash powdery mildew can show the fungus on another plant: 13 of 20 "grape black rot" cases were on Boston ivy, shagbark hickory or a saxifrage, 12 of 15 "squash powdery mildew" cases on cocklebur, beggarticks, New Zealand ribbonwood or pot marigold, and all 4 "cedar apple rust" cases were the galls on juniper. The second review dropped 40 such cases, and none of 93 newer cedar apple rust observations showed an apple ([#135](https://github.com/aacruzgon/GreenLens/pull/135), [#140](https://github.com/aacruzgon/GreenLens/pull/140)).
- **Licence-clean background photos.** The detector needed photos with nothing to find. Our first pick of 400 COCO photos ignored each photo's own Flickr licence, and 308 of them were non-commercial or no-derivatives ([training 5](training.md#5-leaf-detector)). The shipped detector trained on a new pick of 258 CC BY 2.0, 135 CC BY-SA 2.0 and 7 photos with no known copyright ([notices](../../ml/models/NOTICES.md#leaf_detector)). The detector from the first pick let about 5 points fewer no-plant photos through at the same rate of false "no plant"; whether that came from the pick or from chance between runs isn't measured ([models 2](models.md#2-plant-present-check)).
- **The int8 text embedder gave one text different vectors.** Its int8 files quantise activations per batch, so a text's vector depended on what it was batched with: cosine 0.9976–0.9983 between a text alone and in a batch of 64. The fp32 file gives 1.00000, so chat and ingestion both use fp32 ([models 10](models.md#the-pinned-recipe)).
- **FP8 isn't a clean VLM fallback.** Qwen3-VL-8B FP8 reads soil and calls no tomato `leaf_mines`, but it calls a corn leaf's blight `chewed_margins` in every run, boxed as the whole image, and puts a tomato's aphid and `shot_holes` in one shared box ([P5 runs](../../ml/training/eval/reports/p5-qwen35-2026-10-21.md#the-findings5-runs)).

---

## 5. Accomplishments

*Devpost field: Accomplishments that we're proud of (the ML part).*

- **Every third-party model we ship is Apache-2.0 or MIT,** and a dataset goes into our weights only when its licence allows commercial use and adaptation; the caveats, such as PlantDoc's scraped photos, are in the notices. We left out Ultralytics YOLO, DEIMv2, EdgeSAM, SegFormer, SCRFD's weights, DINOv3 and non-commercial datasets for their licences ([README 9](README.md#excluded)), and every model file ships with its [notices](../../ml/models/NOTICES.md).
- **The perception models run on one 8 GB card** (bench, on the RTX 3070): with the four ONNX sessions loaded at the manifest's batch caps, a scan of 6 photos and 40 leaf crops peaks at 4,598 MiB, leaves 3,234 MiB free, and takes 1.2 s ([evaluation 8](evaluation.md#perception-batch-size)).
- **An evaluation set kept apart from training:** checked against every training image, reviewed by two people, split by plant, and its test cases held back to be scored once (section 3).
- **A disease head that does its job on garden-like photos (dev):** the true problem in its top 3 on 67 of the 72 dev leaf photos with a problem (93%), against 62% for guessing, on the 102 dev cases after the second review (section 3).
- **The VLM benchmarked through the worker's own code,** not a notebook: every call valid, two jobs at once on one card (section 3).
- **The weaknesses are written down with their numbers,** beside the strengths, in the documents and here.
- **On the test split, scored once:** BioCLIP 2 names the species first on 41 of 56 cases (73%, short of its 85% target), the disease head has the true problem in its top 3 on 36 of 43 leaf photos with a problem (84%, against 60% for guessing), and the diagnosis through the API (T11) wasn't measured, since its end-to-end harness wasn't built (section 3).

---

## 6. What we learned

*Devpost field: What we learned (the ML part).*

- **A model's own test split overstates it.** The disease head's held-out scores said little about garden photos (section 4). Only photos from new fields and gardens tell you how a model will do.
- **Judge a model by its job.** The disease head proposes candidates that the profiles and the VLM then check, so we chose and judged it by its top 3, not its top-1 ([training 6](training.md#disease-head)).
- **Give every score its baseline.** With 3 candidates per case, the VLM's 9 of 26 `sure` diagnoses on Oct 20's set (35%) and 3 of 7 on the shipped prompts (43%) are both close to the third a random pick gets.
- **Change a prompt one line at a time, on fixed cases.** Changes made together hide which one broke what (section 4).
- **Distrust published splits and labels.** Copies cross splits under new names, and iNaturalist's disease observations can be on another host (section 4).
- **Quantization can break determinism,** not just accuracy: check that one input gives one output (section 4).
- **A licence is per file.** COCO's annotations are CC BY, but each photo keeps its own Flickr licence, and a detector's licence can reach the models trained with it ([README 9](README.md#9-licences-and-notices), [models 3](models.md#3-leaf-detector)).

---

## 7. What's next

*Devpost field: What's next for PDM (the ML part).*

- **Freeze and score:** the final manifest (M17, [#167](https://github.com/aacruzgon/GreenLens/pull/167)) and the test split scored once (M18, [report](../../ml/training/eval/reports/test-split-2026-10-23.md)); M16 refitted the temperatures on the 102 dev cases ([#156](https://github.com/aacruzgon/GreenLens/pull/156)) ([dev-roadmap](dev-roadmap.md#phase-4-oct-2223-calibration-then-freeze-the-models)).
- **Disease as damage:** the structural fix for the VLM's `leaf_mines` (P11).
- **The other four crops' classes** into `classifier_labels.yaml` once their profiles are signed ([#125](https://github.com/aacruzgon/GreenLens/issues/125)).
- **The species head in the worker,** with a rule that lets it lift its crops' share, which zero-shot gets right on only 74% of dev cases ([models 6](models.md#6-species-bioclip)).
- **Full shot sets from real gardens,** with beneficial insects, faces, no-plant shots and LiDAR, to measure what the published photos can't, and to check the trained plant probe on the app's own shots ([models 2](models.md#2-plant-present-check)).
- **More leaves found:** a relabelling pass over PlantDoc's training boxes, the change most likely to raise the detector's recall ([training 5](training.md#5-leaf-detector)).
- **A second VLM worker** on the RX 9070 XT ([README 4](README.md#4-placement-and-vram-budgets)).
- **Feedback as training data:** scans a reviewer marks wrong, with a corrected label, join the heads' training data after review, from gardens whose owners agreed ([training 9](training.md#9-feedback-as-training-data)).

---

## 8. Built with

*Devpost field: Built with (the ML entries).*

python, pytorch, onnx, onnx-runtime, cuda, scikit-learn, rf-detr, sam-2, bioclip-2, open-clip, qwen, vllm, yunet, snowflake-arctic-embed, hugging-face, inaturalist, opencv, numpy

| Tool | Version | Used for |
|---|---|---|
| PyTorch, torchvision | 2.14.1, 0.29.1 (CUDA 13.0 wheels) | Training and export |
| rfdetr | 1.11.2 | Fine-tuning the leaf detector |
| open-clip-torch | 3.3.0 | BioCLIP 2's towers in Python |
| scikit-learn | 1.9.1 | The heads and the plant probe |
| ONNX, ONNX Runtime | 1.22.0, 1.31.0 | Export, checks, and inference on the RTX 3070 and the control plane's CPU |
| vLLM | 0.31.0, on CUDA 13.0.3 | Serving the VLM |
| Hugging Face Hub | 2.2.0 | Fetching models; our private model repository |

Sources: [`ml/requirements.lock`](../../ml/requirements.lock), [README 5](README.md#5-runtimes-and-versions).

---

## 9. Prior work

*Devpost field: prior work (the ML part).*

Build North asks teams to disclose any prior work, templates or scaffolding, and to build meaningful new work during build week, which began with the kickoff at 12:00 ET on Oct 16 ([the event's rules](https://luma.com/build-north), read 2026-10-22).

- **Before build week, Oct 15:** the repository started at 12:25 on Oct 15 ([987a13f](https://github.com/aacruzgon/GreenLens/commit/987a13f)). That day's architecture already planned the ML pipeline: BioCLIP for species, a leaf detector tuned on PlantDoc, SAM-class masks, a quantized 7–8 B VLM, and retrieve-then-verify against curated disease profiles. The vision design's first version is dated Oct 15 in its status lines, as are its readings of model cards and licences.
- **Committed at kickoff:** `docs/vision` was first committed at 12:03 on Oct 16, already at v0.2.3 ([bd499e9](https://github.com/aacruzgon/GreenLens/commit/bd499e9)). It records runs on a Mac laptop with ONNX Runtime on CPU: a BioCLIP v1 export and linear probes on PlantDoc, the text-embedder benchmark (the 4.7 ms above, on an Apple M3 Pro) and YuNet on a group photo. Some may have run on Oct 15.
- **During build week:** all the ML code, the data preparation, every trained or fine-tuned model, the exports, the evaluation set and every other number in this write-up. The first commit under `ml/` is [e0612b8](https://github.com/aacruzgon/GreenLens/commit/e0612b8), at 13:41 on Oct 16.

Everything we didn't make is below, with its licence. Section 2 says what we did with each model. Full citations are in [`NOTICES.md`](../../ml/models/NOTICES.md).

**Pretrained models**

| Model | Authors | Licence |
|---|---|---|
| RF-DETR-S, COCO weights | Roboflow | Apache-2.0 |
| SAM 2.1 Hiera-T | Meta | Apache-2.0 |
| BioCLIP 2 | Imageomics Institute | MIT |
| Qwen3.5-9B, and the community AWQ quantization `cyankiwi/Qwen3.5-9B-AWQ-4bit` | Qwen; cyankiwi | Apache-2.0 |
| Qwen3-VL-8B-Instruct, FP8 | Qwen | Apache-2.0 |
| YuNet `2026may` | Wu, Peng, Yu (OpenCV Zoo) | MIT |
| snowflake-arctic-embed-xs | Snowflake | Apache-2.0 |
| `claude-opus-5-5`, `claude-haiku-5-5` | Anthropic | Hosted API |

Sources: [README 9](README.md#9-licences-and-notices), [NOTICES](../../ml/models/NOTICES.md).

**Datasets in our weights**

| Dataset | Licence | What we did |
|---|---|---|
| PlantDoc, classification and detection | CC BY 4.0 for the annotations; the scraped photos keep their owners' copyright *unverified* | Deduplicated, labels mapped to the knowledge base, detection boxes collapsed to `leaf` |
| TOM2024 | CC BY 4.0 | Deduplicated, mapped, split by field visit |
| Tomato-Pakistan 2026 | CC BY 4.0 as published; it can't cover photos the authors didn't take | Each file is an augmented copy of about 934 originals; the 302 that are PlantDoc's photos are dropped. Other scraped photos may keep their owners' copyright *unverified*, so shipping weights trained on it is a team decision |
| PlantCity | CC BY 4.0 | Copies grouped and deduplicated; its gray leaf spot class dropped |
| Diseases of maize in the field (University of Pretoria) | CC BY 4.0 | Single-label photos, deduplicated |
| Makerere University Maize Image Dataset | CC0 1.0, credited | A fixed sample of 600 per class |
| iNaturalist, training photos | CC0 or CC BY 4.0 per photo, each credited in `ml/models/credits/` | Whole plants, flowers and fruit for the trained plant probe; the species head isn't shipped yet |
| COCO 2017, 400 + 2,372 photos | Annotations CC BY 4.0; only photos that are CC BY 2.0, CC BY-SA 2.0 or have no known copyright, picked per image, each credited in `ml/models/credits/` | The detector's photos with no leaf, and the trained plant probe's photos without a plant |
| iBean | MIT | Bean leaves as plant photos for the trained plant probe |

Sources: [training 3](training.md#3-datasets-and-licences), [NOTICES, Datasets](../../ml/models/NOTICES.md#datasets).

**Datasets for evaluation only, in no shipped weights:** half of COCO val2017's 4,758 photos without a plant (the tune half, 2,386) and 1,000 other COCO train2017 photos, as photos without a plant for the plant check; four CC BY 4.0 field datasets for the evaluation cases (maize from Bangladesh, 2026; two sets of tomato from Bangladesh, 2025; tomato from Ghana, 2023); and CC0 and CC BY iNaturalist observations from 2026 ([training 3](training.md#3-datasets-and-licences), [evaluation 2](evaluation.md#where-the-photos-come-from)). iBean (MIT) is in the plant probe but not in v3c's training data.

**The pretrained models' own training data,** which we didn't choose: for BioCLIP 2, TreeOfLife-200M (CC0 as a compilation, though its images may keep their own terms), starting from a CLIP ViT-L/14 pre-trained on LAION-2B; for SAM 2.1, SA-1B and SA-V (*unverified*); for YuNet, WIDER FACE (CC BY-NC-ND 4.0); and for RF-DETR's base weights, COCO. Whether a dataset's terms reach a model trained on it is unsettled; the notices say so, and the project's own licence is still for the team to choose ([README 9](README.md#9-licences-and-notices)).

---

## 10. For M18

M18 filled every placeholder on Oct 23 from one run, [test-split-2026-10-23](../../ml/training/eval/reports/test-split-2026-10-23.md), written by `python -m ml.training.eval.score_test_split`. It runs `species_check`'s, `calibrate`'s, `plant_check`'s, `vlm_bench`'s and `face_check`'s code on the test cases at the values the final manifest ships, and fits nothing; on the dev split it reproduced every committed dev number first. The script that ran it once is committed ([m18.sh](../../ml/training/eval/m18.sh)), and the report says how the run went, with its first attempt's slip. After Alan's review the report was rebuilt from its own JSON, with no photo or model run again: every count is unchanged, and T7's verdict went from missed to undecided. The checks below are kept as the record, and the pasting checks are left.

Before scoring:

- [x] #134, #135 and #140 are merged, and no count is marked pending any more. Replacing their links is a pasting check, below.
- [x] Every dev number in sections 2 to 6 is re-run on the 102 dev cases after #147, and updated:
  - `species_check` with the seed's matrix: zero-shot, the species head and their average; done in #156 ([report](../../ml/training/eval/reports/species-check-2026-10-23.md));
  - `calibrate` on v3c: the disease head's top 3, its top-1 and its temperature (M16); done in #156 ([report](../../ml/training/eval/reports/calibration-2026-10-23.md)), with the temperature's choice left to M17;
  - `vlm_bench` on `findings@5`, the rerun [vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran) promises; done in #156 ([vlm.md 13](vlm.md#re-check-on-the-shipped-prompts-2026-10-23--ran));
  - `plant_check` on the test cases at M17's pair: the trained probe v2 at 0.94 / 0.31; done by M18 ([report](../../ml/training/eval/reports/plant-check-test-cases-2026-10-23.md)).
- [ ] No disputed case feeds `calibrate`'s fits: #135 drops every case the reviewer called wrong. Its fits still read `probable` cases; each dev number says so, or `calibrate` skips them.
- [x] M16's and M17's final values replace the word "provisional" for the temperatures and `τ_leaf` and `τ_plant` in section 3.
- [x] Each test number is reported twice, in section 3 and in the report: on all the test cases, and without the 7 that were dev cases when choices were made (`apple-inat-401743982`, `apple-inat-401742038`, `apple-inat-401735945`, `squash-inat-407881529`, `corn-maize-bd-2026-gray-leaf-spot-img-500`, `tomato-tomato-bd-2025-late-blight-late-blight-7`, `corn-maize-bd-2026-healthy-leaf-img-43`). T5's second line also leaves out the two repeat leaves.
- [x] `python -m ml.training.eval.check_leakage` on the final manifest passes: its report's header reads "0 not allowed". Update section 3's photo and image counts from it. Done: 61,591 training images, the probe's 1,770 among them, 1 match allowed ([report](../../ml/training/eval/reports/leakage-2026-10-23-m18.md)).
- [ ] Each stage in section 1 runs live through the API. Real scans return 503 until #134 merges, and P2 (the perception harness) and J5 (the end-to-end harness) haven't started. A stage that doesn't run live by Oct 25 is described as built, not as running.

Before pasting:

- [ ] Make every link absolute (`https://github.com/aacruzgon/GreenLens/blob/main/…`); relative links don't resolve on Devpost.
- [ ] Replace each link to #134, #135 and #140 with the document or report it added, where there is one.

How each placeholder was filled, all by `score_test_split` from the run named:

| Placeholder | Script or report | Field |
|---|---|---|
| Report link | The committed test report | Its path, linked from section 3's "Test split" paragraph |
| T1 | `plant_check` run on the test cases' plant photos at M17's pair | 1 minus the share of cases whose plant photos all fail (false `no_plant_found`), per case |
| T2, T3 | `species_check` on the test cases with the final matrix | The "per case, zero-shot" row's Top-1 and Top-3; the "head" or "average" row instead if the worker runs the species head by then |
| T4 | `calibrate`'s species stage at the final `species_temperature`, joined per case with `vlm_bench`'s species guesses | Share of cases with BioCLIP 2's top-1 at 0.8 or more, the VLM naming the same species at medium or high, and both wrong, over the VLM's finished jobs |
| T5 | `calibrate` with the final disease head, on test leaf photos with a problem, `sure` and `probable` | "Top 3 as sent" with its guessing in brackets; then the same on the `sure` photos alone, and without the two test leaves whose repeats were in dev (`tomato-tomato-bd-iphone-2025-early-blight-img-2624`, `tomato-tomato-bd-iphone-2025-late-blight-img-5537-02d`) |
| T6 | `calibrate`'s JSON, its top-1 answers per true class | Macro F1 per crop, computed from those answers: the mean over the classes that are a photo's label or its top 1 |
| T7 | `calibrate` with M17's head file, on the test leaf photos | `ece` (15 bins) at the temperature that ships, set against a perfectly calibrated head with the same confidences, 20,000 draws ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)). Never a temperature fitted on test |
| T8 | `vlm_bench cases --split test`, `run` with the final model and pixel limit at 2 at once, then `score` | Calls whose first try had an error or was tried again, over every call of every job that ran (finished or not); "Finished" after one retry is a check for zero failures |
| T9 | Same `vlm_bench` report | "Species" |
| T10 | Same `vlm_bench` report | "Job p95" at "At once" 2 |
| T11–T16 | Not measured: they need J5's end-to-end run through the API (`e2e.py`, `score.py`, [evaluation 9](evaluation.md#9-harness-and-reports)), which wasn't built | Evaluation 4's end-to-end metrics: `confirmed` precision, coverage, healthy with a problem and sick called healthy on the `sure` cases; p95 wall time on the GPU path and on the fallback on every case |
| T17 | `face_check run --sets test --variants original --thresholds 0.3`: the API's quality gate on each photo as uploaded | Share of test photos the blur and exposure check rejects |
| T18 | The same run: YuNet at the API's settings over the test photos | Share of plant photos with a face box |
| Section 5's last line | This file's T2, T5 and T11 | One plain sentence, each number with its n |

A row whose cases or harness don't exist when M18 runs gets "not measured" and the reason. Every test number goes in section 3's tables and section 5's last line, nowhere else.
