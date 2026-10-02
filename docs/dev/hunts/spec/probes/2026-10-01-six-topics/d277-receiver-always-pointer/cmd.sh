#!/bin/sh
# С1 | placement | d277-receiver-always-pointer
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Receiver-ABI — always-pointer' spec/decisions/02-types.md
grep -nF -- '## D488.' spec/decisions/02-types.md
