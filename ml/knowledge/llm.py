"""The hosted model for the ingestion's model steps (training.md §10): tagging and extraction.

One structured-output call at a time, through the official SDK, with the key api uses
(LLM_API_KEY; `set -a; . infra/dev/.env; set +a` loads it). The schema goes through the same
sanitiser as the fallback worker and the plan writer (`integrations::anthropic::sanitize_schema`,
run as a cargo example, as the dose check is), because the API refuses keywords it can't enforce.

Every reply is cached under the SHA-256 of the whole request in ml/knowledge/.cache/llm.jsonl, so
a rerun with the same chunks, prompt and schema costs nothing and gives the same drafts. Usage is
added up per model, so each script can print what it cost. The model that answered is recorded
with each reply (`served`): Opus 5.5's server-side fallback can hand a request to another model,
and the review files and the cost name that one.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

from ml.knowledge.seed import ROOT

BACKEND = ROOT / "backend"
CACHE = ROOT / "ml/knowledge/.cache/llm.jsonl"
TAG_MODEL = "claude-sonnet-5-5"
EXTRACT_MODEL = "claude-opus-5-5"
# USD per million tokens (input, output), the API's list prices on 2026-10-06. Cache reads cost a
# tenth of the input price, cache writes a quarter more.
PRICES = {"claude-opus-5-5": (4.00, 20.00), "claude-sonnet-5-5": (2.00, 10.00),
          "claude-haiku-5-5": (0.10, 0.50)}
# Opus 5.5 can decline a request in a safety category; the server-side fallback re-runs it on the
# model the API picks for that category, inside the same call. Haiku 5.5 has no fallback.
FALLBACK_BETA = "server-side-fallback-2026-07-01"
FALLBACK_MODELS = {"claude-opus-5-5"}


class LlmError(Exception):
    pass


class LlmCutOff(LlmError):
    """The reply hit max_tokens."""


def sanitize(schema: dict) -> dict:
    """The schema as the API accepts it (`cargo run -p integrations --example sanitize_schema`)."""
    try:
        run = subprocess.run(
            ["cargo", "run", "-q", "-p", "integrations", "--example", "sanitize_schema"],
            cwd=BACKEND, input=json.dumps(schema), capture_output=True, text=True, check=False)
    except FileNotFoundError as e:
        raise LlmError("cargo is not installed (setup: backend role)") from e
    if run.returncode != 0:
        raise LlmError(f"sanitize_schema: {run.stderr.strip() or run.returncode}")
    return json.loads(run.stdout)["schema"]


@dataclass
class Usage:
    calls: int = 0
    cached: int = 0
    tokens: dict = field(default_factory=dict)    # model → [input, output, cache_read, cache_write]

    def add(self, model: str, u) -> None:
        t = self.tokens.setdefault(model, [0, 0, 0, 0])
        t[0] += u.input_tokens or 0
        t[1] += u.output_tokens or 0
        t[2] += getattr(u, "cache_read_input_tokens", 0) or 0
        t[3] += getattr(u, "cache_creation_input_tokens", 0) or 0

    def cost(self) -> float:
        total = 0.0
        for model, (i, o, cr, cw) in self.tokens.items():
            # A model missing from PRICES (one a fallback answered with) is priced as the dearest,
            # not as free, so the report never understates a run.
            pin, pout = PRICES.get(model, max(PRICES.values()))
            total += (i * pin + o * pout + cr * pin * 0.1 + cw * pin * 1.25) / 1e6
        return total

    def report(self) -> str:
        unpriced = " (not in PRICES: priced as the dearest model)"
        parts = [f"{m}: {i} in, {o} out, {cr} cache read{'' if m in PRICES else unpriced}"
                 for m, (i, o, cr, _) in self.tokens.items()]
        return (f"{self.calls} calls ({self.cached} from the cache); "
                + ("; ".join(parts) or "no tokens") + f"; about ${self.cost():.2f}")


class Llm:
    """Structured calls with a reply cache. `client` is an `anthropic.Anthropic` or a fake."""

    def __init__(self, client=None, cache: Path = CACHE):
        self._client = client
        self.cache_path = cache
        self.usage = Usage()
        self.served: str | None = None    # the model that answered the last call
        self._cache: dict[str, tuple[dict, str]] = {}
        if cache.exists():
            for line in cache.read_text(encoding="utf-8").splitlines():
                if line.strip():
                    r = json.loads(line)
                    self._cache[r["key"]] = (r["reply"], r.get("served") or r["model"])

    @property
    def client(self):
        if self._client is None:
            import anthropic
            key = os.environ.get("LLM_API_KEY")
            if not key:
                raise LlmError("LLM_API_KEY is not set: set -a; . infra/dev/.env; set +a")
            self._client = anthropic.Anthropic(api_key=key, max_retries=4)
        return self._client

    def call(self, *, model: str, system: str, user: str, schema: dict, effort: str,
             max_tokens: int, fresh: bool = False) -> dict:
        """The reply to one request, as the JSON object the schema describes. `fresh` asks the
        model again instead of reading the cache, and caches the new reply. `self.served` names
        the model that answered it."""
        request = {"model": model, "system": system, "user": user, "schema": schema,
                   "effort": effort, "max_tokens": max_tokens}
        key = hashlib.sha256(json.dumps(request, sort_keys=True).encode()).hexdigest()
        self.usage.calls += 1
        if key in self._cache and not fresh:
            self.usage.cached += 1
            reply, self.served = self._cache[key]
            return reply
        kwargs = dict(
            model=model, max_tokens=max_tokens,
            # The system prompt repeats across a script's calls; cache it.
            system=[{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
            messages=[{"role": "user", "content": user}],
            output_config={"effort": effort, "format": {"type": "json_schema", "schema": schema}})
        if model in FALLBACK_MODELS:
            msg = self.client.beta.messages.create(betas=[FALLBACK_BETA], fallbacks="default", **kwargs)
        else:
            msg = self.client.messages.create(**kwargs)
        self.served = getattr(msg, "model", None) or model
        self.usage.add(self.served, msg.usage)
        if msg.stop_reason == "refusal":
            raise LlmError(f"{model} declined the request ({getattr(msg, 'stop_details', None)})")
        if msg.stop_reason == "max_tokens":
            raise LlmCutOff(f"{model} ran out of tokens at {max_tokens}")
        text = next((b.text for b in msg.content if b.type == "text"), None)
        if text is None:
            raise LlmError(f"{model} returned no text block")
        reply = json.loads(text)
        self._cache[key] = (reply, self.served)
        self.cache_path.parent.mkdir(parents=True, exist_ok=True)
        with self.cache_path.open("a", encoding="utf-8") as f:
            f.write(json.dumps({"key": key, "model": model, "served": self.served, "reply": reply},
                               ensure_ascii=False) + "\n")
        return reply
