"""Export SAM 2.1 Hiera-T to two ONNX graphs and check them (models.md §4, training.md §7).

    python -m ml.training.export.sam2                # export, fp16 conversion, checks, check.json
    python -m ml.training.export.sam2 --skip-export  # re-run the checks on the existing files

Route: Meta's `sam2` package (facebookresearch/sam2 at SAM2_COMMIT, installed with
SAM2_BUILD_CUDA=0) with the checkpoint `sam2.1_hiera_tiny.pt` from the Hugging Face repo
`facebook/sam2.1-hiera-tiny` at REVISION. Exporter: `torch.onnx.export(..., dynamo=False)`,
opset 17, dynamic axes.

Encoder: images [B, 3, 1024, 1024] float32 (squashed to 1024 x 1024, ImageNet mean/std, done
outside the graph by pdm_vision) -> high_res_feats_0 [B, 32, 256, 256],
high_res_feats_1 [B, 64, 128, 128], image_embed [B, 256, 64, 64] (no_mem_embed added, as
SAM2ImagePredictor.set_image_batch does).

Box decoder: one image's three feature tensors ([1, ...]) and boxes [N, 4] (x0, y0, x1, y1 in
the 1024 x 1024 frame; each box its own prompt, corner labels 2 and 3) -> mask_single
[N, 1, 256, 256], iou_single [N, 1], masks [N, 3, 256, 256], iou_predictions [N, 3],
object_score_logits [N, 1]. Logits out; no upsampling or thresholding. `mask_single` is
SAM2ImagePredictor's multimask_output=False output: build_sam2's default
dynamic_multimask_via_stability falls back to the best multi-mask when token 0's mask is
unstable (stability < 0.98 at delta 0.05). That selection is part of the graph.

Pass condition (training.md §7): mean IoU of thresholded (> 0) masks, ONNX vs PyTorch, 0.98 or
more on real PlantDoc leaves, with the mask resized to the image as the worker does it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import subprocess
import sys
import threading
import time
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
import torch.nn.functional as F

from ml.pdm_vision import preprocess as pp

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "ml/models/files/sam2"
WEIGHTS = OUT / "weights"
DATA = ROOT / "ml/training/data/raw/plantdoc_det"
REPO = "facebook/sam2.1-hiera-tiny"
REVISION = "de431c4043854a71d8101e17995dfe596bf101a5"
CHECKPOINT = "sam2.1_hiera_tiny.pt"
CONFIG = "configs/sam2.1/sam2.1_hiera_t.yaml"
SAM2_COMMIT = "2b90b9f5ceec907a1c18123530e92e794ad901a4"
OPSET = 17
SIDE = 1024
IOU_PASS = 0.98
FEATS = ["high_res_feats_0", "high_res_feats_1", "image_embed"]
DEC_OUT = ["mask_single", "iou_single", "masks", "iou_predictions", "object_score_logits"]
# Every non-batch output dim is fixed; the TorchScript exporter leaves them symbolic.
SHAPES = {
    "high_res_feats_0": [32, 256, 256], "high_res_feats_1": [64, 128, 128], "image_embed": [256, 64, 64],
    "mask_single": [1, 256, 256], "iou_single": [1], "masks": [3, 256, 256], "iou_predictions": [3],
    "object_score_logits": [1],
}


def path(graph: str, precision: str) -> Path:
    return OUT / f"sam2.1-hiera-tiny-{graph}-{precision}.onnx"


def sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def load_model():
    from huggingface_hub import hf_hub_download
    from sam2.build_sam import build_sam2

    ckpt = hf_hub_download(REPO, CHECKPOINT, revision=REVISION, local_dir=WEIGHTS)
    # apply_postprocessing=True (the default) is what SAM2ImagePredictor runs with.
    return build_sam2(CONFIG, ckpt, device="cpu", mode="eval")


# --------------------------------------------------------------------------------------------
# Graphs
# --------------------------------------------------------------------------------------------


class Encoder(torch.nn.Module):
    """SAM2ImagePredictor.set_image_batch after the transforms."""

    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, images):
        out = self.model.forward_image(images)
        _, feats, _, sizes = self.model._prepare_backbone_features(out)
        feats[-1] = feats[-1] + self.model.no_mem_embed
        # [HW, B, C] -> [B, C, H, W]; -1 keeps the batch dynamic.
        f = [x.permute(1, 2, 0).reshape(-1, x.shape[2], h, w) for x, (h, w) in zip(feats, sizes)]
        return f[0], f[1], f[2]


class BoxDecoder(torch.nn.Module):
    """SAM2ImagePredictor._predict for N box prompts on one image, both mask modes at once.

    MaskDecoder.predict_masks is restated with broadcasting instead of repeat_interleave so the
    number of boxes stays a dynamic axis in the traced graph.
    """

    def __init__(self, model):
        super().__init__()
        self.pe = model.sam_prompt_encoder
        self.dec = model.sam_mask_decoder
        assert self.dec.pred_obj_scores and self.dec.use_high_res_features
        assert self.dec.dynamic_multimask_via_stability

    def forward(self, high_res_feats_0, high_res_feats_1, image_embed, boxes):
        coords = boxes.reshape(-1, 2, 2)
        labels = torch.zeros_like(coords[..., 0]) + torch.tensor([2.0, 3.0])
        sparse, dense = self.pe(points=(coords, labels), boxes=None, masks=None)

        d = self.dec
        tokens = torch.cat([d.obj_score_token.weight, d.iou_token.weight, d.mask_tokens.weight], 0)
        tokens = tokens.unsqueeze(0).expand(sparse.shape[0], -1, -1)
        tokens = torch.cat((tokens, sparse), dim=1)
        src = image_embed + dense  # [1, ...] + [N, ...] -> [N, ...]
        pos = self.pe.get_dense_pe().expand_as(src)
        _, c, h, w = src.shape
        hs, src = d.transformer(src, pos, tokens)
        iou_token = hs[:, 1, :]
        mask_tokens = hs[:, 2 : 2 + d.num_mask_tokens, :]
        src = src.transpose(1, 2).reshape(-1, c, h, w)
        dc1, ln1, act1, dc2, act2 = d.output_upscaling
        up = act1(ln1(dc1(src) + high_res_feats_1))
        up = act2(dc2(up) + high_res_feats_0)
        hyper = torch.stack([mlp(mask_tokens[:, i, :]) for i, mlp in enumerate(d.output_hypernetworks_mlps)], 1)
        _, c2, h2, w2 = up.shape
        all_masks = (hyper @ up.reshape(-1, c2, h2 * w2)).reshape(-1, d.num_mask_tokens, h2, w2)
        all_iou = d.iou_prediction_head(iou_token)
        obj = d.pred_obj_score_head(hs[:, 0, :])

        masks, iou = all_masks[:, 1:], all_iou[:, 1:]
        # _dynamic_multimask_via_stability, with gather instead of advanced indexing.
        best = torch.argmax(iou, dim=-1, keepdim=True)
        best_iou = torch.gather(iou, 1, best)
        best_mask = torch.gather(masks, 1, best[:, :, None, None].expand(-1, 1, h2, w2))
        single, single_iou = all_masks[:, 0:1], all_iou[:, 0:1]
        stable = d._get_stability_scores(single) >= d.dynamic_multimask_stability_thresh
        mask_single = torch.where(stable[..., None, None], single, best_mask)
        iou_single = torch.where(stable, single_iou, best_iou)
        return mask_single, iou_single, masks, iou, obj


def export(model) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    enc, dec = Encoder(model).eval(), BoxDecoder(model).eval()
    x = torch.randn(2, 3, SIDE, SIDE)
    with torch.no_grad():
        torch.onnx.export(
            enc, (x,), str(path("encoder", "fp32")), opset_version=OPSET, dynamo=False,
            input_names=["images"], output_names=FEATS,
            dynamic_axes={n: {0: "batch"} for n in ["images", *FEATS]},
        )
        feats = enc(x[:1])
        boxes = torch.tensor([[100.0, 120.0, 600.0, 700.0], [300.0, 50.0, 900.0, 500.0], [0, 0, 1023, 1023]])
        torch.onnx.export(
            dec, (*feats, boxes), str(path("box-decoder", "fp32")), opset_version=OPSET, dynamo=False,
            input_names=[*FEATS, "boxes"], output_names=DEC_OUT,
            dynamic_axes={"boxes": {0: "boxes"}, **{n: {0: "boxes"} for n in DEC_OUT}},
        )
    for g in ("encoder", "box-decoder"):
        p = path(g, "fp32")
        assert not list(OUT.glob("*.onnx.data")), "external data written; re-save as one file"
        m = onnx.load(str(p))
        for o in m.graph.output:
            for d, v in zip(o.type.tensor_type.shape.dim[1:], SHAPES[o.name], strict=True):
                d.Clear()
                d.dim_value = v
        onnx.save(m, str(p))
        onnx.checker.check_model(str(p), full_check=True)


def to_fp16() -> None:
    # onnxconverter_common 1.16's converter crashes with op_block_list on onnx 1.22
    # (remove_unnecessary_cast_node); ONNX Runtime ships a maintained fork of it.
    from onnxruntime.transformers import float16
    from onnxruntime.transformers.onnx_model import OnnxModel

    for g in ("encoder", "box-decoder"):
        m = float16.convert_float_to_float16(onnx.load(str(path(g, "fp32"))), keep_io_types=True)
        om = OnnxModel(m)
        om.topological_sort()  # the converter appends its input casts at the end
        onnx.save(om.model, str(path(g, "fp16")))
        onnx.checker.check_model(str(path(g, "fp16")))


# --------------------------------------------------------------------------------------------
# PyTorch reference: the sam2 package's own code path
# --------------------------------------------------------------------------------------------


@torch.no_grad()
def torch_encode(model, x: torch.Tensor) -> list[torch.Tensor]:
    """SAM2ImagePredictor.set_image_batch, from the transformed tensor on."""
    bb = model.forward_image(x)
    _, vf, _, _ = model._prepare_backbone_features(bb)
    vf[-1] = vf[-1] + model.no_mem_embed
    sizes = [(256, 256), (128, 128), (64, 64)]
    return [f.permute(1, 2, 0).view(x.shape[0], -1, *s) for f, s in zip(vf, sizes)]


@torch.no_grad()
def torch_decode(model, feats: list[torch.Tensor], boxes: torch.Tensor) -> dict[str, np.ndarray]:
    """SAM2ImagePredictor._predict with box prompts, for multimask_output False and True."""
    coords = boxes.reshape(-1, 2, 2)
    labels = torch.tensor([[2, 3]], dtype=torch.int).repeat(boxes.size(0), 1)
    sparse, dense = model.sam_prompt_encoder(points=(coords, labels), boxes=None, masks=None)
    out = {}
    for multi in (False, True):
        m, iou, _, obj = model.sam_mask_decoder(
            image_embeddings=feats[2], image_pe=model.sam_prompt_encoder.get_dense_pe(),
            sparse_prompt_embeddings=sparse, dense_prompt_embeddings=dense,
            multimask_output=multi, repeat_image=boxes.shape[0] > 1, high_res_features=feats[:2],
        )
        if multi:
            out["masks"], out["iou_predictions"] = m.numpy(), iou.numpy()
        else:
            out["mask_single"], out["iou_single"], out["object_score_logits"] = m.numpy(), iou.numpy(), obj.numpy()
    return out


# --------------------------------------------------------------------------------------------
# Data: PlantDoc detection leaves, boxes moved into the 1024 x 1024 squash frame
# --------------------------------------------------------------------------------------------


def plantdoc(n: int, seed: int = 0) -> list[tuple[Path, np.ndarray, tuple[int, int]]]:
    """n test images with their leaf boxes in the 1024 frame. Only images whose decoded size
    matches the XML's size are used, so the boxes are known to be in the right frame."""
    xmls = sorted(DATA.rglob("TEST/*.xml"))
    random.Random(seed).shuffle(xmls)
    picked = []
    for x in xmls:
        jpg = x.with_suffix(".jpg")
        if not jpg.exists():
            continue
        root = ET.parse(x).getroot()
        W, H = int(root.findtext("size/width")), int(root.findtext("size/height"))
        im = pp.load_rgb(jpg)
        if im.size != (W, H) or W == 0:
            continue
        boxes = []
        for o in root.iter("object"):
            b = o.find("bndbox")
            x0, y0, x1, y1 = (float(b.findtext(k)) for k in ("xmin", "ymin", "xmax", "ymax"))
            if x1 - x0 < 4 or y1 - y0 < 4:
                continue
            boxes.append([x0 * SIDE / W, y0 * SIDE / H, x1 * SIDE / W, y1 * SIDE / H])
        if boxes:
            picked.append((jpg, np.asarray(boxes[:16], np.float32), (H, W)))
        if len(picked) == n:
            break
    return picked


