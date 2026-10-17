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
| Disease head | `python -m ml.training.heads.train_disease --version <v> --balance-sources [--views 4]` | `training/runs/heads/<v>/`: head JSON and `classifier_labels.yaml`; the report also in `training/heads/reports/<v>.json` (committed) |
| Label matrix | `python -m ml.training.labels.bioclip_text [--labels <yaml>]` | `models/files/bioclip_labels/` |
| Zero-shot species check | `python -m ml.training.labels.zero_shot_check <matrix.json>` | printed |
| Export the leaf detector and check it | `python -m ml.training.export.detector --run <run dir> [--fp16] [--version V]` | `models/files/` ONNX file; checks class-agnostic mAP50 against PyTorch |
| Fetch the model files for a host | `python3 -m ml.models.fetch --for perception --dest ~/.cache/pdm-models [--cpu]` | The files of `manifest.toml` (`--cpu`: the fp32 set of `manifest.cpu.toml`) in `--dest`, each checked against its SHA-256. Downloads from the private Hugging Face repository `pdm-models` with `HF_TOKEN` ([vision 6](../docs/vision/README.md#6-model-files-versions-and-pinning)) |
| Check the manifests | `python -m ml.models.check` | Passes when the manifests, `sources.toml` and `NOTICES.md` agree |
| Publish new model files | `python -m ml.models.publish` | Uploads new model files to `pdm-models` and writes their `sources.toml` rows |
| Write the CPU manifest | `python -m ml.models.cpu_manifest` | `models/manifest.cpu.toml` (fp32 files) and a `<stem>.cpu.json` twin of each head |
| Check a VLM server (day 1) | `python -m ml.training.eval.vlm_check [--base-url …] [--images …]` | `<out>/<label>.json` and `.md` with the checks and timings ([vlm 4](../docs/vision/vlm.md#4-serving)) |
| Fake VLM server for the worker (replays recorded replies; `--record`, `--fault`) | `python3 -m ml.training.eval.fake_vlm [--port 8001] [--recordings …]` | An OpenAI-compatible server on port 8001, served name `vlm` ([dev-roadmap 5](../docs/vision/dev-roadmap.md#testing-the-vlm-worker)) |
| Leaf score of the plant check on a split (provisional `τ_leaf`) | `python -m ml.training.eval.leaf_scores [--split val] [--cuda]` | `reports/<label>.json` (every photo's score) and `.md` (share below each `τ_leaf`, by dataset and species) ([models 2](../docs/vision/models.md#2-plant-present-check)) |
| Reference outputs for the Rust heads | `python -m ml.tests.make_fixtures` | `tests/fixtures/perception_reference.json` |
| Tests | `python -m pytest ml/tests` | the file formats, the `label_set_version` vector and original-photo names |

Dataset sources, licences and hashes are in [`training/data/sources.yaml`](training/data/sources.yaml); class-to-slug mappings in [`training/data/mappings/`](training/data/mappings/). The slugs there are **proposed** until the knowledge base's seed files exist.
