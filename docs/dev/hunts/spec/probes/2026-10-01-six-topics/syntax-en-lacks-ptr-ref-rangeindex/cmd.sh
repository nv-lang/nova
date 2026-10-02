#!/bin/sh
# С6 | placement | syntax-en-lacks-ptr-ref-rangeindex
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '### Тип-ссылка: `ref T` и `ref mut T` (D480)' spec/syntax.ru.md
grep -nF -- '# Nova — syntax' spec/syntax.md
grep -cwE 'D(238|470|480|483)' spec/syntax.md   # 0 = none of the four is named