# --------------------------------------------------------------------------------------------
# Checks
# --------------------------------------------------------------------------------------------


def session(p: Path, **cuda_opts) -> ort.InferenceSession:
    opts = {"arena_extend_strategy": "kSameAsRequested", **cuda_opts}
    s = ort.InferenceSession(str(p), providers=[("CUDAExecutionProvider", opts), "CPUExecutionProvider"])
    assert s.get_providers()[0] == "CUDAExecutionProvider", s.get_providers()
    return s


def iou(a: np.ndarray, b: np.ndarray) -> float:
    u = np.logical_or(a, b).sum()
    return 1.0 if u == 0 else float(np.logical_and(a, b).sum() / u)


def upsample(logits: np.ndarray, hw: tuple[int, int]) -> np.ndarray:
    """The worker's step: bilinear straight to the image size, then > 0."""
    t = torch.from_numpy(logits.astype(np.float32))
    return (F.interpolate(t, size=hw, mode="bilinear", align_corners=False) > 0).numpy()


def check_precision(precision: str, data, x: np.ndarray, ref_feats, ref_dec, **cuda_opts) -> dict:
    enc, dec = session(path("encoder", precision), **cuda_opts), session(path("box-decoder", precision), **cuda_opts)
    feats = [np.concatenate(z) for z in zip(*[enc.run(FEATS, {"images": x[i : i + 2]}) for i in range(0, len(x), 2)])]
    feat_diff = {n: float(np.abs(f - r.numpy()).max()) for n, f, r in zip(FEATS, feats, ref_feats)}

    # Decoder on the ONNX encoder's features (the full ONNX path) and on PyTorch's features
    # (the decoder alone), both against PyTorch end to end.
    diffs = {k: 0.0 for k in DEC_OUT}
    dec_only = {k: 0.0 for k in DEC_OUT}
    ious_img, ious_256, ious_multi, iou_single = [], [], [], []
    for i, (_, boxes, hw) in enumerate(data):
        o = dict(zip(DEC_OUT, dec.run(DEC_OUT, {**{n: f[i : i + 1] for n, f in zip(FEATS, feats)}, "boxes": boxes})))
        o2 = dict(zip(DEC_OUT, dec.run(DEC_OUT, {**{n: r[i : i + 1].numpy() for n, r in zip(FEATS, ref_feats)}, "boxes": boxes})))
        r = ref_dec[i]
        for k in DEC_OUT:
            diffs[k] = max(diffs[k], float(np.abs(o[k] - r[k]).max()))
            dec_only[k] = max(dec_only[k], float(np.abs(o2[k] - r[k]).max()))
        a, b = upsample(o["mask_single"], hw), upsample(r["mask_single"], hw)
        ious_img += [iou(a[j, 0], b[j, 0]) for j in range(len(boxes))]
        ious_256 += [iou(o["mask_single"][j, 0] > 0, r["mask_single"][j, 0] > 0) for j in range(len(boxes))]
        ious_multi += [iou(o["masks"][j, c] > 0, r["masks"][j, c] > 0) for j in range(len(boxes)) for c in range(3)]
        iou_single += o["iou_single"][:, 0].tolist()
    mean_iou = float(np.mean(ious_img))
    return {
        "provider": "CUDAExecutionProvider",
        "cuda_options": {"arena_extend_strategy": "kSameAsRequested", **cuda_opts},
        "images": len(data),
        "boxes": len(ious_img),
        "encoder_max_abs_diff": feat_diff,
        "decoder_max_abs_diff_full_onnx": diffs,
        "decoder_max_abs_diff_decoder_only": dec_only,
        "mask_single_mean_iou_image_size": mean_iou,
        "mask_single_min_iou_image_size": float(np.min(ious_img)),
        "mask_single_mean_iou_256": float(np.mean(ious_256)),
        "masks_multi_mean_iou_256": float(np.mean(ious_multi)),
        "iou_single_range": [float(np.min(iou_single)), float(np.max(iou_single))],
        "iou_single_median": float(np.median(iou_single)),
        "pass": mean_iou >= IOU_PASS,
    }


