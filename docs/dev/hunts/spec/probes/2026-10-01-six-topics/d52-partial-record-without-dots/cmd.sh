#!/bin/sh
# С1 | patterns | d52-partial-record-without-dots
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '// partial без ..' spec/decisions/02-types.md
grep -nF -- '### §3. Единое правило пропуска' spec/decisions/03-syntax.md
