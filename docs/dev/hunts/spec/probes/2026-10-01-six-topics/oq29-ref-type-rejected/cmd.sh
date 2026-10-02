#!/bin/sh
# С3 | placement | oq29-ref-type-rejected
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '**`ref` как ТИП остаётся отвергнут** (D326 R1)' spec/open-questions.ru.md
grep -nF -- '`ref T` — **валидный тип**' spec/decisions/02-types.md
