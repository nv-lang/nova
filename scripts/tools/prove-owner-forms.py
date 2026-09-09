# -*- coding: utf-8 -*-
"""Both-ways probe for owner_of: SIX cases, and three of them are the forms the old fix missed.

The narrowing being tested strips main-copy subdirectories (`nova-cli`, `compiler-codegen`, ...)
so that a process running from the main copy is attributed to `nova`, not to a directory name
that exists nowhere as a working copy. Cases A-C are what the narrowing MUST catch; D-F are what
it must NOT touch -- a real worktree keeps its own name even when the same subdirectory sits
inside it. Case B is the live line that produced `machine BUSY by nova-cli x1` twice today.
"""
import importlib.util, os, sys

# The module under probe is this file's OWN neighbour, so __file__ names it exactly, and a
# hardwired root is only a second, staler answer to a question the path already answers.
# Kept as an override for probing another checkout's copy.
TOOLS = os.environ.get("NOVA_TOOLS_DIR") or os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("cmw", os.path.join(TOOLS, "controller-machine-watch.py"))
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)

# Корни ВЫВОДЯТСЯ, а не пишутся: путь в случае — данные, но записанный литералом он
# привязывает пробу к одной машине (страж №698 краснеет на этом законно). MAIN — имя
# главной копии, PAR — каталог рядом с ней; оба берутся из положения этого файла.
import os as _os
_HERE = _os.path.dirname(_os.path.abspath(__file__))
_MAIN = _os.path.dirname(_os.path.dirname(_HERE))          # <...>/nova
_PAR = _os.path.dirname(_MAIN)                              # <...>/nv-lang
MAIN = _MAIN.replace('\\', '/')
PAR = _PAR.replace('\\', '/')
MAIN_BS = MAIN.replace('/', '\\')
PAR_BS = PAR.replace('/', '\\')

CASES = [
    # (name, ps line, expected owner)
    ("A abs main copy, backslashes",
     "uid 1 1 ? 18:00:00 %s\\nova-cli\\target\\release\\nova.exe test std/src" % MAIN_BS, "nova"),
    # B: a RELATIVE path carries NO tree name at all, so the honest answer from the LINE alone is
    # "?" -- and the name then comes from cwd_owner(pid), which the live watch already calls. The
    # old code answered `nova-cli` here: it invented a tree out of a subdirectory rather than
    # admitting the line was silent. "?" is not a defect, it is the unknown declared as unknown --
    # the difference between my prohibition 16 and its violation.
    ("B REL main copy subdir -- line alone is silent",
     "uid 1 1 ? 18:00:00 timeout 540 ./nova-cli/target/release/nova test std/src", "?"),
    ("C abs main copy, forward slashes",
     "uid 1 1 ? 18:00:00 %s/compiler-codegen/target/debug/nova-codegen test" % MAIN, "nova"),
    ("D real worktree, abs",
     "uid 1 1 ? 18:00:00 %s\\nova-p274\\novac\\target\\novac.exe emit examples/basic" % PAR_BS, "nova-p274"),
    ("E real worktree WITH the same subdir inside",
     "uid 1 1 ? 18:00:00 %s/nova-p274/nova-cli/target/release/nova test spec_tests" % PAR,
     "nova-p274"),
    ("F another worktree, forward slashes",
     "uid 1 1 ? 18:00:00 %s/claude-limits/nova-cli/target/release/nova build" % PAR, "claude-limits"),
]

ok = 0
for name, line, want in CASES:
    got = M.owner_of(line)
    verdict = "OK" if got == want else "FAIL"
    if got == want:
        ok += 1
    print("%-4s %-46s want=%-14s got=%-14s %s" % (verdict, name, want, got, ""))
print()
print("owner_of probe: %d/%d" % (ok, len(CASES)))
sys.exit(0 if ok == len(CASES) else 1)
