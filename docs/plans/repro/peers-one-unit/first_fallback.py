# -*- coding: utf-8 -*-
"""The FIRST parser fallback of each file, with its source.

Plan 274:3056 settled this: later spans are recovery debris, and grouping them
gives nonsense. The first refusal of a file is unambiguous, so these are the
forms the parser actually stops at -- about ten sites instead of 143 counts.
"""
import glob
import io
import json
import os
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

ICE = ("novac/src/emit_c/emit_place.nv", "novac/src/lower/lowering.nv")
env = dict(os.environ)
env["NOVAC_SELF_PATH"] = "novac/src"
env["NOVAC_UNIT"] = "1"

first = {}
mods = sorted(set(os.path.dirname(p).replace(os.sep, "/")
                  for p in glob.glob("novac/src/*/*.nv")))
for m in mods:
    fs = [p.replace(os.sep, "/") for p in sorted(glob.glob(m + "/*.nv"))
          if not p.endswith("_test.nv") and p.replace(os.sep, "/") not in ICE]
    if not fs:
        continue
    out = subprocess.run(["./novac/target/novac.exe", "check"] + fs, env=env,
                         capture_output=True, text=True,
                         encoding="utf-8", errors="replace").stdout
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if "did not parse this" not in d.get("message", ""):
            continue
        p = (d.get("primary") or {}).get("file", "")
        if p in first:
            continue
        first[p] = ((d.get("primary") or {}).get("start", 0),
                    (d.get("primary") or {}).get("end", 0))

print("files whose parse stops somewhere: %d" % len(first))
print()
for p in sorted(first):
    lo, hi = first[p]
    try:
        t = io.open(p, encoding="utf-8", newline="").read()
    except OSError:
        continue
    ln = t[:lo].count("\n") + 1
    a = t.rfind("\n", 0, lo) + 1
    b = t.find("\n", hi)
    src = t[a:b if b > 0 else len(t)]
    print("%s:%d" % (p, ln))
    for l in src.split("\n")[:3]:
        print("    %s" % l.strip()[:104])
    print()
