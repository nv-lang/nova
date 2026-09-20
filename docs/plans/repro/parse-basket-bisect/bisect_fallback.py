# -*- coding: utf-8 -*-
"""WHICH DECLARATION actually carries the parse fallback.

The assistant's report killed the method the queue was built on: in four
families of eight the line the bucket points at parses FINE on its own. So the
first refusal of a file does not have to name its cause, and an address taken
from it is plausible and wrong.

This narrows by CONSTRUCTION instead: keep the file's header, then add its
top-level declarations one at a time, and report the first one whose arrival
makes the fallback appear. The answer is a declaration that DID produce it,
not a line that happened to be reported first.
"""
import io
import json
import os
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

NOVAC = "./novac/target/novac.exe"
OUT = os.environ.get("BISECT_DIR")
NEEDLE = "novac did not parse this"


def fallback_in(path):
    env = dict(os.environ)
    env["NOVAC_SELF_PATH"] = "novac/src"
    r = subprocess.run([NOVAC, "check", path], env=env, capture_output=True,
                       text=True, encoding="utf-8", errors="replace").stdout
    for line in r.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if NEEDLE in d.get("message", ""):
            return True
    return False


def chunks(text):
    """Top-level declarations: a run starting at column 0 with a keyword."""
    lines = text.split("\n")
    heads = [i for i, l in enumerate(lines)
             if re.match(r"^(export\s+)?(fn|type|const|import|module|test)\b", l)]
    if not heads:
        return [], lines
    out = []
    for n, h in enumerate(heads):
        end = heads[n + 1] if n + 1 < len(heads) else len(lines)
        out.append((h, end))
    return out, lines


def bisect(path):
    text = io.open(path, encoding="utf-8", errors="replace").read()
    cs, lines = chunks(text)
    if not cs:
        return None
    # the header: every `module`/`import` chunk, kept always
    head = [c for c in cs if re.match(r"^(module|import)\b", lines[c[0]])]
    body = [c for c in cs if c not in head]
    keep = []
    for c in body:
        keep.append(c)
        piece = []
        for h in head:
            piece.extend(lines[h[0]:h[1]])
        for k in keep:
            piece.extend(lines[k[0]:k[1]])
        tmp = os.path.join(OUT, "piece.nv")
        io.open(tmp, "w", encoding="utf-8", newline="\n").write("\n".join(piece) + "\n")
        if fallback_in(tmp):
            return (c[0] + 1, lines[c[0]][:86])
    return None


for path in sys.argv[1:]:
    r = bisect(path)
    if r is None:
        print("%-34s no single declaration reproduces it" % path.replace("novac/src/", ""))
    else:
        print("%-34s line %5d  %s" % (path.replace("novac/src/", ""), r[0], r[1]))
