"""Plan 172.14 F.2 atom A3 -- how many sum allocations would A4 actually remove?

Measurement only; nothing here touches the compiler. Inputs:
  1. generated C from target/.nova-cache (one file per compiled program)
  2. detector verdicts from a NOVA_DETECT_SUMREC=1 run (stderr capture)

A "site" is one occurrence of `nova_make_<T>_<V>(` in emitted C that is not the
constructor's own definition. Sites are counted per compilation unit and summed:
the same std constructor appears in every CU that pulls std in, and that is the
honest picture of emitted output volume, so the duplication factor is reported
alongside rather than divided away.

Usage: python a3-alloc-census.py <cache-dir> <verdicts-file>
"""
import os, re, sys
from collections import defaultdict, Counter

cache_dir, verdict_file = sys.argv[1], sys.argv[2]

# ---- 1. detector verdicts -------------------------------------------------
verd = {}                      # sum name -> dict(recursive, payloadless, vnames)
for line in open(verdict_file, encoding="utf-8", errors="replace"):
    m = re.search(r"\[a2-sumrec\] sum=(\S+) file=\d+ variants=(\d+) "
                  r"recursive=(\d) payloadless=(\d) vnames=(\S*)", line)
    if not m:
        continue
    name, _n, rec, pl, vn = m.groups()
    verd[name] = {"rec": rec == "1", "pl": pl == "1",
                  "vnames": [v for v in vn.split(",") if v]}

# (type, variant) pairs, longest type first so `Nova_X_Y` cannot be mis-split.
pairs = []
for t, info in verd.items():
    for v in info["vnames"]:
        pairs.append((t, v))
pairs.sort(key=lambda tv: -len(tv[0]))

def classify_symbol(sym):
    """`nova_make_` already stripped. -> (type, variant) or None."""
    for t, v in pairs:
        if sym == f"{t}_{v}" or sym == f"Nova_{t}_{v}":
            return t, v
        if sym.endswith(f"_{v}"):
            head = sym[: -(len(v) + 1)]
            if head in (t, f"Nova_{t}") or head.startswith(f"{t}____") \
               or head.startswith(f"Nova_{t}____") or head.endswith(f"_{t}"):
                return t, v
    return None

# ---- 2. walk the emitted C ------------------------------------------------
CTOR = re.compile(r"\bnova_make_([A-Za-z0-9_]+)\s*\(")
DEFN = re.compile(r"^\s*(?:static\s+)?[A-Za-z_][A-Za-z0-9_ ]*\*?\s*"
                  r"nova_make_[A-Za-z0-9_]+\s*\([^;]*\)\s*\{\s*$")
