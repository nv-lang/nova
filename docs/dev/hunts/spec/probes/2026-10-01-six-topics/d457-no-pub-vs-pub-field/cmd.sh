#!/bin/sh
# С2 | visibility | d457-no-pub-vs-pub-field
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Слово `pub` в язык не' spec/decisions/02-types.md
grep -nF -- 'explicit `pub` modifier overrides priv default' spec/decisions/02-types.md
# C7 -- what the compiler says. The package is stored as *.txt (a .nv in docs/ would be
# swept by the corpus tools): copy the directory out, drop the .txt suffixes, then
#   nova build main.nv -o probe.exe && ./probe.exe
# seen 2026-10-01 on main 0b296258b + #1559/#1556/#1572: `nova build` -> собирается, печатает `1` (поле `pub id int` принято)
