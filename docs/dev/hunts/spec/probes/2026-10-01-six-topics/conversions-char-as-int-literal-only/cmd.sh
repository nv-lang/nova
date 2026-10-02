#!/bin/sh
# С3 | as | conversions-char-as-int-literal-only
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'for char literals (compile-time-known codepoint), see D54.' spec/conversions.md
grep -nF -- 'любую кодовую точку:** `i32`, `u32`, `int`, `uint`' spec/decisions/03-syntax.md
