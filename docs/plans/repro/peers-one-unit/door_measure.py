# -*- coding: utf-8 -*-
"""Acceptance of wave M: measure through the DOOR, module by module.

The probe's table was built by a script assembling units outside the compiler.
This one asks the compiler itself, with NOVAC_UNIT=1, and the two must agree --
they are independent implementations of one rule, and tonight their
disagreement already found a bug.

The two files that raise the ICE are withheld BY NAME, because an ICE truncates
a run and would make every count a function of argument order (registry 1168).
"""
import glob
import json
import os
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

ICE = ("novac/src/emit_c/emit_place.nv", "novac/src/lower/lowering.nv")
NOVAC = "./novac/target/novac.exe"


def run(files, unit):
    env = dict(os.environ)
    env["NOVAC_SELF_PATH"] = "novac/src"
    if unit:
        env["NOVAC_UNIT"] = "1"
    out = subprocess.run([NOVAC, "check"] + files, env=env, capture_output=True,
                         text=True, encoding="utf-8", errors="replace").stdout
    n = 0
    unread = 0
    ice = 0
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except ValueError:
            unread += 1
            continue
        n += 1
        if d.get("code") == "E_NOVAC_ICE":
            ice += 1
    return n, ice, unread


mods = sorted(set(os.path.dirname(p).replace(os.sep, "/")
                  for p in glob.glob("novac/src/*/*.nv")))
tot_f = tot_u = 0
bad = 0
print("%-22s %6s %6s %6s" % ("module", "files", "byfile", "unit"))
for m in mods:
    files = [p.replace(os.sep, "/") for p in sorted(glob.glob(m + "/*.nv"))
             if not p.endswith("_test.nv")]
    files = [f for f in files if f not in ICE]
    if not files:
        continue
    nf, icf, unf = run(files, False)
    nu, icu, unu = run(files, True)
    bad += unf + unu + icf + icu
    tot_f += nf
    tot_u += nu
    print("%-22s %6d %6d %6d" % (m.split("/")[-1], len(files), nf, nu))

print("%-22s %6s %6d %6d" % ("TOTAL", "", tot_f, tot_u))
print("unread lines + ICEs across every run: %d  (zero is part of the verdict)"
      % bad)
