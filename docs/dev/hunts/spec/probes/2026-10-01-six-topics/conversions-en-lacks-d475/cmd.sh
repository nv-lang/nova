#!/bin/sh
# С6 | as | conversions-en-lacks-d475
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '**И ОБРАТНАЯ СТОРОНА ТОЖЕ НАША (план 285, D475, 2026-09-13).**' spec/conversions.ru.md
grep -nF -- '# Nova — type conversions' spec/conversions.md
grep -c -w D475 spec/conversions.md   # 0 = the English page never names D475
