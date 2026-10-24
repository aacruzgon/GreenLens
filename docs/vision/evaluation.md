# PDM Vision Evaluation

> **Status:** Draft v0.29, 2026-10-24. v0.29: section 3 adds M10's 107 new cases, all dev and `probable`: 64 iNaturalist observations (garden plants, pests on tomato, grape, apple, pepper and rose, and hornworms), 12 Commons look-alikes and 31 nutrient-deficiency photos. Claude screened them at Sam's request, since no one on the team can judge plant photos, and no teammate reviewed them a second time. Section 2's leakage result is the Oct 24 run on the 267 cases, and the guided soil checks are now one bought topsoil, its surface photos and squeeze tests, with no jar test. v0.28: section 2 adds M10's next sources: iNaturalist organisms whose observer recorded the crop they were on (the `Host` fields) and garden plants marked as cultivated; the look-alikes from two CC BY 4.0 nutrient-deficiency sets and 17 Wikimedia Commons cases, with Sam's choices (CC BY-SA allowed for evaluation photos, photos from before 2026 as their own slice); and our own captures through `captures.py` (the app's LiDAR cases and the guided soil checks). New cases go to dev only, since the test split is frozen. Section 3 adds a LiDAR shot's ruler, a look-alike's `look_alike`, a case's `taken` date and `manifest.soil.jsonl` for soil cases. v0.27: Alan's follow-ups on #192: T11–T16 stay unmeasured on the test split for good, since J5 runs on dev only and the test split isn't scored again; section 4 says the ECE rule's baseline is a perfectly calibrated head, not one at 0.05; and the ECE moves by 0.0066 without the 7, not 0.006. v0.26: section 4 has the test split, scored once on Oct 23 (M18): the plant check 58 of 58, species top-1 41 of 56 (missed) and top-3 52 of 56, the disease head's top 3 36 of 43 against 60% guessing, its ECE 0.210 (undecided: a perfectly calibrated head with the same confidences averages 0.165 on 51 photos, so section 4's small-counts rule now sets the ECE against one), the VLM valid on every call at 8.8 s p95, a false face box on 38 of 73 photos (missed), and the end-to-end targets not measured for the submission; sections 1, 2 and 3 no longer promise an Oct 24 score; section 9 adds `score_test_split.py` and the run's script `m18.sh`. v0.25: section 5's plant check ships the trained probe v2 at 0.94 / 0.31 (M17): it passes all 102 dev cases, and 3 of the 1,000 hold-out no-plant photos pass; section 5's agreement threshold needs at least 10 cases scored that high, and the temperature and threshold say where they live. v0.24: section 6's spot colour thresholds are fitted (M15) on PlantSeg's lesion masks inside our own leaf masks, raising the hold-out spots' two-class IoU from 0.382 to 0.497, and specks under 2e-5 of a leaf no longer count as spots: on the 102 dev cases' healthy photos and PlantDoc's, 21.1% have no spot instead of 13.2%, and 5.3% have no spot and no hole instead of 3.5%. v0.23: the team's own photos are dropped, as build week left no time to shoot (#52): section 2 fills the photo groups from public photos, and Sam takes the LiDAR cases and repeat scans with the app's LiDAR module (#148, merged Oct 23) and runs the guided soil tests on bought soils, from which section 6 is refitted; section 4's embedding target uses photos of one plant until then; section 7's zero-shot check on the 102 dev cases misses its 95% top 3; section 8's next run adds the 3 tomato multi-photo cases. v0.22: the face photos are 93 public Open Images photos of people near plants, since the team's didn't come, and section 7 records the face check on them: recall 94.7% against 98%, a face box on 67 of the 130 dev plant photos against 2%, and only threshold 0.15 with growth 0.35 reaching 98%, at 40% of an average plant photo blurred; section 4 says a false blur isn't cheap (#159). v0.21: the fallback's run (section 8's fallback paragraph) cites P9's live run, which settled the box format, time and cost. v0.20: section 5 re-runs the offline calibration on the 102 dev cases with the seed's label matrix: `species_temperature` 1.3747 goes into the manifests (56 cases at 0.8 or more, 98% right), v3c's top 3 holds at 93%, the numbers favour keeping its 1.82, the trained plant probe passes every case, and the VLM words keep their defaults; section 8 records D1's re-check on the shipped prompts; section 5's BioCLIP step keeps the temperature at the fit when the agreement rule fails and raises only the threshold, counting a case whose species the matrix can't name as wrong. v0.19: section 2 gives field-dataset frames up to 3 apart in one folder one plant key, as they may show one plant (runs chain, and a key set by hand is kept), so 4 cases change split (160 in all, 102 dev and 58 test, 9 of the test cases `sure`), and its leakage result is the Oct 23 run on that manifest; section 3 counts the 22 field-dataset replacements left after Alan's review; section 4's small counts follow the 9; section 3 names the 7 test cases that were used on dev, which M18 reports with and without. v0.18: section 3 adds 41 replacement cases, screened for the host, the signs and repeats of a leaf already in the set and reviewed by Alan (160 in all, 104 dev and 56 test, 8 of the test cases `sure`); section 2's leakage result is the Oct 22 run, whose report records the manifest's SHA-256. v0.17: section 3 records the second review: Alan reviewed all 187 cases, and settling it dropped 68 (40 iNaturalist cases whose host isn't the case's plant, 8 with no readable plant, 17 repeats of one leaf and 3 his calls the photos can't settle), so 119 remain (75 dev, 44 test), 7 of the test cases `sure`; section 2 applies the research-grade rule without exception, lets a crop's label cover its genus and gives observations that may show one plant one key, and section 4 reports small counts with a Wilson interval. v0.16: section 5 notes that the seed's label matrix (M12, its 20 species the stand-in's) replaces the stand-in the species temperature was fitted on, so 1.88 is refitted on it; section 7's zero-shot check has a dev-case stand-in until the team's photos. v0.15: section 5 records v3c, the disease head retrained on garden-like photos and now in the manifest: the true problem in its top 3 on 84% of the dev photos with a problem, against v2c's 74%, while its corn top-1 falls from 11 to 7 of 15. v0.14: after review, section 2 states the leakage check as measured: a match within 4 bits, its thresholds calibrated on edited copies of 300 training images, and crops as what it misses; M18 runs it on the final manifest. v0.13: section 5 scores the disease head's top 3 as the API keeps it, on the photos with a problem (73% against 52–57% for guessing), gives the temperature's trade per species with the `healthy` evidence, adds a per-species fit and out-of-fold scores, gives the claims without grape, which is claimed at every value, and leaves the choice to M17. v0.12: section 5 judges the disease head by its top 3 (72–74% on the dev leaf photos, against 51–56% for guessing) and proposes its dev-fitted temperature, which keeps its medium-confidence claims to the 72–75% that are right. v0.11: section 5 records the offline calibration on the dev cases: `species_temperature` 1.88 on the stand-in matrix, and a disease head that reaches 36–40% on the dev leaf photos, which no temperature fixes. v0.10: section 2 records the leakage check: no evaluation photo is a copy of a training image. v0.9: section 8's per-question rates count `sure` cases only (Qwen3.5 15% / 10%), a one-photo case's photo goes to the indicators call twice when a question names the whole plant, and D1's two confirming runs are named (review of #86). v0.8: section 7's ONNX Runtime check passes on the RTX 3070, and section 8 records the perception batch caps measured there: the manifest's caps peak at 4,598 MiB with 3.2 GB free. v0.7: section 8's VLM results name the prompts they ran on, say the diagnosis column is the VLM's answers alone, and note that the bench now routes images by the pipeline's rules. v0.6: Sam chose Qwen3.5-9B AWQ (D1), and a rerun at the desktop's memory budget matched the full-card run (section 8). v0.5: section 8 records the day-2 VLM benchmark on the 65 one-photo tomato and corn dev cases: every call valid and two jobs at once fit, but the indicator questions carry little signal on these photos for any candidate (Qwen3.5-9B AWQ diagnoses 35% of the sure cases, both Qwen3-VL-8B builds 12%). v0.4: tomato and corn diseases come from published field datasets no model trained on (`ml.training.eval.fieldsets`), checked against every training photo and sent one photo per case, and phone-condition variants of the cases (`ml.training.eval.variants`) are scored as their own slices. v0.3: section 2 adds `ml.training.eval.inat` for licensed iNaturalist photos, only from 2026 and reviewed before they become cases, and records that iNaturalist can't supply tomato or corn diseases. v0.2: the fallback run also measures whether 8,000-token calls finish within its `max_runtime`. v0.1, 2026-10-15: First evaluation plan: the evaluation set of real field photos, its labels, metrics and acceptance targets per stage, calibration of the certainty words, the classifier and the diagnosis margin, the thresholds the set decides, and the day-1 checks and day-2 benchmarks.
> **Checked:** the test split was scored once on Oct 23 ([section 4](#results-on-the-test-split-2026-10-23--ran-scored-once)); a miss is reported as it is, and the targets stand.

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
4. **The Devpost numbers.** The test split was scored once, on Oct 23 ([section 4](#results-on-the-test-split-2026-10-23--ran-scored-once)), and those numbers go in the write-up ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)).

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
| Pests and beneficials | 15 | Aphids, whiteflies, spider mites, caterpillars, slugs and snails, flea beetle damage, and at least 5 cases with a beneficial (ladybug adults and larvae, lacewing larvae, spiders, bees). Ladybug larvae are mandatory: they are the classic "killed by mistake" case ([architecture 6.5](../architecture/README.md#65-organisms-and-damage)). From iNaturalist observations whose observer recorded the crop they were on (below) |
| Look-alikes | 10 | Abiotic problems that mimic disease: nitrogen deficiency, sunscald, frost damage, overwatering, herbicide drift if found. iNaturalist has none from 2026 (✓ searched 2026-10-24: 1,829 observations of the five crops, about 30 words). They come from two CC BY 4.0 nutrient-deficiency field sets (`fieldsets.py`) and from Wikimedia Commons (`commons.py`): frost, hail, sunscald and deficiencies. None was found for herbicide drift or overwatering under a usable licence. A look-alike is `diagnosed` with no problems and names its disorder in `look_alike` (section 3) |
| `label_only` species | 10 | Common garden plants outside the supported crops (basil, lettuce, beans, roses, hostas), for species identification and for "no confident diagnosis on an unsupported species" ([architecture 6.2](../architecture/README.md#62-species-identification), step 6) |
| No plant, unusable | 10 | A fence, a pet, a hand, a blurred shot, a dark shot, a plant too far away to read. These must end in `no_plant_found` or `photos_unusable`, or be rejected by the quality gate |
| LiDAR | 20 | Taken on a LiDAR iPhone through the development build, each with a ruler or a printed scale card in at least one shot. Overlaps the groups above. Used for the measurement thresholds (section 6). Not taken yet: Sam takes them with the app's LiDAR module ([#148](https://github.com/aacruzgon/GreenLens/pull/148), frontend roadmap F5, merged Oct 23), and section 6's thresholds keep their proposed values until then |
| Repeat scans | 5 plants × 3 scans | The same plant scanned three times within an hour on a LiDAR phone, for the growth noise floor (section 6). Not taken yet, as for the LiDAR group; section 4's image-embedding target uses photos of one plant in the meantime |
| Guided soil checks | 7 | One bag of topsoil (Sam, 2026-10-24: no sand, potting clay or jars): its surface dry, moist, wet, waterlogged and mulched for the VLM's soil findings, and two squeeze tests, each its own case in `manifest.soil.jsonl` (section 3), through `captures.py` (below). The squeeze result is what Sam's hand showed. With no jar test and no lab report, the topsoil's texture is unknown, so the texture call and the jar test aren't measured. A public set could stand in for dry, moist and wet: a CC0 set of 629 iPhone photos with sensor-measured moisture (GitHub `akabircs/Soil-Moisture-Imaging-Data`) *unverified*. No usable public photos of squeeze or jar tests were found |
| Faces | 93 photos | Public photos of people near plants from Open Images V7's validation and test splits: Flickr originals of 2,000 px or more under CC BY 2.0, with 313 boxed faces, 117 of them partly hidden. Picked by the smallest face's width in the stored image, from 15 photos whose smallest face is under 20 px to 17 whose smallest is 160 px or more, by `ml/training/data/faces.py`; `ml/training/data/splits/faces_openimages.tsv` credits each photo (✓ ran 2026-10-22). They replace the team's garden photos with people (#52), which didn't come. Used only for the face blur's recall, never kept with the plant cases |

That is about 130 cases and 93 face photos. Size matters less than spread: every supported crop, every common look-alike pair, and every image kind should appear.

### Where the photos come from

- **Not the team's own photos.** The plan was for the team to photograph gardens around Toronto, Oct 16–23, as the core of the set, since those photos match what the app will see ([#52](https://github.com/aacruzgon/GreenLens/issues/52)). Sam dropped it before Oct 22, as build week left no time to shoot ([the decision on #52](https://github.com/aacruzgon/GreenLens/issues/52#issuecomment-6374214283)), so every photo case comes from the licensed sources below.
  - **The groups only phones were to give come from public photos too** (next, M10): pests and beneficials (ladybug adults and larvae on a crop whose plant can be named), look-alikes, `label_only` species, no-plant and unusable shots, and soil photos for the VLM's soil findings. No-plant photos come from sources none of our models trained on, so not COCO, whose photos are the plant probe's negatives.
  - **Public photos have no depth,** so Sam takes the LiDAR cases and repeat scans with the app's LiDAR module ([#148](https://github.com/aacruzgon/GreenLens/pull/148), merged Oct 23), and takes the guided soil photos on bought topsoil (table above).
- **Licensed photos** make up the set, only when the licence allows reuse with credit: CC0 or CC BY observations on iNaturalist, and extension photo libraries whose terms allow it. Each one records its source, author and licence.
  - **iNaturalist:** `python -m ml.training.eval.inat` searches the taxa in `ml/training/eval/inat_targets.yaml` (a disease is listed under its pathogen), downloads the CC0 and CC BY photos without metadata, and writes a contact sheet and a review file. A teammate keeps an observation only after checking the host plant and giving each photo its shot kind, and `add` writes it to the manifest as a `source_caption` case, `probable` unless the identification is research grade and the signs are clear. A case is one scan, so it needs a photo that can serve as the `whole_plant` shot.
    - **Pests on a named crop** (added 2026-10-24): a target with a `host` finds animals whose observer recorded the plant they were on, in the observation fields `Host`, `Host Plant ID` or `Eating`, so the case's plant is known, not guessed from an insect close-up. ✓ ran 2026-10-24: 2026 observations with CC0 or CC BY photos name tomato 31 times, pepper 7, grape 116, apple 16 and rose 25 (the totals of iNaturalist's filter, before matching). The reviewer still checks that the plant shows.
    - **Garden plants are casual.** iNaturalist grades an observation marked as cultivated `casual`, so a target for a garden plant (basil, lettuce, bean, rose, hosta, pepper) sets `quality: all`; those cases stay `probable`, as `needs_id` ones do.
- **Look-alikes** (added 2026-10-24): `fieldsets.yaml` adds the maize nutrient-deficiency set (Mendeley 34gb2gr7p2, India, 2025: nitrogen, phosphorus, potassium and magnesium) and `tomato-bd-2025`'s magnesium class under its own name, so its existing picks stay. `commons.yaml` lists 17 Commons cases by hand (frost on tomato and pepper, hail on tomato, maize and apple, sunscald on pepper, apple and grape, potassium and magnesium deficiency, two grape leaves whose "nutrient deficiency" leafroll virus would look like, and blossom-end rot, which the knowledge base has); `commons.py` reads each file's licence, author and date from the Commons API. Two choices of Sam's, on 2026-10-24: **CC BY-SA** is allowed for evaluation photos, which stay in the private photo store (the frost photos have no other licence), and **photos from before 2026** are used, though BioCLIP 2 may have seen them through EOL *unverified*, so each case records when it was `taken` and the report shows Commons cases as their own slice. A lab-checked tomato set (OLID I, Zenodo 8105154: nitrogen 47, potassium 36, both 40) and a pepper set grown hydroponically (Mendeley dh5cpvn48j) are on Zenodo or in one zip, which `fieldsets.py` doesn't read yet.
- **Our own captures** (added 2026-10-24): `python -m ml.training.eval.captures` imports the LiDAR cases and repeat scans the app's depth check saves (a development build, Settings › LiDAR › Depth check › Save: each shot's photo, 16-bit depth PNG, DepthMeta and report, with a `case.json`), and photos taken with the iPhone's Camera app, such as the guided soil checks. A photo with depth keeps its sensor frame and only its EXIF orientation, since the API turns the depth with the photo (contracts 3); every other photo is turned upright with no metadata. `scan` writes a review file with the soil cases' truth filled in from their names and prints each scale-card ruler against its known length; `add` writes the kept cases with `label_basis` `team_consensus`, `source` `team` and `licence` `team-internal`.
    - **The research-grade rule has no exception.** A `needs_id` observation stays `probable` even when both reviewers agree that the signs are clear (the second review applied it to 11 cases).
    - **A crop's label covers its genus:** an apple case may be a crabapple, a grape case a wild vine (*Vitis riparia*, *V. vulpina* or a muscadine), and a squash case any *Cucurbita*, buffalo gourd included. The pathogens and their signs are the same, and the label set has no other *Malus*, *Vitis* or *Cucurbita* (`squash.yaml` names the genus itself). A plant with its own label is never folded in: a leaflet that may be potato isn't kept as tomato.
  - **Only photos from 2026.** BioCLIP 2's training data, TreeOfLife-200M, was compiled before 2026 and draws on GBIF, which holds iNaturalist's licensed observations *unverified*, so an older photo may be one BioCLIP has seen. The script refuses an earlier date. Never use a photo from a web search: PlantDoc was scraped from Google Images.
  - **Not for tomato or corn** (✓ ran 2026-10-19): iNaturalist holds no research-grade CC0 or CC BY observation from 2026 of Septoria leaf spot, early or late blight, common rust or northern leaf blight, and at most 6 unverified ones of each. Apple scab (67), cedar-apple rust (119), grape black rot (99), two-spotted spider mites (41) and ladybug larvae (over 1,000) have enough. Tomato and corn come from published field datasets (next item).
  - The species head's iNaturalist training photos ([training 6](training.md#6-disease-and-species-heads)) leave out every observation in the manifest.
- **Published field datasets** fill tomato and corn, which the team can't find in a Toronto October either. `python -m ml.training.eval.fieldsets` takes photos from the CC BY 4.0 field sets in `ml/training/eval/fieldsets.yaml`, none of them a training source: maize from Bangladesh (taken June 2026), tomato from Bangladesh (2025) and Septoria from Ghana (2023).
  - **Never a training photo, nor a copy of one.** Every pick is embedded with BioCLIP 2 and dropped at cosine 0.95 or more to any image of a training source, in any split, since a published set can hold another's photos (Tomato-Pakistan held 302 of PlantDoc's). These sets photograph each leaf several times, turned or flipped, and those repeats sit at cosine 0.93–0.97 (✓ checked by eye 2026-10-19), below the training splits' 0.97, so picks at 0.93 or more to each other count as one leaf. Repeats from another angle or from below can sit lower: the second review found 17 ([section 3](#second-review-2026-10-22)). Only a set's raw folders are used, never its augmented copies. A test fails if a field set is ever added to the training sources.
  - **One photo per case**, sent as the `whole_plant` shot (the API requires one), as a user who photographs only the sick leaf would. The harness reports these cases as their own slice, by `source`, beside the iNaturalist cases.
  - **Reviewed like iNaturalist's:** a teammate marks each photo `sure`, `probable` or `skip` on a contact sheet, against the profile's signs. Only `sure` cases count toward diagnosis accuracy ([section 3](#how-cases-are-labelled)).
  - **What it gave** (✓ ran 2026-10-19, `--per-class 8`): 118 photos to review. Tomato: early blight 16, late blight 16, bacterial spot 16, yellow leaf curl virus 8, Septoria 8, spider mites 6, leaf mold 3 and healthy 13. Corn: common rust, gray leaf spot, leaf blight and healthy, 8 each. No pick was a copy of a training image: the closest was at cosine 0.932, a different photo (checked by eye).
  - **What they are:** mostly a single picked leaf on paper or a sheet, not a plant in a garden, so they test the diagnosis on a leaf rather than a garden scan. The labels are the sets' own and vary: the first corn leaf blight photos looked at show few of northern leaf blight's long lesions. Hence the review.
  - The 2025 and 2023 sets were published before the VLMs' training data was collected, so a VLM may have seen them *unverified*. The maize set, published in August 2026, can't have been seen; compare the two.
  - **Not our own held-out photos, nor edited training photos.** The training sources' test splits come from the same cameras and fields as their training photos, so they measure in-distribution accuracy (the disease head's reported test numbers). A rotated or recoloured training photo is still that photo to a model ([training 4](training.md#4-data-preparation), step 2), so edits are applied only to evaluation photos, as variants ([section 3](#variants)).
- **Never** from a training source. A PlantDoc, PlantVillage or other training image in the evaluation set would measure memory, not generalisation. `check_leakage.py` checks every photo against every training image and fails on a match (below). It is a command, not part of scoring, so M18 runs it on the final manifest before the test split is scored ([dev roadmap](dev-roadmap.md)).
  - **How:** `python -m ml.training.eval.check_leakage` compares every photo the manifest names with every image under `ml/training/data/raw`, all sources and splits, in two ways. Either one is a match, and a match fails the run until it is looked at by eye: a copy drops the case, and a different photo goes into `ml/training/eval/leakage_allowed.tsv` with the reason. It stops first when a dataset in `sources.yaml`, or one of the COCO folders the detector and the plant check use, has no images on the machine, so it can't quietly cover less. `--calibrate 300` remeasures the thresholds below on edited copies of 300 training images picked at random.
    - **Perceptual hash:** 4 bits or less over the photo's 8 rotations and flips. The hash sets the 32 of its 64 coefficients above their median, so distances are always even and 4 is the largest step below 6. Copies halved, quartered, recompressed at JPEG quality 40, rotated, mirrored or made 15% brighter stay within 4 bits in 98–100% of cases ([calibration](../../ml/training/eval/reports/leakage-2026-10-20.md#calibration) ✓ ran). The margin is one step: the closest different photo is at 6 bits, a tomato leaf on white paper against a COCO photo of a jet in a grey sky. At 8 bits four photos would match, all single leaves on a plain background: three against COCO photos (the jet, a surfer at sea, a beach) and one against a TOM2024 photo of a caterpillar on a maize leaf.
    - **BioCLIP 2 cosine:** 0.98 or more, training 4's duplicate threshold. It backs the hash up: with either signal, 99–100% of those copies match. It doesn't cover crops: with 5% cut off each edge the hash moves a median of 12 bits and 26% of the copies match, and with 10% cut off, 8%. A lower bar doesn't fix that. At 0.95, the bar at which the field-set picks drop a candidate (`fieldsets.COPY`), 81% of the 5% crops match, but one iNaturalist photo of a flowering tomato plant sits above it with 155 training images, all tomato, from six datasets (`--cosine 0.95` lists them ✓ ran). Photos of one species sit close together in BioCLIP's space, so every photo of a common crop would bring pairs to look at by eye.
  - **Result (✓ ran 2026-10-24):** no case is a copy. The 359 photos of the 267 cases were checked against 61,591 training images, and the only match is the one allowed on Oct 20. The next closest pairs, at cosine 0.969 and 0.962, are different photos (checked by eye): the Commons hail-dented apple against a PlantDoc photo of red apples on a branch, and the Commons hail-torn maize leaf against a Makerere maize leaf with blight ([report](../../ml/training/eval/reports/leakage-2026-10-24.md), with the manifest's SHA-256).
  - **Result (✓ ran 2026-10-23):** no case is a copy. The 203 photos of the 160 cases were checked against 59,821 training images: every image under `raw` on the rig, including the PlantCity, Pretoria maize and Makerere maize sets fetched for the disease head. The one match, at cosine 0.983, is two different flowering tomato plants from iNaturalist (checked by eye on Oct 20, allowed); the next closest pair is at 0.950 ([report](../../ml/training/eval/reports/leakage-2026-10-23.md), with the manifest's SHA-256). The Oct 20 run on the first 187 cases found the same ([report](../../ml/training/eval/reports/leakage-2026-10-20.md)). A copy cropped by 5% or more would most likely have been missed (above).

### Splits

- **dev** (about 60%): used to set thresholds, calibrate, choose between models and debug.
- **test** (about 40%): scored once on Oct 23 ([section 4](#results-on-the-test-split-2026-10-23--ran-scored-once)), for the Devpost numbers and the final acceptance. A threshold is never tuned on it.

Cases are split by **plant**, not by image, so two scans of the same plant are always on the same side. iNaturalist observations that may show one plant share a plant key, so they land on the same side: those by one observer, of one crop, less than 100 m apart once the larger of their two public positional accuracies is subtracted. An obscured location's public accuracy is tens of kilometres, so one observer's obscured observations of a crop always share a key. The key is the lowest observation's (`plant` in the review file, [inat.py](../../ml/training/eval/inat.py)), and the split follows it. In the field datasets a plant can't be told from the photos. Repeats of one leaf keep one case, and frames up to 3 apart in one folder, which may show one plant, share a plant key: the lowest frame's, whose split they follow ([fieldsets.py](../../ml/training/eval/fieldsets.py)). Runs chain, so frames 1, 4 and 7 are one plant. The frame is the first number in the file name. A key a reviewer sets by hand from the photos is kept, and the runs of the frames that carry it take it (`fieldsets regroup`). With the replacement cases dev holds 64% of the cases. Repeat scans and face photos are dev only, so the growth noise floor and the face-blur recall are dev numbers and are reported as such in the write-up.

**Cases added after M18 go to dev.** The test split was scored once on Oct 23 (section 4) and is never added to or scored again, so `inat.py` and `captures.py` give every new case `dev`, and refuse one whose plant key is already in test; no existing case changes split.

### Privacy and storage

- No photo in the set has location metadata: the harness strips EXIF on import, as the API does. Faces in plant cases are blurred on import; the face photos are kept separately, outside git and `pdm-models`, and never published.
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
    ruler?: {                      // the app's two-point ruler on a LiDAR shot (captures.py)
      points: [{ x: number; y: number }, { x: number; y: number }];  // tapped on the photo as shown
      measured_mm: number;         // the app's length from the depth
      known_mm?: number;           // the scale card's or ruler's true length, as typed
    };
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
    look_alike?: string;           // an abiotic problem the knowledge base doesn't name (inat.LOOK_ALIKES): with
                                   // outcome diagnosed and problems [], the scan should end without a confirmed disease
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
  device_model?: string;           // the phone, for a capture of ours ("iPhone18,2")
  taken?: string;                  // when the whole-plant photo was taken, for a photo from before 2026 (commons.py)
}
```

**Soil cases** (`manifest.soil.jsonl`, from `captures.py`) aren't scans: a photo of soil in a tray, a squeezed handful or a jar has no plant. They keep `EvalCase`'s fields, with `images` of kind `soil_surface`, `squeeze_test` or `jar_test` and this `truth` in place of the scan's:

```ts
interface SoilTruth {
  soil: Partial<Record<SoilAspect, string>>;   // contracts 6's values; texture from the soil bought
  guided: (                                    // contracts 6's GuidedTexture, per guided photo
    | { image: number; kind: "squeeze_test"; result: "crumbles" | "holds_ball" | "sticky_ribbon" | "unclear" }
    | { image: number; kind: "jar_test"; settled: boolean; texture?: "sandy" | "loamy" | "silty" | "clayey" | "unclear" }
  )[];
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
- **10 iNaturalist cases now share a plant key,** by section 2's rule for observations that may show one plant: six apple observations from one site on Sep 19 and two tomato observations from one garden, all by one observer, and two buffalo gourd observations by another, whose locations iNaturalist obscures. Three apple cases and one squash case move from dev to test, and one tomato case from test to dev.
- Every kept case has `reviewed_by` `aacruzgon`.

| | Cases | Dev | Test | `sure`, dev | `sure`, test |
|---|---|---|---|---|---|
| Before | 187 | 115 | 72 | 82 | 51 |
| After | 119 | 75 | 44 | 14 | 7 |

After the review, apple has 17 cases, corn 25, grape 7, squash 3 and tomato 67.

What it changes:

- Diagnosis accuracy on the test split rested on 7 `sure` cases: 4 corn (common rust, gray leaf spot and 2 healthy) and 3 tomato (early and late blight); the replacements and the regrouped frames add 2 ([below](#replacement-cases-2026-10-22)). That is too few to pass or fail section 4's targets ([small counts](#4-metrics-and-acceptance-targets)).
- No case is left whose label the review disputes: every case Alan called wrong is dropped, and `probable` means the photos don't show the signs. `calibrate.py` reads every dev case, `sure` or `probable`.
- The dev numbers in section 5 were measured on the 115 dev cases, of which 37 are now dropped and 6 moved to test (4 by the plant keys above, 2 by the regrouped frames below): the species temperature, the disease head's top 3 and the plant check. They are re-run on the 102 dev cases, with the replacements and regrouped frames below (M16).
- An iNaturalist observation's host has to be checked against its host fields and its photos before it's kept.

### Replacement cases, 2026-10-22

The same day, more candidates were fetched and screened against their photos for the host, the signs and repeats of a leaf already in the set (✓ ran):

- **iNaturalist:** every 2026 CC0 or CC BY observation of the five targets that had lost cases, 248 new ones. 44 were set aside because their host field names another plant. Of the 204 screened by photo, 19 are kept: 12 grape black rot (one `sure`, the only research-grade one with clear signs) and 7 apple scab. Three of the apple cases are crabapples and several grape cases wild vines, kept by section 2's genus rule.
  - None of the 93 new cedar apple rust observations shows apple. Every one shows the galls on juniper.
  - None of the 19 new spider-mite observations is clearly on tomato.
  - Cucurbit powdery mildew has only 16 matching observations in 2026, all seen before.
  - The wrong hosts were caught through descriptions and photos (Boston ivy, Virginia creeper, Pyracantha, pear, lime, maple), not host fields.
  - One new apple case shares a plant key with an older one by the same observer (section 2, Splits); both are in dev.
- **Field datasets:** `fieldsets fetch --per-class 16` picked 89 new photos, of which 23 were kept and 22 remain after Alan's review: 5 `sure` (4 healthy leaves and 1 Septoria leaf) and 17 `probable`.
  - 20 of the skipped photos repeat a leaf already in the set, among them every new iPhone early blight, late blight and bacterial spot pick.
  - None of the 8 new corn leaf blight picks shows northern leaf blight's long cigar-shaped lesions; they show purpling or red midribs, as most of the set's corn leaf blight cases do.
  - None of the 8 new yellow leaf curl picks shows yellow margins.
  - One kept healthy corn leaf is dropped after Alan's review: it is the frame after another case's, on the same backdrop, so it may be the same plant.
- **Alan reviewed the new cases against their photos** on [#140](https://github.com/aacruzgon/GreenLens/pull/140) (✓ ran), so the 41 kept have `reviewed_by` `aacruzgon`. His call on the corn leaf blight cases is applied to the whole set ([above](#second-review-2026-10-22)).
- **Neighbouring frames share a plant key (2026-10-23):** Alan's review found healthy corn img-41 (test, `sure`) and img-43 (dev) two frames apart on one mesh, and 3 more pairs of frames up to 3 apart sat on both sides of the split. Section 2 now gives such frames the lowest frame's key: 9 cases take a neighbour's key, and 4 change split. Gray leaf spot img-500 (`sure`), healthy corn img-43 and late blight 7 go to test, and bacterial spot 10 goes to dev, with frames 7 and 8.
  - Those three had been used on dev. img-500 and late blight 7 were dev cases from the start: in the calibration that chose v3c, the VLM benches and M16's fits. img-43 is in M16's fits on the 104. With the 4 cases the plant keys above moved, 7 of the 58 test cases helped make choices on dev, 1 of them `sure` (img-500). They stay in test, and M18 reports each test number with and without them.
- **The leakage check passes:** the 203 photos of the 160 cases were compared with all 59,821 training images, and the only match is the one allowed on Oct 20 ([report](../../ml/training/eval/reports/leakage-2026-10-23.md), on the regrouped manifest, whose SHA-256 it records).

| | Cases | Dev | Test | `sure`, dev | `sure`, test |
|---|---|---|---|---|---|
| After the review | 119 | 75 | 44 | 14 | 7 |
| With the replacements | 160 | 104 | 56 | 19 | 8 |
| With neighbouring frames grouped | 160 | 102 | 58 | 18 | 9 |

The set now holds apple 24, corn 30, grape 19, squash 3 and tomato 84. Diagnosis accuracy on the test split rests on 9 `sure` cases, 5 corn and 4 tomato, 6 sick and 3 healthy: these sources hold few photos whose label the photo plainly shows. Diagnosis through the API wasn't measured on them, and won't be: J5's end-to-end harness wasn't built for the submission, it runs on dev only, and the test split isn't scored again. The species and detection metrics were scored on all 58 test cases ([section 4](#results-on-the-test-split-2026-10-23--ran-scored-once)).

### New sources, 2026-10-24 (M10)

The candidates from section 2's new sources were screened against their photos (✓ ran). Sam asked Claude to do it, since no one on the team can tell plants or their problems apart from photos. Claude (`claude-opus-5-5`, with two subagents for the iNaturalist half) looked at every photo on a numbered contact sheet. Sam kept one leaf-miner case on the review page himself, and his answer stands.

It is a screen, not a diagnosis. The label is the source's: the iNaturalist observer's identification and the host plant they recorded, the dataset's class folder, the Commons photographer's caption. The screen checks that the named plant shows and can be recognised, that a photo can serve as the whole-plant shot, that no face shows, and that the signs aren't another problem's.

- **Every new case is `probable`,** so it counts toward detection and species metrics but never toward diagnosis accuracy ([How cases are labelled](#how-cases-are-labelled), step 4). Commons' frost cases, which `commons.yaml` had as `sure` from the photographer's title, are `probable` too.
- **`labelled_by` is `claude-opus-5-5`** and `reviewed_by` is empty: no teammate has reviewed them, so step 3 isn't met. A teammate's review on the [review page](https://claude.ai/artifact/4C5ipnZvB6qbmJHaVZFo5E) can still fill it.
- **iNaturalist, 64 of 145 kept:**
  - Garden plants: 35 of 52 (pepper 10, basil 6, lettuce 6, rose 5, hosta 5, bean 3). Dropped as not healthy: 11, for example a rose that may have rose rosette, and a basil leaf with a lesion. Dropped as another plant: cowpea pods filed as bean.
  - Animals on a named crop: 23 of 60. Rose 8 of 10, grape 8 of 15, apple 4 of 10, tomato 2 of 20 and pepper 1 of 5. 18 of the 20 tomato observations are clip-on macro shots of one whitefly, mealybug or mite with no plant to see, which the second review also dropped ([above](#second-review-2026-10-22)). The animals stay unlabelled, `healthy` with no problem, until their boxes are drawn (step 6).
  - Hornworms: 6 cases of `tomato_hornworms` on tomato: 5 of the 8 tobacco hornworm larvae and 1 of the 8 *Cotesia* wasp observations. The other 7 *Cotesia* observations, the one tomato hornworm and 3 tobacco hornworms were on other plants: shrubs, twigs, weedy nightshades and probably a pepper.
  - Beneficials: none of the 16 lacewing and hoverfly larvae is on a plant that can be named. They were on denim, concrete, hands, wood, nettle or leaves of no crop in the label set. So the set still has no beneficial on a crop, and no ladybug.
  - Reasons, by the first one given: 33 not the named plant, 26 no plant to see, 11 not healthy, 8 the same plant as a kept case on the same day, 3 poor photos.
  - Four groups of kept observations share a plant key by section 2's rule (one observer, one crop, within 100 m); all are in dev.
- **Commons, 12 of 17 kept:** frost on tomato (2) and pepper (1), hail on tomato, maize and apple, sunscald on pepper, potassium deficiency on tomato and maize, magnesium deficiency on maize, a grape leaf with red margins, and blossom-end rot on the vine. Dropped:
  - The pepper at Indale, which looks green and upright.
  - Two picked fruits on white with no plant: the sunburnt apple, which could be a rot, and a blossom-end rot tomato.
  - The sunburnt grape cluster, whose shrivelled berries could be a rot.
  - The Lagrein leaf, which may be autumn colour.
- **Deficiency photos, 31 of 40 kept:** maize nitrogen 6, potassium 7, magnesium 7 and phosphorus 7, and tomato magnesium 4. Skipped: 4 whose signs are unclear (glare, a dead leaf, a mostly green one), and 5 that show another problem too (insect holes, or leaf spots on 4 of the tomato leaves).
- **The leakage check passes** on the 267 cases ([section 2](#where-the-photos-come-from), [report](../../ml/training/eval/reports/leakage-2026-10-24.md)).

| | Cases | Dev | Test | `sure`, dev | `sure`, test |
|---|---|---|---|---|---|
| Before | 160 | 102 | 58 | 18 | 9 |
| With M10's new sources | 267 | 209 | 58 | 18 | 9 |

The set now holds apple 29, basil 6, bean 3, corn 60, grape 28, hosta 5, lettuce 6, pepper 13, rose 13, squash 3 and tomato 101. 42 cases are look-alikes: magnesium deficiency 12, potassium 9, phosphorus 7, nitrogen 6, frost 3, hail 3, sunscald 1 and a nutrient deficiency 1. The test split is unchanged.

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

Targets are for the dev split during the week, and the test split was scored against them once, on Oct 23 ([below](#results-on-the-test-split-2026-10-23--ran-scored-once)). A miss is reported as it is; it doesn't move the target.

**Small counts.** Every rate is reported as k of n with its 95% Wilson interval. A target counts as met or missed only when the whole interval is on one side of it; otherwise it is undecided at that count, and the report says so. On the test split's 9 `sure` cases (6 sick, 3 healthy), even a perfect run leaves sick plants diagnosed `healthy` at 0–39% and healthy plants diagnosed with a problem at 0–56%, and 3 `confirmed` diagnoses out of 3 give a precision of 44–100%. None of those targets can be decided on the test split alone, so they are reported beside the dev split's, and beside the species and detection metrics on all test cases. The ECE has no such interval, and on a few dozen photos even a perfectly calibrated head scores well above 0.05, because each of the 15 bins holds few photos. So the ECE is set against that head: each photo's top 1 drawn right with the probability the head gives it, 20,000 times (`score_test_split.py`). Its target counts as missed only when the ECE is above both the target and 95% of those draws, and met only when the ECE and the draws' average are both at or under it; otherwise it is undecided. The baseline is a perfectly calibrated head, not one whose ECE is 0.05, so on a larger set, where those draws fall well under 0.05, a head whose true ECE is under 0.05 could still be called missed when its measured ECE lands above 0.05. On the test split's 51 photos it changes no verdict. A check that every job finishes after one retry is a check for zero failures, not a verdict: 41 of 41 is still 91.4–100%.

### Upload and privacy (control plane)

| Stage | Metric | Target |
|---|---|---|
| Blur and exposure check | Share of good shots rejected | 5% or less |
| | Share of blurred, dark or blown-out shots caught | 80% or more |
| Face blur | Recall of faces 20 px or more across in the stored image (a face counts as caught when the blurred area covers 90% of its box) | 98% or more |
| | Share of plant photos with a false face blurred over a leaf | 2% or less |

The face blur fails closed (an image isn't stored until blurring succeeds), so recall matters far more than precision. A missed face is a privacy failure. A false blur isn't cheap either: the blurred pixels are gone before any model sees the photo, and the face check found the blur on about half the dev plant photos, often over the symptom (section 7, [#159](https://github.com/aacruzgon/GreenLens/issues/159)).

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

Until the repeat scans exist (section 2), the image-embedding target is measured on photos of one plant from different shots: the 19 multi-photo iNaturalist dev observations and the 6 dev plant keys with more than one case. A photo counts when its nearest neighbour among the dev photos of its crop is another photo of the same plant.

### Measurement (LiDAR cases)

| Quantity | Metric | Target |
|---|---|---|
| Ruler or scale card at 30 cm | Error of a measured length | 3% or less |
| `leaf_length_cm` | Error against a ruler | 5% or 0.5 cm, whichever is larger |
| `lesion_diameter_mm`, `hole_diameter_mm` | Error against callipers or the scale card | 20% or 0.5 mm, whichever is larger |
| `plant_height_cm` | Error against a tape | 10% or 5 cm, whichever is larger |
| Measurable indicators | Answers that contradict the sure label | 5% or less |

These wait for the LiDAR cases (section 2), which Sam takes with the app's LiDAR module ([#148](https://github.com/aacruzgon/GreenLens/pull/148), merged Oct 23).

### Text embedder (control plane)

| Metric | Target |
|---|---|
| Recall@5 on 50 labelled question → chunk pairs from the loaded knowledge base | 80% or more |
| p50 latency of one query on the control plane's CPU | 20 ms or less |

### Results on the test split, 2026-10-23 (✓ ran, scored once)

M18 scored the 58 test cases once with the final manifest's files at the values they ship: the seed's label matrix at `species_temperature` 1.3747, disease head v3c at its file's 1.82, the trained plant probe v2 at `τ_leaf` 0.94 and `τ_plant` 0.31, and Qwen3.5-9B AWQ at the 5070 Ti's production settings (2 jobs at once, 1,003,520 pixels). Nothing was fitted or changed ([report](../../ml/training/eval/reports/test-split-2026-10-23.md), `score_test_split.py`, section 9). Before it ran, `check_leakage` passed on the final state, plant probe v2's 1,770 COCO photos included ([report](../../ml/training/eval/reports/leakage-2026-10-23-m18.md)), and the scorer reproduced every committed dev number on the dev split. Each row is also reported without the 7 test cases that were dev cases when choices were made; no rate moves by more than 1.3 points, the ECE by 0.0066, and a crop's macro F1 by 0.05 on 4–27 photos ([devpost 3](devpost.md#test-split-scored-once-by-m18)). The report says how the run went, with its first attempt's slip, and rebuilds from its own JSON without photos or a GPU.

| Stage | Metric (target) | Test | Verdict |
|---|---|---|---|
| Plant check | Recall on cases with a plant (99%) | 58 of 58 (93.8–100%) | Undecided |
| Species, BioCLIP 2 zero-shot | Top-1 on the five crops (85%) | 41 of 56, 73.2% (60.4–83.0%) | Missed |
| | Top-3 (95%) | 52 of 56, 92.9% (83.0–97.2%) | Undecided |
| | Confident agreement with the VLM on a wrong species (3% or less) | 0 of 41 (0–8.6%) | Undecided |
| Disease head | True problem in the top 3, leaf photos with a problem (85%) | 36 of 43, 83.7% (70.0–91.9%), against 60% guessing; `sure` photos 5 of 5 | Undecided |
| | ECE at the shipped temperature (0.05) | 0.210 on 51 photos; a calibrated head with the same confidences: 0.165 on average, 0.210 or more in 12% of draws | Undecided |
| VLM | Calls valid on the first try (99%; 100% after one retry) | 82 of 82 calls (95.5–100%); 41 of 41 jobs (91.4–100%) | Undecided; no failures after one retry |
| | Job time at p95, two at once (2.5 min) | 8.8 s | Met |
| Upload | Good photos rejected by the quality gate (5%) | 3 of 73, 4.1% (1.4–11.4%) | Undecided |
| | Plant photos with a false face box (2%) | 38 of 73, 52.1% (40.8–63.1%) | Missed |
| End to end | `confirmed` precision, coverage, healthy and sick errors, wall time | Not measured: `e2e.py` and `score.py` (J5) weren't built | |

- **The test split matches dev on species, the plant check, the VLM and the face boxes; the disease head's top 3 is lower.** Species top-1 and top-3 were 74% and 90% on dev, and the face box 67 of 130 dev photos. The disease head's top 3 was 93% on dev, above the test interval (70.0–91.9%), and its ECE 0.087. 14 of the 15 wrong species are tomato leaves, 7 of them called potato. The VLM names the species on 40 of its 41 cases, against BioCLIP 2's 27 on the same cases.
- **The disease head's ECE can't be decided on 51 photos.** A perfectly calibrated head with the same confidences scores 0.165 on average, 0.05 or more in all but 1 of 20,000 draws, and 0.210 or more in 12% of them (Alan's review of #192). Without the 7 it is 0.204 on 47 photos, against 0.174 for that head. Its mean top-1 confidence, 0.50, matches its accuracy, 26 of 51, though the 14 photos it scores 0.4–0.6 are right 4 times (dev: 0.087 on 89). The temperature stays 1.82: the test split chooses nothing.
- **The end-to-end rows stay unmeasured on the test split.** J5 runs on the dev cases only, and the test split isn't scored again, so T11–T16 have no test numbers, now or later.
- **The false face boxes** are D13's ([#159](https://github.com/aacruzgon/GreenLens/issues/159)): at the shipped growth no threshold meets both the recall and the false-blur target on the Open Images and dev photos ([models 9](models.md#9-face-detector)).

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

- Fit a temperature for BioCLIP's softmax on the dev cases the same way as the classifier, then check the top-1 accuracy of cases at 0.8 or more. If it is below 95%, raise the threshold in steps of 0.01 until it isn't: a wrong one-tap confirmation is worse than showing the top 3. A threshold counts only while at least 10 cases score that high; if none holds with 10, the check reports no threshold rather than one no case reaches (`calibrate.agreement_threshold`). The temperature stays at the fit; raising it instead would change every species confidence, not only the one-tap cut. A case whose species the label matrix can't name is left out of the fit, which needs its row, and counted as wrong in this check.
- Where they live: the temperature is `species_temperature` in the `bioclip` entry of both manifests. A job's `model_versions` holds each entry's version, not its config, so it doesn't record the temperature. The threshold has no field: the 0.8 is the high band's lower edge (`Confidence::HIGH`, [contracts 1](../architecture/contracts.md#1-conventions)), and every check so far has kept it. A raised threshold would need a field for the worker or the app to read, which nothing holds yet.

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
  - 90 of the 115 cases are a single `whole_plant` photo of a lone leaf, BioCLIP's hardest input. App scans usually have a whole-plant shot, so expect 1.88 to move when it is refitted on the real matrix (M12) and on cases with a real whole-plant shot.
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

### Results on the 102 dev cases, 2026-10-23 (✓ ran, offline part)

The same steps on the dev split after the second review, the replacements and the frame grouping (sections 2 and 3): 102 cases. Species are scored on the seed's label matrix `edb29fe878952e7b`, the one `knowledge.surql` names, and the disease head is v3c, the manifest's ([report](../../ml/training/eval/reports/calibration-2026-10-23.md)). `calibrate` embeds each photo at the size the bucket stores it, as the worker gets it. A case whose species the matrix can't name would be left out of the fit and counted as wrong at 0.8; none is left. The margin and the weights still wait on J5.

- **`species_temperature` 1.3747, now in both manifests.** Fitted by the NLL, which falls from 0.785 at 1.0 to 0.739, while the 15-bin ECE rises from 0.118 to 0.130 (0.129 out of fold). The agreement threshold stays at 0.8 at either value.
  - At it, 56 cases score 0.8 or more and 55 of them are right (98%), so the agreement rule holds at 0.8. The wrong one is a tomato early-blight leaf read as potato at 0.808.
  - Out of fold (1.29–1.44 per fold): 56 cases at 0.8 or more, 98% right.
  - At 1.0, 63 cases reach 0.8 and 97% are right, with a corn leaf read as hosta at 0.812 as the second wrong one. The rule holds there too, but the fit is this section's value, and it offers fewer one-tap confirmations.
  - Top-1 is 75%: apple 14 of 15, corn 16 of 18, grape 12 of 13, tomato 34 of 55 and squash 0 of 1. Oct 20's 57% was on the 115 cases before the second review; on these cases the stand-in and the seed's matrix score alike ([species check](../../ml/training/eval/reports/species-check-2026-10-23.md)).
- **v3c's top 3.** On the 72 leaf photos with a problem, the true problem is in its top 3 on 93%, against 62% for guessing. On corn and tomato, where it can miss, 90% of 50 against 46%. Both iNaturalist tomato leaves with a problem are in it. Top-1 is 54% of the 89 photos, and tomato's 11 healthy leaves get `healthy` first only twice (bacterial spot 7, leaf mold 2).
- **v3c's temperature.** The file's 1.82 was fitted on Oct 20's dev cases. On these 89 photos, 17 of them healthy (claims: photos, with how many are right in brackets):

  | Temperature | Value | Problem at medium | Tomato (50) | Corn (17) | Healthy leaves with `healthy` at medium | Healthy leaves blocked by a problem | Dev ECE |
  |---|---|---|---|---|---|---|---|
  | In the file (Oct 20's dev cases) | 1.82 | 26 (21) | 3 (1) | 1 (0) | 4 of 17 | 0 of 17 | 0.087 |
  | Fitted on these cases | 1.29 | 31 (23) | 7 (2) | 2 (1) | 6 of 17 | 0 of 17 | 0.134 |
  | The same, out of fold | 1.16–1.40 | 33 (24) | 9 (3) | 2 (1) | 7 of 17 | 0 of 17 | 0.137 |
  | Per species, fitted on these cases | tomato 1.25, corn 1.41, apple 1.77, grape 0.05 | 34 (25) | 10 (4) | 2 (1) | 6 of 17 | 0 of 17 | 0.152 |
  | Fitted on validation and dev | 0.78 | 60 (33) | 32 (11) | 6 (2) | 8 of 17 | 6 of 17 | 0.166 |

  Apple's 10 photos and grape's 12 are claimed at every value, 8 and 12 of them right.
  - **The numbers favour keeping 1.82 (M17 chooses).** The new fit claims 5 more photos, 2 of them right, and gives 2 more healthy leaves the `healthy` evidence. But on tomato it adds 4 claims with 1 right, and its dev ECE is higher (0.134 against 0.087). Only the fit on validation and dev blocks a healthy leaf, 6 of 17. A value per species changes little, and grape's runs to the fit's lower bound again.
- **Plant check** (section 6: the highest `τ_plant` that keeps 99% of the dev cases with a plant; [report](../../ml/training/eval/reports/plant-check-dev-cases-2026-10-23.md), `python -m ml.training.eval.plant_check --cases …`). The 102 cases have 130 plant shots, and a case passes when any shot does.
  - **The shipped pair, zero-shot A at 0.40 / 0.70, passes 101 cases (99.0%).** It fails one iNaturalist tomato (best leaf score 0.36, plant probability 0.56). At `τ_plant` 0.98 it still passes 101 and lets 5.0% of the hold-out no-plant photos through instead of 11.0%, but it fails 3.0% of one dataset's validation plant photos, over the 1% rule of [models 2](models.md#2-plant-present-check). So 0.70 stays for this pair.
  - **The trained probe v2, shipped in M17 at 0.94 / 0.31, passes all 102,** with 3 of the hold-out's 1,000 no-plant photos passing, no validation plant photo failing and 0.99% of iNaturalist's whole-plant photos ([report](../../ml/training/eval/reports/plant-check-dev-cases-2026-10-23-probe-v2.md)). The highest `τ_plant` that keeps 99% of the cases is 0.91, but it fails 2.4% of one validation dataset and 6.2% of iNaturalist's whole-plant photos, so 0.31 stays. Its 34 iNaturalist cases aren't in `inat_species.tsv`, so they are the check on whole plants the threshold search never saw. v1, at 0.99 / 0.71, also passed all 102; it learned from non-commercial photos, so it doesn't ship ([models 2](models.md#2-plant-present-check)).
- **VLM certainty words.** Step 1 matches organism, damage and soil findings to a truth, and these cases label none of the three. Their leaves show disease, so every damage or organism finding on them is false (P11). The words keep their defaults, 0.4, 0.65 and 0.85 (step 4). Only the species guess can be scored (step 5). On the 72 bench cases of section 8's re-check, Qwen3.5 says `low` on 32 guesses, all of them right, and `high` on 40, with 39 right; FP8 says `high` on all 72, with 71 right. So the word says little about whether a species guess is right ([report](../../ml/training/eval/reports/vlm-bench-2026-10-23.md)).

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
| Spot colour (`SpotSegmenter`) | **Fitted on 2026-10-22** ✓ ran (M15): brown to red hues (0–45°) and greys are necrotic, as is anything darker than 0.2 of the leaf's median value; yellow (45–66°) is halo | PlantSeg's lesion masks in place of hand-drawn ones: 576 photos of 12 spot diseases on five crops (357 to fit from its training split, 219 held out from its validation and test splits), inside the leaf masks our own detector and SAM cut. The fit sets the segmenter's own thresholds (HSV relative to the leaf's median value, not CIELAB) by coordinate descent on the two-class IoU, the mean of the lesion and the healthy leaf's IoU; lesion IoU alone rewarded painting the whole leaf on PlantSeg's coarse masks (0.432 on the hold-out, the one way the hold-out shaped the method; no value was chosen on it). On the hold-out spots it rose from 0.382 to 0.497 (lesion IoU 0.195 to 0.381, healthy leaf 0.569 to 0.612, the affected share's median error 16.0 to 10.2 points), and on every crop and disease; on the 175 hold-out photos whose masks paint whole regions (mildews, late blight, leaf mold, northern leaf blight), from 0.311 to 0.390. PlantSeg has no halo labels, so the halo is fitted only as part of the affected pixels, and nothing checks its splits for near-duplicates of each other, so the hold-out may share photos with the fit half *unverified*. **Edges:** five fields ended at the end of their search range (`brown_hue_min` 0, `brown_saturation` 0, `brown_value` 2.5, `pale_value` 0, `halo_saturation` 0), which turns those conditions off once the leaf's median value is at least 0.4: every pixel under 0.08 saturation is necrotic, white glare included, and every hue of 45–66° is halo, yellow-green leaf included. A wider range can't move the four at the end of their scale, and wasn't tried for `brown_value`. **Specks:** every necrotic patch counted as a spot, so glare or grain kept a healthy photo from reading clean (contracts 7, `clean_images`). A 4-connected patch under `min_spot_share` 2e-5 of its leaf is now leaf (`colour-3`), the largest share at which PlantSeg's diseased fit photos read clean at most a point more often (2.5% against 1.7%). The filter works per patch, as the spot count does, so a lesion broken into pieces can lose its small pieces, and it removes nothing on a leaf under 50,000 px, where the share is a pixel or less. On 114 healthy check photos at a depth photo's stored size (2,048 px; the healthy photos of the 102 dev cases at `8d3f04b` and PlantDoc's), the share with no spot rose from 13.2% to 21.1%, with none of 82 dev photos of spot diseases reading clean and the hold-out IoU at 0.496; a share ten times larger would reach 45.6% at 2.5 points more diseased photos read clean. The healthy rule also needs no hole, and holes in SAM's leaf masks (median 8 px) stop more healthy photos than spots do: with no spot and no hole, 3.5% read clean before the filter and 5.3% after (6 of 114). The Rust segmenter gives the same class as the fitting code on every pixel of 40 hold-out leaves from all 20 diseases at 2e-5, and of 20 at 0.005, where the filter turns 97,350 pixels to leaf ([report](../../ml/training/eval/reports/spots-2026-10-22.md), its parity section) |
| Smallest sized spot | 4 pixels | Check the diameter error of spots 4–8 px across on the scale card. Raise it if the error passes 50% |
| Stored size of LiDAR shots | Long side 2048 px or more | Check that the smallest labelled Septoria spots (1.5 mm) are 4 px or more at 30–45 cm |
| Growth noise floor | 2 cm or 5% | From the repeat scans: the 95th percentile of the height difference between scans of the same plant within an hour. The floor is the larger of that and the proposal |
| Leaf distance range | 25–45 cm hint, 0.25–0.6 m valid | The lower bound comes from the LiDAR day-1 check ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)); the eval set confirms the error targets hold across the range |

**Without LiDAR cases (2026-10-23).** The team's photos were dropped ([#52](https://github.com/aacruzgon/GreenLens/issues/52)) and no LiDAR case has been taken yet, so the rows fitted on LiDAR cases or repeat scans keep their proposed values, not fitted: indicator margin, coverage for confidence, plane residual, smallest sized spot, stored size of LiDAR shots, growth noise floor and leaf distance range. They're refitted from the LiDAR cases and repeat scans Sam takes with the app's LiDAR module ([#148](https://github.com/aacruzgon/GreenLens/pull/148), merged Oct 23) as part of M10.

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
| Face detector | Finds every face in the 93 Open Images photos of people near plants, at the upload resolution. ✓ ran 2026-10-22 on the shipped settings ([report](../../ml/training/eval/reports/faces-2026-10-22.md)): 267 of the 282 faces 20 px or more across in the stored image (94.7%; section 4's target is 98%), and, re-run on 2026-10-23 on the 130 photos of the 102 dev cases, a face box on 67 of them (target 2%), on leaf spots, fruit, stems and mulch, blurring 14.2% of an average photo. Uploads at 3,200 px catch 268, at 2,048 px 265 | Only threshold 0.15 with growth 0.35 meets 98%: 99.3%, with a box on 90 of the 130 dev plant photos and 40.2% of an average one blurred ([models 9](models.md#9-face-detector)). Growth 0.35 and a third pass at 640 px reach 96.5% with 17.5% blurred. No setting brings the false blur near 2%; the choice is [#159](https://github.com/aacruzgon/GreenLens/issues/159)'s ([dev-roadmap 9](dev-roadmap.md#9-decisions-and-blockers), D13) |
| BioCLIP zero-shot | Top-3 contains the right species on the dev cases of supported crops (the plan's 20 team photos were dropped, [#52](https://github.com/aacruzgon/GreenLens/issues/52)) | Check the label wording ([training 8](training.md#8-label-lists)) before suspecting the model |
| LiDAR | The checks in [architecture 10](../architecture/README.md#10-models-and-gpu-placement), and a scale card on a leaf at 30 cm measured within 3% | Owned by the app and backend; the evaluation set's LiDAR cases wait for it |

**BioCLIP zero-shot on the dev cases (✓ ran) misses the 95% top 3 of section 4.** On the 102 dev cases left after the second review and the frame grouping, at `species_temperature` 1.3747 (2026-10-23, [#156](https://github.com/aacruzgon/GreenLens/pull/156)), the right species is in the top 3 on 90.1% of the 101 cases of the head's crops and in the top 1 on 74.3% ([report](../../ml/training/eval/reports/species-check-2026-10-23.md)). The first run, on Oct 22 on the 115 cases before the second review: with the seed's matrix, the right species is in the top 3 on 75% of the 105 dev cases of supported crops, a case's shots combined with the worker's weights, and in the top 1 on 61%, as with the stand-in, whose 20 species the seed now has. Squash's 10 cases are 0 of 10, even under its genus ([report](../../ml/training/eval/reports/species-check-2026-10-22.md)).

---

## 8. Day-2 benchmarks

### VLM (architecture open decisions 4 and 15, contracts open questions 1 and 9)

On the dev cases, for each of the two candidates in [vlm.md](vlm.md#2-model-choice). The full shot sets were to come from the team's phones, which were dropped ([#52](https://github.com/aacruzgon/GreenLens/issues/52)). The nearest are the dev split's 19 iNaturalist observations with 2–4 photos each (whole plant, leaf close-ups and undersides, flowers or fruit): 10 apple, 6 grape and 3 tomato. The bench has candidate fixtures for tomato and corn only, so the next run adds the 3 tomato ones, and apple and grape join once they have fixtures (M10). None of the 19 has a labelled organism or a soil shot, so they don't stand in for those checks:

| Measure | How |
|---|---|
| Structured-output validity | Every call; count decode and validation failures |
| `maxLength` and `maxItems` enforcement | Send a schema with `maxLength: 10` and a prompt that invites a long answer; check whether the backend stops it (contracts 6) |
| Box format | Draw the returned boxes on the prepared images for 5 cases and check them by eye; confirm the adapter's `box_format` ([vlm.md](vlm.md#9-box-formats)) |
| Indicator accuracy, organism recall, beneficial-as-pest | Section 4, on these cases |
| Time per job | Time to first token and decode speed per call, wall time per job, p50 and p95 |
| Peak VRAM | `nvidia-smi` on the RTX 5070 Ti and `amd-smi` on the RX 9070 XT during the run, with `VLM_SLOTS` 1 and 2, and vLLM's own KV-cache usage from `/metrics` |
| Image size | `VLM_MAX_PIXELS` at about 0.6, 1.0, 1.4 and 2.0 megapixels: accuracy against time, with small-organism recall reported separately ([vlm.md 5](vlm.md#small-organisms)) |

**Choosing.** First the candidate with 0 beneficial-as-pest errors and valid output; then the higher indicator accuracy; then organism recall; then speed. `VLM_SLOTS` is 2 only when two jobs fit next to the model at the chosen `VLM_MAX_PIXELS` and a job's p95 with two running stays under 2.5 minutes ([contracts 12](../architecture/contracts.md#12-open-questions), question 9).

**Per card.** The run is repeated on each VLM card, with the same checkpoint. Each card gets its own `VLM_SLOTS` from its own timing. On 10 of the cases the two cards' outputs are compared: boxes drawn on the same prepared image should overlap, and indicator answers should agree on most questions. A large disagreement is a configuration problem (checkpoint, preparation or server flags) to fix before both serve.

### VLM results, 2026-10-20 (✓ ran)

`python -m ml.training.eval.vlm_bench` sent the 65 tomato and corn dev cases through the worker's own `VlmAdapter::analyse` (its prompts, schemas, image preparation and validation, in a release build; `main`'s prompts, `system@1,findings@2,indicators@1`) to each candidate, served by vLLM on the 5070 Ti at 0.90 of the card with 2 slots. It ran at `VLM_MAX_PIXELS` 602,112 and 1,003,520, with one and two jobs at once ([report](../../ml/training/eval/reports/vlm-bench-2026-10-20.md); per candidate: [FP8](../../ml/training/eval/reports/vlm-bench-2026-10-20-fp8.md), [Qwen3.5](../../ml/training/eval/reports/vlm-bench-2026-10-20-qwen35-awq.md), [Qwen3-VL AWQ](../../ml/training/eval/reports/vlm-bench-2026-10-20-qwen3vl-awq.md)).

How it differs from the plan above:
- **One photo per case, not 20 full shot sets.** The cases come from the field datasets (section 2), since the team's full cases never came: their photos were dropped ([#52](https://github.com/aacruzgon/GreenLens/issues/52)). So organism recall, small organisms, soil, beneficial-as-pest and the box check aren't measured, and neither is the 9070 XT. The public pest, beneficial and soil cases (section 2) measure them next.
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
- **For D1.** By the choosing rule above, on what was measured, Qwen3.5-9B AWQ leads on indicator answers and is as fast; beneficial-as-pest and organism recall wait on the public pest and beneficial cases (section 2). Whichever is chosen, the diagnosis rests on the disease head's candidates until the questions get answered (the prompt pass, P5, and the questions' wording). The per-question tables in the report show which questions fail.
- **Chosen: Qwen3.5-9B AWQ** (Sam, D1, Oct 20; [vlm.md 13](vlm.md#results-and-the-choice-2026-10-20--ran)). The margin is thin: 9 of 26 against 3 (8 of 26 in the rerun at 0.78), and 15% against 10% on the questions. Two runs confirm it, recorded in vlm.md 13: the 65 cases on [#45](https://github.com/aacruzgon/GreenLens/pull/45)'s prompts after its next pass, and the public pest and beneficial cases (section 2), in place of the team's full cases ([#52](https://github.com/aacruzgon/GreenLens/issues/52)). The first ran on Oct 23, on the shipped prompts and the dev cases left after #147, and the choice holds ([vlm.md 13](vlm.md#re-check-on-the-shipped-prompts-2026-10-23--ran)): Qwen3.5 diagnoses 3 of the 7 `sure` cases from its answers and FP8 1, and on the 52 cases every run since Oct 20 has, each prompt version gives Qwen3.5 2 of 6 and FP8 0 or 1.
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

The same cases through the fallback worker, with the hosted model it uses ([vlm.md](vlm.md#12-the-fallback-worker)): validity, indicator accuracy, organism recall, and box placement. P9 ran 8 of the dev cases on 2026-10-23 ([report](../../ml/training/eval/reports/p9-fallback-2026-10-23.md)) and settled the box format (`abs_px_prepared`), the time (a job at most 15.5 s against the doubled `max_runtime`) and the cost (6.6 cents a scan); contracts open question 26 is closed.

---

## 9. Harness and reports

`ml/training/eval/`:

```
ml/training/eval/
├─ manifest.jsonl          the cases (section 3)
├─ manifest.variants.jsonl the phone-condition variants (section 3), written by variants.py
├─ inat.py                 licensed iNaturalist photos: search, contact sheet, review, cases (section 2)
├─ commons.py, commons.yaml look-alike cases from Wikimedia Commons, listed by hand: licences, contact sheet, cases (section 2)
├─ captures.py             our own captures: the app's LiDAR cases and the guided soil checks, review, cases (section 2)
├─ manifest.soil.jsonl     the soil cases (section 3), written by captures.py
├─ fieldsets.py            published field datasets: picks, copy check, contact sheet, review, cases (section 2)
├─ variants.py             phone-condition variants of the cases (section 3)
├─ fetch.py                downloads the photos by SHA-256 from the photo store
├─ check_leakage.py        perceptual hashes and BioCLIP cosine against every training image; fails on a match not allowed (section 2)
├─ leakage_allowed.tsv     matches looked at by eye and found to be different photos, with the reason
├─ components.py           runs each ONNX model offline, the same files the worker loads
├─ e2e.py                  sends every case through the real API, collects ScanDetail
├─ score.py                compares results with the truth; writes the report
├─ calibrate.py            section 5: temperatures, certainty values, margin sweep
├─ score_test_split.py     section 4: the test split scored once, each stage on its own, at the shipped values (M18)
├─ m18.sh                  the script that ran it once on the rig, committed as it ran
└─ reports/                committed: <date>-<git sha>-<split>.md from score.py; leakage-<date>.md and .json from check_leakage.py; test-split-<date>.md and .json from score_test_split.py
```

- **Components** run offline with `onnxruntime-gpu` on the same model files and manifest hashes the perception worker loads, so a component number belongs to a deployed model. Pre- and post-processing come from one Python package shared with training, and a parity test checks it against the Rust worker on 10 images (the worker's side is in [docs/backend](../backend/vision.md)).
- **End to end** goes through the real API with a dedicated evaluation user and garden, so it measures what users get: the quality gate, face blurring, the pipeline, scoring and the diagnosis. Questions are answered from the case's indicator truth where it has one, and left to time out otherwise. The garden is deleted after each run.
- **The fallback path** is measured by stopping the rig's workers for the run.
- **The test split (M18)** was scored without `e2e.py` and `score.py`, which weren't built: `score_test_split.py` runs `plant_check`, `species_check`, `calibrate`, `vlm_bench` and `face_check`'s code on the test cases at the final manifest's values, fitting nothing, and was checked first by reproducing the committed dev numbers on the dev split. `m18.sh` ran it under the GPU lock and refuses a second test run. The report's JSON keeps what the photos and models gave, so `--rebuild` recomputes every row from it, and a test checks the committed report rebuilds exactly.
- **Each report** records the git SHA, every `model_versions` entry, the configuration values from section 5 and 6, the split, and every table of section 4 with pass or miss. A model change, a prompt change or a knowledge-base change reruns the dev split before it is merged.

---

## 10. Open questions

1. **Size of the set.** Set by what the licensed sources hold, since the team's own photos were dropped ([#52](https://github.com/aacruzgon/GreenLens/issues/52)): 160 cases on Oct 23, with more as M10 adds the groups of section 2. The report says how many cases came from where.
2. **Ground truth for disease.** Most labels will be `team_consensus` from the reviewed profiles. Ask a local extension office or master gardener group on Oct 16 whether someone can confirm 20 cases.
3. **Per-kind certainty values.** If organisms and soil calibrate very differently (section 5), the contract's single mapping becomes a per-kind one. Decide after the dev calibration.
4. **Measurement confidence independence.** Whether measurement confidences should keep fixed values when the VLM's are recalibrated (section 5).
