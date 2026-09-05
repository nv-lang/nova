# -*- coding: utf-8 -*-
"""Read-only measurement on the real nova tree.

check-flag-has-caller counts NOVA_* names read by `env::var("LITERAL")` /
`env::var_os("LITERAL")` inside compiler-codegen/src and nova-cli/src, and
calls a flag "silent" when the name occurs nowhere in scripts/, docs/,
.github/ or AGENTS.md.

Here we apply the SAME zone rule to the other syntaxes of the same property:
  * getenv("NOVA_*") in the shipped C runtime compiler-codegen/nova_rt;
  * env::var(<non-literal>) behind a helper function.
"""
import io
import os
import re
import sys

root = sys.argv[1]
FLAG = re.compile(r"NOVA_[A-Z_0-9]+")
GETENV = re.compile(r'getenv\("(NOVA_[A-Z_0-9]+)"')
LIT = re.compile(r'env::var(?:_os)?\("(NOVA_[A-Z_0-9]+)"')
NONLIT = re.compile(r'env::var(?:_os)?\(\s*[a-z_]')

ZONE_DIRS = ("scripts", "docs", ".github")
ZONE_EXT = (".sh", ".yml", ".yaml", ".md", ".toml", ".py")


def read(p):
    try:
        return io.open(p, encoding="utf-8", errors="replace").read()
    except (IOError, OSError):
        return ""


def walk(sub, exts):
    base = os.path.join(root, *sub.split("/"))
    for dp, _dn, fn in os.walk(base):
        for f in fn:
            if f.endswith(exts):
                yield os.path.join(dp, f)


zone = set()
for d in ZONE_DIRS:
    for p in walk(d, ZONE_EXT):
        zone.update(FLAG.findall(read(p)))
zone.update(FLAG.findall(read(os.path.join(root, "AGENTS.md"))))
joined = "\n".join(sorted(zone))

c_flags = {}
for p in walk("compiler-codegen/nova_rt", (".c", ".h")):
    for name in GETENV.findall(read(p)):
        c_flags.setdefault(name, os.path.relpath(p, root).replace("\\", "/"))

rs_lit = set()
for sub in ("compiler-codegen/src", "nova-cli/src"):
    for p in walk(sub, (".rs",)):
        rs_lit.update(LIT.findall(read(p)))

print("NOVA_* read by getenv in nova_rt: %d" % len(c_flags))
print("of them counted by the guard (also read via env::var in Rust): %d"
      % len([f for f in c_flags if f in rs_lit]))
silent_c = sorted(f for f in c_flags if f not in joined)
print("of them SILENT by the guard's own zone rule: %d" % len(silent_c))
for f in silent_c:
    print("  SILENT-IN-C %s   %s" % (f, c_flags[f]))

print("")
print("second syntax: env::var(<non-literal>) sites in Rust sources")
for sub in ("compiler-codegen/src", "nova-cli/src"):
    for p in walk(sub, (".rs",)):
        t = read(p)
        for i, line in enumerate(t.split("\n"), 1):
            if NONLIT.search(line):
                rel = os.path.relpath(p, root).replace("\\", "/")
                print("  %s:%d  %s" % (rel, i, line.strip()[:90]))
