#!/bin/sh
# С2 | variants | d55-table-let
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`let x T = value` (явная аннотация)' spec/decisions/02-types.md
grep -nF -- '## D184. Keyword refresh' spec/decisions/03-syntax.md
