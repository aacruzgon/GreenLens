"""Report rust fences whose strings or brackets don't balance, or that rustfmt can't parse.

usage: check_fences.py docs/backend [-v]
"""
import re, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
def balanced(code):
    st=[]; k=0; n=len(code)
    while k<n:
        c=code[k]
        if code.startswith('//',k):
            k=code.find('\n',k); k = n if k<0 else k; continue
        m=re.match(r'b?r(#*)"',code[k:])
        if m and (k==0 or not (code[k-1].isalnum() or code[k-1]=='_')):
            e=code.find('"'+m.group(1),k+m.end())
            if e<0: return 'unterminated raw string'
            k=e+1+len(m.group(1)); continue
        if c=='"':
            k+=1
            while k<n and code[k]!='"': k+=2 if code[k]=='\\' else 1
            if k>=n: return 'unterminated string'
            k+=1; continue
        if c=="'":
            m=re.match(r"'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.))'",code[k:])
            k+= m.end() if m else 1; continue
        if c in '([{': st.append(c)
        elif c in ')]}':
            if not st or '([{'[')]}'.index(c)]!=st[-1]: return f'unmatched {c}'
            st.pop()
        k+=1
    return f'unclosed {"".join(st)}' if st else None
bad=0; total=0; unparsed=0
for f in sorted(Path(sys.argv[1]).glob('*.md')):
    for m in re.finditer(r'^```rust\n(.*?)^```', f.read_text(), re.S|re.M):
        total+=1
        code=m.group(1)
        r=balanced(code)
        line=f.read_text()[:m.start()].count('\n')+1
        if r: bad+=1; print(f'{f.name}:{line}: {r}')
        p=subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=code,capture_output=True,text=True)
        if p.returncode and 'error' in p.stderr:
            unparsed+=1
            if '-v' in sys.argv: print(f'{f.name}:{line}: rustfmt: {p.stderr.splitlines()[0]}')
print(f'{total} rust fences, {bad} unbalanced, {unparsed} not parseable by rustfmt')
