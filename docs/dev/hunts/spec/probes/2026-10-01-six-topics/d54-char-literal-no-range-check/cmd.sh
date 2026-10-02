#!/bin/sh
# С5 | as | d54-char-literal-no-range-check
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'range-check не нужен' spec/decisions/03-syntax.md
grep -nF -- 'ошибка компиляции `E_LIT_OUT_OF_RANGE`' spec/decisions/03-syntax.md
