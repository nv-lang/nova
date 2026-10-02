#!/bin/sh
# С2 | fluent | d132-example-bare-at-vs-d409
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '// ✅ bare @' spec/decisions/03-syntax.md
grep -nF -- 'явный `@` в хвосте, `return @`, `=> @`' spec/decisions/03-syntax.md
