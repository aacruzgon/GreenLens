"""Leaf crops from our own detector, for the heads (training.md §4 step 5 and §6; M13).

    python -m ml.training.heads.crops [--splits train val test] [--limit N]
    python -m ml.training.heads.crops --splits train --views 4

The worker's disease stage (backend vision `perception.rs`, `classify`) feeds the head one crop
per leaf the detector keeps on a close-up, or the whole close-up when it keeps none. This runs
the same steps on every photo of the committed splits, each taken as a leaf close-up:

- the photo is stored with its long side at most 1,600 px (`UploadConfig::default_long_side`);
- the shipped leaf detector (`manifest.toml`, fp16 on CUDA) runs on it, squashed to its input;
- `decode` as `vision::perception::detector::decode`: sigmoid at least `score_threshold`, both
  sides at least `min_side_px` in the stored photo, clamped, sorted by score, class-agnostic NMS
  at `nms_iou`, at most `max_per_image`;
- each box grows by the disease head's `crop_grow` on every side and is cropped from the stored
  photo (`pdm_vision.preprocess.crop_box`), then embedded by BioCLIP 2 as the worker embeds it.

The worker's leaf regions carry the detector's own boxes (`perception.rs`, the `Leaf` regions),
so no mask model is needed here.

Cached per photo (by its SHA-256) under data/cache/crops/<detector>-<bioclip>-<settings>/, where
<settings> hashes every setting that changes a crop (the detector's `score_threshold`,
`min_side_px`, `nms_iou` and `max_per_image`, the stored long side, `crop_grow` and the crop
rounding; `settings.json` beside the files lists them), as `.npz` with
`boxes` `[n, 4]` (normalised x, y, w, h), `scores` `[n]` and `emb` `[n, 768]`, so a head trains
from the cache in seconds. A photo with no kept leaf has n = 0; the head then uses its whole-photo
embedding, as the worker does for a close-up.

`--views N` (training.md §6, augmentation) embeds N augmented copies of each cached crop
(`train_disease.augment`: random resized crop 0.7-1.0, flip, colour jitter, mild blur), or of
the whole stored photo when no leaf was kept, under views<N>/ beside the crops, as `emb`
`[n_crops * N, 768]`, crop by crop. The random generator is seeded by the photo's hash, so a rerun
gives the same views.
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np

from ml.pdm_vision import preprocess as pp
from ml.pdm_vision.features import Embedder, cuda_session, file_sha256

ROOT = Path(__file__).resolve().parents[3]
SPLITS = ROOT / "ml/training/data/splits"
RAW = ROOT / "ml/training/data/raw"
CACHE = ROOT / "ml/training/data/cache/crops"
STORED_LONG_SIDE = 1600
# Bumped whenever `pp.crop_box` changes how a box becomes pixels. 2: the worker's floor and ceil.
CROP_ROUNDING = 2


def iou(a: np.ndarray, b: np.ndarray) -> float:
    """IoU of two normalised `[x, y, w, h]` boxes."""
    ix = max(0.0, min(a[0] + a[2], b[0] + b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[1] + a[3], b[1] + b[3]) - max(a[1], b[1]))
    inter = ix * iy
    union = a[2] * a[3] + b[2] * b[3] - inter
    return float(inter / union) if union > 0 else 0.0


def decode(dets: np.ndarray, logits: np.ndarray, stored: tuple[int, int], cfg: dict) -> tuple[np.ndarray, np.ndarray]:
    """`vision::perception::detector::decode` for one photo: `dets` `[queries, 4]` (normalised
    cx, cy, w, h), `logits` `[queries, classes]` with `leaf` at index 0. Returns the kept boxes
    as normalised `[x, y, w, h]` and their scores, best first."""
    scores = (1.0 / (1.0 + np.exp(-logits[:, 0].astype(np.float32)))).astype(np.float32)
    d = dets.astype(np.float32)
    iw, ih = np.float32(stored[0]), np.float32(stored[1])
    keep = (scores >= np.float32(cfg["score_threshold"])) & (np.minimum(d[:, 2] * iw, d[:, 3] * ih) >= np.float32(cfg["min_side_px"]))
    found = []
    for q in np.flatnonzero(keep):
        cx, cy, w, h = d[q]
        x0, y0 = np.clip(cx - w / 2, 0, 1), np.clip(cy - h / 2, 0, 1)
        x1, y1 = np.clip(cx + w / 2, 0, 1), np.clip(cy + h / 2, 0, 1)
        # BBox::new refuses only non-finite values or a box outside the image, so a box clamped
        # to zero width or height keeps its place and can take a `max_per_image` slot.
        if np.isfinite([x0, y0, x1, y1]).all():
            found.append((scores[q], np.array([x0, y0, x1 - x0, y1 - y0], np.float64)))
    found.sort(key=lambda f: -f[0])     # stable, as Rust's sort_by
    kept: list[tuple[np.float32, np.ndarray]] = []
    for s, b in found:
        if len(kept) == cfg["max_per_image"]:
            break
        if all(iou(k, b) <= cfg["nms_iou"] for _, k in kept):
            kept.append((s, b))
    if not kept:
        return np.zeros((0, 4), np.float32), np.zeros(0, np.float32)
    return np.stack([b for _, b in kept]).astype(np.float32), np.asarray([s for s, _ in kept], np.float32)


def read_split(name: str) -> list[dict]:
    rows = []
    for line in (SPLITS / f"{name}.tsv").read_text().splitlines()[1:]:
        dataset, path, species, problem = line.split("\t")
        rows.append({"dataset": dataset, "path": RAW / path, "species_id": species, "problem_id": problem})
    return rows


class CropEmbedder:
    """The shipped detector and BioCLIP 2, producing each photo's leaf crops and their embeddings."""

    def __init__(self, crop_grow: float | None = None, gpu: bool = True):
        """`gpu=False` opens no detector session, for reading the cache (`load`, `load_views`):
        a trainer then holds no GPU memory, so a VLM server can start beside it."""
        import tomllib
        from types import SimpleNamespace

        import onnxruntime as ort

        from ml.training.eval.leaf_scores import MODELS, LeafScorer, detector_entry

        if gpu:
            ort.preload_dlls()   # before LeafScorer's own session, which would otherwise log a CUDA failure
            self.det = LeafScorer(cuda=True)
        else:
            entry = detector_entry(MODELS / "manifest.toml")
            self.det = SimpleNamespace(manifest=MODELS / "manifest.toml", entry=entry, cfg=entry["config"],
                                       model_file=MODELS / "files" / entry["file"])
        self.cfg = self.det.cfg
        manifest = tomllib.loads(self.det.manifest.read_text())["model"]
        heads = [e for e in manifest if e["name"].startswith("disease_head")]
        self.crop_grow = crop_grow if crop_grow is not None else float(heads[0]["config"]["crop_grow"])
        self.emb = Embedder()
        if gpu:
            # On CUDA or not at all: LeafScorer's session falls back to the CPU when the CUDA
            # libraries aren't preloaded, which would give crops the worker never sees.
            self.det.sess = cuda_session(self.det.model_file)
            # The fp16 exports keep fp32 inputs and outputs, so the session says what it takes.
            self.in_dtype = np.float16 if self.det.sess.get_inputs()[0].type == "tensor(float16)" else np.float32
        self.det_sha = file_sha256(self.det.model_file)[:16]
        self.settings = {
            "detector_sha256_16": self.det_sha, "embedder": self.emb.cache_dir.name,
            **{k: self.cfg[k] for k in ("score_threshold", "min_side_px", "nms_iou", "max_per_image")},
            "stored_long_side": STORED_LONG_SIDE, "crop_grow": self.crop_grow, "crop_rounding": CROP_ROUNDING,
        }
        key = hashlib.sha256(json.dumps(self.settings, sort_keys=True).encode()).hexdigest()[:10]
        self.dir = CACHE / f"{self.det_sha}-{self.emb.cache_dir.name}-{key}"

    def path(self, key: str) -> Path:
        return self.dir / f"{key}.npz"

    def _prepare(self, path: Path):
        im = pp.load_rgb(path)
        stored = pp.resize(im, *pp.fit_long_side(im.size, STORED_LONG_SIDE))
        return stored, pp.batch([stored], self.det.recipe)[0]

    def run(self, paths: list[Path], workers: int = 12) -> int:
        """Computes and caches every photo not cached yet; returns how many it computed."""
        self.dir.mkdir(parents=True, exist_ok=True)
        (self.dir / "settings.json").write_text(json.dumps(self.settings, indent=1, sort_keys=True) + "\n")
        keys = [file_sha256(p) for p in paths]
        todo = [(p, k) for p, k in zip(paths, keys) if not self.path(k).exists()]
        step = int(self.cfg.get("max_batch", 8))
        with ThreadPoolExecutor(workers) as pool:
            for s in range(0, len(todo), step):
                chunk = todo[s : s + step]
                prepared = list(pool.map(lambda pk: self._prepare(pk[0]), chunk))
                x = np.stack([t for _, t in prepared]).astype(self.in_dtype)
                dets, logits = self.det.sess.run(self.det.out_names, {self.det.in_name: x})
                for (path, key), (stored, _), d, lg in zip(chunk, prepared, dets, logits):
                    boxes, scores = decode(d, lg, stored.size, self.cfg)
                    crops = [pp.crop_box(stored, tuple(b), self.crop_grow) for b in boxes]
                    emb = self.emb.images(crops) if crops else np.zeros((0, 768), np.float32)
                    np.savez(self.path(key), boxes=boxes, scores=scores, emb=emb.astype(np.float32))
                if (s // step) % 50 == 0:
                    print(f"  {min(s + step, len(todo))}/{len(todo)} photos", file=sys.stderr, flush=True)
        return len(todo)

    def views_dir(self, views: int) -> Path:
        return self.dir / f"views{views}"

    def run_views(self, paths: list[Path], views: int, workers: int = 12) -> int:
        """Embeds `views` augmented copies of every cached crop of each photo (`run` first)."""
        from ml.training.heads.train_disease import augment

        out = self.views_dir(views)
        out.mkdir(parents=True, exist_ok=True)
        todo = [(p, k) for p in paths if not (out / f"{(k := file_sha256(p))}.npz").exists()]

        def prepare(pk):
            path, key = pk
            with np.load(self.path(key)) as z:
                boxes = z["boxes"]
            im = pp.load_rgb(path)
            stored = pp.resize(im, *pp.fit_long_side(im.size, STORED_LONG_SIDE))
            crops = [pp.crop_box(stored, tuple(b), self.crop_grow) for b in boxes] or [stored]
            rng = np.random.default_rng(int(key[:12], 16))
            return key, [augment(c, rng) for c in crops for _ in range(views)]

        step = 64
        with ThreadPoolExecutor(workers) as pool:
            for s in range(0, len(todo), step):
                for key, images in pool.map(prepare, todo[s : s + step]):
                    np.savez(out / f"{key}.npz", emb=self.emb.images(images).astype(np.float32))
                if (s // step) % 10 == 0:
                    print(f"  views: {min(s + step, len(todo))}/{len(todo)} photos", file=sys.stderr, flush=True)
        return len(todo)

    def load_views(self, paths: list[Path], views: int) -> list[np.ndarray]:
        out = self.views_dir(views)
        return [np.load(out / f"{file_sha256(p)}.npz")["emb"] for p in paths]

    def load(self, paths: list[Path]) -> list[dict]:
        out = []
        for p in paths:
            with np.load(self.path(file_sha256(p))) as z:
                out.append({"boxes": z["boxes"], "scores": z["scores"], "emb": z["emb"]})
        return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--splits", nargs="+", default=["train", "val", "test"])
    ap.add_argument("--limit", type=int, help="only the first N photos of each split (a smoke run)")
    ap.add_argument("--views", type=int, default=0, help="also embed N augmented views of each crop")
    args = ap.parse_args(argv)
    ce = CropEmbedder()
    print(f"detector {ce.det.entry['version']} ({ce.det_sha}), crop_grow {ce.crop_grow}, cache {ce.dir}", file=sys.stderr)
    for split in args.splits:
        rows = read_split(split)[: args.limit]
        n = ce.run([r["path"] for r in rows])
        crops = ce.load([r["path"] for r in rows])
        if args.views:
            nv = ce.run_views([r["path"] for r in rows], args.views)
            print(f"{split}: {args.views} views of every crop ({nv} photos computed)")
        per = collections.defaultdict(list)
        for r, c in zip(rows, crops):
            per[r["dataset"]].append(len(c["scores"]))
        print(f"{split}: {len(rows)} photos ({n} computed), {sum(map(len, per.values()))} rows")
        for d, ns in sorted(per.items()):
            ns = np.asarray(ns)
            print(f"   {d:22s} {len(ns):5d} photos, {ns.sum():6d} crops, median {np.median(ns):.0f}, "
                  f"no leaf kept {(ns == 0).mean():.1%}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
