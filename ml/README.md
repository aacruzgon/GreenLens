# ml/

Training, export and evaluation of PDM's models. The design is in [docs/vision](../docs/vision/README.md): [training.md](../docs/vision/training.md) for this folder's layout and steps, [models.md](../docs/vision/models.md) for each model's tensors, [evaluation.md](../docs/vision/evaluation.md) for the checks and targets.

Nothing here runs in production. What the workers load is listed in [`models/manifest.toml`](models/manifest.toml); the JSON formats of the heads and label matrices are in [`models/FORMATS.md`](models/FORMATS.md). Datasets, caches, runs and model files are never committed.

## Setup

On a Linux machine with an NVIDIA GPU and driver 580.65.06 or newer, from the repository root:

```bash
uv python install 3.12 && uv venv --python 3.12 .venv && source .venv/bin/activate
SAM2_BUILD_CUDA=0 uv pip sync ml/requirements.lock --index-strategy unsafe-best-match \
  --extra-index-url https://download.pytorch.org/whl/cu130
```

`ml/requirements.lock` is compiled from `ml/requirements.in` (the command is at the top of that file). Run every script from the repository root as a module (`python -m ml.…`).

## Steps

| Step | Command | Output |
|---|---|---|
| Fetch datasets, BioCLIP 2 and the RF-DETR-S weights | `python -m ml.training.data.fetch [name …]` | `training/data/raw/`, `models/files/bioclip2/`, `models/files/rfdetr/` |
| Export BioCLIP 2 and check it | `python -m ml.training.export.bioclip --images <photos> --fp16` | `models/files/bioclip/` and `check.json` |
| Export SAM 2.1-T and check it | `python -m ml.training.export.sam2` | `models/files/sam2/` and `check.json` |
| Deduplicate and split | `python -m ml.training.data.dedupe [--datasets …]` | `training/data/splits/*.tsv`, `dedupe_report.md` |
| Detector data (COCO) | `python -m ml.training.detector.prepare` | `training/data/cache/leaf-coco/` |
| Train the detector | `python -m ml.training.detector.train` | `training/runs/leaf-rfdetr-s-<date>/` |
| Leaf crops from our detector, as the worker cuts them (GPU) | `python -m ml.training.heads.crops [--splits train val test] [--views 4]` | `training/data/cache/crops/<detector>-<bioclip>-<settings>/`: each photo's boxes, scores and crop embeddings, and with `--views` N augmented views of each crop ([training 6](../docs/vision/training.md#6-disease-and-species-heads)) |
| Disease head | `python -m ml.training.heads.train_disease --version <v> --features crops --supported tomato apple grape pepper corn [--views 4] [--drop-classes …]` | `training/runs/heads/<v>/`: head JSON and `classifier_labels.yaml`; the report also in `training/heads/reports/<v>.json` (committed). With `--features crops`, C is chosen per crop (`--select-on crop`) |
| iNaturalist training photos for the species head and plant probe (2019–2025, CC0 or CC BY) | `python -m ml.training.data.inat_species [--only SLUG]` (`--prune` drops excluded taxa) | `training/data/raw/inat_species/` (git-ignored) and `training/data/splits/inat_species.tsv` (committed: licence and credit per photo) |
| Species head | `python -m ml.training.heads.train_species --version <v>` | `training/runs/heads/<v>/` head JSON (kind `species_head`); the report in `training/heads/reports/<v>.json`, with zero-shot, head and combined top-1 on val, test and the evaluation's dev cases |
| Trained plant probe | `python -m ml.training.heads.train_plant_probe --version <v>`, then `python -m ml.training.eval.plant_check --probe training/runs/heads/<v>/<v>.json` | `training/runs/heads/<v>/`: the probe (kind `plant_probe`) and `negatives_tune.tsv`, the COCO half it didn't train on, which `plant_check` scores it against ([models 2](../docs/vision/models.md#2-plant-present-check)) |
| Label matrix | `python -m ml.training.labels.bioclip_text [--labels <yaml>]` | `models/files/bioclip_labels/` |
| Zero-shot species check | `python -m ml.training.labels.zero_shot_check <matrix.json>` | printed |
| Export the leaf detector and check it | `python -m ml.training.export.detector --run <run dir> [--fp16] [--version V]` | `models/files/` ONNX file; checks class-agnostic mAP50 against PyTorch |
| Fetch the model files for a host | `python3 -m ml.models.fetch --for perception --dest ~/.cache/pdm-models [--cpu]` | The files of `manifest.toml` (`--cpu`: the fp32 set of `manifest.cpu.toml`) in `--dest`, each checked against its SHA-256. Downloads from the private Hugging Face repository `pdm-models` with `HF_TOKEN` ([vision 6](../docs/vision/README.md#6-model-files-versions-and-pinning)) |
| Check the manifests | `python -m ml.models.check` | Passes when the manifests, `sources.toml` and `NOTICES.md` agree |
| Publish new model files | `python -m ml.models.publish` | Uploads new model files to `pdm-models` and writes their `sources.toml` rows |
| Write the CPU manifest | `python -m ml.models.cpu_manifest` | `models/manifest.cpu.toml` (fp32 files) and a `<stem>.cpu.json` twin of each head |
| Check a VLM server (day 1) | `python -m ml.training.eval.vlm_check [--base-url …] [--images …]` | `<out>/<label>.json` and `.md` with the checks and timings ([vlm 4](../docs/vision/vlm.md#4-serving)) |
| Check the perception card (day 1) and the batch caps that fit it (J3) | `python -m ml.training.eval.perception_gpu all --models <fetched dir>` (needs only numpy, onnx and onnxruntime-gpu; the install line is in its docstring) | `perception-gpu-<host>-<date>.json`: CUDA against the CPU on a small model, each perception model opening on CUDA, and each cap set's peak VRAM, free memory and time per scan ([evaluation 7](../docs/vision/evaluation.md#7-day-1-checks), [8](../docs/vision/evaluation.md#perception-batch-size)) |
| Fake VLM server for the worker (replays recorded replies; `--record`, `--fault`) | `python3 -m ml.training.eval.fake_vlm [--port 8001] [--recordings …]` | An OpenAI-compatible server on port 8001, served name `vlm` ([dev-roadmap 5](../docs/vision/dev-roadmap.md#testing-the-vlm-worker)) |
| Leaf score of the plant check on a split (provisional `τ_leaf`) | `python -m ml.training.eval.leaf_scores [--split val] [--cuda]` | `reports/<label>.json` (every photo's score) and `.md` (share below each `τ_leaf`, by dataset and species) ([models 2](../docs/vision/models.md#2-plant-present-check)) |
| Zero-shot plant probe (day 1) | `python -m ml.training.heads.plant_probe --set A` | `models/files/plant_probe/` JSON in the `pdm-linear-head/1` form ([models 2](../docs/vision/models.md#2-plant-present-check)) |
| Plant check on the val split and the COCO negatives (both thresholds) | `python -m ml.training.eval.plant_check [--rescore report.json]` | `reports/<label>.json` and `.md` ([report](training/eval/reports/plant-check-val-2026-10-16.md)) |
| Licensed iNaturalist photos for the evaluation set (2026 only, CC0 or CC BY) | `python -m ml.training.eval.inat fetch [--only TAXON] [--count]`, review, then `… add --labeller <login>` | `training/eval/photos/inat/` (git-ignored): photos, `index.html` contact sheet, `review.yaml`; `add` appends cases to `training/eval/manifest.jsonl` ([evaluation 2](../docs/vision/evaluation.md#where-the-photos-come-from)) |
| Field-dataset photos for the evaluation set: tomato and corn diseases (CC BY 4.0, none a training source) | `python -m ml.training.eval.fieldsets fetch [--per-class 8]` (GPU: the copy check), review, then `… add --labeller <login>` | `training/eval/photos/fieldsets/` (git-ignored): photos, `index.html`, `review.yaml`, `copies.md`; `add` appends cases to `training/eval/manifest.jsonl` ([evaluation 2](../docs/vision/evaluation.md#where-the-photos-come-from)) |
| Day-2 VLM benchmark: the worker's own `analyse` on the dev cases, per model, image size and jobs at once (J2) | `python -m ml.training.eval.vlm_bench cases`, then with the candidate served by vLLM `… run --label <model> [--max-pixels …] [--concurrency 1 2] [--gpu <uuid>]`, then `… score training/runs/vlm-bench/<model> … [--out NAME]` | `training/runs/vlm-bench/` (git-ignored): the cases and each run's raw lines; the report in `training/eval/reports/vlm-bench-<out>.md` and `.json` ([evaluation 8](../docs/vision/evaluation.md#vlm-results-2026-10-20--ran)) |
| Phone-condition variants of the evaluation cases | `python -m ml.training.eval.variants [--transforms tilt,dim,...]` | `training/eval/manifest.variants.jsonl` and `training/eval/photos/variants/` ([evaluation 3](../docs/vision/evaluation.md#variants)) |
| Fetch the COCO val2017 negatives (evaluation only) | `python -m ml.training.data.negatives` | `training/data/splits/negatives_coco.tsv` |
| Reference outputs for the Rust heads | `python -m ml.tests.make_fixtures` | `tests/fixtures/perception_reference.json` |
| Tests | `python -m pytest ml/tests` | the file formats, the `label_set_version` vector and original-photo names |

Dataset sources, licences and hashes are in [`training/data/sources.yaml`](training/data/sources.yaml); class-to-slug mappings in [`training/data/mappings/`](training/data/mappings/). The slugs there are **proposed** until the knowledge base's seed files exist.
