#!/bin/sh
# С3 | variants | conversions-bare-variant-silent
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'may silently pick another type'"'"'s variant' spec/conversions.md
grep -nF -- 'двух и есть то, что уже дало дефект №964' spec/decisions/02-types.md
