# PDM Vision Evaluation

> **Status:** Draft v0.2, 2026-10-16. v0.2: the fallback run also measures whether 8,000-token calls finish within its `max_runtime`. v0.1, 2026-10-15: First evaluation plan: the evaluation set of real field photos, its labels, metrics and acceptance targets per stage, calibration of the certainty words, the classifier and the diagnosis margin, the thresholds the set decides, and the day-1 checks and day-2 benchmarks.
> **Checked:** nothing in this document has run yet. Every target is a proposal to confirm or move on Oct 24, when the test split is scored.

This is how PDM measures its models and sets the numbers the [contracts](../architecture/contracts.md) leave open. The models themselves are in [README.md](README.md) and [models.md](models.md), the VLM in [vlm.md](vlm.md), and the datasets and training in [training.md](training.md).

## Contents

1. [Purpose](#1-purpose)
2. [The evaluation set](#2-the-evaluation-set)
3. [Labels](#3-labels)
4. [Metrics and acceptance targets](#4-metrics-and-acceptance-targets)
5. [Calibration](#5-calibration)
6. [Thresholds set from the evaluation set](#6-thresholds-set-from-the-evaluation-set)
7. [Day-1 checks](#7-day-1-checks)
8. [Day-2 benchmarks](#8-day-2-benchmarks)
9. [Harness and reports](#9-harness-and-reports)
10. [Open questions](#10-open-questions)

---

## 1. Purpose

The evaluation set does four jobs:

1. **Acceptance.** Each stage has a target (section 4). A model that misses its target on the dev split isn't deployed, or is deployed with the stage marked lower confidence.
2. **Calibration.** It sets the values the contracts leave open: the certainty mapping (contracts open question 2), the diagnosis margin (3), the classifier's temperature, and the BioCLIP agreement threshold (section 5).
3. **Thresholds.** It sets the blur and exposure limits, the measurement thresholds (contracts open question 10) and the growth noise floor (section 6).
4. **The Devpost numbers.** The test split is scored once, on Oct 24, and those numbers go in the write-up ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)).

Public benchmarks don't do these jobs. PlantDoc's test split measures a model on PlantDoc's distribution; the evaluation set measures the whole pipeline on the photos our users will take: phone shots of Ontario gardens in October, several shots per plant, with the guided capture of [architecture 6.6](../architecture/README.md#66-capture-guidance).

---

## 2. The evaluation set

### Unit: a case

A **case** is one scan as the app would send it: a set of shots with their `ImageKind`, optional depth, and the truth for that plant on that day. Component metrics (a leaf detector's boxes, a classifier's crops) are computed from the same cases, so there is one set to collect and label.

### Composition

| Group | Cases | What it holds |
|---|---|---|
| Diseased, supported crops | 40 or more | At least 3 cases for each supported crop and at least 1 for each of its top problems found in local gardens. Late-season tomato (early blight, Septoria, late blight), squash and cucumber (powdery mildew), potato, pepper, apple and grape are the likely ones in October |
| Healthy, supported crops | 15 | Clean plants of the supported crops, so the `healthy` rule's false positives and false negatives can both be measured |
| Pests and beneficials | 15 | Aphids, whiteflies, spider mites, caterpillars, slugs and snails, flea beetle damage, and at least 5 cases with a beneficial (ladybug adults and larvae, lacewing larvae, spiders, bees). Ladybug larvae are mandatory: they are the classic "killed by mistake" case ([architecture 6.5](../architecture/README.md#65-organisms-and-damage)) |
| Look-alikes | 10 | Abiotic problems that mimic disease: nitrogen deficiency, sunscald, frost damage, overwatering, herbicide drift if found |
| `label_only` species | 10 | Common garden plants outside the supported crops (basil, lettuce, beans, roses, hostas), for species identification and for "no confident diagnosis on an unsupported species" ([architecture 6.2](../architecture/README.md#62-species-identification), step 6) |
| No plant, unusable | 10 | A fence, a pet, a hand, a blurred shot, a dark shot, a plant too far away to read. These must end in `no_plant_found` or `photos_unusable`, or be rejected by the quality gate |
| LiDAR | 20 | Taken on a LiDAR iPhone through the development build, each with a ruler or a printed scale card in at least one shot. Overlaps the groups above. Used for the measurement thresholds (section 6) |
| Repeat scans | 5 plants × 3 scans | The same plant scanned three times within an hour on a LiDAR phone, for the growth noise floor (section 6) |
| Guided soil checks | 10 | Squeeze tests and jar tests on at least 3 soils (sandy, loamy, clay-rich), with the texture known from a lab report or a reference soil |
| Faces | 30 photos | Garden photos with people in them at different sizes (from a face filling the frame to a person across the garden), plus profile and partly hidden faces. Used only for the face blur's recall, never kept with the plant cases |

That is about 130 cases and 30 face photos. Size matters less than spread: every supported crop, every common look-alike pair, and every image kind should appear.

### Where the photos come from

- **The team's own photos**, taken in gardens and community gardens around Toronto with the gardener's permission, Oct 16–23. They are the core of the set because they match what the app will see.
- **Licensed photos** fill the gaps (a disease we can't find locally in October), only when the licence allows reuse with credit: CC0 or CC BY observations on iNaturalist, and extension photo libraries whose terms allow it. Each one records its source, author and licence.
- **Never** from a training source. A PlantDoc, PlantVillage or other training image in the evaluation set would measure memory, not generalisation. The harness checks each file's perceptual hash against the training sets and refuses a match (section 9).

### Splits

- **dev** (about 60%): used to set thresholds, calibrate, choose between models and debug.
- **test** (about 40%): scored once on Oct 24, for the Devpost numbers and the final acceptance. A threshold is never tuned on it.

Cases are split by **plant**, not by image, so two scans of the same plant are always on the same side. Repeat scans and face photos are dev only, so the growth noise floor and the face-blur recall are dev numbers and are reported as such on Oct 24.

### Privacy and storage

- No photo in the set has location metadata: the harness strips EXIF on import, as the API does. Faces in plant cases are blurred on import; the face photos are kept separately, with consent, and never published.
- The photos live outside git, in a private bucket or a release asset, because they are large and some are under licences that require credit. The manifest (below) is in git, with each file's SHA-256, so a run always uses the same bytes.

---

## 3. Labels

### The manifest

`ml/training/eval/manifest.jsonl`, one case per line:

```ts
interface EvalCase {
  case_id: string;                 // "tomato-0007"
  split: "dev" | "test";
  plant_key: string;               // the same physical plant across cases; splits are by plant
  images: {
    file: string;                  // path under the photo store
    sha256: string;
    kind: ImageKind;               // contracts 3
    depth?: string;                // 16-bit PNG, as the LiDAR module writes it
    depth_confidence?: string;
    depth_meta?: string;           // DepthMeta JSON (contracts 3)
  }[];
  truth: {
    outcome: "diagnosed" | "healthy" | "no_plant_found" | "photos_unusable" | "rejected_at_upload";
    species_id: Slug | null;       // null: not in the knowledge base
    problems: Slug[];              // every problem present; empty when healthy
    organisms: { organism_id: Slug | null; image: number; bbox: BBox }[];  // index into images, normalised box
    damage: { pattern: DamagePattern; image: number; bbox?: BBox }[];
    soil?: Partial<Record<SoilAspect, string>>;
    indicators?: Record<string, "yes" | "no">;   // indicator_id → answer; only the ones the labeller is sure of
    leaves?: { image: number; bbox: BBox }[];   // every leaf at least 32 px across, for detector recall
    masks?: { image: number; file: string }[];   // hand-drawn leaf masks, on a subset
    measured?: Partial<Record<MeasureQuantity, number>>;  // from the ruler or scale card
    faces?: { image: number; bbox: BBox }[];
  };
  label_basis: "extension_office" | "expert" | "team_consensus" | "source_caption";
  label_confidence: "sure" | "probable";
  source: string;                  // "team", or the URL of a licensed photo
  licence: string;                 // "team-internal", "CC BY 4.0", ...
  credit?: string;
  labelled_by: string;
  reviewed_by: string;
}
```

The types are the contracts' types, so the harness can compare a `ScanDetail` with the truth field by field.

### How cases are labelled

1. The photographer records what they know on the spot: the plant, what the gardener says it has, and whether the gardener has had it confirmed.
2. A teammate labels the problems from the reviewed knowledge-base profiles: the indicators that are clearly visible, the look-alikes ruled out, and the profile's sources. The label is `team_consensus` unless an extension office or an expert confirmed it.
3. A second teammate reviews every case. Disagreements are settled by the profile's indicators, or the case is marked `probable`.
4. `probable` cases count toward detection and species metrics but are left out of diagnosis accuracy, so a wrong label can't punish a right answer.
5. Indicator truth is only recorded where the labeller is sure. An indicator the photos can't show is left out, not labelled `no`.
6. Boxes and masks are drawn in CVAT or Label Studio and exported to the manifest's normalised `[x, y, w, h]`. Leaf boxes follow PlantDoc's convention, one box per visible leaf, so detector recall is comparable.

### Feedback joins the set

`POST /scans/{id}/feedback` (contracts 3) stores a reviewer's verdict and corrected label. After build week, a scan with feedback `wrong` and a corrected label is a candidate case: a teammate labels it as above and adds it to dev. Feedback is never added to the set unreviewed, and never to test.

---

## 4. Metrics and acceptance targets

Targets are for the dev split during the week and are confirmed on the test split on Oct 24. A miss is reported as it is; it doesn't move the target.

### Upload and privacy (control plane)

| Stage | Metric | Target |
|---|---|---|
| Blur and exposure check | Share of good shots rejected | 5% or less |
| | Share of blurred, dark or blown-out shots caught | 80% or more |
| Face blur | Recall of faces 20 px or more across in the stored image (a face counts as caught when the blurred area covers 90% of its box) | 98% or more |
| | Share of plant photos with a false face blurred over a leaf | 2% or less |

The face blur fails closed (an image isn't stored until blurring succeeds), so recall matters far more than precision. A false blur on a leaf costs a little evidence; a missed face is a privacy failure.

### Perception (RTX 3070)

| Stage | Metric | Target |
|---|---|---|
| Plant check | Recall on cases with a plant (a miss fails the scan with `no_plant_found`) | 99% or more |
| | Specificity on no-plant cases | 90% or more |
| Leaf detector | Recall of labelled leaves at IoU 0.5, on `leaf_closeup` shots | 85% or more |
| | Class-agnostic mAP50 on the PlantDoc test split, one `leaf` class | Report; the baseline from training is the floor |
| Masks | Mean IoU against the hand-drawn masks | 0.80 or more |
| Species (BioCLIP) | Top-1 accuracy on supported crops | 85% or more |
| | Top-3 accuracy on supported crops | 95% or more |
| | Top-3 accuracy on `label_only` species | 75% or more |
| Species (with the VLM) | Share of cases where BioCLIP and the VLM agree with a high score but are wrong (the user is shown "confirm with one tap" for the wrong plant) | 3% or less |
| Disease classifier | Top-3 accuracy per leaf crop, labels restricted to the working species | 85% or more |
| | Macro F1 of the top-1, per supported crop | Report per crop |
| | Expected calibration error after temperature scaling (section 5) | 0.05 or less |
| Image embeddings | Share of repeat-scan pairs whose nearest neighbour in their garden is the same plant | 80% or more |

### VLM (each VLM card)

| Stage | Metric | Target |
|---|---|---|
| Structured output | Calls that decode and validate on the first try | 99% or more; 100% after the one retry |
| Organisms | Recall at IoU 0.3 against labelled organisms (organisms are small, and a loose box still names the right thing) | 70% or more |
| | Precision of reported organisms, any IoU on the right image | 75% or more |
| | Beneficial organisms reported as pests | **0**. Any occurrence is a blocking bug, fixed in the prompt or the organism descriptions before anything else |
| Damage patterns | Recall of labelled patterns on the right image | 60% or more |
| Soil | Accuracy per aspect where the truth is labelled | 70% or more for moisture and cover; report the rest |
| Indicators | Accuracy of `yes` and `no` answers against sure labels | 80% or more |
| | Share of answers `unsure` on indicators the photos clearly show | 25% or less |
| Species guess | Top-1 agreement with the truth | Report next to BioCLIP's |
| Guided checks | Texture class correct, or `unclear` | 70% correct; wrong class 15% or less |
| Timing | p95 time of a `vlm` job at the chosen `VLM_SLOTS` | Under half its `max_runtime` (2.5 minutes), as contracts 5 requires |

### End to end

| Metric | Target |
|---|---|
| Precision of `confirmed` diagnoses: the primary problem is among the case's true problems | 90% or more |
| Share of diseased cases that end `confirmed` (coverage) | Report; expect 50–70%, the rest `uncertain` with a question |
| Healthy plants diagnosed with a problem (`confirmed`) | 10% or less |
| Sick plants diagnosed `healthy` | 5% or less. This is the worst error: the user does nothing |
| No-plant and unusable cases that end correctly (`no_plant_found`, `photos_unusable` or a quality rejection) | 90% or more |
| `label_only` species diagnosed `confirmed` with a supported crop's disease | 0 |
| Wall time from submit to `diagnosed`, p95, GPU path | 90 seconds or less |
| Same on the fallback worker | Report |

Precision of `confirmed` is the number the margin is tuned for (section 5). PDM prefers an honest `uncertain` with a good question to a confident wrong answer.

### Measurement (LiDAR cases)

| Quantity | Metric | Target |
|---|---|---|
| Ruler or scale card at 30 cm | Error of a measured length | 3% or less |
| `leaf_length_cm` | Error against a ruler | 5% or 0.5 cm, whichever is larger |
| `lesion_diameter_mm`, `hole_diameter_mm` | Error against callipers or the scale card | 20% or 0.5 mm, whichever is larger |
| `plant_height_cm` | Error against a tape | 10% or 5 cm, whichever is larger |
| Measurable indicators | Answers that contradict the sure label | 5% or less |

### Text embedder (control plane)

| Metric | Target |
|---|---|
| Recall@5 on 50 labelled question → chunk pairs from the loaded knowledge base | 80% or more |
| p50 latency of one query on the control plane's CPU | 20 ms or less |

---

## 5. Calibration

Confidence is a number from 0 to 1 with a band computed by the API: high ≥ 0.8, medium ≥ 0.5, low below ([contracts 1](../architecture/contracts.md#wire-format)). Calibration makes that number mean what it says: of the findings shown as high, about 85% should be right.

### VLM certainty words (contracts open question 2)

The VLM reports `low`, `medium` or `high`, never a number, and the worker maps them to 0.4, 0.65 and 0.85 ([contracts 6](../architecture/contracts.md#after-decoding)).

1. On the dev split, match every organism, damage and soil finding to the truth (organisms at IoU 0.3 on the right image).
2. For each word, compute the precision: the share of findings with that word that are right.
3. The calibrated value for each word is that precision rounded to 0.05, kept strictly increasing (low < medium < high) and **inside the word's band**: low below 0.5, medium from 0.5 to 0.75, high from 0.8 to 0.85. The contract caps a VLM finding at "high (0.85)" ([contracts 6](../architecture/contracts.md#after-decoding)), and a word that changes band would change what the app shows. If the measured precision falls outside a word's band (for example "high" findings right only 70% of the time), the value goes to the band's edge and the gap is reported as a contract question ([README 12](README.md#12-findings-for-the-architecture-and-contracts), finding 19).
4. A word with fewer than 20 findings keeps its default.
5. The same values also set the confidence of the VLM's species guess and of guided-check textures ([contracts 6](../architecture/contracts.md#after-decoding)), which feed the readable-plant rule; they inherit the calibrated values, and their precision is reported alongside.
6. If the precision per finding kind differs by more than 0.15 (for example organisms much worse than soil), report it: the contract has one mapping, and a per-kind mapping would be a contract change.

The values are worker configuration (`VLM_CERTAINTY_VALUES`), not code, and are recorded in `model_versions` so a stored confidence can be traced to the mapping that made it.

Measurements use the same three values (contracts 7). If calibration moves the VLM's values, the measurement confidences move with them. Measurement confidence comes from rules, not from the VLM, so the team may want them independent; that is a contract change, listed as [README 12](README.md#12-findings-for-the-architecture-and-contracts) finding 15, not something to split silently.

### Disease classifier

- **Temperature scaling** on held-out data: the PlantDoc validation split plus the dev cases. One scalar `T`, fitted by minimising the negative log-likelihood, stored in the model manifest ([README 6](README.md#6-model-files-versions-and-pinning)).
- The worker restricts the logits to the working species' labels **before** the softmax (contracts 5), so `T` is fitted on the same restricted softmax: for each crop in the calibration set, only that crop's labels.
- Reported: expected calibration error (15 bins) before and after, and a reliability diagram per supported crop in the report.

### BioCLIP

BioCLIP's zero-shot scores are a softmax over the label list with the model's own logit scale. Agreement with the VLM "with a high score" ([architecture 6.2](../architecture/README.md#62-species-identification), step 3) means BioCLIP's top-1 score is in the high band (0.8 or more) and the VLM's `species_guess` names the same species with `medium` or `high` certainty.

- Fit a temperature for BioCLIP's softmax on the dev cases the same way as the classifier, then check the top-1 accuracy of cases at 0.8 or more. If it is below 95%, raise the threshold until it isn't: a wrong one-tap confirmation is worse than showing the top 3.
- The threshold and temperature are perception configuration recorded in `model_versions`.

### Diagnosis margin (contracts open question 3)

`confirmed` needs the top candidate to lead the second by at least the margin, in indicator-weight points (weights are 1–3, so a single matched discriminating indicator scores up to 3).

1. Run scoring on every dev case with the VLM's and measurements' real answers, and record the top two scores.
2. For each margin from 1 to 6, compute the precision of `confirmed` and the coverage.
3. Choose the smallest margin whose precision is 90% or more. Proposed starting value: **2**.
4. Report the precision–coverage curve with the chosen point marked.

This is the one place where coverage is traded for precision on purpose.

### Indicator weights (contracts open question 4)

After the margin is set, look at the dev cases that ended `uncertain` with a lead of exactly the margin minus one. If many of them were right, weights of 1–3 are too coarse. Report it; changing weights is a knowledge-base change reviewed by a teammate, not a tuning knob.

---

## 6. Thresholds set from the evaluation set

Each threshold below has a proposed value in the contracts or the architecture. The method decides whether it holds.

| Threshold | Proposed | Method |
|---|---|---|
| Blur (Laplacian variance) | From data | Computed on the shot downscaled to 512 px on its long side, so the score doesn't depend on resolution. Choose the value that catches 80% of the bad shots while rejecting 5% or fewer good ones; if both can't hold, favour the 5% |
| Exposure (mean luma, clipped shares) | From data | Same method, on the share of pixels below 16 and above 239 of 255 and the mean luma |
| Plant check score | From data | The highest threshold that keeps 99% recall on the dev cases with a plant ([models 2](models.md#2-plant-present-check)) |
| Indicator margin | 10% of the bound, at least 2 pixels | On the LiDAR cases with sure size labels: count measured answers that contradict the label. Widen the margin until it is 5% or fewer |
| Coverage for confidence | 60% high, 30% medium | On the LiDAR cases: the error of measurements by coverage bucket. High should hold the section 4 targets |
| Plane residual | 5 mm high, 10 mm medium | Same, by residual bucket |
| Spot colour (`SpotSegmenter`) | From data | Hand-drawn lesion masks on 30 leaves. Fit thresholds on the leaf's own green in CIELAB (the a\* and b\* shift from the leaf's median) that maximise IoU with the lesions, and separately for the yellow halo |
| Smallest sized spot | 4 pixels | Check the diameter error of spots 4–8 px across on the scale card. Raise it if the error passes 50% |
| Stored size of LiDAR shots | Long side 2048 px or more | Check that the smallest labelled Septoria spots (1.5 mm) are 4 px or more at 30–45 cm |
| Growth noise floor | 2 cm or 5% | From the repeat scans: the 95th percentile of the height difference between scans of the same plant within an hour. The floor is the larger of that and the proposal |
| Leaf distance range | 25–45 cm hint, 0.25–0.6 m valid | The lower bound comes from the LiDAR day-1 check ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)); the eval set confirms the error targets hold across the range |

The measurement code is the backend's ([docs/backend](../backend/vision.md)). The evaluation decides its constants and reports any that miss.

---

## 7. Day-1 checks

Oct 16–17, before anything is built on the models. Each check has a pass condition; a failure changes the plan that day.

| Check | Pass condition | If it fails |
|---|---|---|
| ONNX Runtime 1.31 with the CUDA 13 execution provider on both cards: a small dynamic-batch model opens with `error_on_failure` and runs batches of 1, 4 and 16 ([README 5](README.md#5-runtimes-and-versions)) | Runs on the RTX 3070 (sm_86) and the RTX 5070 Ti (sm_120), outputs equal to CPU within 1e-4 | Check the driver (580 or later); try the 1.24.1 CUDA 12.8 build on the 3070 only |
| PyTorch training step on the 5070 Ti | A forward and backward pass of the classifier backbone in mixed precision | Train on the 3070 at a smaller batch |
| vLLM serves the primary VLM candidate on the 5070 Ti | Starts with the flags in [vlm.md](vlm.md#4-serving), answers one image request with a schema | Try the llama.cpp path ([vlm.md](vlm.md#4-serving)) |
| The RX 9070 XT as a second VLM worker ([README 4](README.md#4-placement-and-vram-budgets)) | The host shows `gfx1201` in `rocminfo` and the NVIDIA cards still work; vLLM's ROCm image starts with only `/dev/kfd` and the card's render node; the same checkpoint and flags as the 5070 Ti answer a findings and an indicators call with our schema, and vLLM's log shows a native FP8 or W4A16 kernel, not a dequantising fallback | Use the W4A16 checkpoint on both cards; if vLLM still fails by the end of day 2, llama.cpp's Vulkan server with the same model's GGUF |
| Text embedder | The chosen model's ONNX file gives the same vectors in Rust and Python to 1e-5, and a query embeds in 20 ms or less on the control plane's CPU ([models 10](models.md#10-text-embedder)) | Choose the runner-up; the dimension is fixed only after this check passes |
| Face detector | Finds every face in 10 team photos with people, at the upload resolution | Lower the score threshold; add a tile pass ([models 9](models.md#9-face-detector)) |
| BioCLIP zero-shot | Top-3 contains the right species on 20 team photos of supported crops | Check the label wording ([training 8](training.md#8-label-lists)) before suspecting the model |
| LiDAR | The checks in [architecture 10](../architecture/README.md#10-models-and-gpu-placement), and a scale card on a leaf at 30 cm measured within 3% | Owned by the app and backend; the evaluation set's LiDAR cases wait for it |

---

## 8. Day-2 benchmarks

### VLM (architecture open decisions 4 and 15, contracts open questions 1 and 9)

On 20 dev cases with the full shot set, for each of the two candidates in [vlm.md](vlm.md#2-model-choice):

| Measure | How |
|---|---|
| Structured-output validity | Every call; count decode and validation failures |
| `maxLength` and `maxItems` enforcement | Send a schema with `maxLength: 10` and a prompt that invites a long answer; check whether the backend stops it (contracts 6) |
| Box format | Draw the returned boxes on the prepared images for 5 cases and check them by eye; confirm the adapter's `box_format` ([vlm.md](vlm.md#9-box-formats)) |
| Indicator accuracy, organism recall, beneficial-as-pest | Section 4, on these 20 cases |
| Time per job | Time to first token and decode speed per call, wall time per job, p50 and p95 |
| Peak VRAM | `nvidia-smi` on the RTX 5070 Ti and `amd-smi` on the RX 9070 XT during the run, with `VLM_SLOTS` 1 and 2, and vLLM's own KV-cache usage from `/metrics` |
| Image size | `VLM_MAX_PIXELS` at about 0.6, 1.0, 1.4 and 2.0 megapixels: accuracy against time, with small-organism recall reported separately ([vlm.md 5](vlm.md#small-organisms)) |

**Choosing.** First the candidate with 0 beneficial-as-pest errors and valid output; then the higher indicator accuracy; then organism recall; then speed. `VLM_SLOTS` is 2 only when two jobs fit next to the model at the chosen `VLM_MAX_PIXELS` and a job's p95 with two running stays under 2.5 minutes ([contracts 12](../architecture/contracts.md#12-open-questions), question 9).

**Per card.** The run is repeated on each VLM card, with the same checkpoint. Each card gets its own `VLM_SLOTS` from its own timing. On 10 of the cases the two cards' outputs are compared: boxes drawn on the same prepared image should overlap, and indicator answers should agree on most questions. A large disagreement is a configuration problem (checkpoint, preparation or server flags) to fix before both serve.

### Perception batch size

Each model's own cap, with every session loaded, on a scan with 6 shots and 40 leaf crops: the SAM encoder at 1, 2 and 4 images, the detector at 4, 8 and 16, BioCLIP 2 at 8, 16 and 32, and the SAM decoder at 8, 16 and 32 boxes, in fp16 and fp32 where the export allows both. Record the peak VRAM of the whole perception worker and the time per job. Choose the largest caps that leave at least 1 GB of the 3070's 8 GB free at peak ([README 4](README.md#4-placement-and-vram-budgets)). `PERCEPTION_MAX_BATCH` stays the job-wide ceiling above them. If SAM 2.1-T can't run at an encoder batch of 2 in fp16, MobileSAM is benchmarked the same way.

### Fallback worker

The same 20 cases through the fallback worker, with the hosted model it uses ([vlm.md](vlm.md#12-the-fallback-worker)): validity, indicator accuracy, organism recall, box placement (one live call settles its box format), whether calls with the 8,000-token thinking budget finish within the fallback's doubled `max_runtime` (contracts open question 26), and the cost per scan in API credits.

---

## 9. Harness and reports

`ml/training/eval/`:

```
ml/training/eval/
├─ manifest.jsonl          the cases (section 3)
├─ fetch.py                downloads the photos by SHA-256 from the photo store
├─ check_leakage.py        perceptual hashes against every training set; fails on a match
├─ components.py           runs each ONNX model offline, the same files the worker loads
├─ e2e.py                  sends every case through the real API, collects ScanDetail
├─ score.py                compares results with the truth; writes the report
├─ calibrate.py            section 5: temperatures, certainty values, margin sweep
└─ reports/                <date>-<git sha>-<split>.md, committed
```

- **Components** run offline with `onnxruntime-gpu` on the same model files and manifest hashes the perception worker loads, so a component number belongs to a deployed model. Pre- and post-processing come from one Python package shared with training, and a parity test checks it against the Rust worker on 10 images (the worker's side is in [docs/backend](../backend/vision.md)).
- **End to end** goes through the real API with a dedicated evaluation user and garden, so it measures what users get: the quality gate, face blurring, the pipeline, scoring and the diagnosis. Questions are answered from the case's indicator truth where it has one, and left to time out otherwise. The garden is deleted after each run.
- **The fallback path** is measured by stopping the rig's workers for the run.
- **Each report** records the git SHA, every `model_versions` entry, the configuration values from section 5 and 6, the split, and every table of section 4 with pass or miss. A model change, a prompt change or a knowledge-base change reruns the dev split before it is merged.

---

## 10. Open questions

1. **Size of the set.** About 130 cases is what the team can photograph and label in six days. If October gardens don't show a supported crop's problems, the gap is filled with licensed photos, and the report says how many cases came from where.
2. **Ground truth for disease.** Most labels will be `team_consensus` from the reviewed profiles. Ask a local extension office or master gardener group on Oct 16 whether someone can confirm 20 cases.
3. **Per-kind certainty values.** If organisms and soil calibrate very differently (section 5), the contract's single mapping becomes a per-kind one. Decide after the dev calibration.
4. **Measurement confidence independence.** Whether measurement confidences should keep fixed values when the VLM's are recalibrated (section 5).
