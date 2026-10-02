#!/bin/sh
# С2 | visibility | d47-whole-reexport-example
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'export import std.duration' spec/decisions/07-modules.md
grep -nF -- 'items=None && alias=None' spec/decisions/07-modules.md
# C7 -- what the compiler says. The package is stored as *.txt (a .nv in docs/ would be
# swept by the corpus tools): copy the directory out, drop the .txt suffixes, then
#   nova check lib.nv
# seen 2026-10-01 on main 0b296258b + #1559/#1556/#1572: `nova check lib.nv` -> `[E_REEXPORT_GLOB] export import std.time re-exports the entire module`
