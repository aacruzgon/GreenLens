"""Embeds knowledge chunks with the pinned text model (training.md §10, step 3; the recipe in
models.md §10), the same ONNX file and tokenizer `api` queries with.

    python3 -m ml.models.fetch --for ingest --dest ~/.cache/pdm-models   # once
    python3 -m ml.knowledge.embed "a document" "another"                 # prints the first values

The model is read from $MODEL_MANIFEST (default ~/.cache/pdm-models/manifest.toml). Documents
get no prefix; queries get QUERY_PREFIX, which the loader never uses. fp32 throughout, so a
text's vector doesn't depend on its batch. Vectors are cached in ml/knowledge/.cache under
(embedding_model, sha256(text)), so an unchanged chunk isn't embedded again.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import tomllib
from pathlib import Path

import numpy as np

from ml.knowledge.seed import ROOT

NAME = "snowflake-arctic-embed-xs"   # the stored name's stem, as the Rust embedder writes it
DIMENSION = 384
QUERY_PREFIX = "Represent this sentence for searching relevant passages: "
BATCH = 64
CACHE = ROOT / "ml/knowledge/.cache/embeddings.jsonl"


def default_manifest() -> Path:
    return Path(os.environ.get("MODEL_MANIFEST",
                               Path.home() / ".cache/pdm-models/manifest.toml"))


class Embedder:
    """The pinned text embedder. `name` is what every stored vector records."""

    def __init__(self, manifest: Path | None = None):
        import onnxruntime as ort
        from tokenizers import Tokenizer

        manifest = manifest or default_manifest()
        if not manifest.is_file():
            raise FileNotFoundError(
                f"{manifest} not found: run python3 -m ml.models.fetch --for ingest "
                "--dest ~/.cache/pdm-models, or set MODEL_MANIFEST")
        entries = {m["name"]: m for m in tomllib.loads(manifest.read_text())["model"]}
        model, tok = entries["text_embedder"], entries["text_embedder_tokenizer"]
        self.name = f"{NAME}@{model['version']}"
        self.tokenizer = Tokenizer.from_file(str(manifest.parent / tok["file"]))
        self.tokenizer.enable_truncation(max_length=512)
        self.tokenizer.enable_padding(pad_id=0, pad_token="[PAD]")
        opts = ort.SessionOptions()
        opts.log_severity_level = 3
        self.session = ort.InferenceSession(str(manifest.parent / model["file"]), opts,
                                            providers=["CPUExecutionProvider"])

    def embed_documents(self, texts: list[str]) -> np.ndarray:
        """`[len(texts), 384]` fp32, each row unit length (CLS pooling, models.md §10)."""
        rows = []
        for i in range(0, len(texts), BATCH):
            enc = self.tokenizer.encode_batch(texts[i:i + BATCH])
            ids = np.array([e.ids for e in enc], dtype=np.int64)
            mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
            (hidden,) = self.session.run(["last_hidden_state"], {
                "input_ids": ids, "attention_mask": mask, "token_type_ids": np.zeros_like(ids)})
            cls = hidden[:, 0, :].astype(np.float32)
            rows.append(cls / np.maximum(np.linalg.norm(cls, axis=1, keepdims=True), 1e-12))
        return np.concatenate(rows) if rows else np.zeros((0, DIMENSION), np.float32)


def text_hash(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


class CachedEmbedder:
    """An embedder behind the vector cache. `embedder` is built only when a text is missing."""

    def __init__(self, make=Embedder, cache: Path = CACHE, name: str | None = None):
        self.make, self.cache, self._embedder = make, cache, None
        self._name = name
        self.vectors: dict[tuple[str, str], list[float]] = {}
        if cache.is_file():
            for line in cache.read_text().splitlines():
                if line.strip():
                    r = json.loads(line)
                    self.vectors[(r["model"], r["hash"])] = r["vector"]

    @property
    def embedder(self):
        if self._embedder is None:
            self._embedder = self.make()
        return self._embedder

    @property
    def name(self) -> str:
        if self._name is None:
            self._name = self.embedder.name
        return self._name

    def embed_documents(self, texts: list[str]) -> list[list[float]]:
        keys = [(self.name, text_hash(t)) for t in texts]
        missing = sorted({k[1]: t for k, t in zip(keys, texts) if k not in self.vectors}.items())
        if missing:
            vecs = self.embedder.embed_documents([t for _, t in missing])
            self.cache.parent.mkdir(parents=True, exist_ok=True)
            with self.cache.open("a") as f:
                for (h, _), v in zip(missing, vecs):
                    row = [float(x) for x in np.asarray(v, np.float32)]
                    self.vectors[(self.name, h)] = row
                    f.write(json.dumps({"model": self.name, "hash": h, "vector": row}) + "\n")
        return [self.vectors[k] for k in keys]


def main() -> int:
    e = Embedder()
    for t, v in zip(sys.argv[1:], e.embed_documents(sys.argv[1:])):
        print(f"{e.name} {t!r}: {[round(float(x), 5) for x in v[:8]]} norm {np.linalg.norm(v):.6f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
