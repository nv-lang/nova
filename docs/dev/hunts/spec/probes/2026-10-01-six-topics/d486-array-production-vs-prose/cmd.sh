#!/bin/sh
# С5 | patterns | d486-array-production-vs-prose
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'array-pat   = '"'"'['"'"'' spec/decisions/03-syntax.md
grep -nF -- 'как в D59 без изменений: `[a, .., z]`, `[head, ..rest]`' spec/decisions/03-syntax.md
