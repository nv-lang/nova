import os, re, json
from collections import defaultdict, deque

ROOTS = ["std/src", "examples", "spec_tests"]
BASE = "D:/Sources/nv-lang/nova-p17214"

enum_re = re.compile(r'^\s*(?:export\s+|pub\s+)?type\s+([A-Za-z_][A-Za-z0-9_]*)\s*(\[[^\]]*\])?\s+enum\b(.*)$')
rec_re  = re.compile(r'^\s*(?:export\s+|pub\s+)?type\s+([A-Za-z_][A-Za-z0-9_]*)\s*(\[[^\]]*\])?\s+(?:value\s+)?\{(.*)$')
ident_re = re.compile(r'\b([A-Z][A-Za-z0-9_]*)\b')

def strip_comment(l):
    out = []; i = 0; instr = False
    while i < len(l):
        c = l[i]
        if instr:
            if c == '\\': i += 2; out.append('  '); continue
            if c == '"': instr = False
            out.append(c); i += 1; continue
        if c == '"': instr = True; out.append(c); i += 1; continue
        if c == '/' and i+1 < len(l) and l[i+1] == '/': break
        out.append(c); i += 1
    return ''.join(out)

files = []
for r in ROOTS:
    for dp, dn, fn in os.walk(os.path.join(BASE, r)):
        for f in fn:
            if f.endswith('.nv'): files.append(os.path.join(dp, f))
files.sort()

nodes = {}          # nid -> record
by_mod_name = defaultdict(list)   # (mod, name) -> [nid]
by_name = defaultdict(list)       # name -> [nid]

def add(kind, name, mod, rel, line, params, payload_text, variants=None):
    nid = f"{rel}#{line}#{name}"
    nodes[nid] = {'kind': kind, 'name': name, 'mod': mod, 'file': rel, 'line': line,
                  'params': params, 'text': payload_text, 'variants': variants or []}
    by_mod_name[(mod, name)].append(nid)
    by_name[name].append(nid)
    return nid

for path in files:
    rel = os.path.relpath(path, BASE).replace('\\','/')
    mod = os.path.dirname(rel)
    try: lines = open(path, encoding='utf-8').read().split('\n')
    except Exception: continue
    clean = [strip_comment(x) for x in lines]
    n = len(lines)
    for i in range(n):
        l = clean[i]
        m = enum_re.match(l)
        if m:
            name, params, rest = m.group(1), m.group(2), m.group(3)
            body = rest.strip(); j = i + 1
            while j < n:
                nx = clean[j].strip()
                if nx.startswith('|'):
                    body += ' ' + nx
                    while body.count('{') > body.count('}') and j + 1 < n:
                        j += 1; body += ' ' + clean[j].strip()
                    j += 1
                else: break
            while body.count('{') > body.count('}') and j < n:
                body += ' ' + clean[j].strip(); j += 1
            parts=[]; depth=0; cur=''
            for ch in body:
                if ch in '([{': depth+=1
                elif ch in ')]}': depth-=1
                if ch=='|' and depth==0: parts.append(cur); cur=''
                else: cur+=ch
            parts.append(cur)
            variants=[]
            for p in parts:
                p=p.strip()
                if not p: continue
                mv=re.match(r'^([A-Za-z_][A-Za-z0-9_]*)\s*(.*)$', p, re.S)
                if mv: variants.append((mv.group(1), mv.group(2).strip()))
            add('sum', name, mod, rel, i+1,
                [x.strip() for x in (params or '[]')[1:-1].split(',') if x.strip()],
                ' '.join(p for _, p in variants), variants)
            continue
        m = rec_re.match(l)
        if m:
            name, params, rest = m.group(1), m.group(2), m.group(3)
            body = rest; j = i; depth = 1
            while j + 1 < n and depth > 0:
                j += 1; seg = clean[j]
                depth += seg.count('{') - seg.count('}')
                body += '\n' + seg
                if j - i > 300: break
            add('rec', name, mod, rel, i+1,
                [x.strip() for x in (params or '[]')[1:-1].split(',') if x.strip()], body)

