#!/bin/sh
# С3 | as | conversions-rangeerror-unit
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`RangeError` — a unit type' spec/conversions.md
grep -nF -- '`type RangeError enum AboveMax | BelowMin`' spec/decisions/04-effects.md
