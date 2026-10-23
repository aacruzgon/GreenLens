"""Step 1 of the ingestion, continued (training.md §10, chunking): raw pages → chunk text.

    python3 -m ml.knowledge.chunk      # ml/knowledge/raw/<id>.html or .pdf → ml/knowledge/build/chunks/<id>.jsonl

A page is read as a list of blocks, headings and paragraphs, from its main content only: the
<article>, else <main>, else the body; an EDIS publication from the document its page embeds.
Navigation, headers and footers, forms, photos and their captions, and tables are left out.
Tables are where spray schedules and rates live, and the seed never carries a rate. Reading stops
at a reference list.

A PDF (raw/<id>.pdf, as fetch.py saves one) is read into the same blocks by type size: the size
most of its characters have is the body, a larger line is a heading (the larger, the higher), and
an all-caps line at a column's left edge is the lowest heading, with a "(Latin name)" line under it
joined to it. Other smaller type, such as captions, boxes and tables, is left out, and so is the
running header and the page number in the top and bottom margins. Columns are read left before
right, and a paragraph ends at a gap in the lines, or at a column's foot when its last line ends a
sentence.

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
                        r"toc|menu|comments?|feedback|newsletter|byline|authors-reviewed|skip|visually-hidden|"
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
                            r"supplied with the understanding|was this helpful|author\(s\)|"
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
    return _up_to_back_matter(r.blocks)


# --- PDF -------------------------------------------------------------------------------------

PDF_MARGIN = 45.0     # points: the running header and the page number sit in these
PDF_PARA_GAP = 1.4    # a gap over this many line spacings ends a paragraph


@dataclass
class PdfLine:
    page: int
    x0: float
    y0: float         # the baseline side, in points from the page's foot
    page_width: float
    page_height: float
    text: str
    sizes: list[float]   # the size of each character but spaces


def pdf_lines(path: Path) -> list[PdfLine]:
    """The text lines of a PDF, with their positions and type sizes (pdfminer.six's layout)."""
    from pdfminer.high_level import extract_pages
    from pdfminer.layout import LTChar, LTTextContainer, LTTextLine

    out = []
    for n, page in enumerate(extract_pages(str(path)), 1):
        for box in page:
            if not isinstance(box, LTTextContainer):
                continue
            for line in box:
                if not isinstance(line, LTTextLine):
                    continue
                chars = [c for c in line if isinstance(c, LTChar) and c.get_text().strip()]
                if chars:
                    out.append(PdfLine(n, line.x0, line.y0, page.width, page.height,
                                       line.get_text().strip(), [round(c.size, 1) for c in chars]))
    return out


def _mode(values):
    counts: dict = {}
    for v in values:
        counts[v] = counts.get(v, 0) + 1
    return max(counts, key=counts.get)


def pdf_blocks(lines: list[PdfLine]) -> list[Block]:
    """A PDF's headings and paragraphs, read by type size (see the module's docstring)."""
    lines = [ln for ln in lines if PDF_MARGIN < ln.y0 < ln.page_height - PDF_MARGIN]
    if not lines:
        return []
    body = _mode(s for ln in lines for s in ln.sizes)

    def is_body(ln):
        # A run-in label in other type ("Management Strategies:") doesn't make a line a heading.
        return 3 * sum(abs(s - body) < 0.3 for s in ln.sizes) >= len(ln.sizes)

    def column(ln):
        return ln.x0 >= ln.page_width / 2

    lines.sort(key=lambda ln: (ln.page, column(ln), -ln.y0))
    bodies = [ln for ln in lines if is_body(ln)]
    edges = [round(ln.x0) for ln in bodies]
    lefts = sorted(set(edges), key=edges.count)[-2:]      # the two columns' left edges
    larger = sorted({max(ln.sizes) for ln in lines if not is_body(ln) and max(ln.sizes) > body + 0.3},
                    reverse=True)
    spacing = _mode([round(a.y0 - b.y0) for a, b in zip(bodies, bodies[1:])
                     if a.page == b.page and column(a) == column(b) and 0 < a.y0 - b.y0 < 3 * body] or [body])

    def kind(ln):
        """0 for body text, a heading level, "name" for a (Latin name) line, or None to skip."""
        if is_body(ln):
            return 0
        if not any(abs(ln.x0 - x) < 4 for x in lefts):
            return None           # a box, a caption or a table cell, off the columns' edges
        if max(ln.sizes) > body + 0.3:
            return min(larger.index(max(ln.sizes)) + 1, 6)
        letters = [ch for ch in ln.text if ch.isalpha()]
        if letters and all(ch.isupper() for ch in letters):
            return min(len(larger) + 1, 6)
        if ln.text.startswith("(") and ln.text.endswith(")"):
            return "name"
        return None

    out: list[Block] = []
    prev, prev_kind = None, None
    for ln in lines:
        k = kind(ln)
        after_heading = isinstance(prev_kind, int) and prev_kind > 0
        if k is None or (k == "name" and not after_heading):
            continue      # a caption between two columns' halves of a paragraph doesn't end it
        near = prev is not None and prev.page == ln.page and column(prev) == column(ln)
        if k == "name" or (k and k == prev_kind and near and prev.y0 - ln.y0 < 2.5 * max(ln.sizes)):
            out[-1].text += " " + ln.text          # a heading on two lines, or its Latin name
        elif k and out and out[-1].level == k:
            out[-1].text += " / " + ln.text        # one section under two headings ("COMMON SMUT", "HEAD SMUT")
        elif k:
            out.append(Block(k, ln.text))
        elif prev_kind == 0 and ((near and prev.y0 - ln.y0 <= PDF_PARA_GAP * spacing)
                                 or (not near and not re.search(r"[.!?:)”\"]$", out[-1].text))):
            t = out[-1].text
            out[-1].text = t + ln.text if t.endswith("-") and ln.text[:1].islower() else f"{t} {ln.text}"
        else:
            out.append(Block(0, ln.text))
        prev, prev_kind = ln, k
    return _up_to_back_matter(out)


def _up_to_back_matter(page_blocks: list[Block]) -> list[Block]:
    out = []
    for b in page_blocks:
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
        page = next((p for p in (raw / f"{s['id']}.pdf", raw / f"{s['id']}.html") if p.exists()), None)
        if page is None:
            errors.append(f"{s['id']}: not fetched (python3 -m ml.knowledge.fetch)")
            continue
        if page.suffix == ".pdf":
            recs = chunks(s["id"], pdf_blocks(pdf_lines(page)))
        else:
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
