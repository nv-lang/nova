#!/bin/sh
# С3 | variants | syntax-details-d17
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Details — [D17](decisions/02-types.md#d17).' spec/syntax.md
grep -nF -- '⚠️ **REVISED.** Заменено [D52]' spec/decisions/02-types.md
