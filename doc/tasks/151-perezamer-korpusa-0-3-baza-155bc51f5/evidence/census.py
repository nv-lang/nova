# -*- coding: utf-8 -*-
"""True first-diagnostic census of the 55 FAILs, from the raw logs."""
import pathlib, re
from collections import Counter, defaultdict

S = pathlib.Path(r'D:\Sources\nv-lang\nova-opencode\scratch-opencode\measure-0.3-1118')
rows = {}
for line in (S / 'table-118.tsv').read_text(encoding='utf-8').splitlines()[1:]:
    if line.strip():
        c = line.split('\t')
        rows[c[0]] = c

groups = defaultdict(list)
for n in sorted(rows):
    if rows[n][2] != 'FAIL':
        continue
    log = (S / 'logs-118' / (n + '.log')).read_text(encoding='utf-8', errors='replace')
    # prefer novac's JSON diagnostics (emit-fail wraps them), then plain text errors
    msgs = re.findall(r'"message":"(.*?)"(?:,"poisoned_by")?', log)
    err_lines = [l for l in log.splitlines() if 'error' in l.lower() or 'did not parse' in l]
    if msgs:
        first = msgs[0]
    elif err_lines:
        first = err_lines[0][:150]
    else:
        first = (log.strip().splitlines() or ['?'])[0][:150]
    # normalize to a cluster key: first 60 chars of the message
    key = first[:60]
    groups[key].append(n)

for key, ns in sorted(groups.items(), key=lambda kv: -len(kv[1])):
    print(f"{len(ns):3}  {key}")
    for n in ns[:6]:
        print(f"      {n} {rows[n][1]}")
