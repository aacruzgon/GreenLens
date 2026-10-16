"""Export the leaf detector (RF-DETR-S, one class) to ONNX and check it (training.md §5 and §7).

    python -m ml.training.export.detector --run ml/training/runs/leaf-rfdetr-s-20261016 [--fp16] [--version V]

The graph: `input` [batch, 3, 512, 512] → `dets` [batch, 300, 4] (normalised cx, cy, w, h) and
`labels` [batch, 300, 2] (logits; `leaf` at index 0, background last). Pre-processing (squash
to 512, ImageNet normalisation) is pdm_vision's, as the worker does it; no NMS in the graph.

Check: ONNX (CUDA) against the PyTorch network on the same tensors from the deduplicated
PlantDoc test split. Pass condition: class-agnostic mAP50 within 0.5 points of PyTorch's.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import shutil
from datetime import date
from pathlib import Path

import numpy as np
import onnx
import torch

from ml.pdm_vision import preprocess as pp
from ml.pdm_vision.features import cuda_session

ROOT = Path(__file__).resolve().parents[3]
COCO = ROOT / "ml/training/data/cache/leaf-coco"
OUT = ROOT / "ml/models/files/leaf_detector"
BATCH = 8


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def decode(dets: np.ndarray, logits: np.ndarray, size: tuple[int, int], min_score: float = 0.001) -> list[dict]:
    """One image's queries → COCO results in pixels (models.md §3, without the size and count
    filters, so mAP sees the whole ranking)."""
    w, h = size
    score = 1 / (1 + np.exp(-logits[:, 0]))
    out = []
    for (cx, cy, bw, bh), s in zip(dets, score):
        if s < min_score:
            continue
        out.append({"bbox": [float((cx - bw / 2) * w), float((cy - bh / 2) * h), float(bw * w), float(bh * h)],
                    "score": float(s), "category_id": 1})
    return out


def coco_map(gt_path: Path, results: list[dict]) -> dict:
    from faster_coco_eval import COCO as Coco, COCOeval_faster

    with contextlib.redirect_stdout(io.StringIO()):
        gt = Coco(str(gt_path))
        dt = gt.loadRes(results) if results else None
        ev = COCOeval_faster(gt, dt, "bbox")
        ev.evaluate()
        ev.accumulate()
        ev.summarize()
    return {"map50_95": float(ev.stats[0]), "map50": float(ev.stats[1]), "mar100": float(ev.stats[8])}


def run_all(images: list[dict], fn) -> list[dict]:
    results = []
    for s in range(0, len(images), BATCH):
        chunk = images[s : s + BATCH]
        ims = [pp.load_rgb(COCO / "test" / im["file_name"]) for im in chunk]
        dets, logits = fn(pp.batch(ims, pp.RFDETR))
        for im, d, l, pil in zip(chunk, dets, logits, ims):
            for r in decode(d, l, pil.size):
                r["image_id"] = im["id"]
                results.append(r)
    return results


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", type=Path, required=True)
    ap.add_argument("--fp16", action="store_true")
    ap.add_argument("--version", help="the manifest version; default rfdetr-s-leaf-<today>, which a second run the same day would overwrite")
    args = ap.parse_args()

    from rfdetr import RFDETRSmall

    ckpt = (args.run / "checkpoint_best_ema.pth").resolve()
    model = RFDETRSmall(pretrain_weights=str(ckpt))
    tmp = (args.run / "onnx").resolve()
    with contextlib.redirect_stdout(io.StringIO()):  # the exporter prints the whole graph
        exported = Path(model.export(output_dir=str(tmp), format="onnx", dynamic_batch=True, fp16=False))
    if exported.is_dir():
        exported = next(exported.glob("*.onnx"))
    version = args.version or f"rfdetr-s-leaf-{date.today():%Y-%m-%d}"
    OUT.mkdir(parents=True, exist_ok=True)
    fp32 = OUT / f"{version}-fp32.onnx"
    m = onnx.load(str(exported), load_external_data=True)
    onnx.save(m, str(fp32))  # one file, so the manifest's hash covers everything
    onnx.checker.check_model(str(fp32))
    io_names = {
        "inputs": {i.name: [d.dim_param or d.dim_value for d in i.type.tensor_type.shape.dim] for i in m.graph.input},
        "outputs": {o.name: [d.dim_param or d.dim_value for d in o.type.tensor_type.shape.dim] for o in m.graph.output},
        "opset": [o.version for o in m.opset_import if o.domain in ("", "ai.onnx")][0],
    }

    net = model.model.model.eval().cuda()
    gt_path = COCO / "test" / "_annotations.coco.json"
    images = json.loads(gt_path.read_text())["images"]

    def torch_fn(x: np.ndarray):
        with torch.no_grad():
            o = net(torch.from_numpy(x).cuda())
        return o["pred_boxes"].float().cpu().numpy(), o["pred_logits"].float().cpu().numpy()

    def onnx_fn(sess):
        def f(x: np.ndarray):
            d, l = sess.run(["dets", "labels"], {"input": x})
            return d, l
        return f

    ref = coco_map(gt_path, run_all(images, torch_fn))
    report = {"version": version, "checkpoint": str(ckpt.relative_to(ROOT)), "io": io_names, "torch": ref}

    sess = cuda_session(fp32)
    x = pp.batch([pp.load_rgb(COCO / "test" / im["file_name"]) for im in images[:BATCH]], pp.RFDETR)
    td, tl = torch_fn(x)
    od, ol = onnx_fn(sess)(x)
    b1 = [onnx_fn(sess)(x[i : i + 1]) for i in range(len(x))]
    report["fp32"] = {
        **coco_map(gt_path, run_all(images, onnx_fn(sess))),
        "max_diff_dets": float(np.abs(td - od).max()),
        "max_diff_logits": float(np.abs(tl - ol).max()),
        "batch1_vs_batch8_max_diff": float(max(np.abs(b[0][0] - od[i]).max() for i, b in enumerate(b1))),
        "sha256": sha256(fp32),
    }
    report["fp32"]["pass"] = abs(report["fp32"]["map50"] - ref["map50"]) <= 0.005
    (OUT / "check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2), flush=True)

    if args.fp16:
        from onnxruntime.transformers import float16

        fp16 = OUT / f"{version}-fp16.onnx"
        m16 = float16.convert_float_to_float16(
            onnx.load(str(fp32)), keep_io_types=True,
            # GridSample: the CUDA EP runs it in fp32 only (models.md §3). Range: its inputs must
            # stay integer or fp32, or the graph is invalid.
            op_block_list=["GridSample", "Range", "LayerNormalization", "Softmax", "ReduceMean"],
        )
        onnx.save(m16, str(fp16))
        s16 = cuda_session(fp16)
        report["fp16"] = {**coco_map(gt_path, run_all(images, onnx_fn(s16))), "sha256": sha256(fp16)}
        report["fp16"]["pass"] = abs(report["fp16"]["map50"] - ref["map50"]) <= 0.005

    shutil.rmtree(tmp, ignore_errors=True)
    (OUT / "check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
