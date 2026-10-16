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
| Fetch datasets and BioCLIP 2 | `python ml/training/data/fetch.py [name …]` | `training/data/raw/`, `models/files/bioclip2/` |
| Export BioCLIP 2 and check it | `python -m ml.training.export.bioclip --images <photos> --fp16` | `models/files/bioclip/` and `check.json` |
| Export SAM 2.1-T and check it | `python -m ml.training.export.sam2` | `models/files/sam2/` and `check.json` |
| Deduplicate and split | `python -m ml.training.data.dedupe [--datasets …]` | `training/data/splits/*.tsv`, `dedupe_report.md` |
| Detector data (COCO) | `python ml/training/detector/prepare.py` | `training/data/cache/leaf-coco/` |
| Train the detector | `python ml/training/detector/train.py` | `training/runs/leaf-rfdetr-s-<date>/` |
| Disease head | `python -m ml.training.heads.train_disease --version <v> [--views 4]` | `training/runs/heads/<v>/`: head JSON, `classifier_labels.yaml`, report |
| Label matrix | `python -m ml.training.labels.bioclip_text [--labels <yaml>]` | `models/files/bioclip_labels/` |
| Zero-shot species check | `python -m ml.training.labels.zero_shot_check <matrix.json>` | printed |

Dataset sources, licences and hashes are in [`training/data/sources.yaml`](training/data/sources.yaml); class-to-slug mappings in [`training/data/mappings/`](training/data/mappings/). The slugs there are **proposed** until the knowledge base's seed files exist.
