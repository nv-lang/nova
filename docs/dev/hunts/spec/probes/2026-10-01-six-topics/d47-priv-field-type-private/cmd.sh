#!/bin/sh
# С1 | visibility | d47-priv-field-type-private
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'accessible **только из методов own type'"'"'а**' spec/decisions/07-modules.md
grep -nF -- '## D281. Module-level field privacy' spec/decisions/02-types.md