def check_random_boxes(precision: str, x: np.ndarray, ref_feats, model) -> dict:
    """16 random boxes per image (mostly not one object). mask_single's stability fallback is a
    hard threshold, so near-threshold masks can flip to another token under small numeric
    changes; this counts how often (IoU < 0.9 against PyTorch, or between N=1 and N=16)."""
    rng = np.random.default_rng(1)
    dec = session(path("box-decoder", precision))
    vs_ref, vs_n1, ious = 0, 0, []
    for i in range(len(x)):
        xy, wh = rng.uniform(0, 900, (16, 2)), rng.uniform(40, 500, (16, 2))
        b = np.concatenate([xy, np.minimum(xy + wh, 1023)], 1).astype(np.float32)
        feeds = {n: r[i : i + 1].numpy() for n, r in zip(FEATS, ref_feats)}
        o = dec.run(["mask_single"], {**feeds, "boxes": b})[0]
        o1 = np.concatenate([dec.run(["mask_single"], {**feeds, "boxes": b[j : j + 1]})[0] for j in range(16)])
        r = torch_decode(model, [f[i : i + 1] for f in ref_feats], torch.from_numpy(b))["mask_single"]
        q = [iou(o[j, 0] > 0, r[j, 0] > 0) for j in range(16)]
        ious += q
        vs_ref += sum(v < 0.9 for v in q)
        vs_n1 += sum(iou(o[j, 0] > 0, o1[j, 0] > 0) < 0.9 for j in range(16))
    return {"boxes": len(ious), "mean_iou_vs_pytorch": float(np.mean(ious)),
            "flips_vs_pytorch": int(vs_ref), "flips_n1_vs_n16": int(vs_n1)}


