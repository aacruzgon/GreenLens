"""The day-1 plant probe: zero-shot BioCLIP 2, written as a one-class linear head (models.md 2).

    python -m ml.training.heads.plant_probe --set B [--device cpu]

Each label of a prompt set is wrapped in the 80 OpenAI ImageNet templates and embedded with
BioCLIP 2's text tower, averaged and normalised, as the label matrix is (`bioclip_text.embed`).
Each side's prototype is the mean of its labels, normalised again: `t_plant` and `t_not`. A
two-way softmax between them at BioCLIP 2's logit scale `s` is

    p(plant) = exp(s e·t_plant) / (exp(s e·t_plant) + exp(s e·t_not)) = σ(s e·(t_plant − t_not))

which is exactly the worker's probe, `σ((w·e + b) / T)` on the L2-normalised image embedding,
with `w = s (t_plant − t_not)`, `b = 0` and `T = 1` (backend vision `PlantProbe`; FORMATS.md). So
the zero-shot probe ships as a `pdm-linear-head/1` file of kind `plant_probe`, and M13 replaces
it with a trained probe in the same format.

Prompt sets (fixed before measuring; `ml.training.eval.plant_check` scores both):
  A  models.md 2 as written.
  B  A, plus the plant photos the leaf detector keeps no leaf on (reports/tau-leaf-val-2026-10-16):
     wilted and dying plants, a single leaf filling the frame, trees.

Writes ml/models/files/plant_probe/<version>.json. `tau_plant` goes in the manifest.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

from ml.pdm_vision import headfile
from ml.pdm_vision.features import DEFAULT_MODEL, file_sha256

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "ml/models/files/plant_probe"
VERSION = "plant-probe-zs-2026-10-16"
BIOCLIP = "bioclip-2@2957b322"

NOT_PLANT = ["wall", "person", "pet", "room", "hand", "car"]
PROMPT_SETS = {
    "A": {"plant": ["plant", "leaves", "garden plant"], "not": NOT_PLANT},
    "B": {"plant": ["plant", "leaves", "garden plant", "wilted plant", "dying plant", "leaf", "tree"],
          "not": NOT_PLANT},
}


def text_rows(labels: list[str], device: str) -> np.ndarray:
    """One normalised row per label: the 80 templates, embedded, averaged and normalised."""
    from ml.training.labels.bioclip_text import embed

    return embed([{"label": label} for label in labels], device=device)


def logit_scale() -> float:
    """BioCLIP 2's learnt logit scale, exp(logit_scale), from the pinned weights."""
    import open_clip

    from ml.training.labels.bioclip_text import WEIGHTS

    model, _, _ = open_clip.create_model_and_transforms(f"local-dir:{WEIGHTS}")
    return float(model.logit_scale.detach().exp())


def prototype(rows: np.ndarray) -> np.ndarray:
    m = rows.mean(0)
    return (m / np.linalg.norm(m)).astype(np.float32)


def weight(t_plant: np.ndarray, t_not: np.ndarray, scale: float) -> np.ndarray:
    return (scale * (t_plant - t_not)).astype(np.float32)


def probability(embeddings: np.ndarray, w: np.ndarray, bias: float = 0.0) -> np.ndarray:
    """The worker's `PlantProbe::probability` on L2-normalised embeddings `[n, 768]`."""
    z = embeddings.astype(np.float32) @ w + np.float32(bias)
    return 1.0 / (1.0 + np.exp(-z))


def weights(device: str = "cpu") -> tuple[dict[str, np.ndarray], float]:
    """Every prompt set's weight vector, and the logit scale."""
    labels = sorted({label for s in PROMPT_SETS.values() for side in s.values() for label in side})
    rows = dict(zip(labels, text_rows(labels, device)))
    scale = logit_scale()
    out = {}
    for name, s in PROMPT_SETS.items():
        t_plant = prototype(np.stack([rows[label] for label in s["plant"]]))
        t_not = prototype(np.stack([rows[label] for label in s["not"]]))
        out[name] = weight(t_plant, t_not, scale)
    return out, scale


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--set", required=True, choices=sorted(PROMPT_SETS))
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--version", default=VERSION)
    args = ap.parse_args(argv)
    ws, scale = weights(args.device)
    w = ws[args.set]
    path = OUT / f"{args.version}.json"
    headfile.write_linear_head(
        path,
        kind="plant_probe",
        version=args.version,
        features=f"{BIOCLIP}+{file_sha256(DEFAULT_MODEL)[:16]}",
        weight=w[None, :],
        bias=np.zeros(1, np.float32),
        temperature=1.0,
        labels=[{"index": 0, "label": "plant"}],
        extra={
            "method": "zero-shot: sigma(s * e . (t_plant - t_not)), two text prototypes "
                      "(ml/training/heads/plant_probe.py)",
            "prompt_set": args.set,
            "plant_labels": PROMPT_SETS[args.set]["plant"],
            "not_labels": PROMPT_SETS[args.set]["not"],
            "templates": "openai-imagenet-80",
            "logit_scale": round(scale, 4),
        },
    )
    print(f"prompt set {args.set}, logit scale {scale:.2f} → {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
