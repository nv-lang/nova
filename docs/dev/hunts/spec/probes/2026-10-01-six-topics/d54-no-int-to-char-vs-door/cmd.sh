#!/bin/sh
# С5 | as | d54-no-int-to-char-vs-door
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'СЕГОДНЯ НЕТ ВООБЩЕ' spec/decisions/03-syntax.md
grep -nF -- '**Единственная дверь** из числа в `char`' spec/decisions/03-syntax.md
