#!/bin/sh
# С5 | fluent | d409-rule3-no-delegation
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Исключение — делегация в другой' spec/decisions/03-syntax.md
grep -nF -- '3. Explicit `@`/`return @`/`=> @` в `-> @`-теле' spec/decisions/03-syntax.md
