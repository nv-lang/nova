#!/bin/sh
# С4 | variants | own-module-first-not-in-spec
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'сужает владельцев до сумм модуля читающего файла' docs/plans/221.1-bug-sweep.md
grep -nF -- 'Имя варианта, видимое РОВНО ИЗ ОДНОЙ суммы' spec/decisions/02-types.md
