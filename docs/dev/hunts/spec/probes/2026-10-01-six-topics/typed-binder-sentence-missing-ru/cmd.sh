#!/bin/sh
# С6 | patterns | typed-binder-sentence-missing-ru
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'refused (`E_CONSUME_IN_CONDITION`). Any name in a' spec/syntax.md
grep -nF -- 'только режим перед всем паттерном (`E_CONSUME_IN_CONDITION`)' spec/syntax.ru.md
