# -*- coding: utf-8 -*-
"""Acceptance points 3 and 4 of wave M, both ways."""
import glob
import json
import os
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

BT = chr(96)
PAT = re.compile(r"carries no " + BT + r"([^" + BT + r"]+)" + BT + r" for (\S+)")
ICE = ("novac/src/emit_c/emit_place.nv", "novac/src/lower/lowering.nv")


def run(files, unit=True):
    env = dict(os.environ)
    env["NOVAC_SELF_PATH"] = "novac/src"
    if unit:
        env["NOVAC_UNIT"] = "1"
    out = subprocess.run(["./novac/target/novac.exe", "check"] + files, env=env,
                         capture_output=True, text=True,
                         encoding="utf-8", errors="replace").stdout
    msgs = []
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            msgs.append(json.loads(line).get("message", ""))
        except ValueError:
            pass
    return msgs


mods = sorted(set(os.path.dirname(p).replace(os.sep, "/")
                  for p in glob.glob("novac/src/*/*.nv")))
hits = []
for m in mods:
    fs = [p.replace(os.sep, "/") for p in sorted(glob.glob(m + "/*.nv"))
          if not p.endswith("_test.nv") and p.replace(os.sep, "/") not in ICE]
    if not fs:
        continue
    for msg in run(fs):
        h = PAT.search(msg)
        if h:
            hits.append((m.split("/")[-1], h.group(2), h.group(1)))

print("POINT 3 -- refusals still blaming the shell for a type of OURS: %d"
      % len(hits))
for m, t, meth in hits[:12]:
    print("   %-10s %-16s %s" % (m, t, meth))

# POINT 4, the other half: a method of a type from a FOREIGN module must still
# be refused with the same wording. std's `str` is the foreign case at hand.
print()
print("POINT 4 -- the control half: a foreign type's missing method")
S = os.path.dirname(os.path.abspath(__file__)).replace(os.sep, "/") + "/foreign"
if not os.path.isdir(S):
    os.makedirs(S)
p = S + "/probe.nv"
io_src = ("module probe\n\n"
          "fn f(s str) -> int {\n"
          "    ro n = s.no_such_method_here()\n"
          "    return n\n"
          "}\n")
open(p, "w", encoding="utf-8", newline="\n").write(io_src)
for msg in run([p]):
    print("   ->", msg[:150])
