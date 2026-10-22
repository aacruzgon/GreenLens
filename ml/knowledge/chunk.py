"""Step 1 of the ingestion, continued (training.md §10, chunking): raw pages → chunk text.

    python3 -m ml.knowledge.chunk      # ml/knowledge/raw/<id>.html → ml/knowledge/build/chunks/<id>.jsonl

A page is read as a list of blocks, headings and paragraphs, from its main content only: the
<article>, else <main>, else the body; an EDIS publication from the document its page embeds.
Navigation, headers and footers, forms, photos and their captions, and tables are left out.
Tables are where spray schedules and rates live, and the seed never carries a rate. Reading stops
at a reference list.

Chunks are then made as the chunking rules say: split on the page's own headings first, then on
paragraphs, into chunks of about 120–200 words; a chunk never crosses a heading, and chunks don't
overlap. A paragraph over 200 words is split between sentences. Each chunk keeps its heading path
for the tagger, which sees it; the text itself is the page's words, unchanged. `id` is
`<source_id>.<n>` in page order, so it stays the same while the page doesn't change.

The output is not the seed: tag.py tags these chunks, drops the irrelevant ones and writes
seed/chunks/<source_id>.jsonl.
"""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path

from ml.knowledge.fetch import RAW, text_sources
from ml.knowledge.seed import ROOT, SEED, load

CHUNKS = ROOT / "ml/knowledge/build/chunks"
MIN_WORDS, MAX_WORDS = 120, 200

# Elements whose content is never text to keep.
SKIP_TAGS = {"script", "style", "noscript", "template", "svg", "nav", "header", "footer",
             "aside", "form", "button", "select", "figure", "figcaption", "table", "iframe"}
# Elements skipped by a class or id that names page furniture: the name itself, the last part
# of a hyphenated name ("post-meta"), or a BEM element of it ("toc__wrapper"), never a prefix
# ("toc-end" is Ontario's main content).
SKIP_CLASS = re.compile(r"^(?:[a-z0-9]+[-_])*(share|social|breadcrumbs?|related|meta|sidebar|"
                        r"toc|menu|comments?|feedback|newsletter|byline|skip|visually-hidden|"
                        r"sr-only|caption|photo-credit)(?:__[a-z0-9_-]+)?$", re.I)
HEADINGS = {f"h{n}": n for n in range(1, 7)}
BLOCKS = {"p", "li", "dd", "dt", "blockquote", "div", "section", "article", "main", "ul",
          "ol", "dl", "br", "hr", "td", "th", "tr", "pre"} | set(HEADINGS)
VOID = {"br", "hr", "img", "input", "meta", "link", "source", "wbr", "area", "col", "embed"}
# A heading that starts the back matter ends the page's text.
STOP_HEADING = re.compile(r"^(references|literature cited|selected references|sources|"
                          r"further reading|additional (resources|information)|related "
                          r"(posts|articles|resources)|acknowledge?ments?|for more information|original "
                          r"author|caution using pesticides)\b",
                          re.I)
# Legal and editorial lines that aren't about the plant, and a block that is only a link.
DROP_PARAGRAPH = re.compile(r"^(links to this article are strongly encouraged|last reviewed|"
                            r"this article was originally published|this information is "
                            r"supplied with the understanding|was this helpful|author\(s\)|authors?:|"
                            r"revisions by|originally published)", re.I)


@dataclass
class Block:
    level: int        # 0 for a paragraph, 1–6 for a heading
    text: str


