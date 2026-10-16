"""Check every relative link and #anchor in the given Markdown files.

usage: checklinks.py docs/backend/*.md
"""
import re, sys
from pathlib import Path
def slug(h):
    h = re.sub(r'<[^>]+>', '', h).strip().lower()
    h = re.sub(r'[^\w\- ]', '', h)
    return h.replace(' ', '-')
def anchors(p):
    out = set(); counts = {}
    incode = False
    for line in p.read_text().split('\n'):
        if line.startswith('```'): incode = not incode; continue
        if incode: continue
        m = re.match(r'^(#{1,6}) (.+)$', line)
        if m:
            s = slug(m.group(2)); n = counts.get(s, 0); counts[s] = n + 1
            out.add(s if n == 0 else f'{s}-{n}')
    return out
bad = 0
for doc in sys.argv[1:]:
    p = Path(doc); incode = False
    for i, line in enumerate(p.read_text().split('\n'), 1):
        if line.startswith('```'): incode = not incode; continue
        if incode: continue
        for target in re.findall(r'\]\(([^)\s]+)\)', line):
            if target.startswith('http'): continue
            path, _, frag = target.partition('#')
            tp = (p.parent / path).resolve() if path else p
            if not tp.exists():
                print(f'{p.name}:{i}: missing file {target}'); bad += 1; continue
            if frag and tp.suffix == '.md' and frag not in anchors(tp):
                print(f'{p.name}:{i}: missing anchor {target}'); bad += 1
print('bad links:', bad)