def check_dynamic(precision: str, x: np.ndarray) -> dict:
    """Nothing bakes the trace's batch into the graph: per-item results agree across sizes.
    x: two real images; the 16 boxes are random."""
    rng = np.random.default_rng(0)
    enc, dec = session(path("encoder", precision)), session(path("box-decoder", precision))
    b2 = enc.run(FEATS, {"images": x})
    b1 = [enc.run(FEATS, {"images": x[i : i + 1]}) for i in range(2)]
    enc_spread = max(float(np.abs(b2[k][i] - b1[i][k][0]).max()) for i in range(2) for k in range(3))
    shapes = {"encoder": {str(b): [list(t.shape) for t in (b2 if b == 2 else b1[0])] for b in (1, 2)}}
    feeds = {n: f[:1] for n, f in zip(FEATS, b2)}
    xy = rng.uniform(0, 900, (16, 2))
    wh = rng.uniform(40, 500, (16, 2))
    boxes = np.concatenate([xy, np.minimum(xy + wh, 1023)], 1).astype(np.float32)
    full = dec.run(DEC_OUT, {**feeds, "boxes": boxes})
    dec_spread, shapes["box_decoder"] = {k: 0.0 for k in DEC_OUT}, {}
    for n in (1, 5, 16):
        o = dec.run(DEC_OUT, {**feeds, "boxes": boxes[:n]})
        shapes["box_decoder"][str(n)] = [list(t.shape) for t in o]
        for k, a, f in zip(DEC_OUT, o, full):
            dec_spread[k] = max(dec_spread[k], float(np.abs(a - f[:n]).max()))
    return {"encoder_b1_vs_b2_max_diff": enc_spread, "decoder_n1_n5_vs_n16_max_diff": dec_spread, "shapes": shapes}


