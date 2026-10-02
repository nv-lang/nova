#!/bin/sh
# С5 | fluent | d409-tail-discarded-vs-rule2
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'discard'"'"'ится как statement' spec/decisions/03-syntax.md
grep -nF -- 'кроме приёмника, — ошибка (как и раньше)' spec/decisions/03-syntax.md
