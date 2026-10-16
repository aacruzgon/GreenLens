#!/usr/bin/env python3
"""Expand a Markdown template with Rust code copied from the compiled domain crate.

Directives, each on a line of its own:
  @@items <file> <Name> [<Name> ...]@@   the named items, with their doc comments and attributes
  @@impl <file> <text>@@                 the impl block whose header line contains <text>
  @@file <file>@@                        the whole file without its #[cfg(test)] module
  @@test <file> <fn>@@                   one test function
  @@lines <file> <start>-<end>@@         a line range (1-based, inclusive)
Files are relative to the crate's src/. Unknown items fail loudly.
"""
import re
import sys
from pathlib import Path

SRC = Path(sys.argv[1])
TEMPLATE = Path(sys.argv[2])
OUT = Path(sys.argv[3])

DECL = r'^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:const\s+fn|async\s+fn|fn|struct|enum|trait|type|const|mod|macro_rules!)\s+{name}\b'


def read(rel):
    return (SRC / rel).read_text().split('\n')


def start_of(lines, i):
    """Walk back over doc comments, plain comments and attributes (multi-line too)."""
    j = i
    while j > 0:
        prev = lines[j - 1].strip()
        if prev.startswith('///') or prev.startswith('//') or prev.startswith('#['):
            j -= 1
            continue
        if prev.endswith(')]') or prev.endswith('),') or prev.endswith(','):
            # inside a multi-line attribute: go back to its opening line
            k = j - 1
            while k > 0 and not lines[k].strip().startswith('#['):
                k -= 1
            if lines[k].strip().startswith('#['):
                j = k
                continue
        break
    return j


CHAR = re.compile(r"'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.))'")
RAW = re.compile(r'b?r(#*)"')


IN_STRING = set()  # absolute line numbers that start inside a string literal


def mark_string(text, a, b, line_of):
    """Record the lines that start inside the literal text[a:b]."""
    k = text.find('\n', a, b)
    while k >= 0:
        IN_STRING.add(line_of(k) + 1)
        k = text.find('\n', k + 1, b)


def end_of(lines, i):
    """The last line of the item starting at line i.

    Scans characters across lines, so string literals (plain and raw), char
    literals, comments and brackets spanning several lines are skipped as a
    whole. The item ends at a `}` or `;` outside every bracket: the brace that
    closes a body, or the semicolon after a const, static, type or unit struct.
    """
    text = '\n'.join(lines[i:])
    starts = [0]
    for line in lines[i:]:
        starts.append(starts[-1] + len(line) + 1)

    def line_of(pos):
        lo, hi = 0, len(starts) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if starts[mid] <= pos:
                lo = mid
            else:
                hi = mid - 1
        return i + lo

    brace = 0
    other = 0
    seen = False
    k = 0
    n = len(text)
    while k < n:
        c = text[k]
        prev = text[k - 1] if k else ' '
        if text.startswith('//', k):
            k = text.find('\n', k)
            if k < 0:
                break
            continue
        if text.startswith('/*', k):
            depth, k = 1, k + 2
            while k < n and depth:
                if text.startswith('/*', k):
                    depth, k = depth + 1, k + 2
                elif text.startswith('*/', k):
                    depth, k = depth - 1, k + 2
                else:
                    k += 1
            continue
        m = RAW.match(text, k)
        if m and not (prev.isalnum() or prev == '_'):
            close = '"' + m.group(1)
            end = text.find(close, m.end())
            if end < 0:
                raise SystemExit(f'unterminated raw string in item at line {i + 1}')
            mark_string(text, k, end, line_of)
            k = end + len(close)
            continue
        if c == '"':
            a = k
            k += 1
            while k < n and text[k] != '"':
                k += 2 if text[k] == '\\' else 1
            mark_string(text, a, k, line_of)
            k += 1
            continue
        if c == "'":
            m = CHAR.match(text, k)
            k = m.end() if m else k + 1
            continue
        if c == '{':
            brace += 1
            seen = True
        elif c == '}':
            brace -= 1
            if seen and brace == 0 and other == 0:
                return line_of(k)
        elif c in '([':
            other += 1
        elif c in ')]':
            other -= 1
        elif c == ';' and brace == 0 and other == 0:
            return line_of(k)
        k += 1
    raise SystemExit(f'unterminated item at line {i + 1}')


