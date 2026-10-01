#!/bin/sh
# С3 | patterns | oq-pattern-mut-open
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'В match/let-pattern'"'"'ах нельзя использовать `mut`-модификатор' spec/open-questions.ru.md
grep -nF -- '| `mut` на биндере | ✅ | ✅ | ✅ |' spec/decisions/03-syntax.md
