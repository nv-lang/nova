# -*- coding: utf-8 -*-
import pathlib, re, json
S = pathlib.Path(r'D:\Sources\nv-lang\nova-opencode\scratch-opencode\measure-0.3-1118')
WT = pathlib.Path(r'D:\Sources\nv-lang\worktrees\nova-opencode-novac-tail-value')

def load(p):
    rows = {}
    for line in p.read_text(encoding='utf-8').splitlines():
        if line.strip():
            c = line.split('\t')
            rows[c[0]] = c
    return rows

new = load(S / 'table-118.tsv')
f141 = load(WT / 'doc/tasks/141-zamer-korpusa-0-3-vse-primery-i-pakety-k/evidence/fail-diagnostics-118.tsv')

same = diff = 0
diffs = []
out = ['n\tpath\tclass_new\tcode_new\tfirst_diag_new\tfirst_diag_141\tsame_as_141']
for n in sorted(new):
    if new[n][2] != 'FAIL':
        continue
    msg_new = code_new = ''
    m = re.search(r'"code":"(E_\w+)".*?"message":"(.*?)"(?:,"poisoned)', new[n][9]) or re.search(r'"message":"(.*?)"', new[n][9])
    if isinstance(m, re.Match) and m.lastindex and m.lastindex >= 2:
        code_new, msg_new = m.group(1), m.group(2)
    else:
        mm = re.search(r'"code":"(E_\w+)"', new[n][9])
        m2 = re.search(r'"message":"(.*?)"', new[n][9])
        code_new = mm.group(1) if mm else new[n][8]
        msg_new = m2.group(1) if m2 else new[n][9][:200]
    row141 = f141.get(n)
    msg_141 = row141[4] if row141 else ''
    eq = msg_new[:120] == msg_141[:120]
    same += eq
    diff += (not eq)
    if not eq:
        diffs.append((n, new[n][1], msg_new[:100], msg_141[:100]))
    out.append('\t'.join([n, new[n][1], code_new, code_new, msg_new, msg_141, 'SAME' if eq else 'DIFF']))
(S / 'fail-diagnostics-118.tsv').write_text('\n'.join(out) + '\n', encoding='utf-8')
print(f"first-diagnostic vs 141: SAME {same}, DIFF {diff}")
for d in diffs:
    print('  ~', d[0], d[1])
    print('     new:', d[2])
    print('     141:', d[3])
