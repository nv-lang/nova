#!/bin/sh
# С3 | patterns | syntax-closed-table-lacks-array
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Перечень форм образца ЗАКРЫТ' spec/syntax.ru.md
grep -nF -- 'array-pat | disjunction' spec/decisions/03-syntax.md
