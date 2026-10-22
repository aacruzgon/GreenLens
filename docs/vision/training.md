# PDM Training, Export and Knowledge Ingestion

> **Status:** Draft v0.2.14, 2026-10-22. v0.2.14: section 10's tools take the signing review's fixes (#127): `plan.yaml` records a per-source tagging model, `validate.py` checks the withheld chunks against the seed and the page, `review.py` keeps each file's own citations and every section it doesn't write and reports an unknown chunk id, facts-only ones included, `--keep-flags` keeps a table in the reviewer's notes, and the extraction prompt's broad-spectrum rule matches `products.yaml`'s. v0.2.13: section 10 adds `plan.yaml`'s `withheld_chunks`, for chunks a reviewer keeps out of the seed, and two runs that bring the review files back in line with hand-edited YAML: `review.py`, which rebuilds the record sections with their citations, and `discriminate.py --keep-flags`, which refreshes the pair tables without touching the flags (signing review of #51). v0.2.12: section 10's scripts exist and drafted the tomato slice, and after its first review they tag with Sonnet 5.5 and let a chunk name several problems, keep sources without a reuse grant out of the committed text, cite chunks by plain id and quote whole excerpts in the review files, drop size indicators no shot can measure, refuse an unreviewed record and sign `reviewed_by` and `reviewed_at` together, review treatments in their problems' files; a new subsection says how to run them and what the two runs showed, and three model checks are planned. v0.2.11: section 3 adds three field sets for the disease head (PlantCity's tomato and healthy corn, and Pretoria's and Makerere's maize); section 4 reads big archives in part, groups unnamed copies by perceptual hash and a turn search, links shots of one plant through every copy, adds datasets without moving committed photos, and says Makerere's look-alike close-ups span its splits; section 6 chooses `C` by the top 3 on the dev cases, checks that no one dataset's val decides it, and ships v3c, retrained on the new sets: the true problem in its top 3 on 84% of the dev photos with a problem against v2c's 74%, while Tomato-Pakistan's test falls from 0.762 to 0.688 and corn's dev top-1 from 11 to 7 of 15. v0.2.10: section 4's step 6 names the leakage check, which M18 runs on the final manifest. v0.2.9: section 6 records the heads trained on our detector's leaf crops as the worker cuts them, with C chosen per crop (disease head v2 at 0.84 top-1 per crop on test with 21 classes, 0.88 with 19), the species head on iNaturalist photos (0.90 against zero-shot's 0.64 on the dev cases' supported crops) and the trained plant probe; section 3 lists the iNaturalist training photos. v0.2.8: section 5 adds the leaf detector's background images, COCO train2017 photos with no plant whose licence allows commercial use, and section 3 lists COCO train2017. v0.2.7: section 4 applies the names settled on #4 (genus-level `tomato_spider_mites`; TOM2024's alternaria and wilt classes dropped) by editing the committed splits in place, since a rerun of `dedupe.py` moves whole field visits. v0.2.6: section 3 lists COCO val2017 as evaluation-only negatives for the plant check. v0.2.5: section 1's layout shows what `ml/models/` holds since PR #6 (`FORMATS.md`, `templates/`) and marks README 6's files as planned. v0.2.4: section 1's layout lists `ml/models/`'s files from README 6 (`sources.toml`, `fetch.py`, `check.py`, the notices). v0.2.3: the schema link points to `backend/migrations/schema.surql` (merged from `main`). v0.2.2: data preparation groups augmented copies by their original photo, drops re-published copies of another source, groups the field sets by visit, and holds out a field test split, with one near-copy rule (0.97) for every split (review of PR #6). v0.2.1: the feature pre-processing resizes with Lanczos3, as the worker does (issue #2). v0.2: the heads and label matrices are written in the backend's JSON format (base64 `f32`), and an MLP head would need that format and the worker extended. v0.1, 2026-10-15: First training design: the Python environment, the datasets and their licences, data preparation with deduplication, the leaf detector fine-tune, the disease and species heads on BioCLIP 2 features, ONNX export and its checks, the label lists generated from the knowledge base, feedback as training data, and the model steps of knowledge ingestion.
> **Checked:** ✓ means read in the primary source on that date; ✓ ran means executed. Dataset licences, package versions and wheel indexes were read on 2026-10-15 (✓). PlantDoc's duplicates and the BioCLIP probes were run on a Mac, on CPU and its Apple GPU (✓ ran). No training has run on the rig.

Everything here lives in `ml/` ([architecture 13](../architecture/README.md#13-repository-layout)) and runs on dev machines or the rig, never in production. What it produces (ONNX files, label lists, temperatures, the knowledge seed) is consumed by the workers and `api` through the model manifest ([README 6](README.md#6-model-files-versions-and-pinning)).

## Contents

1. [Layout](#1-layout)
2. [Environment](#2-environment)
3. [Datasets and licences](#3-datasets-and-licences)
4. [Data preparation](#4-data-preparation)
5. [Leaf detector](#5-leaf-detector)
6. [Disease and species heads](#6-disease-and-species-heads)
7. [Export to ONNX](#7-export-to-onnx)
8. [Label lists](#8-label-lists)
9. [Feedback as training data](#9-feedback-as-training-data)
10. [Knowledge ingestion: the model steps](#10-knowledge-ingestion-the-model-steps)
11. [Schedule](#11-schedule)
12. [Open questions](#12-open-questions)

---

## 1. Layout

```
ml/
├─ pdm_vision/              shared Python package: pre-processing, label mapping, metrics
│                           (the same numbers the Rust worker uses, checked by a parity test)
├─ training/
│  ├─ data/                 download scripts and manifests; no images in git
│  │  ├─ sources.yaml       every dataset: URL, licence, version, SHA-256 of the archive
│  │  ├─ fetch.py
│  │  ├─ dedupe.py          section 4
│  │  └─ splits/            the deduplicated train / val / test lists, committed
│  ├─ detector/             section 5
│  ├─ heads/                section 6: feature extraction, disease and species heads
│  ├─ export/               section 7: ONNX export, fp16, checks
│  ├─ labels/               section 8: classifier_labels.yaml, the BioCLIP label matrix
│  └─ eval/                 the evaluation set and harness (evaluation 9)
├─ models/                 README 6, delivering model files
│  ├─ manifest.toml         every model file the system loads, as the Rust code parses it
│  ├─ FORMATS.md            the head and label-matrix JSON formats
│  ├─ templates/            the 80 prompt templates of the label matrix
│  ├─ sources.toml          planned: where each file is downloaded from, keyed by its SHA-256
│  ├─ fetch.py              planned: fills a host's /models and checks every hash
│  ├─ check.py              planned: checks the manifest, sources.toml and the files agree
│  ├─ NOTICES.md            planned: the licence notices shipped with the files
│  └─ licences/             planned
└─ knowledge/               ingestion (section 10) and the reviewed seed
```

Images and model files are never committed. Datasets are fetched by script with their checksums, and model files are published to the model store that `sources.toml` names ([README 6](README.md#delivering-model-files-to-the-backend)).

---

## 2. Environment

| Package | Pin | Note |
|---|---|---|
| Python | 3.12 | All wheels below exist for cp312 ✓ |
| `torch`, `torchvision` | 2.14.1, 0.29.1 from the **cu130** index ✓ | cu126 has no sm_120 ✓. The cu130 and cu132 wheels include sm_120 (the 5070 Ti) and sm_86 (the 3070) in PyTorch's support matrix for 2.12 and 2.13 ✓; the matrix has no 2.14 row, so this is *unverified* for 2.14.1 until the day-1 check. The cu128 and cu129 indexes stop at torch 2.11 and 2.13 ✓. Needs a driver of 580.65.06 or later ✓ |
| `open_clip_torch` | 3.3.0 ✓ | BioCLIP 2 loading and the text tower. MIT on PyPI; read its LICENSE file before relying on it, since its header reads oddly ✓ |
| `timm` | 1.0.30, Apache-2.0 ✓ | Only for the ConvNeXt-Tiny runner-up |
| `onnx`, `onnxscript` | 1.22.0, 0.7.2 | ONNX Runtime 1.31 builds against onnx 1.22.0 ✓; if the resolver refuses the pair, use onnx 1.23.2 and keep opset ≤ 23 |
| `onnxruntime-gpu` | 1.31.0 (CUDA 13) ✓ | The same runtime the worker binds, for every numerical check |
| `onnxconverter-common` | 1.16.0 ✓ | fp16 conversion |
| `tokenizers` | the version matching the Rust crate | Text embedder parity (section 10) |
| `scikit-learn` | latest | The heads' logistic regression and temperature fitting |

```bash
pip install torch==2.14.1 torchvision==0.29.1 --index-url https://download.pytorch.org/whl/cu130
```

The lock file is `ml/requirements.lock`, generated with hashes. vLLM pins its own torch (2.13.0 ✓) inside its image; the training environment never shares a Python with it.

**Where training runs.** The 5070 Ti trains, because it has the memory and is sm_120-ready, but it also serves the VLM. Training on it means stopping vLLM, so long runs happen on Oct 17–18, before the VLM pipeline is live, and later runs are scheduled when the rig isn't serving the demo. The heads (section 6) train on CPU in seconds once features are extracted, so they don't need the GPU at all. Whether the third card can take training is open ([README 4](README.md#4-placement-and-vram-budgets)).

---

## 3. Datasets and licences

The repository is public and the app is a hosted service, so a dataset is used for **shipped weights** only when its licence allows commercial use and adaptation. Download scripts and manifests are committed, never images.

| Dataset | Content | Images | Licence | Use |
|---|---|---|---|---|
| **PlantDoc** (classification) | 13 crops, 28 classes; in-the-wild photos scraped from the web ✓ | 2,342 train, 236 test ✓ | **CC BY 4.0** ✓ | Core for the disease and species heads |
| **PlantDoc** (detection) | The same photos with VOC boxes and CSVs: 2,345 train images with 8,469 boxes over 29 classes, 236 test images with 452 boxes ✓ ran | 2,581 ✓ | CC BY 4.0 for the annotations ✓; the scraped photos keep their owners' copyright *unverified* | Leaf detector |
| **TOM2024** | Tomato, onion, maize field photos, Burkina Faso, 30 classes ✓ | 12,227 labelled ✓ | **CC BY 4.0** ✓ | Tomato and corn field data |
| **Tomato Leaf Disease (Pakistan, 2026)** | Tomato: early blight, late blight, Septoria, leaf mould, yellow leaf curl, healthy; smartphone field photos ✓ | 7,200 ✓ | **CC BY 4.0** ✓ | Tomato field data |
| **iBean** | Bean: healthy, angular leaf spot, rust; smartphone field photos ✓ | 1,296 ✓ | **MIT** ✓ | Bean, if beans become supported |
| **PlantCity** ([Mendeley `w8kh2xkspx`](https://data.mendeley.com/datasets/w8kh2xkspx/4)) | Leaf close-ups in fields in Charsadda (tomato) and Chitral (maize), Pakistan, phone and DSLR ✓. Each photo ships with flipped, quarter-turned, mirrored and turned copies under unrelated names, and test.zip repeats train.zip's photos ✓ ran | Our 10 classes: 14,321 files; outside gray leaf spot, 1,121 photos once copies are grouped and 1,043 after deduplication ✓ ran | **CC BY 4.0** ✓ 2026-10-20 (the Data in Brief article is CC BY-NC; the data isn't) | Tomato field data: bacterial spot, early and late blight, leaf mould, Septoria, spider mites, healthy, and "leaf curl" as yellow leaf curl virus (not lab-confirmed); corn healthy. Its maize gray leaf spot is dropped: the photos show pale flecks, not the lesions |
| **Diseases of maize in the field** ([Figshare `20237613`](https://doi.org/10.25403/UPresearchdata.20237613.v1), University of Pretoria) | Handheld maize field photos, South Africa, 2009–2020, labelled per image in a CSV ✓ | The 1,270 with one label: gray leaf spot 713, northern leaf blight 170, common rust 119, healthy 268 ✓ ran; 1,165 after deduplication | **CC BY 4.0** ✓ 2026-10-20 | Corn field data |
| **Makerere University Maize Image Dataset** ([Dataverse `DVN/LPGHKK`](https://doi.org/10.7910/DVN/LPGHKK)) | Phone photos in Ugandan maize fields, May 2021: northern leaf blight ("Maize Leaf Blight", by its datasheet) 5,279, healthy 5,326 ✓ | 600 per class, a fixed sample by hash; 1,194 after deduplication ✓ ran | **CC0 1.0** ✓ 2026-10-20 (its datasheet says CC BY) | Corn field data, capped so it doesn't outweigh the other corn sources |
| **DiaMOS Plant** | Pear leaf diseases ✓ | 3,505 ✓ | CC BY 4.0 ✓ | Only if pear is supported |
| **Plant Pathology 2020 / 2021** | Apple field photos: scab, rust, frog-eye, powdery mildew ✓ | 3,651 / about 23,000 ✓ | Kaggle competition terms, **not confirmed** *unverified* | Only after a teammate reads the terms |
| **PlantVillage** | 14 crops, 38 classes, one leaf on a plain background ✓ | 54,306 ✓ | CC BY-SA 3.0 ✓ | Minority source at a low sampling weight, for rare classes. Lab photos generalise poorly to the field ✓ ([Mohanty 2016](https://pmc.ncbi.nlm.nih.gov/articles/PMC5032846/): 31% on field images) |
| **iNaturalist** (training) | The five supported crops as whole plants, flowers and fruit, and five plant families (cacti, stonecrops, aroids, mints, gourds); observed 2019–2025 with an agreeing identification, one photo per observation; the wild chiltepin left out of pepper ✓ ran | 2,515: 1,115 of the crops, 1,400 of the families | Per photo; **only CC0 and CC BY** ✓ (the default is CC BY-NC), each photo's credit in `splits/inat_species.tsv` | Species head and the trained plant probe. `ml/training/data/inat_species.py`; the evaluation's iNaturalist cases are all from 2026 and skipped by ID, so the two can't share a photo |
| iNaturalist / GBIF photos (other) | Healthy foliage of each crop, pests, beneficials | as needed | Per photo; **only CC0 and CC BY** ✓ | Gap filling, with credit recorded per image |
| **PlantWild** | In-the-wild disease photos, 89 classes ✓ | 18,542 ✓ | **CC BY-NC-ND 4.0** ✓ | **Not in shipped weights** |
| **PlantSeg** | In-the-wild disease photos with lesion masks, 115 diseases ✓ | 11,458 ✓ | **Conflicting:** Zenodo record 13958858 says CC BY-NC-ND 4.0 ✓, record 14935094 says CC BY 4.0 ✓, and the GitHub repository has none ✓; the images are scraped from the web | Treated as NC-ND until a teammate settles it. Its lesion masks are the best check of the `SpotSegmenter` ([evaluation 6](evaluation.md#6-thresholds-set-from-the-evaluation-set)) as a private evaluation, never training |
| **PhenoBench** | Top-down sugar-beet field images with complete leaf instance masks ✓ | 2,179 ✓ | CC BY-SA 4.0 ✓ | Complete leaf labels, but top-down field views; a possible extra for the detector, since ShareAlike binds redistributed data and its reach to weights is unsettled |
| **COCO train2017** | Everyday scenes: 400 indoor and people/pets photos with no plant as the leaf detector's background images; 1,000 photos without a potted plant or broccoli as the plant check's hold-out negatives ✓ ran | 400 + 1,000 | Annotations **CC BY 4.0** ✓; each image keeps its Flickr licence, most of them non-commercial ✓ ran | **Background images**: only CC BY, CC BY-SA or no known copyright ([section 5](#5-leaf-detector)), never redistributed. **Hold-out**: evaluation only. `ml/training/detector/background.py`, `ml/training/data/negatives_holdout.py` |
| **COCO val2017** | Photos of everyday scenes, used as negatives for the plant check: those without a potted plant or broccoli (4,758 of 5,000), grouped indoor, people and pets, outdoor ✓ ran | 4,758 | Annotations **CC BY 4.0** ✓; the images keep their Flickr licences | **Evaluation only**: never redistributed, never trained on. `ml/training/data/negatives.py`, `sources.yaml` (`coco_val2017`, `coco_annotations_2017`) |
| Agri-Foundation-145k | Aggregate of the above | 144,751 ✓ | Labelled CC BY 4.0, but repackages NC-ND and SA data ✓ | **Excluded**: its label can't be trusted |

**PlantDoc's quality.** Its photos were scraped from the web, and its splits leak: 13 of its 236 test images have a training image at cosine similarity 0.98 or more in BioCLIP's embedding space, and only 4 of those 13 pairs carry the same label ✓ ran. Section 4 removes the duplicates and the conflicts before anything is trained or reported.

**Coverage decides the crops** (architecture open decision 3). PlantDoc and the field sets give real disease classes for tomato, potato, pepper, apple, grape, corn and strawberry. Squash, raspberry, blueberry, cherry, peach and soybean have only healthy images or one disease ✓. The classifier supports what the data covers; the VLM's indicators carry the rest, and the knowledge base can still hold a crop's profiles without classifier support (`classifier_supported` is per species).

Every dataset's licence and citation goes into `ml/training/data/sources.yaml` and the notices ([README 9](README.md#9-licences-and-notices)).

---

## 4. Data preparation

1. **Fetch** each dataset by script, checking the archive's SHA-256. From a big archive, `fetch.py` reads only the classes we use over HTTP range requests (PlantCity, Pretoria), pinned by the hash of the files' listing.
2. **Deduplicate across all sources and splits.** Embed every image with BioCLIP 2 (section 6). Pairs at cosine 0.98 or more are duplicates; check a sample by eye and adjust the threshold. Files that are augmented copies of one photo are duplicates too, whatever their cosine: a set published already augmented (Tomato-Pakistan's `_aug_N`, rotations, mirrors and Roboflow exports) names its copies, and its mapping file says how to strip the suffixes back to the original's name. Rotated copies fall below 0.98, so the names are the usual link; when they don't link copies (PlantCity), `copies.py` groups them by perceptual hash within 4 bits, flipped, quarter-turned, or turned by any angle with black corners (turned back by the angle its corners give). At 6 bits, different corn leaves start to merge ✓ ran. The corner gives no usable angle on some files, and two copies turned different ways were never compared, which left 86 of the first split's PlantCity groups as copies of another group, 35 of them in a different split (a review of the first split, Oct 20). So a file with black corners is also turned in 1° steps, and its central 40% at each angle is compared with the central 40% of every file of its 3 nearest groups by BioCLIP 2, within 8 bits. On a sheet of 22 matches, every one at 6 bits or less was the same photo; at 8 bits the tomato matches were, while some plain healthy corn leaves were different leaves, which costs a photo each; at 10 bits every pair was a different photo ✓ ran. A set that re-publishes another source's photos (Tomato-Pakistan holds PlantDoc's web photos under their PlantDoc names) has those files dropped. In each group keep one image: the one in the test split if any (so the test set stays as published, minus conflicts), then the validation split, then the copy closest to the original, otherwise the largest. A group with **conflicting labels** is dropped entirely unless a teammate relabels it. `ml/training/data/dedupe.py` writes a report of every count.
3. **Map labels** from each dataset's class names to `(species_id, problem_id | healthy)` with one mapping file per dataset, reviewed like the knowledge base. A class that has no knowledge-base problem is dropped, not guessed. A class renamed or dropped after the splits are written is renamed or removed in the committed split files in place: a rerun of `dedupe.py` refills every class's quotas and moves whole field visits between splits (205 TOM2024 photos when [#4](https://github.com/aacruzgon/GreenLens/issues/4) dropped two classes, ✓ ran 2026-10-19), which would put photos a shipped head trained on into test. New datasets are added with `dedupe.py --keep-splits`: every committed photo keeps its split, and a new photo joins a group's committed photos or fills its own set's quotas (without it, adding the three garden sets moved 238 TOM2024 photos, ✓ ran 2026-10-20). The report counts the photos new to a dataset already in the splits: with the garden sets, one TOM2024 healthy leaf, whose copy in `alternaria_d` made its group a label conflict until #4 dropped that class in place, joins train ✓ ran 2026-10-21.
4. **Split by source and by plant where known**, so a model is never validated or tested on a near-copy of its training data. "Same plant" groups join near-copies at cosine 0.97 or more across all datasets (within a dataset, between any of two photos' copies, since two shots of one PlantCity leaf may match only in orientations dedupe didn't keep; across datasets, between kept images only, since a photo's 13 or so copies would chain other sets' healthy leaves into its group: 187 healthy tomato photos from four sets, in a trial ✓ ran), and in a set whose file names carry the capture time (TOM2024), every photo of one field visit (one UTC day); there, the visit replaces the cosine links, which would chain its look-alike 640 × 480 photos across visits. Where a few days hold most of a set (Makerere, and Pretoria by its EXIF times), a session is one camera's photos at most 60 s apart instead. Makerere shot about every 7 s without a break, so on its 11% sample those gaps are arbitrary cuts rather than plants or rows, and its test share comes from the same fields in the same week: it measures accuracy on those fields, not on a new one. There 154 pairs at cosine 0.97 or more span its splits (26 in Pretoria). They are look-alikes, not copies: Makerere's photos are close-ups of one leaf filling the frame, 200 of its 289 such pairs join different days, and on a sheet of 8 pairs across splits every one was two different leaves. Linking them doesn't work either: with the 60 s sessions they chain 467 of its 1,194 photos into one group, and with whole days all of them ✓ ran 2026-10-21. So Makerere's val and test are not held out the way the other sets' are; section 6 checks that its val doesn't decide `C`. Published test splits (PlantDoc, iBean) stay test, and a group's other images are dropped as near-copies of a test image. The rest fill each class's quotas in a stable hash order: 15% validation, and for the field sets, which publish no test split, 15% test, so each has a held-out test of field visits or original photos the heads never saw. The detector's train/valid split follows the same 0.97 rule.
5. **Crops for the heads come from our own detector** (section 5), not from the datasets' boxes, so training sees the same crops the worker will. Until the detector is trained (Oct 18), PlantDoc's boxes stand in.
6. **Never** use an evaluation-set photo. `ml/training/eval/check_leakage.py` checks the evaluation photos against every training image by perceptual hash and BioCLIP cosine, and M18 runs it on the final manifest before the test split is scored ([evaluation 2](evaluation.md#where-the-photos-come-from)).

---

## 5. Leaf detector

RF-DETR-S, one class `leaf` ([models 3](models.md#3-leaf-detector)). RF-DETR's trainer is Apache-2.0 and installs with `pip install rfdetr==1.11.2` ✓; it reads COCO JSON or YOLO folders ✓.

### Data

1. **PlantDoc's detection boxes**, every class collapsed to `leaf`: disease is a separate stage, and one class doubles the examples per class.
2. **Clean them** the way Roboflow's fixed copy did ✓: clip the 4 out-of-bounds boxes, drop the 4 zero-sized ones, and drop the slides and collages (found by eye in the duplicate review of section 4). The 16 file names in both splits ✓ ran are resolved by the deduplication.
3. **Fill in the missing leaves.** PlantDoc boxes mostly the leaves that mattered to its annotators ✓ ran, so after the first training run (Oct 17), the detector pseudo-labels the training images at a low threshold, and a teammate accepts, fixes or adds boxes on 300 images in CVAT or Label Studio (about 3 hours). A later run trains on the corrected set; the second run (Oct 19) went ahead without it. This is the single change most likely to raise recall.
4. **Add garden photos**: 150 or more of the team's own photos that are not in the evaluation set, labelled the same way, so the detector sees phone close-ups of local plants.
5. **Add photos with nothing to find** ✓ ran: 400 COCO train2017 photos, indoor scenes and people or pets, with no potted plant or broccoli, no pixel of COCO-Stuff's `plant` supercategory (tree, bush, grass, leaves, flower, moss, branch, straw, plant-other) and a Flickr licence that allows commercial use and adaptation (CC BY, CC BY-SA, no known copyright restrictions), go in the training split with no box (`ml/training/detector/background.py`, pinned by SHA-256 with each licence in `splits/background_coco_train.tsv`). The licence follows this section's rule for shipped weights: an earlier pick ignored it, and 308 of its 400 photos were non-commercial or no-derivatives. Trained only on PlantDoc, the detector fired on half the photos without a plant ([models 2](models.md#2-plant-present-check)). Foliage is left out because labelling it "no leaf" would teach the detector to ignore the hedges and shrubs users photograph. train2017 keeps the val2017 photos that score the plant check unseen, and the 1,000 hold-out photos the thresholds are checked on (`splits/negatives_coco_train_holdout.tsv`) are disjoint from these.
6. Convert to COCO JSON (`train/_annotations.coco.json`, `valid/…`, `test/…`) ✓; about 20 lines from PlantDoc's CSVs.

### Training

```python
from rfdetr import RFDETRSmall

model = RFDETRSmall()                  # Apache-2.0 COCO weights
model.train(
    dataset_dir="data/leaf-coco",
    epochs=50,
    batch_size="auto",                 # probes the largest batch that fits ✓
    grad_accum_steps=4,
    lr=1e-4,
    resolution=512,
    output_dir="runs/leaf-rfdetr-s",
)
```

`batch_size="auto"` and `grad_accum_steps` are from RF-DETR's training documentation ✓; the other argument names are *unverified* until the first run. On the 5070 Ti in mixed precision, 50 epochs of about 2,500 images should take 1–2 hours *estimate*.

### Acceptance

- Class-agnostic mAP50 on the deduplicated PlantDoc test split, as the floor for later runs.
- **Recall of every visible leaf** at IoU 0.5 on the evaluation set's `leaf_closeup` shots, target 85% ([evaluation 4](evaluation.md#4-metrics-and-acceptance-targets)). PlantDoc's test boxes are incomplete, so its mAP can't measure recall.
- The export check of section 7.

If RF-DETR-S misses the targets, D-FINE-S (Apache-2.0) is trained on the same COCO files with its own trainer.

---

## 6. Disease and species heads

Both heads sit on frozen BioCLIP 2 image features ([models 6–7](models.md#7-disease-classifier)). They are small, train in seconds on CPU, and are shipped as a JSON file holding the weight matrix and bias as base64 little-endian `f32`, which the worker applies in Rust ([backend vision 5](../backend/vision.md#5-the-model-manifest); the format is the backend's). The worker runs it on the embeddings it already has.

### Features

- Run BioCLIP 2's vision tower once over every training image and every leaf crop, with the **exact** pre-processing the worker uses (shortest side to 224 with Lanczos3, centre crop, CLIP normalisation; [models 6](models.md#6-species-bioclip)) from `pdm_vision`, and cache the L2-normalised 768-dimensional vectors by image hash.
- About 60–70 seconds for 2,342 images on a Mac (Apple GPU through PyTorch's MPS backend) with BioCLIP v1 ✓ ran; BioCLIP 2 is roughly 3–4 times the compute *estimate*, a few minutes on the 5070 Ti.
- **Leaf crops as the worker cuts them** (`ml/training/heads/crops.py`, ✓ ran). Each photo is stored with its long side at 1,600 px, the shipped detector runs on it, and its output is decoded as `detector::decode` does (score, minimum side, clamp, sort, NMS, cap; a box clamped to zero width keeps its slot). Each box grows by `crop_grow` (0.10) of its size on every side and is cut with `PixelRect::from_normalised`'s rounding (left and top floored, right and bottom ceiled). A photo where the detector keeps no leaf is embedded whole, as the worker does for a close-up. The cache key covers every setting that changes a crop, so a changed threshold can't reuse old crops. About 3 minutes for the 6,544 split photos on the 5070 Ti, and 5 more for 4 augmented views of each training crop.

### Disease head

- **Classes:** the rows of `classifier_labels.yaml` (section 8): every `(species, problem | healthy)` pair with at least 30 deduplicated training crops. A pair below that is left out and its problem left to the VLM.
- **Model:** multinomial logistic regression (scikit-learn, `C` chosen on validation by restricted top-1 **per crop**, on the val photos with exactly one crop, whose label is the crop's own: the worker returns each crop's top 3 and never averages a photo's crops; for the shipped v3c, by the top 3 on the dev cases, below) first; a 2-layer MLP (768 → 512 → classes, dropout 0.2) if it beats it by more than 2 points top-1 on validation. The worker's head is one linear layer today, so an MLP also needs the file format and the worker extended to two layers.
- **Class balance:** weight classes by inverse frequency, capped at 10 times the median weight.
- **Augmentation:** for the logistic head, extract features of 4 augmented views per training crop (random resized crop at 0.7–1.0 scale, horizontal flip, colour jitter of 0.2, mild blur) and train on all of them. Cheap, and it makes the head robust to framing.
- **Restricted softmax.** At serving time the worker restricts the logits to the working species before the softmax ([contracts 5](../architecture/contracts.md#results)). Validation reports both the full-label and the restricted accuracy; the restricted number is the one that matters (0.801 against 0.742 on BioCLIP v1 features ✓ ran).
- **Temperature:** one scalar `T_disease`, fitted by minimising the restricted softmax's negative log-likelihood on the evaluation set's dev cases, their leaf photos with one crop ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)). Those are the photos most like the app's: fitted on validation, the head claimed a problem at medium confidence on 61 of 94 dev photos and was right on 44%. `--temperature-on val` keeps the validation fit, and the report gives both. In the BioCLIP v1 probe, temperature scaling cut the expected calibration error from 0.150 to 0.093 with T = 0.71 ✓ ran.
- **Report:** top-1 and top-3 per crop on the deduplicated PlantDoc test split and on the field sets' held-out source, the confusion matrix per crop, and the calibration error, in `ml/training/heads/reports/`.
- **The dev cases judge the head by its job,** its top 3, which the API adds to the candidates ([architecture 6.3](../architecture/README.md#63-retrieve-then-verify)). Every `C` in the grid is also scored on the dev cases' leaf photos with a problem: is the true problem among the 3 classes the worker sends, as the API keeps them? The report gives that top 3 against guessing, per crop and per source, and the medium-confidence claims at both temperatures, as `ml.training.eval.calibrate` scores them. `--select-on dev-top3` keeps every `C` within one standard error of the best dev top 3 (about 3 of 83 photos) and, of those, takes the best val top-1 per crop. Taking the best dev top 3 alone chased a one-photo lead to C = 0.1, the grid's edge, for 7 points of val. A new head is compared with the last on that dev top 3 ([evaluation 5](evaluation.md#results-on-the-dev-cases-2026-10-20--ran-offline-part)).

**On our detector's crops (2026-10-20, ✓ ran).** Four heads on `rfdetr-s-leaf-2026-10-19-bglic`'s crops for the five supported crops, restricted to the true species. "Per crop" scores each crop of a photo with exactly one crop, which is what the worker returns; "per photo" averages a photo's crops, which the worker doesn't do; "all crops" scores every crop against its photo's label, a floor, since a healthy leaf in a diseased photo carries the photo's label:

| Head | Classes | Views | C | Per crop, val / test | Per photo, val / test | All crops, val / test | One-crop ECE after T, val / test |
|---|---|---|---|---|---|---|---|
| [v2](../../ml/training/heads/reports/disease-head-v2-2026-10-20.json) | 21 | 0 | 50 | 0.822 / 0.842 | 0.841 / 0.850 | 0.755 / 0.786 | 0.039 / 0.030 |
| [v2a](../../ml/training/heads/reports/disease-head-v2a-2026-10-20.json) | 21 | 4 | 500 | 0.834 / 0.851 | 0.849 / 0.855 | 0.756 / 0.805 | 0.057 / 0.059 |
| [v2b](../../ml/training/heads/reports/disease-head-v2b-2026-10-20.json) | 19 | 0 | 50 | 0.846 / 0.868 | 0.860 / 0.869 | 0.788 / 0.816 | 0.049 / 0.041 |
| [v2c](../../ml/training/heads/reports/disease-head-v2c-2026-10-20.json) | 19 | 4 | 100 | 0.846 / 0.881 | 0.861 / 0.877 | 0.792 / 0.831 | 0.037 / 0.037 |

- **Above the 75% bar** (decision D8), so the head stays and ConvNeXt-Tiny isn't needed.
- **The 19-class heads** drop `tomato_mosaic_virus` and `corn_curvularia_leaf_spot`, which have no profile in the knowledge seed. Their numbers leave those classes' photos out, so part of the gain is only in what is scored. Corn's per-photo test top-1 goes from 0.873 to 0.935. The class list waits on Sara ([#85](https://github.com/aacruzgon/GreenLens/pull/85)); v3c (below) ships with the 19.
- **Augmentation** adds about a point with 21 classes but worsens calibration. With 19 it adds 1.3 points on test, ties on val and is better calibrated (ECE 0.037 against v2b's 0.049 / 0.041).
- **T** is fitted per crop on val's one-crop photos (0.81 for v2, 1.15 for v2c). Per photo, it can make calibration worse, since the worker never averages a photo's crops.
- **Weak classes in v2 (val recall):**
  - tomato bacterial spot 0.40 (n = 15);
  - tomato early blight 0.70 (n = 20);
  - corn gray leaf spot 0.56 (n = 9);
  - tomato mosaic virus 0.50 (n = 6).

**With garden and field photos: v3c, shipped (2026-10-20, ✓ ran).** v3c adds PlantCity's tomato and healthy corn (39 photos), and Pretoria's and Makerere's maize (section 3): 5,331 training photos in all, with v2c's settings (19 classes, 4 views per crop) and `C` chosen by `--select-on dev-top3`. Down the grid, the dev top 3 is 84–87% for C from 0.01 to 5, 80% at C = 20 and 71–75% above, while val top-1 per crop climbs from 0.748 at C = 0.01 to 0.860 at C = 5 and stays near it: so C = 5. With any one dataset's val left out in turn, the rule still picks C = 5 (`C_with_dataset_left_out_of_val` in the report), so Makerere's look-alike val (section 4), at 0.983 per crop, doesn't decide it. Val and test hold the three sets' own shares too:

| Head | Classes | Views | C | Per crop, val / test | Per photo, val / test | All crops, val / test | One-crop test ECE, before / after T |
|---|---|---|---|---|---|---|---|
| [v3c](../../ml/training/heads/reports/disease-head-v3c-2026-10-20.json) | 19 | 4 | 5 | 0.860 / 0.850 | 0.869 / 0.847 | 0.810 / 0.814 | 0.079 / 0.235 |

v3c's test numbers were read once, after it was chosen on val and the dev cases. The old sets' test photos had been read before, by v3 and v3b on the first split (below), so on Oct 20 PlantDoc's, TOM2024's and Tomato-Pakistan's test was seen three times. The dev top 3 is in-sample: dev chooses `C`, fits T and reports the 84%; C = 5 isn't dev's best (87% at C = 0.01), which keeps that bias small. On the dev cases' leaf photos with one crop ([report](../../ml/training/eval/reports/calibration-2026-10-20-garden.md)): the top 3 on the 83 with a problem, each species' guessing in brackets, and top-1 per photo on each test split. The old split's PlantDoc, TOM2024 and Pakistan photos are unchanged, so v2c is scored on the same photos; it never saw the new sets ([scores](../../ml/training/heads/reports/disease-head-v2c-2026-10-20-on-garden-test.json), from `ml/training/heads/score_test.py`, which gives v2c's own report numbers on the old sets):

| Head | Dev top 3 (57%) | Tomato, 52 (38%) | iNaturalist tomato, 12 (38%) | Corn, 12 (75%) | Dev top-1, 94 | Problem at medium or more at the dev T (right) | Old test: PlantDoc / TOM2024 / Pakistan | New sets' test: PlantCity / Pretoria / Makerere |
|---|---|---|---|---|---|---|---|---|
| v2c | 74% (61) | 30 | 3 | 12 | 40% | 18 (13), T = 3.66 | 0.801 / 0.946 / 0.762 | 0.386 / 0.382 / 0.868 |
| **v3c** | **84% (70)** | **40** | **8** | 11 | 41% | 24 (17), T = 1.82 | 0.808 / 0.963 / **0.688** | 0.797 / 0.713 / 0.934 |

- **v3c ships** in the manifest (`disease_head_disease-head-v3c-2026-10-20`, pdm-models 8473aad), replacing v1d. Its gain is tomato's, 10 more of the 52 leaves with a problem: `tomato-bd-2025` 13 to 17 of 20, `tomato-bd-iphone-2025` 9 to 10 of 15, and the **iNaturalist garden leaves, the photos most like the app's, 3 to 8 of 12**, where v2c was below guessing.
- **Its claims.** At its dev-fitted temperature (1.82) it claims a problem at medium confidence on 24 photos, 17 of them right; without grape, which is claimed and right at every value, 16 and 9 (v2c: 10 and 5). Apple takes 11 (8 right), tomato 4 (1), corn 1 (0). No healthy leaf is blocked, and `healthy` evidence stays at 2 of the 11 healthy leaves, both corn. Its per-species fits agree (tomato 1.91, corn 1.79, apple 1.80; grape runs to the bound), so a temperature per species would change one claim: M17's trade is small for this head.
- **Corn's top-1 got worse on the dev cases.** Its top 3 as the API keeps it loses 1 of 12, but on the 15 `maize-bd-2026` leaves top-1 falls from 11 to 7: northern leaf blight from 5 of 5 to 1 (3 called healthy, 1 gray leaf spot) and common rust from 3 of 5 to 1 (3 called gray leaf spot), while gray leaf spot (2) and healthy (3) are all right. One leaf with a problem is called healthy, and at the dev T corn's claims go from 2 (both right) to 1 (wrong). Makerere is now 439 of the 801 northern leaf blight training photos, and 30 of its blight photos drawn at random all show the long tan lesions, so its labels hold; Pretoria is 470 of the 520 gray leaf spot photos. Fifteen leaves are too few to tune on; M17 and the test split read corn again.
- **The one loss on test is Tomato-Pakistan:** 0.688 against v2c's 0.762, 6 of its 80 photos; on val it loses 1 of 81 (0.704 to 0.691). It comes with the low `C` that helps the garden photos (C = 20 kept 0.738 on the first split). Accepted (Sam, Oct 20): the app's photos are garden photos.
- **PlantCity's test is still a little kind:** shots of one leaf at cosine 0.965–0.97 can sit in different splits (section 4). It is 0.797 here, against 0.846 on the first split, before the review found the copies. Makerere's test comes from the same fields and week as its training photos, with 154 look-alike pairs across its splits (section 4), so its 0.934 isn't held out the way the other sets' tests are.
- **The 19 classes** follow the knowledge seed, which has no profile for tomato mosaic virus or corn curvularia leaf spot. If Sara's review keeps either, a 21-class v3c is one retrain. Against v1d, the manifest's head before, it goes from 34 classes on 14 crops to 19 on the five supported ones (apple, corn, grape, pepper, tomato): no potato, bean or squash powdery mildew, no healthy-only crops, and spider mites as the genus-level `tomato_spider_mites`. The seed's validator needs `classifier_supported` true for exactly those five species.
- **The first split's heads, v3 and v3b** (C = 20 and 50, v3b with every source weighted the same within a class), are dropped: a review found 34 of PlantCity's 169 test photos were copies or other shots of training photos. On that split, balancing sources didn't help the dev top 3 and cost 2.4 points on TOM2024. Their test numbers were read too ([v3](../../ml/training/heads/reports/disease-head-v3-2026-10-20.json), [v3b](../../ml/training/heads/reports/disease-head-v3b-2026-10-20.json)): v3 had PlantDoc 0.836, TOM2024 0.946, Tomato-Pakistan 0.738, and on the first split's new-set photos PlantCity 0.846, Pretoria 0.737 and Makerere 0.951.

### Species head

- **Classes:** the species with `classifier_supported: true`.
- **Data:** whole-plant photos are what matters most for species, and the disease datasets are mostly leaves. So the head also trains on CC0 and CC BY iNaturalist photos of each supported crop: whole plants, flowers and fruit, 164 to 280 per species (section 3). Every species weighs the same in total, and within it each source does.
- **Model and temperature:** as the disease head, on whole-photo embeddings (the worker's pass 1); C is chosen on val by the mean of the two sources' top-1. It is used only if it improves the dev split's species top-1 ([models 6](models.md#a-trained-species-head-for-the-supported-crops)).
- **Measured (2026-10-20, ✓ ran,** [report](../../ml/training/heads/reports/species-head-v1-2026-10-20.json)). Top-1, where zero-shot is over the 20-species stand-in label matrix (M12 replaces it), and "combined" is models 6's rule:

  | Photos | n | Zero-shot | Head | Combined |
  |---|---|---|---|---|
  | Val, iNaturalist | 209 | 0.986 | 1.000 | 0.990 |
  | Val, leaf datasets | 628 | 0.844 | 0.994 | 0.854 |
  | Test, leaf datasets | 565 | 0.773 | 0.982 | 0.779 |
  | Evaluation dev cases, supported crops | 134 | 0.642 | 0.896 | 0.664 |

  - **The head beats zero-shot clearly** on every set, by 25 points on the dev cases. The dev row was measured before two microscope photos of a mite were dropped from the evaluation set (one of them dev), so a rerun moves it by under a point.
  - **The rule as written keeps little of that.** It keeps zero-shot's share for the unsupported species, so a photo that zero-shot gives mostly to another species stays wrong whatever the head says. Models 6 records the change to decide.

### Trained plant probe

The plant check's second signal ([models 2](models.md#2-plant-present-check)): logistic regression on whole-photo BioCLIP 2 embeddings, in the same `pdm-linear-head/1` form as the zero-shot probe (`ml/training/heads/train_plant_probe.py`).
- **Plant photos:** every train-split photo and every iNaturalist training photo (section 3: crops, cacti, succulents, aroids, mints, gourds).
- **Photos without a plant:** half of COCO val2017's 4,758 photos without a potted plant or broccoli, chosen by a hash of the path. The other half, written beside the probe as `negatives_tune.tsv`, chooses the thresholds. `plant_check --probe` defaults to it and refuses a list holding any photo the probe trained on.
- **Weighting:** the two sides weigh the same in total.
- **C** is chosen on val: the fewest tune-half photos passing at the `τ_plant` where the probe alone fails 1% of val's plant photos, leaf datasets and iNaturalist together. C = 20 ([report](../../ml/training/heads/reports/plant-probe-v1-2026-10-20.json)).
- **Results** are in models 2: with `τ_leaf` 0.99 and `τ_plant` 0.71 it passes 0.1% of the hold-out's no-plant photos, against 11.0% for the shipped zero-shot pair.

### Runner-up: ConvNeXt-Tiny

If the disease head plateaus below 75% top-1 on the deduplicated field validation set, fine-tune `timm/convnext_tiny.fb_in22k_ft_in1k` (Apache-2.0 ✓) at 288 pixels on the same crops: AdamW, learning rate 2e-4 with cosine decay, 30 epochs, label smoothing 0.1, the augmentations above plus TrivialAugment, mixed precision on the 5070 Ti. A few hours *estimate*. It then becomes its own ONNX session on the 3070 ([README 4](README.md#4-placement-and-vram-budgets)).

---

## 7. Export to ONNX

### Rules

| Rule | Value |
|---|---|
| Exporter | The one each model's export was verified with: RF-DETR's `model.export(format="onnx", dynamic_batch=True)` ✓ ran, and `torch.onnx.export(..., dynamo=False)` with `dynamic_axes` for SAM 2.1, MobileSAM, UperNet and BioCLIP v1 ✓ ran. BioCLIP 2 uses the same open_clip code path as BioCLIP v1 and the heads are a single `Gemm`, but neither has been exported yet *unverified* until the day-1 export. The TorchScript exporter is deprecated but still in PyTorch 2.14.1 ✓; the dynamo exporter (`dynamo=True`, the default since 2.9 ✓) is tried first for any new model, and its large weights are written to a separate `.onnx.data` file ✓ ran |
| Opset | **17**, set explicitly so a PyTorch upgrade can't change it. Every model exported so far was exported and checked at 17 (RF-DETR with its own exporter; SAM 2.1, MobileSAM, UperNet and BioCLIP v1 with the TorchScript exporter) ✓ ran, and it is the first opset with `LayerNormalization`, which the CUDA execution provider implements from 17 ✓. A new model may use up to 20 (the dynamo exporter's default ✓) when its export needs it, recorded in its manifest entry. Never 24 or later: PyTorch's exporter stops at 23 ✓ and ONNX Runtime 1.31 reads up to 26 ✓ |
| Batch | A dynamic first axis on every input and output (`dynamic_axes={"images": {0: "batch"}, …}`, or `Dim("batch", min=1, max=64)` with the dynamo exporter). Check that nothing bakes the trace's batch size into the graph: MobileSAM's TinyViT does (`B = len(x)` in `PatchMerging`) and needs `x.reshape(-1, H, W, C)` before export ✓ ran. SAM 2's Hiera keeps H and W fixed at 1024, which is correct ✓ |
| Graph | The raw network only: no NMS, no box decoding, no resize. L2 normalisation of embeddings is inside the graph |
| Weights | A single file under 2 GB. The dynamo exporter writes large weights to a separate `.onnx.data` file ✓ ran; re-save as one file so the manifest's hash covers everything |
| Names | Inputs and outputs named in the manifest (`images`, `embeddings`, `logits`, …), never positional |

### fp16

Convert with `onnxconverter_common.float16.convert_float_to_float16(model, keep_io_types=True)` ✓ and keep the fp16 file only if it passes the checks below; otherwise ship fp32. Always keep `GridSample` in fp32 through `op_block_list`: the CUDA execution provider implements it for float only ✓, and RF-DETR's deformable attention has 3 of them ✓ ran. Keep `LayerNormalization`, `Softmax` and `ReduceMean` in fp32 too if a ViT's embeddings drift. Every op in the exported RF-DETR, SAM 2.1, MobileSAM and UperNet graphs has a CUDA kernel in ONNX Runtime 1.31 ✓ ran (checked against its kernel table); BioCLIP 2's graph gets the same check after its export. The fp16 speed-up on the 3070's tensor cores is expected but unmeasured *estimate*; ORT's documentation names tensor-core GPUs as the ones that gain most ✓ without quantifying it. For ViT models (BioCLIP 2), run `onnxruntime.transformers.optimizer` with `model_type="vit"` ✓ at export and keep its output only if the checks pass and it is faster.

### Checks before a file enters the manifest

`ml/training/export/check.py` runs every exported file under ONNX Runtime 1.31 with the CUDA execution provider on the 3070, against the PyTorch model on 50 real photos:

| Model | Pass condition |
|---|---|
| Embeddings (BioCLIP 2) | Cosine similarity 0.999 or more for every image; ran at batch 1, 3 and the maximum, with identical results per image across batch sizes (BioCLIP v1's export matched PyTorch within 1.2e-6 at batch 1 and 5 ✓ ran) |
| Heads | Same top-1 on every image, probabilities within 1e-4 |
| Detector | mAP50 on the PlantDoc test split within 0.5 points of PyTorch |
| Mask model | Mean IoU of masks against PyTorch's 0.98 or more |

Then it writes the manifest entry: file, SHA-256, opset, precision, input and output names and shapes, pre-processing values, and the check's numbers.

---

## 8. Label lists

Two lists tie the models to the knowledge base. Both go through the knowledge loader, which validates them and generates their versions ([contracts 10](../architecture/contracts.md#checks-and-generated-files)); neither is edited by hand.

### `classifier_labels.yaml`

- **Written by training.** `ml/training/labels/` writes it from the disease head's class list into the seed directory, `ml/knowledge/seed/classifier_labels.yaml` ([contracts 10](../architecture/contracts.md#files)), in the same pull request as the head: `model_version`, then for each output `index`, the training `label`, `species_id` and `problem_id` or `healthy`.
- **Loaded by the loader** like every seed file: every `species_id` and `problem_id` must exist, and each species' `classifier_supported` must agree with whether it has rows ([contracts 10](../architecture/contracts.md#checks-and-generated-files)). The rows go into the `classifier_label` table keyed by `[model_version, index]` ([schema](../../backend/migrations/schema.surql)), and the loader derives `classifier_labels_version` from `model_version` ([README 12](README.md#12-findings-for-the-architecture-and-contracts), finding 21).
- `classifier_labels_version` in every perception payload names it. The head file and the seed change in one deploy; the worker keeps the previous head loaded during the change-over, and refuses a job naming a version it doesn't have with a non-retryable error ([README 12](README.md#12-findings-for-the-architecture-and-contracts), finding 23).

### The BioCLIP label matrix

- **The labels.** Every species' and organism's `bioclip_label`, written in the seed as `<scientific name> with common name <common name>` ([README 12](README.md#12-findings-for-the-architecture-and-contracts), finding 20).
- **The matrix.** `ml/training/labels/bioclip_text.py` embeds each label with BioCLIP 2's text tower, wrapped in each of the 80 OpenAI ImageNet templates (`a photo of {label}.`, …), averaged and re-normalised ✓ (pybioclip's method). The output is an `N × 768` fp32 matrix with each row's `species_id` or `organism_id` and its table, about 0.6 MB for 200 labels, written in the same JSON format as the heads (base64 little-endian `f32`) as the manifest entry `bioclip_labels_<label_set_version>`.
- **The version.** The loader generates `label_set_version` ([contracts 10](../architecture/contracts.md#checks-and-generated-files)) as the first 16 hex digits of the SHA-256 over the BioCLIP model revision, the template list and the sorted labels, so a change in any of them is a new version. It runs `bioclip_text.py` (or checks the matrix training produced against the same hash) in the same step.
- The matrix is published to the model store and listed in the manifest. The perception worker loads the current matrix and, during a change-over, the previous one, and picks the payload's.
- **It needs only names.** Species and organisms need no reviewed problem profile to have a label, so the matrix can be built from the species and organism seed files as soon as they exist (Oct 18), long before every profile is reviewed.

**Label wording.** Text type matters by only about 4 points on leaf photos (✓ ran: 54–58% top-1 across five caption types), and the scientific name guards against ambiguous common names ("pepper"), so the label carries both. For organisms with stages (ladybug larva against adult), the label names the stage: `Coccinellidae larva with common name ladybug larva`.

---

## 9. Feedback as training data

`POST /scans/{id}/feedback` stores a reviewer's verdict and corrected label ([contracts 3](../architecture/contracts.md#post-scansidfeedback)), and observations keep their `model_version`. After build week:

1. `ml/training/data/feedback.py` exports scans with feedback `wrong` and a corrected label: the image, the regions, the predicted and corrected labels, the model versions. It runs against a database export, never production directly, and only on gardens whose owners agreed to contribute photos.
2. A teammate reviews each one, as for the evaluation set ([evaluation 3](evaluation.md#3-labels)).
3. Reviewed crops join the head's training data with source `feedback`; a few go to the evaluation dev split. They are deduplicated against everything else first.

---

## 10. Knowledge ingestion: the model steps

The ingestion pipeline is in [architecture 5](../architecture/README.md#knowledge-base) and its seed format in [contracts 10](../architecture/contracts.md#10-knowledge-seed-format). Four of its steps use a model, and they are covered here. Fetching, cleaning, validation and the migration are plain code.

```
ml/knowledge/
├─ plan.yaml           the records to draft, by slug; the tagger's id enums; facts-only sources
├─ llm.py              the hosted model: structured output, reply cache, cost
├─ fetch.py            step 1: sources.yaml → raw documents (not committed)
├─ chunk.py            step 1: raw → chunk text (build/, not committed)
├─ tag.py              step 2: tags each chunk; build/tags/ holds every decision
├─ embed.py            step 3: embeds chunks with the pinned text model
├─ extract.py          step 4: drafts profiles (strongest LLM)
├─ discriminate.py     step 4: proposes the 3 discriminating indicators (text embedder)
├─ review.py           step 5: rebuilds the review files from the YAML after hand edits (no model)
├─ validate.py         JSON Schema from the domain types, plus the loader's checks
├─ build_migration.py  the .surql migration, with vectors
└─ seed/               the seed files (committed): drafts and their review files until signed
```

### Chunking (step 1)

- Split on the document's own headings first, then on paragraphs, into chunks of about 120–200 words. A chunk never crosses a heading, so its topic stays single.
- No overlap: chunks are short excerpts shown with citations, and overlap would repeat text in the app.
- `id` is `<source_id>.<n>` ([contracts 10](../architecture/contracts.md#other-entity-shapes)), stable while the source doesn't change.

### Tagging (step 2)

- Model: `claude-sonnet-5-5` at medium effort, with structured output. Ingestion runs offline, so it picks its own model rather than the app's `TextClassifier` route ([architecture 7](../architecture/README.md#router-and-safety-gate)): Haiku 5.5 failed the spot check on the tomato slice (see Running it), and Opus is more than picking labels from fixed lists needs.
- Output per chunk: `crop`, `problem`, `organism` (each a seed or plan slug, or null), `topic` (the `ChunkSeed` enum), `relevant`, and the lists `problems` and `organisms`: every one the chunk describes, compares or advises on. A chunk that sets early blight against Septoria reaches both drafts, which is how extraction finds the look-alikes. The enums are generated from the seed and `plan.yaml`, so a tag can't name an unknown ID. `seed/chunks/` keeps the single tags of the `ChunkSeed` shape; the lists stay in `build/tags/`, for extraction.
- Irrelevant chunks (navigation, legal text, other crops, clinic notices) are dropped. A chunk the dose check fails on is **withheld**: kept out of the seed, but read by extraction for the advice around the rate, since the drafts are dose-checked themselves. `plan.yaml`'s `withheld_chunks` withholds the same way the chunks a reviewer names: one that gives a rate the dose check misses, or one the chat could return as wrong advice (the signing review of #51 withheld 6). `validate.py` fails on a withheld id whose source isn't in `sources.yaml` or that is in `seed/chunks`, and, once fetch and chunk have run, on one that isn't in `build/chunks`: ids are positions on the page, so a typo or a page that lost a paragraph would otherwise commit the excerpt unnoticed.
- A source in `plan.yaml`'s `facts_only_sources` has no reuse grant (Clemson HGIC, [sources.yaml](../../ml/knowledge/seed/sources.yaml)). Its chunks are tagged and extracted from, so its facts reach the profiles in our own words with a citation, but none of its wording is committed.
- A teammate spot-checks 10% of each source's tags; a source with more than 1 wrong tag in 10 is re-tagged with `claude-opus-5-5`, named in `plan.yaml`'s `tag_models` so every later run, on any machine, tags it with Opus again. `--model` overrides the plan for every source.

### Embedding (step 3)

- Model: the **same ONNX file and tokenizer** as `api`'s `TextEmbedder` ([models 10](models.md#10-text-embedder)), run with `onnxruntime` on CPU. The `tokenizers` Python package wraps the same Rust core as the crate. Load `tokenizer.json` from the file (not `AutoTokenizer` or `SentenceTransformer`, which can differ in special tokens and truncation), enable truncation at 512 and padding with id 0, feed `input_ids`, `attention_mask` and zero `token_type_ids` as int64, take the CLS vector and L2-normalise it in fp32 ✓ ran. Documents get no prefix.
- Batches of 64 ([architecture 5](../architecture/README.md#knowledge-base)). In fp32 a text's vector doesn't depend on its batch ✓ ran, so the batch size is free. A 200-word chunk is about 290 tokens, far below the 512 limit ✓ ran.
- Each vector is cached under `(embedding_model, sha256(text))`, so a chunk whose text hasn't changed keeps its vector.
- Vectors are not in the seed files ([contracts 10](../architecture/contracts.md#other-entity-shapes)). `build_migration.py` writes them into the migration with `embedding_model` on every row, so the database and the query side always name the same model.
- Changing the model re-embeds every chunk and every memory, and a different dimension changes the HNSW index ([database README](../database/README.md)). That is a migration, never a silent swap.
- The golden test of [models 10](models.md#the-pinned-recipe) runs in the ingestion tests too.

### Profile extraction (step 4)

- Model: the strongest LLM API model (`claude-opus-5-5`) with structured output. Extraction runs once per profile, so cost is small and quality matters most.
- Input: the tagged chunks for one problem or organism from every source (`build/tags/`, every chunk whose lists name it), each with its chunk id. Output: a draft `ProblemSeed` or `OrganismSeed`, plus, for every indicator, condition, look-alike, treatment and product, the chunk ids it came from. The review file next to the draft quotes each cited excerpt in full; a facts-only or withheld excerpt is named and linked instead, and the reviewer reads it in `build/chunks/` after running fetch and chunk (no model call). The review file names the model that answered, which Opus's server-side fallback can change.
- The schema comes from the `domain` types through `schemars`, with the keywords the hosted API's structured output rejects (`maxLength`, `maxItems`, `minimum`, `maximum`) stripped ✓ ([vlm.md 12](vlm.md#12-the-fallback-worker)); `validate.py` then checks the full schema and the loader's checks.
- The prompt's rules:
  - use only the given excerpts;
  - write each symptom as one checkable indicator, with a yes / no `question` a person could answer from a photo, in plain words a 7–8B VLM reads well ("Do the brown spots have rings inside them, like a target?");
  - give a `measure` only when the source states a size, converted to the quantity's unit, and split a size that comes with other signs into a size indicator and a sign indicator ([contracts 10](../architecture/contracts.md#problem-profile)). The prompt lists the shots each quantity can be measured from; a size indicator whose measure no shot can take is dropped with a note, since a small VLM can't judge millimetres by eye, and the size goes into the sign in relative words ("pinpoint specks");
  - set `where` and `image_kinds` from where the symptom shows;
  - propose a `weight`: 3 for a symptom the source calls characteristic or diagnostic, 2 for common, 1 for "may" or "sometimes";
  - never write a dose, a rate or a mixing ratio; name a chemical step's active ingredient or group ("a pyrethroid"), never only "an insecticide", so the plan validator can tell whether it harms beneficials, and keep the source's warnings in words;
  - word a protectant fungicide or copper as prevention when the source says so;
  - leave `discriminating` false everywhere: the next step proposes it.

### Choosing the discriminating indicators (step 4, continued)

Each problem asks exactly 3 indicators: "the highest-weighted ones that differ from its `looks_like` neighbours" ([architecture 5](../architecture/README.md#knowledge-base)). The script measures "differ", then a person decides:

1. Embed every indicator's `text` with the pinned text embedder (as documents).
2. An indicator's **overlap** is its highest cosine similarity to any indicator of any `looks_like` neighbour.
3. It is **shared** when the overlap is 0.85 or more. That starting value is checked by reading the pairs for one crop and adjusted.
4. Propose the top 3 non-shared indicators by weight, ties broken by lower overlap. Prefer a set where at least two indicators can be seen in a `leaf_closeup` or `whole_plant`, the shots most scans have. If fewer than 3 are non-shared, fill with the lowest-overlap shared ones and flag the profile.
5. The reviewer sees each pair with its similarity and can override. The loader enforces exactly 3 ([contracts 10](../architecture/contracts.md#checks-and-generated-files)).

### Review (step 5)

The reviewer works from the draft, the source excerpts and the discrimination report, and signs the file with `reviewed_by` and `reviewed_at`. Model output is never loaded unreviewed: bad data here produces confident wrong advice ([architecture 5](../architecture/README.md#knowledge-base)). `validate.py` enforces it: a species, problem or organism with an empty `reviewed_by` fails, so `build_migration.py` writes nothing until every draft is signed; `--drafts` runs the other checks on drafts. A draft problem has both `reviewed_by` and `reviewed_at` empty, and one signed without the other fails even with `--drafts`. Treatments carry no signature: each is reviewed, with its text and sources, in the review file of every problem that links it, and a new treatment whose id is taken is renamed and noted there rather than linked to the other text.

Planned before the next crop (a follow-up to #51): three model checks that point the reviewer at what is likely wrong, each written into the review file. They don't replace the signature.

| Check | Model | What it does |
|---|---|---|
| Look-alikes | `claude-opus-5-5` | Reads every excerpt for a crop and proposes the pairs that get confused, each with the sign that tells them apart and its citation |
| Citations | `claude-sonnet-5-5` | For every claim and the excerpt it cites: supported, partly or not, with a reason. A different model from the drafter, so the check is independent |
| Questions | `claude-haiku-5-5` | Whether each `question` asks about one visible sign in plain words; units, dose words and typos are checked in code |

### Running it

From the repository root, with the key loaded (`set -a; . infra/dev/.env; set +a`) and the ingest models fetched (`python3 -m ml.models.fetch --for ingest`):

```
python3 -m ml.knowledge.fetch          # raw/ ; obeys robots.txt (a server error on it blocks the host), names itself, never retries a refusal
python3 -m ml.knowledge.chunk          # build/chunks/
python3 -m ml.knowledge.tag            # seed/chunks/ ; build/tags/ holds every decision and the spot-check sample
python3 -m ml.knowledge.extract        # reads build/tags/; drafts + <id>.review.md for every plan.yaml entry without a file
python3 -m ml.knowledge.discriminate   # the 3 flags, and the pair table in each review file
python3 -m ml.knowledge.validate --drafts
```

After the YAML is edited by hand at review, two runs bring the review files back in line with it, and neither calls a model nor changes the YAML:

```
python3 -m ml.knowledge.discriminate --keep-flags   # refreshes the pair tables, shows the YAML's set beside the proposal, keeps the notes under each table
python3 -m ml.knowledge.review                      # rebuilds indicators, conditions, look-alikes, treatments, products, recognition and care
```

`review.py` keeps each line's chunk ids from the old file and quotes them from `seed/chunks`, linking one the seed doesn't carry. A treatment or product keeps its own file's ids, so a citation the reviewer removes stays removed; only one newly linked to the problem starts from the ids other problems' files give it. A line added by hand shows "(no excerpt cited)" until the reviewer writes `` - `<chunk id>` `` under it. An id the seed doesn't carry, facts-only ones included, is checked against `build/tags`, or `build/chunks` when only that is there: one `tag.py` never saw or dropped, or one the page doesn't have, is an error, and the run exits 1. With neither, only its source is checked, and the run lists the ids it couldn't check; the line written for an id is the same either way, so runs on different machines don't rewrite each other's. The header, the model notes (headed "as first drafted") and every other section, a reviewer's own included, are kept as they are; one placed between the record sections moves after them. `discriminate.py --keep-flags` keeps everything under the pair table, tables included.

Every model reply is cached under the hash of its whole request (`ml/knowledge/.cache/`, not committed), so a rerun on the same machine costs nothing. A reviewer on another machine runs fetch and chunk only, which call no model, to read the facts-only and withheld excerpts. What the tomato slice showed:

- **First run (✓ ran, 2026-10-19), tagging:** 11 pages gave 155 chunks. Haiku at low effort dropped good chunks whose problem wasn't in the plan (potato aphid, spider-mite damage, Ontario's seed hygiene), and a second run dropped a different set; more than 1 wrong tag in 10 for several sources, so all of them were re-tagged with `claude-opus-5-5`, which kept 135 and dropped only furniture, research outlooks and the 3 chunks that fail the dose check. Cost: $0.39 for the Opus pass, $0.43 with the two Haiku runs.
- **Extraction:** the first schema, with chunk ids as an enum at every citation, was refused once the treatment list grew ("The compiled grammar is too large"). Chunk and treatment ids are plain strings now, checked after the reply: an id the model wasn't given is dropped and named in the review file. One reply came back empty; an empty draft is asked for again past the cache. A size on a fruit photo is dropped, since the depth camera measures spots only on leaf shots. Product names are cut to the ingredient (`copper`), as the plan validator matches them inside step text. 16 records cost about $2 at high effort.
- **The first review (PR #51)** found what the scripts couldn't: one tag per chunk hid the source's own look-alike guide from extraction, the review files cut each excerpt at 400 characters, size indicators survived their dropped measure, and Clemson's text, which has no reuse grant, was committed nearly in full. The second run below fixes those in the scripts; the content points were fixed by hand against the excerpts and are listed in each review file.
- **Second run (✓ ran, 2026-10-20), tagging:** 10 pages (the houseplant whitefly article swapped for EDIS IN286, the lady beetle dropped) gave 161 chunks. `claude-sonnet-5-5` at medium effort dropped 15 (furniture, clinic notices, a pepper-only paragraph, stink bugs, which the slice doesn't cover) and withheld 3 for the dose check; the 10% spot check found 1 wrong tag (ISU spider mite .4, a management paragraph dropped as a pesticide list), so that source was re-tagged with Opus, which kept it. Ontario's look-alike paragraph now lists early blight, Septoria and both bacterial diseases. Cost: $0.29 for Sonnet, $0.03 for the Opus re-tag; 1 chunk needed more than 4,000 output tokens and was retried with more room.
- **Second run, extraction:** 18 records cost $2.05. The drafts came back with look-alikes between early blight, Septoria and the two bacterial diseases, but with 75 treatments, many of them near-duplicates across problems, so the merge stays a review step.
- **Coverage:** `tomato_mosaic_virus` has no source that covers it on its own; it is out of `plan.yaml` until one is found (#46).

---

## 11. Schedule

| Date | Training and export |
|---|---|
| Oct 16 | Environment on the 5070 Ti (day-1 check, [evaluation 7](evaluation.md#7-day-1-checks)). Fetch and deduplicate the datasets. BioCLIP 2 exported and checked |
| Oct 17 | Leaf detector first fine-tune on the 5070 Ti (section 5), then vLLM back up. Text embedder pinned. Disease head v0 on PlantDoc's boxes (CPU) |
| Oct 18 | First detector exported and checked. Relabelling pass (people). The BioCLIP label matrix from the species and organism seed files. Night: second detector run on the 5070 Ti with vLLM stopped, while the 9070 XT serves |
| Oct 19 | Crops re-extracted with the second detector; disease head v1, species head, `classifier_labels.yaml` |
| Oct 20–21 | Heads retrained as profiles are reviewed and the label list grows. ConvNeXt-Tiny only if the head misses its target |
| Oct 22–23 | Calibration on the dev split ([evaluation 5](evaluation.md#5-calibration)); final manifests |
| Oct 24 | Test split scored once |

---

## 12. Open questions

1. **Plant Pathology 2020/2021 terms**: a teammate reads the Kaggle rules before any apple photo from them is used.
2. **The face detector's and BioCLIP's training data**: WIDER FACE is CC BY-NC-ND ✓; TreeOfLife-10M includes CC BY-NC-SA images ✓, and TreeOfLife-200M (BioCLIP 2's data, tagged CC0 as a compilation ✓) probably keeps per-image terms the same way *unverified*. Both models' weights are MIT. Recorded in the notices; a legal reading is the team's call ([README 9](README.md#9-licences-and-notices)).
3. **Minimum crops per class** (30) and the head type (logistic or MLP): set from validation.
4. **French chunks**: only if the knowledge sources include French guides, which would need a multilingual embedder decided before ingestion ([models 10](models.md#10-text-embedder)).
