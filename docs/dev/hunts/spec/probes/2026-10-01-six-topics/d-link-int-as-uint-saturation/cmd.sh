#!/bin/sh
# С2 | as | d-link-int-as-uint-saturation
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`int as uint` saturation (cross-type bridge)' spec/decisions/02-types.md
grep -nF -- '**2. Q2 СНЯТ: `int as uint` — bit-pattern' spec/decisions/02-types.md
