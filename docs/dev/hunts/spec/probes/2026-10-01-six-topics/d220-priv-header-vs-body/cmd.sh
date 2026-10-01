#!/bin/sh
# С5 | visibility | d220-priv-header-vs-body
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Explicit `priv` — field accessible **только из методов own type'"'"'а**' spec/decisions/02-types.md
grep -nF -- 'Field-level explicit `priv` тоже module-private' spec/decisions/02-types.md
