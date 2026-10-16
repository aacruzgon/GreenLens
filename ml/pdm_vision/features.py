"""BioCLIP 2 image embeddings with the ONNX file the worker loads, cached by image hash
(training.md §6, Features). The cache key includes the model file's SHA-256, so a new export
never mixes with old vectors."""

from __future__ import annotations

import hashlib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image

from . import preprocess as pp

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MODEL = ROOT / "ml/models/files/bioclip/bioclip-2-vision-fp16.onnx"
CACHE = ROOT / "ml/training/data/cache/features"


def preprocess_key(recipe: pp.Recipe = pp.BIOCLIP) -> str:
    """Everything about the pre-processing that changes a vector: the resize filter and the
    recipe. Part of the cache key, so a change (issue #2) can't silently reuse old vectors."""
    spec = f"{pp.RESIZE_FILTER}|{recipe.side}|{recipe.resize.value}|{recipe.mean}|{recipe.std}|{recipe.bgr}"
    return hashlib.sha256(spec.encode()).hexdigest()[:8]


def cuda_session(model: Path) -> ort.InferenceSession:
    """A session on the CUDA execution provider, or an error: never a silent CPU fallback
    (the worker uses `error_on_failure` for the same reason, README 5). The CUDA and cuDNN
    libraries come from the NVIDIA pip wheels, which ONNX Runtime loads only on request."""
    ort.preload_dlls()
    opts = ort.SessionOptions()
    opts.log_severity_level = 3
    sess = ort.InferenceSession(str(model), opts, providers=["CUDAExecutionProvider"])
    if sess.get_providers()[0] != "CUDAExecutionProvider":
        raise RuntimeError(f"{model}: CUDA execution provider failed to load")
    return sess


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


class Embedder:
    def __init__(self, model: Path = DEFAULT_MODEL, batch: int = 32):
        self.model = Path(model)
        self.model_sha = file_sha256(self.model)[:16]
        self.batch = batch
        self._sess = None
        self.cache_dir = CACHE / f"{self.model_sha}-{preprocess_key()}"
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    @property
    def sess(self) -> ort.InferenceSession:
        """Opened on first use, so a run served entirely from the cache needs no GPU memory."""
        if self._sess is None:
            self._sess = cuda_session(self.model)
        return self._sess

    def run(self, tensors: np.ndarray) -> np.ndarray:
        return self.sess.run(["embeddings"], {"images": tensors})[0]

    def images(self, images: list[Image.Image]) -> np.ndarray:
        """Embeddings of in-memory images (crops, augmented views), uncached."""
        out = []
        for i in range(0, len(images), self.batch):
            out.append(self.run(pp.batch(images[i : i + self.batch], pp.BIOCLIP)))
        return np.concatenate(out) if out else np.zeros((0, 768), np.float32)

    def files(self, paths: list[Path], workers: int = 12) -> np.ndarray:
        """Embeddings of image files, `[n, 768]`, cached per file content."""
        keys = [file_sha256(p) for p in paths]
        out = np.zeros((len(paths), 768), np.float32)
        todo = []
        for i, k in enumerate(keys):
            f = self.cache_dir / f"{k}.npy"
            if f.exists():
                out[i] = np.load(f)
            else:
                todo.append(i)
        with ThreadPoolExecutor(workers) as pool:
            for s in range(0, len(todo), self.batch):
                idx = todo[s : s + self.batch]
                tensors = np.stack(list(pool.map(lambda i: pp.to_tensor(pp.load_rgb(paths[i]), pp.BIOCLIP), idx)))
                emb = self.run(tensors)
                for j, i in enumerate(idx):
                    out[i] = emb[j]
                    np.save(self.cache_dir / f"{keys[i]}.npy", emb[j])
        return out
