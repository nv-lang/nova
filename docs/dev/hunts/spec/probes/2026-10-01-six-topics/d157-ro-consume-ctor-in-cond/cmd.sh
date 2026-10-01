#!/bin/sh
# С2 | patterns | d157-ro-consume-ctor-in-cond
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'if ro consume Some(t) = opt { t.commit() }' spec/decisions/05-memory.md
grep -nF -- '`if consume Some(x) = …` — по-прежнему `E_CONSUME_IN_CONDITION`' spec/decisions/03-syntax.md
# C7 -- what the compiler says. The package is stored as *.txt (a .nv in docs/ would be
# swept by the corpus tools): copy the directory out, drop the .txt suffixes, then
#   nova build main.nv -o probe.exe && ./probe.exe
# seen 2026-10-01 on main 0b296258b + #1559/#1556/#1572: `nova build` -> собирается, печатает `4` (`if ro consume Some(t) = opt` принято)
