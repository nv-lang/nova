#!/bin/sh
# С1 | variants | d17-banner-announces-old-sum
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`type X | A | B` (sum)' spec/decisions/02-types.md
grep -nF -- '## D406. Sum-type синтаксис' spec/decisions/02-types.md