box_re = re.compile(r'\b(Vec|HashMap|HashSet|Map|Set|Chan|Channel)\s*\[')
def cut_boxed(txt):
    out = txt
    for _ in range(80):
        m = box_re.search(out)
        if not m: break
        start = m.end()-1; depth=0; k=start
        while k < len(out):
            if out[k]=='[': depth+=1
            elif out[k]==']':
                depth-=1
                if depth==0: break
            k+=1
        out = out[:m.start()] + ' BOXED ' + (out[k+1:] if k < len(out) else '')
    return out

def resolve(name, mod):
    """module-scoped resolution: same dir first, then unique global, else all."""
    if (mod, name) in by_mod_name: return by_mod_name[(mod, name)]
    cands = by_name.get(name, [])
    if len(cands) == 1: return cands
    # ambiguous cross-module: only std/prelude-ish well-known names resolve globally.
    # be CONSERVATIVE for recursion: include all candidates (over-detect recursion).
    return cands

def build(mode):
    g = defaultdict(set)
    for nid, d in nodes.items():
        t = d['text']
        if mode == 'boxed': t = cut_boxed(t)
        own = set(d['params'])
        for m in ident_re.finditer(t):
            nm = m.group(1)
            if nm in own: continue
            for tgt in resolve(nm, d['mod']): g[nid].add(tgt)
    return g

def recset(g, roots):
    out = {}
    for start in roots:
        q = deque([(x, [start, x]) for x in g.get(start, ())]); seen=set(); found=None
        while q:
            x, path = q.popleft()
            if x == start: found = path; break
            if x in seen: continue
            seen.add(x)
            for y in g.get(x, ()): q.append((y, path+[y]))
        if found: out[start] = found
    return out

sum_ids = [nid for nid, d in nodes.items() if d['kind']=='sum']
res = {}
for mode in ('strict','boxed'):
    res[mode] = recset(build(mode), sum_ids)

payloadless = [nid for nid in sum_ids if all(p=='' for _, p in nodes[nid]['variants'])]

print("FILES:", len(files))
print("SUM decl SITES:", len(sum_ids))
print("RECORD/VALUE decl SITES:", len(nodes)-len(sum_ids))
for mode in ('strict','boxed'):
    r = res[mode]
    print(f"[{mode}] recursive SITES = {len(r)}   nonrecursive SITES = {len(sum_ids)-len(r)}")
    print(f"[{mode}]   payload-less SITES = {len(payloadless)}   nonrec & payload-less = {len([x for x in payloadless if x not in r])}")
print()
print("RECURSIVE sites (boxed) with cycle path:")
for nid in sorted(res['boxed']):
    d = nodes[nid]
    path = " -> ".join(nodes[x]['name'] for x in res['boxed'][nid])
    print(f"    {d['name']:24s} {d['file']}:{d['line']}   [{path}]")
print()
print("recursive ONLY in strict (cycle passes through Vec/Map element):")
for nid in sorted(set(res['strict']) - set(res['boxed'])):
    d = nodes[nid]
    print(f"    {d['name']:24s} {d['file']}:{d['line']}   [{' -> '.join(nodes[x]['name'] for x in res['strict'][nid])}]")
print()
byroot = defaultdict(lambda: [0,0,0])
for nid in sum_ids:
    r = nodes[nid]['file'].split('/')[0]
    byroot[r][0]+=1
    if nid in res['boxed']: byroot[r][1]+=1
    if nid in payloadless and nid not in res['boxed']: byroot[r][2]+=1
print("root: total / recursive / nonrec-payloadless")
for k,v in sorted(byroot.items()): print("   ",k,v)
print()
print("total variants:", sum(len(nodes[n]['variants']) for n in sum_ids))
# distinct names
print("distinct sum names:", len({nodes[n]['name'] for n in sum_ids}))
print("nonrec distinct names:", len({nodes[n]['name'] for n in sum_ids if n not in res['boxed']}))
json.dump({'sites':[{k:nodes[n][k] for k in ('name','file','line','variants','params')} for n in sum_ids],
           'recursive_boxed':[nodes[n]['file']+':'+str(nodes[n]['line'])+' '+nodes[n]['name'] for n in res['boxed']],
           'payloadless':[nodes[n]['file']+':'+str(nodes[n]['line'])+' '+nodes[n]['name'] for n in payloadless]},
          open(os.path.join(os.path.dirname(os.path.abspath(__file__)),'census3.json'),'w'), indent=1)