def find(lines, name, rel):
    kinds = r'(?:const\s+fn|async\s+fn|fn|struct|enum|trait|type|const|static|mod|macro_rules!)'
    if ':' in name:
        kind, name = name.split(':', 1)
        kinds = re.escape(kind)
    rx = re.compile(r'^\s*(?:pub(?:\([a-z]+\))?\s+)?' + kinds + r'\s+' + re.escape(name) + r'\b')
    hits = [i for i, l in enumerate(lines) if rx.match(l)]
    if not hits:
        raise SystemExit(f'item {name} not found in {rel}')
    i = min(hits, key=lambda h: len(lines[h]) - len(lines[h].lstrip()))
    return start_of(lines, i), end_of(lines, i)


def dedent(block):
    indents = [len(l) - len(l.lstrip()) for l in block if l.strip()]
    cut = min(indents) if indents else 0
    return [l[cut:] for l in block]


def dedent_item(lines, a, b):
    """lines[a:b + 1] without the common indent of its code lines.

    Lines that start inside a multi-line string literal are string content and
    don't count towards the indent. They lose it too only when all of them are
    indented at least as far as the code, so no string loses characters.
    """
    block = lines[a:b + 1]

    def indent(l):
        return len(l) - len(l.lstrip())

    code = [l for k, l in enumerate(block, a) if k not in IN_STRING and l.strip()]
    text = [l for k, l in enumerate(block, a) if k in IN_STRING and l.strip()]
    cut = min((indent(l) for l in code), default=0)
    keep = any(indent(l) < cut for l in text)
    return [l if k in IN_STRING and (keep or not l.strip()) else l[cut:] for k, l in enumerate(block, a)]


def strip_tests(lines):
    out = []
    for i, l in enumerate(lines):
        if l.strip() == '#[cfg(test)]' and i + 1 < len(lines) and lines[i + 1].startswith('mod tests'):
            break
        out.append(l)
    while out and not out[-1].strip():
        out.pop()
    return out


def fence(block, lang='rust'):
    return ['```' + lang] + block + ['```']


out = []
for line in TEMPLATE.read_text().split('\n'):
    m = re.fullmatch(r'@@(\w+) (.+)@@', line.strip())
    if not m:
        out.append(line)
        continue
    kind, args = m.group(1), m.group(2).split()
    rel = args[0]
    lines = read(rel)
    IN_STRING.clear()
    if kind == 'items':
        block = []
        for name in args[1:]:
            a, b = find(lines, name, rel)
            if block:
                block.append('')
            block += dedent_item(lines, a, b)
        out += fence(block)
    elif kind == 'impl':
        text = ' '.join(args[1:])
        hits = [i for i, l in enumerate(lines) if l.lstrip().startswith('impl') and text in l]
        if not hits:
            raise SystemExit(f'impl {text} not found in {rel}')
        i = hits[0]
        out += fence(dedent_item(lines, start_of(lines, i), end_of(lines, i)))
    elif kind == 'file':
        lang = 'toml' if rel.endswith('.toml') else 'rust'
        out += fence(strip_tests(lines), lang)
    elif kind == 'test':
        name = args[1]
        a, b = find(lines, name, rel)
        out += fence(dedent_item(lines, a, b))
    elif kind == 'lines':
        s, e = (int(x) for x in args[1].split('-'))
        out += fence(dedent(lines[s - 1:e]))
    else:
        raise SystemExit(f'unknown directive {kind}')

OUT.write_text('\n'.join(out))
print(f'wrote {OUT} ({len(out)} lines)')
