"""Step 1 of the ingestion (training.md §10): fetches every text source in seed/sources.yaml.

    python3 -m ml.knowledge.fetch              # fetch what isn't in ml/knowledge/raw/ yet
    python3 -m ml.knowledge.fetch --refresh    # fetch everything again

Each source is one document. Its page goes to ml/knowledge/raw/<id>.html as served, with
raw/<id>.json beside it (the final URL, the status, the time and the SHA-256), so chunking can
run again without the network. raw/ is not committed: the seed keeps excerpts only as far as
each source's terms allow, and none from a source with no reuse grant (architecture 5, licensing).

The fetch is polite and never works around a refusal: it names itself in User-Agent, obeys the
host's robots.txt, waits between requests to the same host, and treats any status but 200 as a
failure to report, not to retry. A source whose terms were not checked has no place in
sources.yaml, so nothing here decides what may be fetched. Photo sources (those a species'
reference_images names) aren't documents and are skipped.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
import urllib.robotparser
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit

import httpx

from ml.knowledge.seed import ROOT, SEED, Seed, load

RAW = ROOT / "ml/knowledge/raw"
USER_AGENT = "GreenLens-knowledge-fetch/0.1 (+https://github.com/aacruzgon/GreenLens)"
DELAY_S = 2.0          # between two requests to the same host
TIMEOUT_S = 30.0


class FetchError(Exception):
    pass


def text_sources(seed: Seed) -> list[dict]:
    """The sources that are documents, in id order."""
    photo = {img["source_id"] for s in seed.species for img in s.get("reference_images") or []}
    return sorted((s for s in seed.sources if s["id"] not in photo), key=lambda s: s["id"])


class Fetcher:
    """One client, one robots.txt per host, and the delay between requests to a host."""

    def __init__(self, client: httpx.Client | None = None, delay: float = DELAY_S,
                 sleep=time.sleep, clock=time.monotonic):
        self.client = client or httpx.Client(headers={"User-Agent": USER_AGENT},
                                             timeout=TIMEOUT_S, follow_redirects=True)
        self.delay, self.sleep, self.clock = delay, sleep, clock
        self.robots: dict[str, urllib.robotparser.RobotFileParser] = {}
        self.last: dict[str, float] = {}

    def _wait(self, host: str) -> None:
        if host in self.last:
            left = self.last[host] + self.delay - self.clock()
            if left > 0:
                self.sleep(left)
        self.last[host] = self.clock()

    def _get(self, url: str) -> httpx.Response:
        self._wait(urlsplit(url).netloc)
        return self.client.get(url)

    def allowed(self, url: str) -> bool:
        parts = urlsplit(url)
        host = f"{parts.scheme}://{parts.netloc}"
        if host not in self.robots:
            rp = urllib.robotparser.RobotFileParser()
            try:
                r = self._get(f"{host}/robots.txt")
            except httpx.HTTPError:
                r = None
            # RFC 9309 §2.3.1: no robots.txt (a 4xx) allows everything; a server error or no
            # answer disallows everything. A refusal of robots.txt itself (401, 403) is read
            # as a refusal of the host.
            if r is None or r.status_code >= 500 or r.status_code in (401, 403):
                rp.disallow_all = True
            elif r.status_code == 200:
                rp.parse(r.text.splitlines())
            else:
                rp.allow_all = True
            self.robots[host] = rp
        return self.robots[host].can_fetch(USER_AGENT, url)

    def fetch(self, url: str) -> tuple[bytes, dict]:
        if not self.allowed(url):
            raise FetchError(f"robots.txt disallows {url}")
        r = self._get(url)
        if r.status_code != 200:
            raise FetchError(f"HTTP {r.status_code} for {url}")
        meta = {"url": url, "final_url": str(r.url), "status": r.status_code,
                "content_type": r.headers.get("content-type", ""),
                "fetched_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                "sha256": hashlib.sha256(r.content).hexdigest()}
        return r.content, meta


def run(seed: Seed, raw: Path = RAW, refresh: bool = False,
        fetcher: Fetcher | None = None) -> list[str]:
    """Fetches the missing (or, with refresh, all) documents; returns the errors."""
    fetcher = fetcher or Fetcher()
    raw.mkdir(parents=True, exist_ok=True)
    errors = []
    for s in text_sources(seed):
        page, meta_path = raw / f"{s['id']}.html", raw / f"{s['id']}.json"
        if page.exists() and meta_path.exists() and not refresh:
            continue
        try:
            body, meta = fetcher.fetch(s["url"])
        except (FetchError, httpx.HTTPError) as e:
            errors.append(f"{s['id']}: {e}")
            continue
        page.write_bytes(body)
        meta_path.write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")
        print(f"fetched {s['id']}: {len(body) / 1024:.0f} KiB", file=sys.stderr)
    return errors


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--raw", type=Path, default=RAW)
    ap.add_argument("--refresh", action="store_true")
    args = ap.parse_args()
    errors = run(load(args.seed), args.raw, args.refresh)
    for e in errors:
        print(f"FAIL {e}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
