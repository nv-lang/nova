#!/bin/sh
# С2 | as | d227-saturation-wording
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '(D54 saturation rules apply)' spec/decisions/03-syntax.md
grep -nF -- '| `iN → uM` | bit-pattern truncate |' spec/decisions/03-syntax.md
