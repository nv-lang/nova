#!/bin/sh
# С2 | visibility | d5-two-levels-vs-priv-ladder
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Два уровня видимости: **`export`**' spec/decisions/07-modules.md
grep -nF -- '`priv(file)` ⊂ (module-default) ⊂ `export`' spec/decisions/02-types.md
