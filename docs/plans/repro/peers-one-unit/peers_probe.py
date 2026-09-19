# -*- coding: utf-8 -*-
"""Probe: do the peer refusals vanish when a module's files are ONE unit?

Today novac checks one file at a time and sees its peers only as handed
declarations, so every call of a peer's method is refused as "the shell carries
no X for Y" -- about novac's OWN types. If the same text, concatenated into a
single unit, stops producing those refusals, the lever is confirmed and the
work is "compile a module's peers as one unit", not "widen the probe".

The probe is deliberately crude: concatenation is not the implementation, it is
the MEASUREMENT of what the implementation would buy.
"""
import glob
import json
import os
import re
import subprocess
import sys
from collections import Counter

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

BT = chr(96)
PAT = re.compile(r"carries no " + BT + r"([^" + BT + r"]+)" + BT + r" for (\S+)")
SCRATCH = os.path.dirname(os.path.abspath(__file__)).replace(os.sep, "/")

MODULE = sys.argv[1] if len(sys.argv) > 1 else "parse"
peers = [f.replace(os.sep, "/")
         for f in sorted(glob.glob("novac/src/%s/*.nv" % MODULE))
         if not f.endswith("_test.nv")]
print("module %s: %d peer files" % (MODULE, len(peers)))

env = dict(os.environ)
env["NOVAC_SELF_PATH"] = "novac/src"


def run(files):
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
    shell = Counter()
    for m in msgs:
        h = PAT.search(m)
        if h:
            shell[(h.group(2), h.group(1))] += 1
    return len(msgs), sum(shell.values()), shell


n_before, shell_before, det_before = run(peers)
print("AS TODAY (file by file): diagnostics %d, of them peer-method refusals %d"
      % (n_before, shell_before))

# --- build the single unit -------------------------------------------------
module_line = None
imports = []
bodies = []
for p in peers:
    for l in open(p, encoding="utf-8").read().split("\n"):
        st = l.strip()
        if st.startswith("module "):
            module_line = module_line or l
            continue
        if st.startswith("import "):
            if l not in imports:
                imports.append(l)
            continue
        bodies.append(l)

UNIT = "%s/unit_%s.nv" % (SCRATCH, MODULE)
open(UNIT, "w", encoding="utf-8", newline="\n").write(
    "\n".join([module_line] + imports + [""] + bodies) + "\n")
print("one unit written: %d lines" % (len(imports) + len(bodies) + 2))

n_after, shell_after, det_after = run([UNIT])
print("AS ONE UNIT:            diagnostics %d, of them peer-method refusals %d"
      % (n_after, shell_after))
print()
print("peer-method refusals: %d -> %d" % (shell_before, shell_after))
if det_after:
    print("what still asks the shell:")
    for (t, meth), n in det_after.most_common(10):
        print("  %-14s %-20s %d" % (t, meth, n))
