#!/bin/sh
# С2 | as | d54-suffix-literal-examples
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`-1i32 as u16 == 65535`' spec/decisions/03-syntax.md
grep -nF -- '100u32        // ✗ syntax error (D44)' spec/decisions/03-syntax.md
# C7 -- what the compiler says. The package is stored as *.txt (a .nv in docs/ would be
# swept by the corpus tools): copy the directory out, drop the .txt suffixes, then
#   nova build main.nv -o probe.exe && ./probe.exe
# seen 2026-10-01 on main 0b296258b + #1559/#1556/#1572: `nova build` -> `[E_STMT_SEP_MISSING]` на `300u32` (суффикс не литерал)
