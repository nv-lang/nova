#!/bin/sh
# С5 | as | d130-saturates-not-struck
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`int as uint` cast **saturates** (negative → 0);' spec/decisions/02-types.md
grep -nF -- '**2. Q2 СНЯТ: `int as uint` — bit-pattern' spec/decisions/02-types.md
