# -*- coding: utf-8 -*-
"""Census of control-flow forms among the corpus files novac actually EMITS (the refused ones never reach the
printer, so a green byte-identity check over them proves nothing about a form). Reads the list of corpus files on
stdin (one path per line) and the emission directory whose _refused.txt names the refused ones (argv[1])."""
import io, os, re, sys
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
outdir = sys.argv[1]
refused = set()
try:
    for l in io.open(os.path.join(outdir, '_refused.txt'), encoding='utf-8', errors='replace'):
        parts = l.strip().split(' ', 1)
        if len(parts) == 2:
            refused.add(parts[1].strip())
except FileNotFoundError:
    pass
corpus = [l.strip() for l in sys.stdin if l.strip()]
emitted = [p for p in corpus if p not in refused]
plain_if = re.compile(r'^\s*if\s+(?!Some\()')
els = re.compile(r'\}\s*else\s*\{'); elif_ = re.compile(r'else\s+if\s'); wh = re.compile(r'^\s*while\s')
rng = re.compile(r'^\s*for\s+\w+\s+in\s+.*\.\.'); ifv = re.compile(r'(=|=>|return)\s*if\s')
c = {'plain if statement': 0, 'else on a plain if statement': 0, 'else if chain': 0, 'while': 0, 'range for': 0, 'if-value': 0}
for p in emitted:
    lines = io.open(p, encoding='utf-8', errors='replace').read().split('\n')
    has_else = False
    for i, l in enumerate(lines):
        if els.search(l):
            for j in range(i, -1, -1):
                if re.match(r'^\s*if\s', lines[j]):
                    # an if-value (`= if`) or an if-let is not an if STATEMENT
                    has_else = has_else or (bool(plain_if.match(lines[j])) and not ifv.search(lines[j]))
                    break
    f = {'plain if statement': any(plain_if.match(l) and not ifv.search(l) for l in lines),
         'else on a plain if statement': has_else,
         'else if chain': any(elif_.search(l) for l in lines),
         'while': any(wh.match(l) for l in lines),
         'range for': any(rng.match(l) for l in lines),
         'if-value': any(ifv.search(l) for l in lines)}
    for k, v in f.items():
        c[k] += 1 if v else 0
print('corpus %d, refused %d, emitted %d' % (len(corpus), len(refused), len(emitted)))
for k in c:
    print('%-30s carriers among EMITTED: %3d' % (k, c[k]))
