# PDM Devpost Write-up: the ML Part

> **Status:** Draft v0.1, 2026-10-22. v0.1: First draft of the ML part of each Devpost field (dev-roadmap M19): what the models do on a scan, how they were built and measured, challenges, accomplishments, lessons, next steps, built with and prior work, with the test split's numbers left as placeholders for M18.

This file holds the machine-learning part of each field on PDM's Devpost page, ready to paste; each section names its field. The app, the backend and the farm connectors are written up elsewhere. Every number links its source: a section of the vision documents, a committed report, or an open pull request ([#134](https://github.com/aacruzgon/GreenLens/pull/134), [#135](https://github.com/aacruzgon/GreenLens/pull/135)) where the fact isn't on `main` yet. Three kinds of number appear, always labelled:

- **Dev:** the evaluation set's dev cases. We chose and tuned the models on them, so they flatter the models.
- **Held-out:** photos set aside from the training sources: the training datasets' own test photos, and for the plant check 1,000 COCO photos no threshold was chosen on. They come from the same cameras and fields as the training photos, and they are not the evaluation set.
- **Test:** the evaluation set's test cases, scored once by task M18 ([dev-roadmap, phase 5](dev-roadmap.md#phase-5-oct-2425-score-once-ship)). Until then every test number is a placeholder `⟨M18: …⟩`, and [section 10](#10-for-m18) says what fills each one. A placeholder M18 can't fill becomes "not measured" with the reason, never an estimate.

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

A scan is a few photos of one plant: the whole plant, leaf close-ups, a leaf's underside, the soil. The photos pass through these models, each pinned by hash in one manifest ([README 6](README.md#6-model-files-versions-and-pinning)):

