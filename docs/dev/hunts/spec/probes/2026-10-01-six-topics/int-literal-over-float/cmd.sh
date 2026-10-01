#!/bin/sh
# С4 | patterns | int-literal-over-float
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'целый литерал в позиции float законен, D44' docs/plans/221.1-bug-sweep.md
grep -nF -- 'целый — только над целым типом' spec/decisions/03-syntax.md
# C7 -- what the compiler says. The package is stored as *.txt (a .nv in docs/ would be
# swept by the corpus tools): copy the directory out, drop the .txt suffixes, then
#   nova build main.nv -o probe.exe && ./probe.exe
# seen 2026-10-01 on main 0b296258b + #1559/#1556/#1572: `nova build` -> собирается, печатает `10 0` (образец `1` над `f64` совпал с `1.0`)