# --------------------------------------------------------------------------------------------
# GPU memory: one fresh process per measurement, nvidia-smi polled for that process
# --------------------------------------------------------------------------------------------


def _pid_mib(pid: int) -> int:
    q = subprocess.run(["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
                       capture_output=True, text=True).stdout
    return sum(int(m) for p, m in (l.split(",") for l in q.strip().splitlines() if l) if int(p) == pid)


def measure_child(graph: str, precision: str, n: int) -> None:
    """Runs in its own process; prints JSON with MiB after loading and the peak during runs."""
    pid, peak, stop = os.getpid(), [0], threading.Event()

    def poll():
        while not stop.is_set():
            peak[0] = max(peak[0], _pid_mib(pid))
            time.sleep(0.02)

    s = session(path(graph, precision))
    time.sleep(0.5)
    loaded = _pid_mib(pid)
    t = threading.Thread(target=poll, daemon=True)
    t.start()
    rng = np.random.default_rng(0)
    if graph == "encoder":
        feeds = {"images": rng.standard_normal((n, 3, SIDE, SIDE)).astype(np.float32)}
    else:
        shapes = [(1, 32, 256, 256), (1, 64, 128, 128), (1, 256, 64, 64)]
        feeds = {k: rng.standard_normal(sh).astype(np.float32) for k, sh in zip(FEATS, shapes)}
        feeds["boxes"] = np.tile(np.array([[100, 100, 600, 700]], np.float32), (n, 1))
    for _ in range(3):
        s.run(None, feeds)
    time.sleep(0.3)
    stop.set()
    t.join()
    peak[0] = max(peak[0], _pid_mib(pid))
    print(json.dumps({"loaded_mib": loaded, "peak_mib": peak[0], "peak_minus_loaded_mib": peak[0] - loaded}))


def measure(graph: str, precision: str, n: int) -> dict:
    r = subprocess.run([sys.executable, "-m", "ml.training.export.sam2", "--measure", graph, precision, str(n)],
                       capture_output=True, text=True, cwd=ROOT)
    if r.returncode != 0:
        return {"error": r.stderr[-2000:]}
    return json.loads(r.stdout.strip().splitlines()[-1])


# --------------------------------------------------------------------------------------------


def io_spec(p: Path) -> dict:
    m = onnx.load(str(p), load_external_data=False)

    def dims(v):
        return [d.dim_param or d.dim_value for d in v.type.tensor_type.shape.dim]

    def dtype(v):
        return onnx.helper.tensor_dtype_to_np_dtype(v.type.tensor_type.elem_type).name

    return {
        "opset": next(o.version for o in m.opset_import if o.domain in ("", "ai.onnx")),
        "inputs": [{"name": v.name, "shape": dims(v), "dtype": dtype(v)} for v in m.graph.input],
        "outputs": [{"name": v.name, "shape": dims(v), "dtype": dtype(v)} for v in m.graph.output],
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=20, help="PlantDoc test images")
    ap.add_argument("--skip-export", action="store_true")
    ap.add_argument("--measure", nargs=3, metavar=("GRAPH", "PRECISION", "N"))
    args = ap.parse_args()
    if args.measure:
        measure_child(args.measure[0], args.measure[1], int(args.measure[2]))
        return

    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    model = load_model()
    if not args.skip_export:
        export(model)
        to_fp16()

    data = plantdoc(args.n)
    x = pp.batch([pp.load_rgb(p) for p, _, _ in data], pp.SAM2)
    ref_feats = [torch.cat(z) for z in zip(*[torch_encode(model, torch.from_numpy(x[i : i + 2])) for i in range(0, len(x), 2)])]
    ref_dec = [torch_decode(model, [f[i : i + 1] for f in ref_feats], torch.from_numpy(b)) for i, (_, b, _) in enumerate(data)]

    report = {
        "model": REPO,
        "revision": REVISION,
        "checkpoint": CHECKPOINT,
        "checkpoint_sha256": sha256(WEIGHTS / CHECKPOINT),
        "code": f"facebookresearch/sam2@{SAM2_COMMIT} ({CONFIG}, build_sam2 apply_postprocessing=True)",
        "exporter": f"torch.onnx.export(dynamo=False), torch {torch.__version__}",
        "onnxruntime": ort.__version__,
        "reference": "PyTorch fp32 on CPU, sam2's own encoder and decoder code path",
        "preprocessing": {"side": SIDE, "resize": "squash", "mean": list(pp.IMAGENET_MEAN), "std": list(pp.IMAGENET_STD)},
        "data": f"PlantDoc detection TEST split, {len(data)} images (XML size == decoded size), up to 16 leaf boxes each",
        "pass_condition": f"mean IoU of mask_single > 0 at image size, ONNX vs PyTorch >= {IOU_PASS}",
        "results": {},
        "files": {},
        "gpu_memory_mib": {},
    }
    for precision in ("fp32", "fp16"):
        res = check_precision(precision, data, x, ref_feats, ref_dec)
        res["dynamic_axes"] = check_dynamic(precision, x[:2])
        res["random_boxes"] = check_random_boxes(precision, x, ref_feats, model)
        report["results"][precision] = res
        print(precision, json.dumps(res, indent=2), flush=True)
    # ORT's CUDA EP runs fp32 MatMul/Conv in TF32 by default; this isolates the export itself.
    report["results"]["fp32_no_tf32"] = check_precision("fp32", data, x, ref_feats, ref_dec, use_tf32=0)
    report["shipped_precision"] = "fp16" if report["results"]["fp16"]["pass"] else "fp32"
    for precision in ("fp32", "fp16"):
        for g in ("encoder", "box-decoder"):
            p = path(g, precision)
            report["files"][p.name] = {"bytes": p.stat().st_size, "sha256": sha256(p), "precision": precision,
                                       "io_dtype": "float32", **io_spec(p)}
        report["gpu_memory_mib"][precision] = {
            "encoder_b1": measure("encoder", precision, 1),
            "encoder_b2": measure("encoder", precision, 2),
            "box_decoder_n16": measure("box-decoder", precision, 16),
        }
    report["gpu_memory_method"] = ("nvidia-smi per-process used_memory polled every 20 ms in a fresh process per "
                                   "measurement; CUDA EP with arena_extend_strategy=kSameAsRequested, other options "
                                   "default; loaded_mib includes the CUDA context and weights")
    (OUT / "check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in ("shipped_precision", "gpu_memory_mib")}, indent=2))


if __name__ == "__main__":
    main()