1. **Face blurring, on upload.** Before a photo is stored, YuNet looks for faces on the control plane's CPU, on the whole photo at 1,280 pixels and on full-resolution tiles, and the API blurs what it finds. The upload fails closed: a photo isn't stored until blurring succeeds ([models 9](models.md#9-face-detector)).
2. **Leaf detector.** On the RTX 3070, RF-DETR-S, which we fine-tuned to one class, `leaf`, boxes every leaf ([models 3](models.md#3-leaf-detector)).
3. **Plant-present check.** A scan goes on if any plant photo has a leaf box scoring at least `τ_leaf`, or a BioCLIP 2 plant probe scoring at least `τ_plant`; otherwise it ends with `no_plant_found`. The check leans towards "present", because a false "no plant" kills a real scan ([models 2](models.md#2-plant-present-check)).
4. **Masks.** SAM 2.1 Hiera-T, prompted with the leaf boxes, outlines each leaf and the plant ([models 4](models.md#4-mask-model)).
5. **Species.** BioCLIP 2 embeds every plant photo and compares it, zero-shot, with a label matrix: its own text embedding of each species and organism in the team's knowledge seed, averaged over 80 prompt templates ([models 6](models.md#6-species-bioclip); the seed's matrix is in [#134](https://github.com/aacruzgon/GreenLens/pull/134)). The photos' scores are combined, and the top 3 species go to the app. We also trained a species head on the same embeddings for the five crops below. It is measured, but the worker doesn't run it yet ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
6. **Disease head.** BioCLIP 2 embeds each leaf crop, and our disease head, a logistic regression on that embedding, scores 19 problem-or-healthy classes on five crops: apple, corn, grape, pepper and tomato. Its scores are restricted to the plant's species, and each leaf's top 3 become candidate diagnoses ([models 7](models.md#7-disease-classifier)).
7. **The VLM's findings.** On the RTX 5070 Ti, Qwen3.5-9B (4-bit AWQ, served by vLLM, thinking off) reads the photos and returns JSON held to a schema: soil readings, organisms with boxes, damage patterns, a species cross-check, and a yes, no or unsure answer to each candidate diagnosis's indicator questions ([vlm.md](vlm.md)). When no GPU worker is free, a hosted model does the same job in the same shapes ([README 3](README.md#3-model-inventory)).
8. **Diagnosis.** The API checks the candidates against curated disease profiles, using the VLM's answers. It confirms one only when it leads the next by a margin, and otherwise reports it as uncertain and asks a question ([architecture 6.3](../architecture/README.md#63-retrieve-then-verify)). No single model makes the call.
9. **Text embedder for chat and search.** snowflake-arctic-embed-xs, on the control plane's CPU, embeds chat questions, memories and knowledge-base passages, so the chat can find and cite its sources ([models 10](models.md#10-text-embedder)).

BioCLIP 2 does four jobs on the 3070 from one session: species, the plant probe, the disease head's features, and image embeddings for finding similar past scans ([models 6](models.md#6-species-bioclip)).

---

## 2. How we built it

*Devpost field: How we built it (the ML part).*

| Model | Job | Runs on | Licence | What we did |
|---|---|---|---|---|
| RF-DETR-S (Roboflow) | Leaf boxes; one signal of the plant check | RTX 3070, fp16 ONNX | Apache-2.0 | Fine-tuned to one class, `leaf` |
| SAM 2.1 Hiera-T (Meta) | Leaf and plant masks | RTX 3070, fp16 ONNX | Apache-2.0 | Used as is; our own export to two ONNX graphs |
| BioCLIP 2 (Imageomics) | Species, plant probe, features for our heads, image embeddings | RTX 3070, fp16 ONNX vision tower | MIT | Used as is; its text tower builds our label matrix |
| Disease head (ours) | Problem or healthy, per leaf crop | The perception worker's CPU | Ours | Trained: logistic regression on BioCLIP 2 features, 19 classes |
| Plant probe (ours) | Plant or not, per photo | The perception worker's CPU | Ours | Made from BioCLIP 2 text prompts, with no images; a trained probe is measured, not shipped |
| Species head (ours) | Species of the five crops | Not in the worker yet | Ours | Trained: logistic regression on BioCLIP 2 features |
| Qwen3.5-9B, as `cyankiwi/Qwen3.5-9B-AWQ-4bit` | Findings and indicator answers | RTX 5070 Ti, vLLM 0.31.0 | Apache-2.0 | Used as is, a community 4-bit quantization; our prompts and JSON schemas |
| Qwen3-VL-8B-Instruct | The VLM's fallback, by configuration | RTX 5070 Ti, vLLM 0.31.0 | Apache-2.0 | Used as is |
| YuNet `2026may` (OpenCV Zoo) | Faces to blur | Control plane CPU | MIT | Used as is |
| snowflake-arctic-embed-xs | Text embeddings, 384 dimensions | Control plane CPU | Apache-2.0 | Used as is, in fp32 |
| `claude-opus-5-5` | Vision fallback when no GPU worker is free | Fallback worker | Hosted API | Used as is, with the same output shapes |
| `claude-haiku-5-5`, `claude-opus-5-5` | Tagging and drafting knowledge-base entries, which a teammate reviews | Developers' machines | Hosted API | Used as is |

Sources: [README 3](README.md#3-model-inventory) (inventory), [4](README.md#4-placement-and-vram-budgets) (placement) and [9](README.md#9-licences-and-notices) (licences). Our own heads and probe will carry the project's licence, which the team hasn't chosen yet ([README 9](README.md#for-the-team-to-decide)).

**Placement.** The perception models share the 8 GB RTX 3070: with the four ONNX sessions loaded at the manifest's batch caps, a scan of 6 photos and 40 leaf crops peaks at 4,598 MiB, leaves 3,234 MiB free, and takes 1.2 s ([evaluation 8](evaluation.md#perception-batch-size)). The VLM has the 16 GB RTX 5070 Ti to itself, where two jobs at once fit ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)). A second VLM worker on the rig's RX 9070 XT is planned; nothing has run Qwen3.5 on it yet ([README 4](README.md#4-placement-and-vram-budgets)). Our model files live in a private Hugging Face model repository and reach the hosts pinned by SHA-256 in [`manifest.toml`](../../ml/models/manifest.toml), with a [notices file](../../ml/models/NOTICES.md) for every entry ([README 6](README.md#6-model-files-versions-and-pinning)).

**Leaf detector.** We chose RF-DETR-S because the YOLO models of the same class are AGPL-3.0, and Ultralytics states that models trained with them are too; RF-DETR-S is Apache-2.0 and scores 53.0 COCO AP at 3.5 ms on a T4 ([models 3](models.md#3-leaf-detector)). We fine-tuned it on PlantDoc's detection boxes with every class collapsed to `leaf`, after clipping 4 out-of-bounds boxes and dropping 4 empty ones ([training 5](training.md#5-leaf-detector)). Trained on PlantDoc alone, it had never seen a photo with nothing to find: its leaf score alone, at 0.35, passed 51.9% of photos without a plant. Retrained with 400 COCO photos with no plant pixel, shown with no box, it passed 6.2% ([models 2](models.md#2-plant-present-check)). Held-out: mAP50 0.935 on PlantDoc's 236 test photos, the same in PyTorch and in the fp16 ONNX file the worker loads ([roadmap 3](roadmap.md#oct-1819)).

**Heads on frozen features.** A logistic regression on BioCLIP 2's embedding trains in seconds on a CPU and adds no model to the GPU; the worker applies it in Rust ([training 6](training.md#6-disease-and-species-heads)). The disease head trains on the crops the worker will see: our detector's boxes, grown and rounded exactly as the worker cuts them.
- **Disease head v3c,** the one shipped, has 19 classes on five crops. It trained on 5,331 photos from PlantDoc, TOM2024, Tomato-Pakistan and three sets of handheld field photos: PlantCity, Pretoria's maize and Makerere's maize. Its regularisation was chosen first by how often the true problem is in its top 3 on the dev cases, its job in the pipeline, and only then by top-1 on its validation photos ([training 6](training.md#disease-head)). Held-out: top-1 0.850 per crop ([training 6](training.md#6-disease-and-species-heads)), which section 3 shows is optimistic for garden photos.
- **Its classes map to the knowledge base** through `classifier_labels.yaml`. Tomato's 8 classes map now; apple's, corn's, grape's and pepper's wait on their problems' profiles in [#125](https://github.com/aacruzgon/GreenLens/issues/125) ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
- **The species head** trained on whole-photo embeddings of the training photos, plus 1,115 CC0 and CC BY iNaturalist photos of the five crops' whole plants, flowers and fruit ([training 3](training.md#3-datasets-and-licences), [training 6](training.md#species-head)).

**The VLM.** Of the open 8–9 B vision-language models, Qwen3-VL-8B and Qwen3.5-9B were the only ones with documented box grounding in a JSON form (`bbox_2d`). Both are Apache-2.0 and share an image processor and box scale, so switching between them is configuration, not code ([vlm.md 2](vlm.md#2-model-choice)). We ran three builds through the worker's own calls (its prompts, schemas, image preparation and validation) on the 65 tomato and corn dev cases, and chose Qwen3.5-9B AWQ: its indicator answers were the only ones with any gap between the true problem's questions and the other candidates', and it was as fast ([vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran)).

**The text embedder.** arctic-embed-xs over arctic-embed-s: the same tokenizer and dimension at half the latency, 4.7 ms per query on one Apple M3 thread, in fp32 for the reason in section 4 ([models 10](models.md#10-text-embedder)).

**The evaluation set.** 187 cases built for this: 103 one-photo cases from four published tomato and corn field datasets that no model trained on, and 84 iNaturalist observations of apple, grape, squash and tomato from 2026, newer than BioCLIP 2's training data ([evaluation 2](evaluation.md#where-the-photos-come-from), [roadmap 1](roadmap.md#1-where-we-are)). A second review cut it to 122 (section 3).

---

## 3. How we measured it

*Devpost field: How we built it (continued), or Accomplishments.*

### The evaluation set

- **A case is one scan** as the app would send it: its photos, each with its kind, and the truth for that plant ([evaluation 2](evaluation.md#2-the-evaluation-set)).
- **Split by plant** into dev and test, so two photos of one plant are always on the same side. The test cases are scored once, and no threshold is tuned on them ([evaluation 2](evaluation.md#splits)).
- **Labelled** from the knowledge base's reviewed disease profiles, as `sure` or `probable`. Only `sure` cases count toward diagnosis accuracy, so a doubtful label can't punish a right answer ([evaluation 3](evaluation.md#how-cases-are-labelled)).
- **Reviewed twice.** A second teammate reviewed all 187 cases on Oct 22. Settling the review dropped 65: 40 iNaturalist cases whose host wasn't the case's plant, 8 with no readable plant, and 17 field photos of a leaf already in the set. 122 cases remain, 80 dev and 42 test; 37 are `sure`, 11 of them in test (6 corn, 4 tomato, 1 apple) ([#135](https://github.com/aacruzgon/GreenLens/pull/135)).
- **Kept apart from training.** A leakage check compared the set's 253 photos with all 59,821 training images, by perceptual hash (within 4 bits over the photo's 8 rotations and flips) and by BioCLIP 2 cosine (0.98 or more). The one match, at 0.983, is two different flowering tomato plants; the next closest pair is at 0.950. Edited copies of training images (halved, recompressed, rotated, mirrored, brightened) match in 98–100% of cases, but a copy with 5% cropped off each edge matches only 26% of the time, so a cropped copy could slip through ([evaluation 2](evaluation.md#where-the-photos-come-from), [report](../../ml/training/eval/reports/leakage-2026-10-20.md)). M18 runs the check again on the final manifest before scoring.
- **What it lacks.** Every case is a published photo; none has the full set of shots the app asks for, and none shows a ladybug. The team's own garden photos, with full shot sets, faces and no-plant shots, hadn't arrived by Oct 21 ([roadmap 1](roadmap.md#1-where-we-are)). So the VLM's organisms, soil and beneficial insects aren't measured on the set ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)), and neither is face blurring.

### Results so far: dev cases and held-out photos

Each result says where it was measured. The dev numbers were measured on the 115 dev cases before the second review, which dropped 35 of them; they are re-run on the 80 kept ([#135](https://github.com/aacruzgon/GreenLens/pull/135)). Targets are from [evaluation 4](evaluation.md#4-metrics-and-acceptance-targets).

- **Species, BioCLIP 2 zero-shot on the seed's matrix (dev):** the right species first on 77.1% of the 105 dev cases of the five crops, and in the top 3 on 96.2%; the 20-species stand-in matrix it replaces got 61.0%. Target: 85% top-1. Tomato is the weak crop, 42 of 63, with 11 of the misses called pepper and 7 grape. The 10 squash cases can't be named, since squash isn't in the seed yet ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
- **Species head (dev):** 90.5% on the same cases, and 87.6% averaged with zero-shot. Not in the worker yet ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
- **Disease head v3c (dev):** on the 83 dev leaf photos with a problem, the true problem is in its top 3 on 70 (84%), against 57% for guessing 3 of the species' classes and 61 of 83 for the head before it, v2c. On the 12 iNaturalist garden tomato leaves, the photos most like the app's, 8, against v2c's 3. Target: 85%. Its top-1 is 41% of 94 photos, and corn's top-1 fell from 11 to 7 of 15 ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part), [report](../../ml/training/eval/reports/calibration-2026-10-20-garden.md)).
  - The dev cases chose its regularisation, so the 84% is in-sample; the value chosen isn't dev's best (87%), which keeps that bias small ([training 6](training.md#disease-head)).
  - The numbers assume the species is right, and send each one-photo case's photo as a leaf close-up ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)).
  - Its temperature, which sets how often it claims a problem at medium confidence, is provisional: M17 chooses the final one.
- **Plant check (held-out, not the evaluation set):** at the provisional pair `τ_leaf` 0.40 and `τ_plant` 0.70, 0.23% of 854 held-out plant photos fail and 11.0% of 1,000 COCO photos without a plant pass, against targets of 1% and 10%. About half of those that pass show trees, hedges or grass. A trained probe passes 0.1% with no plant photo failing, and ships once it is checked on the team's own photos ([models 2](models.md#2-plant-present-check)).
- **VLM (dev):** the 65 tomato and corn dev cases, one photo each, on the 5070 Ti at 1 megapixel, with the prompts of Oct 20 ([evaluation 8](evaluation.md#vlm-results-2026-10-20--ran)):
  - all three builds returned valid output on every call, on the first try;
  - Qwen3.5-9B AWQ took 5.3 s per job at the median and 6.4 s at p95, one job at a time; with two at once, every build's p95 stayed at or under 9.6 s, against a 2.5-minute limit;
  - from its indicator answers alone it named the right problem on 35% of the 26 `sure` cases, against 12% for both Qwen3-VL-8B builds. On a diseased leaf it said "yes" to that disease's questions 15% of the time and to the other candidates' 10%. So the diagnosis rests on the disease head's candidates until the questions improve;
  - on 10 of the 11 healthy leaves (91%), its answers pointed to no disease.
- **Text embedder:** 4.7 ms per query on one Apple M3 thread, against a 20 ms target on the control plane, whose CPU is likely 2–5 times slower per thread (*unverified*) ([models 10](models.md#10-text-embedder)). Its recall target, on 50 labelled question and passage pairs, isn't measured yet ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)).

### Test split, scored once by M18

The test cases after the second review: 42, of which 11 are `sure` ([#135](https://github.com/aacruzgon/GreenLens/pull/135)). Each row names the cases it rests on; M18 gives the count. Report: `⟨M18: report link⟩`.

**Offline, from the scripts that score the dev cases:**

| ID | Stage | Metric | Target | Cases | Test result |
|---|---|---|---|---|---|
| T1 | Plant check | Recall on cases with a plant | 99% or more | Every test case | `⟨M18: T1, % (n)⟩` |
| T2 | Species, BioCLIP 2 | Top-1 on the five crops | 85% or more | Test cases of the five crops | `⟨M18: T2, % (n)⟩` |
| T3 | Species, BioCLIP 2 | Top-3 on the five crops | 95% or more | As T2 | `⟨M18: T3, % (n)⟩` |
| T4 | Species, with the VLM | Cases where BioCLIP 2 (0.8 or more) and the VLM agree on a wrong species | 3% or less | Tomato and corn test cases | `⟨M18: T4, % (n)⟩` |
| T5 | Disease head | True problem in the top 3, per leaf, restricted to the species | 85% or more | Test leaf photos with a problem | `⟨M18: T5, % (n), guessing %⟩` |
| T6 | Disease head | Macro F1 of the top-1, per crop | Report | Test leaf photos | `⟨M18: T6, F1 per crop⟩` |
| T7 | Disease head | Expected calibration error at the final temperature | 0.05 or less | Test leaf photos | `⟨M18: T7, ECE⟩` |
| T8 | VLM | Calls that decode and validate on the first try | 99% or more; 100% after one retry | Tomato and corn test cases | `⟨M18: T8, % of calls⟩` |
| T9 | VLM | Species guess, top-1 | Report beside BioCLIP 2's | Tomato and corn test cases | `⟨M18: T9, % (n)⟩` |
| T10 | VLM | Job time at p95, two jobs at once | Under 2.5 minutes | Tomato and corn test cases | `⟨M18: T10, s⟩` |

**Through the API, once the end-to-end harness (J5) runs:**

| ID | Stage | Metric | Target | Cases | Test result |
|---|---|---|---|---|---|
| T11 | Diagnosis | Precision of `confirmed`: the primary problem is a true one | 90% or more | The `sure` test cases | `⟨M18: T11, % (n)⟩` |
| T12 | Diagnosis | Diseased cases that end `confirmed` | Report | Diseased `sure` test cases | `⟨M18: T12, % (n)⟩` |
| T13 | Diagnosis | Healthy plants diagnosed with a problem | 10% or less | Healthy test cases | `⟨M18: T13, % (n)⟩` |
| T14 | Diagnosis | Sick plants diagnosed `healthy` | 5% or less | Diseased test cases | `⟨M18: T14, % (n)⟩` |
| T15 | Timing | Submit to `diagnosed`, p95, GPU path | 90 s or less | Every test case | `⟨M18: T15, s⟩` |
| T16 | Timing | The same on the fallback worker | Report | Every test case | `⟨M18: T16, s⟩` |
| T17 | Upload | Good photos rejected by the blur and exposure check | 5% or less | Every test photo | `⟨M18: T17, % (n)⟩` |
| T18 | Upload | Plant photos with a false face blurred over them | 2% or less | Every test photo | `⟨M18: T18, % (n)⟩` |

**Not measurable on this test split.** The rest of [evaluation 4](evaluation.md#4-metrics-and-acceptance-targets) needs cases or labels the set doesn't have. The manifest's truth holds each case's outcome, species, problems, organisms and damage, and every case's organisms and damage are empty ([manifest](../../ml/training/eval/manifest.jsonl)).

| Metric | Why not |
|---|---|
| Plant check specificity; no-plant and unusable cases ending correctly; blurred or dark photos caught | No no-plant, blurred or dark cases |
| Face blur recall | No face photos; they would be dev only ([evaluation 2](evaluation.md#splits)) |
| Leaf detector recall | No leaf boxes in the truth. Held-out mAP50 on PlantDoc is 0.935 ([roadmap 3](roadmap.md#oct-1819)) |
| Mask IoU | No hand-drawn masks |
| Species top 3 on `label_only` plants; such plants diagnosed with a crop's disease | No cases of the garden plants outside the crops (basil, lettuce, beans, roses, hostas) |
| Same-plant retrieval from image embeddings | No repeat scans |
| VLM organisms, beneficials reported as pests, damage, soil, indicator answers, guided soil checks | No organism, damage, soil or indicator truth, and no soil or guided-check photos |
| LiDAR measurements | No LiDAR cases |
| Text embedder recall and latency on the control plane | Not part of the evaluation set; see the results above |

---

## 4. Challenges we ran into

*Devpost field: Challenges we ran into (the ML part).*

- **The VLM reads disease as insect damage.** Qwen3.5-9B calls a tomato's early-blight spots `leaf_mines`, the tunnels of a larva, under every findings prompt we tried, and returns other disease lesions as `other` damage. Changing one prompt line at a time showed how fragile the wording is: one damage section turned a corn leaf's blight into 3 `leaf_mines`, and asking for the species by its id took the soil readings from 8 to 0. The shipped prompt, `findings@5`, keeps only the changes that held, and the early-blight case moved to a structural fix (task P11), such as making each damage finding name the animal or trace that made it ([vlm.md 4](vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran), [dev-roadmap phase 4](dev-roadmap.md#phase-4-oct-2223-calibration-then-freeze-the-models)).
- **The disease head was no better than guessing on garden tomato leaves.** Its held-out top-1 per crop was 0.84–0.88, but on the dev leaf photos its top-1 was 36–40%, and on the 12 iNaturalist garden tomato leaves its top 3 held the true problem 3 or 4 times, against 33–38% for guessing ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)). Retraining with three sets of handheld field photos (v3c) raised that to 8 of 12. It cost Tomato-Pakistan's held-out top-1 (0.762 to 0.688), which we accepted because the app's photos are garden photos, and corn's dev top-1 (11 to 7 of 15), which 15 leaves are too few to tune on ([training 6](training.md#disease-head)).
- **Published datasets hide copies.** PlantCity ships each photo with flipped and turned copies under unrelated names, and its test archive repeats its training archive ([training 3](training.md#3-datasets-and-licences)). A review found 34 of PlantCity's 169 test photos in our first split were copies or other shots of training photos, so those heads were dropped and the copies grouped by perceptual hash, with turned copies compared in 1° steps ([training 4](training.md#4-data-preparation), [training 6](training.md#disease-head)). PlantDoc's own splits leak too: 13 of its 236 test images have a training image at cosine 0.98 or more, and only 4 of those pairs share a label ([training 3](training.md#3-datasets-and-licences)). The evaluation set's second review found 17 more repeats of one leaf, four of those leaves spanning dev and test ([#135](https://github.com/aacruzgon/GreenLens/pull/135)).
- **iNaturalist files a disease under its pathogen.** So an observation of grape black rot or squash powdery mildew can show the fungus on another plant: 13 of 20 "grape black rot" cases were on Boston ivy, shagbark hickory or a saxifrage, and 12 of 15 "squash powdery mildew" cases on cocklebur, beggarticks, New Zealand ribbonwood or pot marigold. The second review dropped 40 such cases ([#135](https://github.com/aacruzgon/GreenLens/pull/135)).
- **Licence-clean background photos.** The detector needed photos with nothing to find. Our first pick of 400 COCO photos ignored each photo's own Flickr licence, and 308 of them were non-commercial or no-derivatives ([training 5](training.md#5-leaf-detector)). The shipped detector trained on a new pick of 258 CC BY, 135 CC BY-SA and 7 photos with no known copyright ([notices](../../ml/models/NOTICES.md#leaf_detector)). The detector from the first pick let about 5 points fewer no-plant photos through at the same rate of false "no plant"; whether that came from the pick or from chance between runs isn't measured ([models 2](models.md#2-plant-present-check)).
- **The int8 text embedder gave one text different vectors.** Its int8 files quantise activations per batch, so a text's vector depended on what it was batched with: cosine 0.9976–0.9983 between a text alone and in a batch of 64. The fp32 file gives 1.00000, so chat and ingestion both use fp32 ([models 10](models.md#the-pinned-recipe)).
- **FP8 isn't a clean VLM fallback.** Qwen3-VL-8B FP8 reads soil and calls no tomato `leaf_mines`, but it calls a corn leaf's blight `chewed_margins` in every run, boxed as the whole image, and puts a tomato's aphid and `shot_holes` in one shared box ([P5 runs](../../ml/training/eval/reports/p5-qwen35-2026-10-21.md#the-findings5-runs)).
- **No phone photos yet.** The set has no full shot sets, so the VLM's organism, soil and beneficial-insect results, and face blurring, aren't measured on it (section 3).

---

## 5. Accomplishments

*Devpost field: Accomplishments that we're proud of (the ML part).*

- **Every third-party model we ship is Apache-2.0 or MIT,** and a dataset goes into our weights only when its licence allows commercial use and adaptation; the caveats, such as PlantDoc's scraped photos, are in the notices. We left out Ultralytics YOLO, DEIMv2, EdgeSAM, SegFormer, SCRFD's weights, DINOv3 and non-commercial datasets for their licences ([README 9](README.md#excluded)), and every model file ships with its [notices](../../ml/models/NOTICES.md).
- **The perception models run on one 8 GB card** with 3.2 GB to spare, 1.2 s per scan of 6 photos and 40 leaves ([evaluation 8](evaluation.md#perception-batch-size)).
- **An evaluation set kept apart from training:** checked against all 59,821 training images, reviewed by two people, split by plant, and its test cases held back to be scored once (section 3).
- **A disease head that does its job on garden photos (dev):** the true problem in its top 3 on 84% of dev leaf photos with a problem against 57% for guessing, and 8 of 12 garden tomato leaves against 3 before (section 3).
- **Zero-shot species from the team's own knowledge seed (dev):** 77% against the stand-in's 61% ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
- **The VLM benchmarked through the worker's own code,** not a notebook: every call valid, two jobs at once on one card (section 3).
- **The weaknesses are written down with their numbers,** beside the strengths, in the documents and here.
- **On the test split, scored once:** `⟨M18: one sentence with T2, T5 and T11⟩`.

---

## 6. What we learned

*Devpost field: What we learned (the ML part).*

- **A model's own test split overstates it.** The disease head's held-out top-1 was 0.84–0.88; on dev leaf photos like the app's, 36–40% ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)). Only photos from new fields and gardens tell you how a model will do.
- **Judge a model by its job.** The disease head proposes candidates that the profiles and the VLM then check, so we chose and judged it by its top 3, not its top-1 ([training 6](training.md#disease-head)).
- **Change a prompt one line at a time.** A species line silenced the soil readings, and a damage section moved disease into `leaf_mines`; together, the cause couldn't be seen ([vlm.md 4](vlm.md#p5s-prompt-changes-on-qwen35-2026-10-21--ran)).
- **Distrust published splits and labels.** Copies cross splits under new names, and iNaturalist's disease observations can be on another host (section 4).
- **Quantization can break determinism,** not just accuracy: check that one input gives one output (section 4).
- **A licence is per file.** COCO's annotations are CC BY, but each photo keeps its own Flickr licence, and a detector's licence can reach the models trained with it ([README 9](README.md#9-licences-and-notices), [models 3](models.md#3-leaf-detector)).

---

## 7. What's next

*Devpost field: What's next for PDM (the ML part).*

- **Calibrate, freeze and score:** temperatures and thresholds fitted on the 80 dev cases (M16, [#135](https://github.com/aacruzgon/GreenLens/pull/135)), the final manifest (M17), the test split scored once (M18) ([dev-roadmap](dev-roadmap.md#phase-4-oct-2223-calibration-then-freeze-the-models)).
- **Disease as damage:** the structural fix for the VLM's `leaf_mines` (P11).
- **The other four crops' classes** into `classifier_labels.yaml` once their profiles are signed ([#125](https://github.com/aacruzgon/GreenLens/issues/125)).
- **The species head in the worker,** with the proposed rule: its probabilities in place of zero-shot's over its crops ([#134](https://github.com/aacruzgon/GreenLens/pull/134)).
- **The team's own garden photos,** with full shot sets, beneficial insects, faces, no-plant shots and LiDAR, to measure what the published photos can't, and to check the trained plant probe before it ships ([roadmap 1](roadmap.md#1-where-we-are)).
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

The design in `docs/vision` was first drafted on Oct 15 ([README](README.md)'s status line), and the first commit under `ml/`, [e0612b8](https://github.com/aacruzgon/GreenLens/commit/e0612b8), is from Oct 16, the first day of build week. Everything we didn't make is below, with its licence and what we did on top. Full citations are in [`NOTICES.md`](../../ml/models/NOTICES.md).

**Pretrained models**

| Model | Authors | Licence | What we did |
|---|---|---|---|
| RF-DETR-S, COCO weights | Roboflow | Apache-2.0 | Fine-tuned to one class, `leaf`; exported to fp16 ONNX |
| SAM 2.1 Hiera-T | Meta | Apache-2.0 | Exported to two fp16 ONNX graphs, weights unchanged |
| BioCLIP 2 | Imageomics Institute | MIT | Vision tower exported to fp16 ONNX, weights unchanged; text tower used for our label matrix and plant probe; our heads trained on its features |
| Qwen3.5-9B, and the community AWQ quantization `cyankiwi/Qwen3.5-9B-AWQ-4bit` | Qwen; cyankiwi | Apache-2.0 | Served as is; our prompts and schemas |
| Qwen3-VL-8B-Instruct, FP8 | Qwen | Apache-2.0 | Served as is, as the fallback |
| YuNet `2026may` | Wu, Peng, Yu (OpenCV Zoo) | MIT | Used as is |
| snowflake-arctic-embed-xs | Snowflake | Apache-2.0 | Used as is |
| `claude-opus-5-5`, `claude-haiku-5-5` | Anthropic | Hosted API | Called as is |

Sources: [README 9](README.md#9-licences-and-notices), [NOTICES](../../ml/models/NOTICES.md).

**Datasets in our weights**

| Dataset | Licence | What we did |
|---|---|---|
| PlantDoc, classification and detection | CC BY 4.0 for the annotations; the scraped photos keep their owners' copyright *unverified* | Deduplicated, labels mapped to the knowledge base, detection boxes collapsed to `leaf` |
| TOM2024 | CC BY 4.0 | Deduplicated, mapped, split by field visit |
| Tomato-Pakistan 2026 | CC BY 4.0 | Each file is an augmented copy of about 934 originals; the 302 that are PlantDoc's photos are dropped |
| PlantCity | CC BY 4.0 | Copies grouped and deduplicated; its gray leaf spot class dropped |
| Diseases of maize in the field (University of Pretoria) | CC BY 4.0 | Single-label photos, deduplicated |
| Makerere University Maize Image Dataset | CC0 1.0, credited | A fixed sample of 600 per class |
| iNaturalist, training photos | CC0 or CC BY 4.0 per photo, each credited | The species head and the trained plant probe |
| COCO train2017, 400 photos | Annotations CC BY 4.0; only photos that are CC BY, CC BY-SA or have no known copyright | The detector's photos with no leaf |

Sources: [training 3](training.md#3-datasets-and-licences), [NOTICES, Datasets](../../ml/models/NOTICES.md#datasets).

**Datasets for evaluation only, in no shipped weights:** COCO val2017 (4,758 photos) and 1,000 other COCO train2017 photos, as photos without a plant for the plant check (half of val2017 also trained the probe that isn't shipped); four CC BY 4.0 field datasets for the evaluation cases (maize from Bangladesh, 2026; two sets of tomato from Bangladesh, 2025; tomato from Ghana, 2023); and CC0 and CC BY iNaturalist observations from 2026 ([training 3](training.md#3-datasets-and-licences), [evaluation 2](evaluation.md#where-the-photos-come-from)). iBean (MIT) is fetched but not in v3c's training data.

**The pretrained models' own training data,** which we didn't choose: TreeOfLife-200M for BioCLIP 2 (CC0 as a compilation, though its images may keep their own terms), WIDER FACE for YuNet (CC BY-NC-ND 4.0), and COCO for RF-DETR's base weights. Whether a dataset's terms reach a model trained on it is unsettled; the notices say so, and the project's own licence is still for the team to choose ([README 9](README.md#9-licences-and-notices)).

---

## 10. For M18

Fill each placeholder from the scored test run, then commit this file with the report. The offline scripts score the dev cases today: `calibrate` and `species_check` read only `split == "dev"`, with no option for test, and `plant_check` scores the training splits, not the evaluation cases. Each needs a test option, or `score.py` ([evaluation 9](evaluation.md#9-harness-and-reports)) needs writing, before M18.

Before scoring:

- [ ] `python -m ml.training.eval.check_leakage` on the final manifest passes: its report's header reads "0 not allowed". Update section 3's photo and image counts from it.
- [ ] The dev numbers in sections 3 to 6 are re-run on the 80 dev cases ([#135](https://github.com/aacruzgon/GreenLens/pull/135)) and updated.
- [ ] M17's final values replace the word "provisional" for the temperatures and `τ_leaf` and `τ_plant` in section 3, without quoting values that aren't final.
- [ ] Case counts in section 3 (42 test, 11 `sure`) match the manifest M18 scores.

Each placeholder:

| Placeholder | Script or report | Field |
|---|---|---|
| Report link | The committed test report | Its path, linked from section 3's "Test split" paragraph |
| T1 | `plant_check` run on the test cases' plant photos at M17's pair | 1 minus the share of cases whose plant photos all fail (false `no_plant_found`), per case |
| T2, T3 | `species_check` ([#134](https://github.com/aacruzgon/GreenLens/pull/134)) on the test cases with the final matrix | The "per case, zero-shot" row's Top-1 and Top-3; the "head" or "average" row instead if the worker runs the species head by then |
| T4 | `calibrate`'s species stage at the final `species_temperature`, joined per case with `vlm_bench`'s species guesses | Share of cases with BioCLIP 2's top-1 at 0.8 or more, the VLM naming the same species at medium or high, and both wrong. No script joins them today |
| T5 | `calibrate` with the final disease head | "Top 3 as sent" on the test photos with a problem, with its guessing in brackets |
| T6 | `calibrate`'s JSON, its top-1 answers per true class | Macro F1 per crop, computed from those answers; not a column today |
| T7 | `calibrate` | `ece` (15 bins) at M17's temperature on the test leaf photos |
| T8 | `vlm_bench cases --split test`, `run` with the final model and pixel limit at 2 at once, then `score` | 1 minus "Retried calls" divided by the calls made, two per job in "Finished" |
| T9 | Same `vlm_bench` report | "Species" |
| T10 | Same `vlm_bench` report | "Job p95" at "At once" 2 |
| T11–T16 | J5's end-to-end run through the API (`e2e.py`, `score.py`, [evaluation 9](evaluation.md#9-harness-and-reports)) | Evaluation 4's end-to-end metrics: `confirmed` precision on `sure` cases, coverage, healthy with a problem, sick called healthy, p95 wall time on the GPU path and on the fallback |
| T17 | J5's run | Share of test photos the blur and exposure check rejects |
| T18 | J5's run, if it records blurred regions, or YuNet at the API's settings over the test photos | Share of plant photos with a face box |
| Section 5's last line | This file's T2, T5 and T11 | One plain sentence, each number with its n |

A row whose cases or harness don't exist when M18 runs gets "not measured" and the reason. Every test number goes in section 3's tables and section 5's last line, nowhere else.
