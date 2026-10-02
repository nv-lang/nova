#!/bin/sh
# С1 | patterns | d184-consume-in-cond-unnarrowed
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '- **`consume` запрещён** в conditions — `E_CONSUME_IN_CONDITION`.' spec/decisions/03-syntax.md
grep -nF -- '### §5. `consume` в условиях — амендмент D34 правила 3' spec/decisions/03-syntax.md
