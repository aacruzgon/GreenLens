# PDM Vision Evaluation

> **Status:** Draft v0.17, 2026-10-22. v0.17: section 3 records the second review: Alan reviewed all 187 cases, and settling it dropped 68 (40 iNaturalist cases whose host isn't the case's plant, 8 with no readable plant, 17 repeats of one leaf and 3 his calls the photos can't settle), so 119 remain (76 dev, 43 test), 7 of the test cases `sure`; section 2 applies the research-grade rule without exception, lets a crop's label cover its genus and gives observations that may show one plant one key, and section 4 reports small counts with a Wilson interval. v0.16: section 5 notes that the seed's label matrix (M12, its 20 species the stand-in's) replaces the stand-in the species temperature was fitted on, so 1.88 is refitted on it; section 7's zero-shot check has a dev-case stand-in until the team's photos. v0.15: section 5 records v3c, the disease head retrained on garden-like photos and now in the manifest: the true problem in its top 3 on 84% of the dev photos with a problem, against v2c's 74%, while its corn top-1 falls from 11 to 7 of 15. v0.14: after review, section 2 states the leakage check as measured: a match within 4 bits, its thresholds calibrated on edited copies of 300 training images, and crops as what it misses; M18 runs it on the final manifest. v0.13: section 5 scores the disease head's top 3 as the API keeps it, on the photos with a problem (73% against 52–57% for guessing), gives the temperature's trade per species with the `healthy` evidence, adds a per-species fit and out-of-fold scores, gives the claims without grape, which is claimed at every value, and leaves the choice to M17. v0.12: section 5 judges the disease head by its top 3 (72–74% on the dev leaf photos, against 51–56% for guessing) and proposes its dev-fitted temperature, which keeps its medium-confidence claims to the 72–75% that are right. v0.11: section 5 records the offline calibration on the dev cases: `species_temperature` 1.88 on the stand-in matrix, and a disease head that reaches 36–40% on the dev leaf photos, which no temperature fixes. v0.10: section 2 records the leakage check: no evaluation photo is a copy of a training image. v0.9: section 8's per-question rates count `sure` cases only (Qwen3.5 15% / 10%), a one-photo case's photo goes to the indicators call twice when a question names the whole plant, and D1's two confirming runs are named (review of #86). v0.8: section 7's ONNX Runtime check passes on the RTX 3070, and section 8 records the perception batch caps measured there: the manifest's caps peak at 4,598 MiB with 3.2 GB free. v0.7: section 8's VLM results name the prompts they ran on, say the diagnosis column is the VLM's answers alone, and note that the bench now routes images by the pipeline's rules. v0.6: Sam chose Qwen3.5-9B AWQ (D1), and a rerun at the desktop's memory budget matched the full-card run (section 8). v0.5: section 8 records the day-2 VLM benchmark on the 65 one-photo tomato and corn dev cases: every call valid and two jobs at once fit, but the indicator questions carry little signal on these photos for any candidate (Qwen3.5-9B AWQ diagnoses 35% of the sure cases, both Qwen3-VL-8B builds 12%). v0.4: tomato and corn diseases come from published field datasets no model trained on (`ml.training.eval.fieldsets`), checked against every training photo and sent one photo per case, and phone-condition variants of the cases (`ml.training.eval.variants`) are scored as their own slices. v0.3: section 2 adds `ml.training.eval.inat` for licensed iNaturalist photos, only from 2026 and reviewed before they become cases, and records that iNaturalist can't supply tomato or corn diseases. v0.2: the fallback run also measures whether 8,000-token calls finish within its `max_runtime`. v0.1, 2026-10-15: First evaluation plan: the evaluation set of real field photos, its labels, metrics and acceptance targets per stage, calibration of the certainty words, the classifier and the diagnosis margin, the thresholds the set decides, and the day-1 checks and day-2 benchmarks.
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
| Diseased, supported crops | 40 or more | At least 3 cases for each supported crop and at least 1 for each of its top problems found in local gardens. Late-season tomato (early blight, Septoria, late blight), squash and cucumber (powdery mildew), potato, pepper, apple and grape are the likely ones in October. Tomato and corn come from published field datasets ([below](#where-the-photos-come-from)) |
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
  - **iNaturalist:** `python -m ml.training.eval.inat` searches the taxa in `ml/training/eval/inat_targets.yaml` (a disease is listed under its pathogen), downloads the CC0 and CC BY photos without metadata, and writes a contact sheet and a review file. A teammate keeps an observation only after checking the host plant and giving each photo its shot kind, and `add` writes it to the manifest as a `source_caption` case, `probable` unless the identification is research grade and the signs are clear. A case is one scan, so it needs a photo that can serve as the `whole_plant` shot.
    - **The research-grade rule has no exception.** A `needs_id` observation stays `probable` even when both reviewers agree that the signs are clear (the second review applied it to 11 cases).
    - **A crop's label covers its genus:** an apple case may be a crabapple, a grape case a wild vine (*Vitis riparia*, *V. vulpina* or a muscadine), and a squash case any *Cucurbita*, buffalo gourd included. The pathogens and their signs are the same, and the label set has no other *Malus*, *Vitis* or *Cucurbita* (`squash.yaml` names the genus itself). A plant with its own label is never folded in: a leaflet that may be potato isn't kept as tomato.
  - **Only photos from 2026.** BioCLIP 2's training data, TreeOfLife-200M, was compiled before 2026 and draws on GBIF, which holds iNaturalist's licensed observations *unverified*, so an older photo may be one BioCLIP has seen. The script refuses an earlier date. Never use a photo from a web search: PlantDoc was scraped from Google Images.
  - **Not for tomato or corn** (✓ ran 2026-10-19): iNaturalist holds no research-grade CC0 or CC BY observation from 2026 of Septoria leaf spot, early or late blight, common rust or northern leaf blight, and at most 6 unverified ones of each. Apple scab (67), cedar-apple rust (119), grape black rot (99), two-spotted spider mites (41) and ladybug larvae (over 1,000) have enough. Tomato and corn come from published field datasets (next item).
  - The species head's iNaturalist training photos ([training 6](training.md#6-disease-and-species-heads)) leave out every observation in the manifest.
- **Published field datasets** fill tomato and corn, which the team can't find in a Toronto October either. `python -m ml.training.eval.fieldsets` takes photos from the CC BY 4.0 field sets in `ml/training/eval/fieldsets.yaml`, none of them a training source: maize from Bangladesh (taken June 2026), tomato from Bangladesh (2025) and Septoria from Ghana (2023).
  - **Never a training photo, nor a copy of one.** Every pick is embedded with BioCLIP 2 and dropped at cosine 0.95 or more to any image of a training source, in any split, since a published set can hold another's photos (Tomato-Pakistan held 302 of PlantDoc's). These sets photograph each leaf several times, turned or flipped, and those repeats sit at cosine 0.93–0.97 (✓ checked by eye 2026-10-19), below the training splits' 0.97, so picks at 0.93 or more to each other count as one leaf. Repeats from another angle or from below can sit lower: the second review found 17 ([section 3](#second-review-2026-10-22)). Only a set's raw folders are used, never its augmented copies. A test fails if a field set is ever added to the training sources.
  - **One photo per case**, sent as the `whole_plant` shot (the API requires one), as a user who photographs only the sick leaf would. The harness reports these cases as their own slice, by `source`, beside the team's scans.
  - **Reviewed like iNaturalist's:** a teammate marks each photo `sure`, `probable` or `skip` on a contact sheet, against the profile's signs. Only `sure` cases count toward diagnosis accuracy ([section 3](#how-cases-are-labelled)).
  - **What it gave** (✓ ran 2026-10-19, `--per-class 8`): 118 photos to review. Tomato: early blight 16, late blight 16, bacterial spot 16, yellow leaf curl virus 8, Septoria 8, spider mites 6, leaf mold 3 and healthy 13. Corn: common rust, gray leaf spot, leaf blight and healthy, 8 each. No pick was a copy of a training image: the closest was at cosine 0.932, a different photo (checked by eye).
  - **What they are:** mostly a single picked leaf on paper or a sheet, not a plant in a garden, so they test the diagnosis on a leaf rather than a garden scan. The labels are the sets' own and vary: the first corn leaf blight photos looked at show few of northern leaf blight's long lesions. Hence the review.
  - The 2025 and 2023 sets were published before the VLMs' training data was collected, so a VLM may have seen them *unverified*. The maize set, published in August 2026, can't have been seen; compare the two.
  - **Not our own held-out photos, nor edited training photos.** The training sources' test splits come from the same cameras and fields as their training photos, so they measure in-distribution accuracy (the disease head's reported test numbers). A rotated or recoloured training photo is still that photo to a model ([training 4](training.md#4-data-preparation), step 2), so edits are applied only to evaluation photos, as variants ([section 3](#variants)).
- **Never** from a training source. A PlantDoc, PlantVillage or other training image in the evaluation set would measure memory, not generalisation. `check_leakage.py` checks every photo against every training image and fails on a match (below). It is a command, not part of scoring, so M18 runs it on the final manifest before the test split is scored ([dev roadmap](dev-roadmap.md)).
  - **How:** `python -m ml.training.eval.check_leakage` compares every photo the manifest names with every image under `ml/training/data/raw`, all sources and splits, in two ways. Either one is a match, and a match fails the run until it is looked at by eye: a copy drops the case, and a different photo goes into `ml/training/eval/leakage_allowed.tsv` with the reason. It stops first when a dataset in `sources.yaml`, or one of the COCO folders the detector and the plant check use, has no images on the machine, so it can't quietly cover less. `--calibrate 300` remeasures the thresholds below on edited copies of 300 training images picked at random.
    - **Perceptual hash:** 4 bits or less over the photo's 8 rotations and flips. The hash sets the 32 of its 64 coefficients above their median, so distances are always even and 4 is the largest step below 6. Copies halved, quartered, recompressed at JPEG quality 40, rotated, mirrored or made 15% brighter stay within 4 bits in 98–100% of cases ([calibration](../../ml/training/eval/reports/leakage-2026-10-20.md#calibration) ✓ ran). The margin is one step: the closest different photo is at 6 bits, a tomato leaf on white paper against a COCO photo of a jet in a grey sky. At 8 bits four photos would match, all single leaves on a plain background: three against COCO photos (the jet, a surfer at sea, a beach) and one against a TOM2024 photo of a caterpillar on a maize leaf.
    - **BioCLIP 2 cosine:** 0.98 or more, training 4's duplicate threshold. It backs the hash up: with either signal, 99–100% of those copies match. It doesn't cover crops: with 5% cut off each edge the hash moves a median of 12 bits and 26% of the copies match, and with 10% cut off, 8%. A lower bar doesn't fix that. At 0.95, the bar at which the field-set picks drop a candidate (`fieldsets.COPY`), 81% of the 5% crops match, but one iNaturalist photo of a flowering tomato plant sits above it with 155 training images, all tomato, from six datasets (`--cosine 0.95` lists them ✓ ran). Photos of one species sit close together in BioCLIP's space, so every photo of a common crop would bring pairs to look at by eye.
  - **Result (✓ ran 2026-10-20):** no case is a copy. The 253 photos of the 187 cases were checked against 59,821 training images: every image under `raw` on the rig, including the PlantCity, Pretoria maize and Makerere maize sets fetched for the disease head and not yet in `sources.yaml` on `main`. The one match, at cosine 0.983, is two different flowering tomato plants from iNaturalist (checked by eye, allowed); the next closest pair is at 0.950 ([report](../../ml/training/eval/reports/leakage-2026-10-20.md)). A copy cropped by 5% or more would most likely have been missed (above).

### Splits

- **dev** (about 60%): used to set thresholds, calibrate, choose between models and debug.
- **test** (about 40%): scored once on Oct 24, for the Devpost numbers and the final acceptance. A threshold is never tuned on it.

Cases are split by **plant**, not by image, so two scans of the same plant are always on the same side. iNaturalist observations that may show one plant share a plant key, so they land on the same side: those by one observer, of one crop, less than 100 m apart once the larger of their two positional accuracies is subtracted. The key is the lowest observation's (`plant` in the review file, [inat.py](../../ml/training/eval/inat.py)). After the second review dev holds 64% of the cases. Repeat scans and face photos are dev only, so the growth noise floor and the face-blur recall are dev numbers and are reported as such on Oct 24.

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
  derived_from?: string;           // a variant's original case_id (manifest.variants.jsonl only)
  transform?: string;              // a variant's transform: tilt, dim, warm, cool, blur, far, jpeg, offcentre
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

### Second review, 2026-10-22

Alan reviewed all 187 cases on a page that showed each case's photos, its labels and the profile's signs (✓ ran): 71 looked right, 28 had a wrong label and 88 he couldn't tell from the photos ([#52](https://github.com/aacruzgon/GreenLens/issues/52#issuecomment-6346712013)). Every case he flagged, and every iNaturalist case, was then checked against its photos, the observation's host fields and the profile's indicators, and his review of the result ([#135](https://github.com/aacruzgon/GreenLens/pull/135)) settled the rest:

- **40 iNaturalist cases are dropped because the host isn't the case's plant.** iNaturalist files a disease under its pathogen and a pest under itself, so these observations show the fungus or the mite on whatever it grew on:
  - Squash powdery mildew: 12 of 15 were on cocklebur, beggarticks, New Zealand ribbonwood or pot marigold.
  - Grape black rot: 13 of 20 were on Boston ivy (11), shagbark hickory or a saxifrage.
  - Cedar apple rust: all 4 were the galls on juniper.
  - Spider mites: 9 of 18 were on bean, eggplant, scarlet sage, violet (2), hazel, a willow-like shrub, a daisy or a cucurbit.
  - One early blight case was on black nightshade, and one apple scab case was a quince.
- **8 spider-mite cases are dropped because no plant is readable.** They are microscope or macro shots of a mite.
- **17 field-dataset cases are dropped because they show the same leaf as another case.** Eight leaves had 2–8 cases each, and four of those leaves spanned dev and test. One case per leaf is kept.
- **3 cases are dropped because the photos can't settle Alan's call:** an early blight leaf he read as late blight, too far gone to show either disease's signs; an apple scab case with pale stippling, not scab's olive lesions; and an early blight leaflet that could be potato, which has its own label. Every case he called wrong is now dropped.
- **60 cases go from `sure` to `probable`,** and 38 were `probable` already:
  - 42 that Alan couldn't tell from the photos, most of them detached, dying tomato-dataset leaves.
  - 11 iNaturalist cases he passed that aren't research grade, which section 2 requires; 2 of them are on buffalo gourd.
  - 7 corn cases he passed whose photos don't show the label's signs: 5 leaf blight cases with purpling or red midribs but no cigar-shaped lesions, and 2 common rust cases with no pustules in sight.
- **Two CCMT "Septoria" leaves are relabelled `tomato_leaf_mold`,** with `label_basis` `team_consensus`. They show leaf mold's olive growth on the underside.
- **8 iNaturalist cases now share a plant key,** by section 2's rule for observations that may show one plant: six apple observations from one site on Sep 19 and two tomato observations from one garden, all by one observer. Three apple cases move from dev to test, and one tomato case from test to dev.
- Every kept case has `reviewed_by` `aacruzgon`.

| | Cases | Dev | Test | `sure`, dev | `sure`, test |
|---|---|---|---|---|---|
| Before | 187 | 115 | 72 | 82 | 51 |
| After | 119 | 76 | 43 | 14 | 7 |

After the review, apple has 17 cases, corn 25, grape 7, squash 3 and tomato 67.

What it changes:

- Diagnosis accuracy on the test split rests on 7 `sure` cases: 4 corn (common rust, gray leaf spot and 2 healthy) and 3 tomato (early and late blight). That is too few to pass or fail section 4's targets ([small counts](#4-metrics-and-acceptance-targets)).
- No case is left whose label the review disputes: every case Alan called wrong is dropped, and `probable` means the photos don't show the signs. `calibrate.py` reads every dev case, `sure` or `probable`.
- The dev numbers in section 5 were measured on the 115 dev cases, of which 37 are now dropped and 3 moved to test: the species temperature, the disease head's top 3 and the plant check. They are re-run on the 76.
- An iNaturalist observation's host has to be checked against its host fields and its photos before it's kept.

### Variants

Published photos are sharp, well lit and centred, and the app's often won't be. `python -m ml.training.eval.variants` writes `manifest.variants.jsonl`: for every `diagnosed` or `healthy` case, one copy per transform.

| Transform | What it imitates |
|---|---|
| `tilt` | The phone held at 8–20°, cropped so no corner is blank |
| `dim` | 1–1.3 stops under-exposed, with low-light sensor noise |
| `warm`, `cool` | An evening-sun or incandescent cast; open shade or overcast |
| `blur` | Slightly out of focus: a Gaussian blur of 0.15–0.25% of the long side |
| `far` | Taken from further away: a quarter to a third of the pixels |
| `jpeg` | Sent through a messaging app: 1,280 px at JPEG quality 30–40 |
| `offcentre` | Framed carelessly: 60–70% of each side, towards a corner |

- **A variant is not a new case.** It keeps its original's plant and split. The harness scores each transform as a slice beside the originals, with the originals' count of cases, and never pools it with them.
- **It keeps the truth that doesn't depend on geometry:** outcome, species, problems, soil and indicators. Boxes, masks, depth and measurements are dropped, since tilting and cropping move them.
- **Each transform is mild enough that a person can still see the problem** (✓ checked by eye on a common rust photo, 2026-10-19; a 0.3% blur hid the pustules, so the blur is half that). It is seeded by the case and the transform, so a rerun writes the same bytes.
- **Only evaluation cases get variants.** A transformed training photo would measure memory ([section 2](#where-the-photos-come-from)).

### Feedback joins the set

`POST /scans/{id}/feedback` (contracts 3) stores a reviewer's verdict and corrected label. After build week, a scan with feedback `wrong` and a corrected label is a candidate case: a teammate labels it as above and adds it to dev. Feedback is never added to the set unreviewed, and never to test.

---

## 4. Metrics and acceptance targets

Targets are for the dev split during the week and are confirmed on the test split on Oct 24. A miss is reported as it is; it doesn't move the target.

**Small counts.** Every rate is reported as k of n with its 95% Wilson interval. A target counts as met or missed only when the whole interval is on one side of it; otherwise it is undecided at that count, and the report says so. On the test split's 7 `sure` cases (5 sick, 2 healthy), even a perfect run leaves sick plants diagnosed `healthy` at 0–43% and healthy plants diagnosed with a problem at 0–66%, and 3 `confirmed` diagnoses out of 3 give a precision of 44–100%. None of those targets can be decided on the test split alone, so they are reported beside the dev split's, and beside the species and detection metrics on all test cases.

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

- **The head is judged by its top 3,** its job in the candidate set ([architecture 6.3](../architecture/README.md#63-retrieve-then-verify)). The worker sends each leaf's top 3 classes, `healthy` among them, and the API keeps the problems among them ([contracts 5](../architecture/contracts.md#5-worker-api-and-jobs)), so on a photo with a problem the measure is whether the true problem is among the head's top 3, against guessing 3 of the species' classes. It's also given without the species of 3 classes or fewer, where the head and guessing always score. Healthy leaves are scored by the `healthy` evidence below. Top-1 is reported beside it.
- **Temperature scaling** on the dev cases, the photos most like the app's: one scalar `T`, fitted by minimising the negative log-likelihood, stored in the model manifest ([README 6](README.md#6-model-files-versions-and-pinning)). It doesn't change the top 3. It sets how often the head claims a problem at medium confidence or more, which shows a possible problem and keeps a scan from `healthy`, and how often its top class is `healthy` at medium or more, which counts as positive evidence of health ([contracts 7](../architecture/contracts.md#diagnosis)). Both are reported overall and per species, beside the fits on the head's validation split, on validation plus dev, and per species on dev. Every fit on dev is also scored out of fold: each case at a temperature fitted on the other 4 of 5 folds.
- The worker restricts the logits to the working species' labels **before** the softmax (contracts 5), so `T` is fitted on the same restricted softmax: for each crop in the calibration set, only that crop's labels.
- Reported: expected calibration error (15 bins) before and after, and a reliability diagram per temperature in the report.

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

### Results on the dev cases, 2026-10-20 (✓ ran, offline part)

`python -m ml.training.eval.calibrate` runs the dev cases through the worker's species and disease steps, on the shipped detector and BioCLIP export ([report](../../ml/training/eval/reports/calibration-2026-10-20.md)). The VLM words, the margin and the weights wait on the cases through the API (J5). Nothing is written into a model file yet: the label matrix is the 20-species stand-in until M12, and the disease head isn't chosen. Every fit on the dev cases is also scored out of fold, each case at a temperature fitted on the other 4 of 5 folds.

- **BioCLIP species, `species_temperature`,** on 115 dev cases:
  - At 1.0, the shipped value, 59 cases score 0.8 or more and 92% of them are right, so the agreement rule fails; it would need 0.85.
  - Fitted, 1.88: 32 cases score 0.8 or more, all of them right, and the NLL falls from 1.68 to 1.40. Out of fold (1.74–1.98 per fold), 33 cases score 0.8 or more and all are right.
  - Top-1 is 57% at either value. A lone tomato leaf is often read as potato or pepper, its relatives (tomato 44% of 63). Squash is 0 of 10: the stand-in names *Cucurbita pepo* only, and these powdery-mildew leaves read as cucumber, grape or bean. The real matrix (M12) should cover the squash the cases show.
  - 90 of the 115 cases are a single `whole_plant` photo of a lone leaf, BioCLIP's hardest input. App scans usually have a whole-plant shot, so expect 1.88 to move when it is refitted on the real matrix (M12) and the team's scans.
  - **The real matrix exists (2026-10-22):** `bioclip_labels_edb29fe878952e7b`, the seed's 20 species and 5 organisms ([training 8](training.md#the-bioclip-label-matrix)). Its species are the stand-in's, with squash under its genus *Cucurbita* ([#131](https://github.com/aacruzgon/GreenLens/pull/131)), so on the same cases zero-shot top-1 is still 56%: tomato 28 of 63 (10 called potato, 8 pepper), and squash 0 of 10 even under its genus ([report](../../ml/training/eval/reports/species-check-2026-10-22.md)). `calibrate` now takes the matrix the seed names, so `species_temperature` is refitted on it with M16's next run.
- **Disease head, `temperature`,** on the dev cases' leaf photos with one crop, 94 of them. Three limits apply:
  - **A one-photo case's photo is sent as a `leaf_closeup`,** as section 8's bench sends it. As labelled (`whole_plant`), the worker wouldn't classify it (`no_leaves_found`). That's 82 of the 94 photos, all 11 healthy leaves among them, so these numbers hold for J5 only if the API run sends those cases the same way.
  - **The head is restricted to the case's true species.** The worker restricts it to the working species, BioCLIP's top 1 unless the user names one, and BioCLIP gets tomato right 44% of the time here. The numbers assume the species is right.
  - **The healthy leaves are all field-set photos:** 3 from maize-bd, 3 from tomato-bd and 5 from tomato-bd-iphone, none from iNaturalist. They say nothing yet about healthy garden leaves.
- **Its job is its top 3.** The worker sends each leaf's top 3 classes, `healthy` among them, and the API keeps the problems among them as the scan's classifier candidates (contracts 5). On the 83 photos with a problem:
  - **The true problem is in the top 3 on 73% for both heads, against 52% (v2, 21 classes) and 57% (v2c, 19) for guessing 3 of the species' classes.** Apple and grape have 2 or 3 classes, so both the head and guessing always score there. On the 64 corn and tomato photos: 66% against 38% and 45%. Tomato alone (9 classes in v2, 8 in v2c): 31 and 30 of 52, against 33% and 38%.
  - **On the iNaturalist garden tomato leaves, the photos most like the app's, it does no better than guessing:** 4 of 12 (v2) and 3 of 12 (v2c), against 33% and 38%. Top-1 there is 0 and 1 of 12.
  - If the worker sent 3 problems per leaf, the top 3 once `healthy` is dropped, it would reach 75–76% against 57–64%. That's 1–2 points more than as sent, too little to change the worker for.
  - Top-1 is 36% and 40% of the 94, against 0.84 and 0.88 on the head's own test split. The same code gives 0.82 and 0.84 on its validation crops, so the pipeline isn't the cause. Tomato is 10 and 14 of 60. v2c answers bacterial spot or leaf mold for 35 of the 60 tomato leaves, 6 of the 8 healthy ones among them. v2 spreads its answers more (late blight 14, bacterial spot 14, leaf mold 11). Neither head names spider mites on any of the 12 mite leaves. The report lists every answer per true class.
- **What the temperature changes.** At medium confidence (0.5) or more, a problem on top is shown as a possible problem when the indicators are unsure, and keeps a scan from `healthy`. `healthy` on top counts as positive evidence of health (contracts 7). For v2c, on the 94 photos (claims: photos, with how many are right in brackets):

  | Temperature | Value | Problem at medium | Tomato (60) | Corn (15) | Apple (11) | Healthy leaves with `healthy` at medium | Healthy leaves blocked by a problem |
  |---|---|---|---|---|---|---|---|
  | In the file (fitted on validation) | 1.15 | 61 (27) | 29 (5) | 13 (9) | 11 (5) | 2 of 11 | 5 of 11 |
  | Fitted on validation and dev | 1.66 | 47 (23) | 18 (3) | 11 (8) | 10 (4) | 2 of 11 | 3 of 11 |
  | Fitted on dev | 3.66 | 18 (13) | 0 | 2 (2) | 8 (3) | 2 of 11 | 0 of 11 |
  | Per species, fitted on dev | tomato 4.61, corn 1.57, apple 4.87, grape 0.07 | 24 (19) | 0 | 11 (8) | 5 (3) | 2 of 11 | 0 of 11 |

  Grape's 8 photos are claimed at every value, all of them right: with 2 classes, the top one is always at 0.5 or more. v2 behaves the same way, at 0.81, 1.05, 2.02 and per species: 60 (25), 48 (24), 24 (18) and 28 (21) claims.
  - **The trade.** Fitted on dev, the head stops claiming on tomato, where 24 of its 29 claims were wrong, and no healthy leaf is blocked. But it also drops 7 of corn's 9 right claims and 2 of apple's 5. Right claims fall from 27 to 13, wrong ones from 34 to 5. Without grape, which is claimed and right at every value, the dev fit's claims are right 5 times in 10 (v2: 10 in 16).
  - **The `healthy` evidence barely moves.** In sample, it comes from 2 of the 3 healthy corn leaves at every value (v2: 2 at the file's value, 1 at the others), and from none of the 8 healthy tomato leaves at any value: the head puts `healthy` first on only 1 of them (v2: none). So a healthy tomato reaches `healthy` only through the indicators, whatever the temperature.
  - **Out of fold, the fits hold.** Fitted on dev: 18 claims, 13 right (v2: 23, 17), and no healthy leaf blocked, though the `healthy` evidence falls to 1 of 11 (v2c: 2 in sample). Per species: 25 claims, 19 right (v2: 29, 21), with the evidence at 2 (v2: 1).
  - **For M17 to choose, with this trade in view.** The dev fit (3.66 for v2c, 2.02 for v2) needs no code change. A temperature per species is the better trade on these photos, since it keeps corn's claims. But the head file holds one `temperature` ([`ml/models/FORMATS.md`](../../ml/models/FORMATS.md)), so it needs the format and `DiseaseHead::top` changed on both sides. Its values rest on 8 to 60 photos per species, and grape's runs towards the fit's lower bound, because its 8 photos are all right. At 0.07, grape's 8 claims all come out at 1.0, in the high band rather than medium, so a species whose fit runs to the bound should take the pooled value instead. Either way, the test split checks the choice once (M18).
  - The head's validation crops become underconfident at the dev fit (ECE 0.04 to 0.26–0.31): on photos like those, its claims come out more cautious than they could be, the safer error.
  - **The fix for its accuracy is training data,** not a temperature: garden-like photos of the head's classes, with the retrained head chosen and judged by its top 3 on the dev cases (M13, D8).
  - **Done in v3c, now in the manifest** ([training 6](training.md#6-disease-and-species-heads), [report](../../ml/training/eval/reports/calibration-2026-10-20-garden.md)): the true problem is in its top 3 on 70 of the 83 photos with a problem (84%, against v2c's 61), tomato 40 of 52 and the iNaturalist garden tomato leaves 8 of 12. At its dev fit, 1.82, it claims a problem on 24 photos, 17 right (without grape 16 and 9), and blocks no healthy leaf. Its per-species fits sit near the pooled one (tomato 1.91, corn 1.79, apple 1.80), so for v3c a temperature per species changes one claim. Corn is the exception to the gain: its top 3 loses 1 of 12, and its top-1 falls from 11 to 7 of 15, with 3 of the 5 leaves with northern leaf blight called healthy and 3 of the 5 with common rust called gray leaf spot ([training 6](training.md#6-disease-and-species-heads)).

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
| ONNX Runtime 1.31 with the CUDA 13 execution provider on both cards: a small dynamic-batch model opens with `error_on_failure` and runs batches of 1, 4 and 16 ([README 5](README.md#5-runtimes-and-versions)) | Runs on the RTX 3070 (sm_86) and the RTX 5070 Ti (sm_120), outputs equal to CPU within 1e-4. ✓ ran on both on 2026-10-20 ([#114](https://github.com/aacruzgon/GreenLens/pull/114)): at most 1.8e-7 on a small fp32 model at batches 1, 4 and 16, and every perception model opens and runs on CUDA | Check the driver (580 or later); try the 1.24.1 CUDA 12.8 build on the 3070 only |
| PyTorch training step on the 5070 Ti | A forward and backward pass of the classifier backbone in mixed precision | Train on the 3070 at a smaller batch |
| vLLM serves the primary VLM candidate on the 5070 Ti | Starts with the flags in [vlm.md](vlm.md#4-serving), answers one image request with a schema | Try the llama.cpp path ([vlm.md](vlm.md#4-serving)) |
| The RX 9070 XT as a second VLM worker ([README 4](README.md#4-placement-and-vram-budgets)) | The host shows `gfx1201` in `rocminfo` and the NVIDIA cards still work; vLLM's ROCm image starts with only `/dev/kfd` and the card's render node; the same checkpoint and flags as the 5070 Ti answer a findings and an indicators call with our schema, and vLLM's log shows a native FP8 or W4A16 kernel, not a dequantising fallback | Use the W4A16 checkpoint on both cards; if vLLM still fails by the end of day 2, llama.cpp's Vulkan server with the same model's GGUF |
| Text embedder | The chosen model's ONNX file gives the same vectors in Rust and Python to 1e-5, and a query embeds in 20 ms or less on the control plane's CPU ([models 10](models.md#10-text-embedder)) | Choose the runner-up; the dimension is fixed only after this check passes |
| Face detector | Finds every face in 10 team photos with people, at the upload resolution | Lower the score threshold; add a tile pass ([models 9](models.md#9-face-detector)) |
| BioCLIP zero-shot | Top-3 contains the right species on 20 team photos of supported crops | Check the label wording ([training 8](training.md#8-label-lists)) before suspecting the model |
| LiDAR | The checks in [architecture 10](../architecture/README.md#10-models-and-gpu-placement), and a scale card on a leaf at 30 cm measured within 3% | Owned by the app and backend; the evaluation set's LiDAR cases wait for it |

**BioCLIP zero-shot on the dev cases, until the team's photos exist (2026-10-22, ✓ ran):** with the seed's matrix, the right species is in the top 3 on 75% of the 105 dev cases of supported crops, a case's shots combined with the worker's weights, and in the top 1 on 61%, as with the stand-in, whose 20 species the seed now has. Squash's 10 cases are 0 of 10, even under its genus ([report](../../ml/training/eval/reports/species-check-2026-10-22.md)).

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

### VLM results, 2026-10-20 (✓ ran)

`python -m ml.training.eval.vlm_bench` sent the 65 tomato and corn dev cases through the worker's own `VlmAdapter::analyse` (its prompts, schemas, image preparation and validation, in a release build; `main`'s prompts, `system@1,findings@2,indicators@1`) to each candidate, served by vLLM on the 5070 Ti at 0.90 of the card with 2 slots. It ran at `VLM_MAX_PIXELS` 602,112 and 1,003,520, with one and two jobs at once ([report](../../ml/training/eval/reports/vlm-bench-2026-10-20.md); per candidate: [FP8](../../ml/training/eval/reports/vlm-bench-2026-10-20-fp8.md), [Qwen3.5](../../ml/training/eval/reports/vlm-bench-2026-10-20-qwen35-awq.md), [Qwen3-VL AWQ](../../ml/training/eval/reports/vlm-bench-2026-10-20-qwen3vl-awq.md)).

How it differs from the plan above:
- **One photo per case, not 20 full shot sets.** The cases come from the field datasets (section 2), since the team's phones haven't supplied full cases yet ([#52](https://github.com/aacruzgon/GreenLens/issues/52)). So organism recall, small organisms, soil, beneficial-as-pest and the box check aren't measured, and neither is the 9070 XT.
- **The candidates are the worker's test fixtures:** three per crop, with three questions each, since the knowledge base isn't loaded. 26 `sure` cases have their problem among them, and 11 are healthy.
- **The photo goes to the indicator call twice.** A question gets only the images its indicator names, and most name `leaf_closeup`. So a one-photo case also sends its photo as the `leaf_closeup` it is. When a question also names the whole plant, the call gets the same photo twice, as `leaf_closeup` and as `whole_plant`: about 980 more image tokens per job, so the timings below run a little above a real one-leaf scan's. The first run, the night of Oct 19, sent only `whole_plant`, and 82–87% of every model's answers were `unsure` ([report](../../ml/training/eval/reports/vlm-bench-2026-10-20-whole-plant-only.md)).
- **Two sizes, not four.** That night's four sizes, 0.6 to 2.0 MP, moved a model's diagnosis by at most 2 of the 26 cases.

At 1,003,520 pixels, the worker's default:

| Candidate | Valid on the first try | Job p50 / p95 | 65 jobs: 1 / 2 at once | Diagnosis, `sure` (n = 26) | `unsure` | "Yes" to the true problem's questions / to the others (`sure` cases) | Healthy kept clean (n = 11) | Damage reported | Organisms reported |
|---|---|---|---|---|---|---|---|---|---|
| Qwen3.5-9B AWQ | 65 of 65 | 5.3 / 6.4 s | 360 / 213 s | 35% | 25% | 15% / 10% | 91% | 79, 66 of them `other` | 0 |
| Qwen3-VL-8B FP8 | 65 of 65 | 6.2 / 8.4 s | 423 / 234 s | 12% | 32% | 6% / 7% | 100% | 79: 31 `chewed_margins`, 13 `leaf_mines` | 19 |
| Qwen3-VL-8B AWQ | 65 of 65 | 4.4 / 5.1 s | 302 / 176 s | 12% | 33% | 6% / 6% | 100% | 55 | 4 |

Diagnosis is the top candidate by its unweighted share of "yes" answers. The payload carries no indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.

- **Valid and fast.** No call needed its retry, and species agreed on 97–100% of cases.
  - Two jobs at once finish the 65 in 0.55–0.6 of the time, with p95 at most 9.6 s, far under the 2.5-minute limit, so `VLM_SLOTS` 2 fits on the 5070 Ti at 1.0 MP (contracts question 9).
  - vLLM holds about 14 GB at 0.90 whatever the model, and the KV cache peaked at 4–33%.
- **The questions carry little signal on these photos.** On a leaf with a problem, every model answers "yes" to that problem's questions barely more often than to the other candidates' questions. Qwen3.5 is the only one with a gap, and it is small.
  - **About a third of those answers are `unsure` by design:** the question needs the whole plant or an underside (`lower_leaves_first`, `dark_stem_lesions`, `white_mould_underside`), which one leaf can't show.
  - **Most of the rest are "no".** Qwen3.5's examples:
    - `concentric_rings`: 0 of 6 early-blight leaves;
    - `water_soaked_patches`: 0 of 8 late-blight leaves;
    - the common-rust pustules: 0 of 3.
  - **`small_spots` and `yellow_halo` say "yes" to about 4 in 10 leaves with another problem.** So late blight is often scored as Septoria or early blight.
  - No candidate meets section 4's indicator targets.
- **Not the photo size.** Half the tomato cases are 0.16 or 0.41 MP, but Qwen3.5 gets 5 of those 11 right, against 4 of the 15 larger ones.
- **False findings.** Every model reports damage these leaves don't show: Qwen3.5 under `other`, and FP8 as chewed margins and leaf mines (the known `leaf_mines` error, #45). Only FP8 reports organisms on them. None is labelled, so Qwen3.5's 0 could also be a miss.
- **For D1.** By the choosing rule above, on what was measured, Qwen3.5-9B AWQ leads on indicator answers and is as fast; beneficial-as-pest and organism recall wait on the team's cases. Whichever is chosen, the diagnosis rests on the disease head's candidates until the questions get answered (the prompt pass, P5, and the questions' wording). The per-question tables in the report show which questions fail.
- **Chosen: Qwen3.5-9B AWQ** (Sam, D1, Oct 20; [vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran)). The margin is thin: 9 of 26 against 3 (8 of 26 in the rerun at 0.78), and 15% against 10% on the questions. Two runs confirm it, recorded in vlm.md 13: the 65 cases on [#45](https://github.com/aacruzgon/GreenLens/pull/45)'s prompts after its next pass, and the team's full cases from [#52](https://github.com/aacruzgon/GreenLens/issues/52).
- **The bench now picks each call's images with the pipeline's routing rules** (`domain::rules::routing`), as a scan would. On these 65 cases they pick the same images the run sent; two later cases with a flower or fruit shot no longer send it to the indicators call, since no question names it.
- **At the desktop's budget.** The same 65 cases on Qwen3.5 at 0.78 of the card, the share the 5070 Ti's desktop leaves ([report](../../ml/training/eval/reports/vlm-bench-2026-10-20-qwen35-awq-desk.md)): 359 and 214 s for one and two at once, as at 0.90; 12,500 MiB peak and no preemption. It diagnosed 8 of the 26 against 9 at 0.90: one case changed between two runs at temperature 0, a reminder of how thin a 26-case comparison is.

### Perception batch size

Each model's own cap, with every session loaded, on a scan with 6 shots and 40 leaf crops: the SAM encoder at 1, 2 and 4 images, the detector at 4, 8 and 16, BioCLIP 2 at 8, 16 and 32, and the SAM decoder at 8, 16 and 32 boxes, in fp16 and fp32 where the export allows both. Record the peak VRAM of the whole perception worker and the time per job. Choose the largest caps that leave at least 1 GB of the 3070's 8 GB free at peak ([README 4](README.md#4-placement-and-vram-budgets)). `PERCEPTION_MAX_BATCH` stays the job-wide ceiling above them. If SAM 2.1-T can't run at an encoder batch of 2 in fp16, MobileSAM is benchmarked the same way.

**On the RTX 3070, 2026-10-20 (✓ ran, `python -m ml.training.eval.perception_gpu`, [#114](https://github.com/aacruzgon/GreenLens/pull/114)).** The 3070's host, driver 615.71.09, ONNX Runtime 1.31.0 with the training environment's CUDA 13 wheels, the four perception sessions loaded with the worker's settings (`kSameAsRequested` arena, heuristic cuDNN search, the manifest's `gpu_mem_limit_mb`), each warmed at its cap, then a scan of 6 shots and 40 crops with all 40 boxes on one image. The desktop held about 360 MiB.

| Caps: detector, SAM encoder, SAM decoder, BioCLIP 2 | Peak with the manifest's limits, and free | Without the limits | Scan |
|---|---|---|---|
| 4, 1, 8, 8 | 3,134 MiB, 4,695 free | 3,132 MiB | 1.2 s |
| **8, 2, 16, 16 (the manifest's)** | **4,598 MiB, 3,234 free** | 5,054 MiB, 2,778 free | 1.2 s |
| 16, 4, 32, 32 | fails: the detector at 16 exceeds its 1 GB limit | fails: out of memory in the SAM decoder | — |

- **The manifest's caps stay.** They leave 3.2 GB free at peak against the 1 GB rule, so neither MobileSAM nor BioCLIP v1 is needed.
- **The limits matter:** without them the arenas grow about 450 MiB more.
- **The worker's own run (P2) confirms the caps** in Rust, which writes them to the worker's environment.

### Fallback worker

The same 20 cases through the fallback worker, with the hosted model it uses ([vlm.md](vlm.md#12-the-fallback-worker)): validity, indicator accuracy, organism recall, box placement (one live call settles its box format), whether calls with the 8,000-token thinking budget finish within the fallback's doubled `max_runtime` (contracts open question 26), and the cost per scan in API credits.

---

## 9. Harness and reports

`ml/training/eval/`:

```
ml/training/eval/
├─ manifest.jsonl          the cases (section 3)
├─ manifest.variants.jsonl the phone-condition variants (section 3), written by variants.py
├─ inat.py                 licensed iNaturalist photos: search, contact sheet, review, cases (section 2)
├─ fieldsets.py            published field datasets: picks, copy check, contact sheet, review, cases (section 2)
├─ variants.py             phone-condition variants of the cases (section 3)
├─ fetch.py                downloads the photos by SHA-256 from the photo store
├─ check_leakage.py        perceptual hashes and BioCLIP cosine against every training image; fails on a match not allowed (section 2)
├─ leakage_allowed.tsv     matches looked at by eye and found to be different photos, with the reason
├─ components.py           runs each ONNX model offline, the same files the worker loads
├─ e2e.py                  sends every case through the real API, collects ScanDetail
├─ score.py                compares results with the truth; writes the report
├─ calibrate.py            section 5: temperatures, certainty values, margin sweep
└─ reports/                committed: <date>-<git sha>-<split>.md from score.py; leakage-<date>.md and .json from check_leakage.py
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