# Generated C puts every function header at column 0, so anchoring there is what
# separates a real header from a nested `if (...) {`. The name is the last token
# before the parameter list. An earlier version excluded any header containing
# `=`, which silently matched almost nothing and parked every site under
# <toplevel> -- the giveaway was 1704 Align sites all claiming one function.
FUNC = re.compile(r"^[A-Za-z_].*\)\s*\{\s*$")
FUNC_NAME = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(")

sites = Counter()              # (type, variant) -> call sites
zero_arg = Counter()           # (type, variant) -> sites with no arguments
by_enclosing = defaultdict(Counter)   # type -> enclosing C function -> sites
unknown = Counter()
cus = 0
fenced_opt, fenced_vec = set(), set()

for fn in sorted(os.listdir(cache_dir)):
    if not fn.endswith(".c"):
        continue
    cus += 1
    text = open(os.path.join(cache_dir, fn), encoding="utf-8", errors="replace").read()
    # Types fenced out of A4 because they already live inside Option / Vec.
    for m in re.finditer(r"NovaOpt_Nova_([A-Za-z0-9_]+?)_p\b", text):
        fenced_opt.add(m.group(1))
    for m in re.finditer(r"Vec____Nova_([A-Za-z0-9_]+?)_p\b", text):
        fenced_vec.add(m.group(1))
    cur_fn = "<toplevel>"
    for line in text.splitlines():
        # A `}` in column 0 ends a function; without this reset the last
        # matched header sticks and every later site is misattributed to it
        # (the first run of this script credited 420 Align sites to
        # `Nova_str_method_plus`, which is how the bug was noticed).
        if line.startswith("}"):
            cur_fn = "<toplevel>"
        if FUNC.match(line) and "nova_make_" not in line:
            names = FUNC_NAME.findall(line)
            if names:
                cur_fn = names[0]
        is_defn = bool(DEFN.match(line))
        for m in CTOR.finditer(line):
            if is_defn:
                continue
            sym = m.group(1)
            tv = classify_symbol(sym)
            if tv is None:
                unknown[sym] += 1
                continue
            sites[tv] += 1
            by_enclosing[tv[0]][cur_fn] += 1
            rest = line[m.end():]
            if rest.startswith(")"):
                zero_arg[tv] += 1

# ---- 3. report ------------------------------------------------------------
def bucket(tv):
    t, _v = tv
    info = verd.get(t)
    if info is None:
        return "unknown"
    if info["rec"]:
        return "recursive"
    return "nonrec_payloadless" if info["pl"] else "nonrec_withpayload"

tot = sum(sites.values())
b = Counter()
for tv, n in sites.items():
    b[bucket(tv)] += n

print(f"compilation units scanned : {cus}")
print(f"distinct (type,variant)   : {len(sites)}")
print(f"nova_make_* CALL SITES    : {tot}")
print()
print(f"  recursive sums          : {b['recursive']}")
print(f"  non-recursive, payload  : {b['nonrec_withpayload']}")
print(f"  non-recursive, payloadless: {b['nonrec_payloadless']}")
print(f"  unclassified            : {b['unknown']}")
print()
print(f"  zero-argument calls (unit variant, allocation for nothing): {sum(zero_arg.values())}")
zl = sum(n for tv, n in zero_arg.items() if bucket(tv).startswith("nonrec"))
print(f"    of those, on NON-recursive sums                        : {zl}")
print()
res = sum(n for sym, n in unknown.items() if sym.startswith("NovaRes_"))
opt = sum(n for sym, n in unknown.items() if sym.startswith(("Option_", "Result_")))
print(f"  prelude Result ctors (NovaRes_*, DO allocate, own path, out of A4 scope): {res}")
print(f"  prelude Option ctors                                                    : {opt}")
other = [(s, n) for s, n in unknown.items()
         if not s.startswith(("Option_", "NovaRes_", "Result_"))]
print(f"  unmatched symbols (other)                : {sum(n for _s, n in other)}")
for s, n in sorted(other, key=lambda x: -x[1])[:8]:
    print(f"      {s}: {n}")
print()
print("TOP constructors by call sites:")
for tv, n in sites.most_common(15):
    t, v = tv
    print(f"  {n:6d}  {t}.{v:<16s} [{bucket(tv)}]")
print()
print("TOP enclosing C functions for the hottest types:")
seen = set()
for tv, _n in sites.most_common(40):
    t = tv[0]
    if t in seen:
        continue
    seen.add(t)
    if len(seen) > 6:
        break
    top = by_enclosing[t].most_common(3)
    print(f"  {t}: " + ", ".join(f"{fnm}={c}" for fnm, c in top))
print()
print(f"types fenced out of A4 by Option usage : {len(fenced_opt)}")
print(f"types fenced out of A4 by Vec usage    : {len(fenced_vec)}")
fenced = fenced_opt | fenced_vec
fenced_sites = sum(n for tv, n in sites.items()
                   if tv[0] in fenced and bucket(tv) == "nonrec_payloadless")
print(f"payloadless non-recursive sites on fenced types: {fenced_sites}")

print()
print("==== what A4 (payload-less, non-recursive, unfenced) would remove ====")
removable = b["nonrec_payloadless"] - fenced_sites
print(f"  removable now (A4)                          : {removable}")
print(f"  stays: recursive                            : {b['recursive']}")
print(f"  stays: non-recursive WITH payload  (-> A7)  : {b['nonrec_withpayload']}")
print(f"  stays: payload-less but Option/Vec (-> A5/A6): {fenced_sites}")
print(f"  stays: prelude Result ctors (own path)      : {res}")
stays_user = b["recursive"] + b["nonrec_withpayload"] + fenced_sites
print(f"  -- user-sum sites still allocating after A4 : {stays_user} of {tot}")
if tot:
    print(f"  -- A4 share of user-sum sites               : {100.0*removable/tot:.1f}%")
grand = tot + res
print(f"  -- A4 share of ALL sum allocations           : {100.0*removable/grand:.1f}%")
print(f"  fenced payloadless types: "
      f"{sorted(t for t in fenced if t in verd and verd[t]['pl'] and not verd[t]['rec'])[:12]}")
