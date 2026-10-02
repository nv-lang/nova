#!/bin/sh
# С5 | patterns | d34-body-vs-header
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '3. **`consume` запрещён** в conditions — `E_CONSUME_IN_CONDITION`.' spec/decisions/03-syntax.md
grep -nF -- 'правило 3 сужено — запрещён только OUTER `consume`' spec/decisions/03-syntax.md
