#!/bin/sh
# С5 | visibility | d47-prefix-convention-cancelled
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Convention `_prefix` не enforced' spec/decisions/07-modules.md
grep -nF -- 'Заменена на compile-time `priv` field modifier' spec/decisions/07-modules.md