class _Reader(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.blocks: list[Block] = []
        self.buf: list[str] = []
        self.level = 0
        self.skip: list[str] = []     # the open skipped elements, outermost first

    def _flush(self):
        text = re.sub(r"\s+", " ", "".join(self.buf)).strip()
        self.buf = []
        if text:
            self.blocks.append(Block(self.level, text))
        self.level = 0

    def handle_starttag(self, tag, attrs):
        if tag in VOID:
            if tag in BLOCKS and not self.skip:
                self._flush()
            return
        a = dict(attrs)
        names = f"{a.get('class') or ''} {a.get('id') or ''}".split()
        if self.skip or tag in SKIP_TAGS or any(SKIP_CLASS.match(n) for n in names) \
                or a.get("aria-hidden") == "true" or "hidden" in a:
            self.skip.append(tag)
            return
        if tag in BLOCKS:
            self._flush()
            self.level = HEADINGS.get(tag, 0)

    def handle_endtag(self, tag):
        if self.skip:
            # Close the innermost open element of that name; stray end tags are ignored.
            for i in range(len(self.skip) - 1, -1, -1):
                if self.skip[i] == tag:
                    del self.skip[i:]
                    break
            return
        if tag in BLOCKS:
            self._flush()

    def handle_data(self, data):
        if not self.skip:
            self.buf.append(data)

    def close(self):
        super().close()
        self._flush()


def _main_content(page: str) -> str:
    """The page's main content: an EDIS document, the <article>, the <main>, or the body."""
    m = re.search(r'<script id="__NEXT_DATA__"[^>]*>(.*?)</script>', page, re.S)
    if m:
        doc = json.loads(m.group(1)).get("props", {}).get("pageProps", {}).get("details", {})
        if doc.get("docContent"):
            return doc["docContent"]
    for tag in ("article", "main", "body"):
        m = re.search(rf"<{tag}\b[^>]*>(.*)</{tag}>", page, re.S | re.I)
        if m:
            return m.group(1)
    return page


def blocks(page: str) -> list[Block]:
    """The headings and paragraphs of the page's main content, up to its back matter."""
    r = _Reader()
    r.feed(_main_content(page))
    r.close()
    out = []
    for b in r.blocks:
        if b.level and STOP_HEADING.match(b.text):
            break
        if not b.level and (DROP_PARAGRAPH.match(b.text) or re.fullmatch(r"https?://\S+", b.text)):
            continue
        out.append(b)
    return out


def _words(s: str) -> int:
    return len(s.split())


def _pieces(text: str) -> list[str]:
    """A paragraph as pieces of at most MAX_WORDS, split between sentences."""
    if _words(text) <= MAX_WORDS:
        return [text]
    out, cur = [], []
    for sentence in re.split(r"(?<=[.!?])\s+(?=[A-Z(“\"])", text):
        if cur and _words(" ".join(cur + [sentence])) > MAX_WORDS:
            out.append(" ".join(cur))
            cur = []
        cur.append(sentence)
    if cur:
        out.append(" ".join(cur))
    return out


def chunks(source_id: str, page_blocks: list[Block]) -> list[dict]:
    """Chunk records {id, source_id, heading, text}, in page order."""
    sections: list[tuple[list[str], list[str]]] = []
    path: list[tuple[int, str]] = []
    for b in page_blocks:
        if b.level:
            path = [p for p in path if p[0] < b.level] + [(b.level, b.text)]
            sections.append(([t for lvl, t in path if lvl > 1], []))
        else:
            if not sections:
                sections.append(([], []))
            sections[-1][1].append(b.text)
    out = []
    for heading, paragraphs in sections:
        cur: list[str] = []
        for piece in (p for para in paragraphs for p in _pieces(para)):
            if cur and _words(" ".join(cur + [piece])) > MAX_WORDS and _words(" ".join(cur)) >= MIN_WORDS:
                out.append((heading, cur))
                cur = []
            elif cur and _words(" ".join(cur + [piece])) > MAX_WORDS:
                # Under the minimum but the next piece would overflow: close it anyway rather
                # than pass the maximum, which is the limit the seed format sets.
                out.append((heading, cur))
                cur = []
            cur.append(piece)
        if cur:
            out.append((heading, cur))
    return [{"id": f"{source_id}.{n}", "source_id": source_id, "heading": " › ".join(h),
             "text": html.unescape("\n".join(c))} for n, (h, c) in enumerate(out, 1)]


def run(seed_root: Path = SEED, raw: Path = RAW, out: Path = CHUNKS) -> list[str]:
    errors = []
    out.mkdir(parents=True, exist_ok=True)
    for s in text_sources(load(seed_root)):
        page = raw / f"{s['id']}.html"
        if not page.exists():
            errors.append(f"{s['id']}: not fetched (python3 -m ml.knowledge.fetch)")
            continue
        recs = chunks(s["id"], blocks(page.read_text(encoding="utf-8", errors="replace")))
        if not recs:
            errors.append(f"{s['id']}: no text found in {page.name}")
            continue
        (out / f"{s['id']}.jsonl").write_text(
            "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in recs), encoding="utf-8")
        words = [_words(r["text"]) for r in recs]
        print(f"{s['id']}: {len(recs)} chunks, {sum(words)} words "
              f"({min(words)}–{max(words)} per chunk)", file=sys.stderr)
    return errors


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=Path, default=SEED)
    ap.add_argument("--raw", type=Path, default=RAW)
    ap.add_argument("--out", type=Path, default=CHUNKS)
    args = ap.parse_args()
    errors = run(args.seed, args.raw, args.out)
    for e in errors:
        print(f"FAIL {e}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
