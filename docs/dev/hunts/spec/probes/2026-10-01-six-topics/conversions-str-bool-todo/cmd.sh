#!/bin/sh
# С3 | as | conversions-str-bool-todo
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'str → bool — see the TODO above' spec/conversions.md
grep -nF -- '`s.to_bool()` — strictly' spec/conversions.md
