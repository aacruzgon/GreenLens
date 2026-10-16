"""Export BioCLIP 2's vision tower to ONNX and check it (training.md §7, models.md §6).

    python -m ml.training.export.bioclip --images <dir of real photos> [--fp16]

Graph: images [batch, 3, 224, 224] float32 → embeddings [batch, 768], L2-normalised inside
the graph. Pre-processing (shortest side to 224, centre crop, CLIP normalisation) stays out of
the graph and comes from pdm_vision, as the Rust worker does it.

Pass condition (training.md §7): cosine ≥ 0.999 against PyTorch for every image, at batch 1,
3 and the maximum, with identical results per image across batch sizes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import open_clip
import torch

from ml.pdm_vision import preprocess as pp

ROOT = Path(__file__).resolve().parents[3]
WEIGHTS = ROOT / "ml/models/files/bioclip2"
OUT = ROOT / "ml/models/files/bioclip"
REVISION = "2957b322090f9cb17ae72c71981c7218a28d81e0"
OPSET = 17
MAX_BATCH = 16


class VisionTower(torch.nn.Module):
    def __init__(self, model):
        super().__init__()
        self.visual = model.visual

    def forward(self, images):
        e = self.visual(images)
        return e / e.norm(dim=-1, keepdim=True).clamp_min(1e-12)


def load_model():
    model, _, _ = open_clip.create_model_and_transforms(f"local-dir:{WEIGHTS}")
    return model.eval()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def photos(folder: Path, n: int) -> list[Path]:
    files = sorted(p for p in folder.rglob("*") if p.suffix.lower() in {".jpg", ".jpeg", ".png", ".webp"})
    random.Random(0).shuffle(files)
    return files[:n]


def check(onnx_path: Path, tower: VisionTower, images: list[Path], provider: str) -> dict:
    sess = ort.InferenceSession(str(onnx_path), providers=[provider, "CPUExecutionProvider"])
    used = sess.get_providers()[0]
    x = pp.batch([pp.load_rgb(p) for p in images], pp.BIOCLIP)
    with torch.no_grad():
        ref = tower(torch.from_numpy(x)).numpy()
    per_batch = {}
    for b in (1, 3, MAX_BATCH):
        outs = [sess.run(["embeddings"], {"images": x[i : i + b]})[0] for i in range(0, len(x), b)]
        per_batch[b] = np.concatenate(outs)
    cos = {b: (e * ref).sum(1) for b, e in per_batch.items()}
    spread = max(float(np.abs(per_batch[b] - per_batch[1]).max()) for b in per_batch)
    return {
        "provider": used,
        "images": len(images),
        "min_cosine": {str(b): float(c.min()) for b, c in cos.items()},
        "max_diff_across_batches": spread,
        "pass": all(float(c.min()) >= 0.999 for c in cos.values()),
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--images", type=Path, required=True)
    ap.add_argument("--n", type=int, default=50)
    ap.add_argument("--fp16", action="store_true")
    args = ap.parse_args()

    OUT.mkdir(parents=True, exist_ok=True)
    tower = VisionTower(load_model()).eval()
    fp32 = OUT / "bioclip-2-vision-fp32.onnx"
    torch.onnx.export(
        tower,
        torch.randn(2, 3, 224, 224),
        str(fp32),
        opset_version=OPSET,
        dynamo=False,
        input_names=["images"],
        output_names=["embeddings"],
        dynamic_axes={"images": {0: "batch"}, "embeddings": {0: "batch"}},
    )
    onnx.checker.check_model(str(fp32))
    images = photos(args.images, args.n)
    report = {"fp32": check(fp32, tower, images, "CUDAExecutionProvider")}
    report["fp32"]["sha256"] = sha256(fp32)
    print(json.dumps(report, indent=2), flush=True)

    if args.fp16:
        # onnxconverter_common 1.16's converter crashes with op_block_list on onnx 1.22
        # (remove_unnecessary_cast_node); ONNX Runtime ships a maintained fork of it.
        from onnxruntime.transformers import float16

        fp16 = OUT / "bioclip-2-vision-fp16.onnx"
        m = float16.convert_float_to_float16(
            onnx.load(str(fp32)),
            keep_io_types=True,
            op_block_list=["LayerNormalization", "Softmax", "ReduceMean"],
        )
        onnx.save(m, str(fp16))
        report["fp16"] = check(fp16, tower, images, "CUDAExecutionProvider")
        report["fp16"]["sha256"] = sha256(fp16)

    report["revision"] = REVISION
    report["opset"] = OPSET
    (OUT / "check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
