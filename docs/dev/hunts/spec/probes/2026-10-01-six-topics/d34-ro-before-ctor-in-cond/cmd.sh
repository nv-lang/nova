#!/bin/sh
# С2 | patterns | d34-ro-before-ctor-in-cond
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'if ro Some(user) = db.find(id) && user.is_active' spec/decisions/03-syntax.md
grep -nF -- 'Слово перед паттерном-КОНСТРУКТОРОМ — outer-режим, и он в условии запрещён' spec/decisions/03-syntax.md
# C7 -- what the compiler says. The package is stored as *.txt (a .nv in docs/ would be
# swept by the corpus tools): copy the directory out, drop the .txt suffixes, then
#   nova build main.nv -o probe.exe && ./probe.exe
# seen 2026-10-01 on main 0b296258b + #1559/#1556/#1572: `nova build` -> собирается, печатает `u 3` (`if ro Some(user) = ... && ...` принято)
