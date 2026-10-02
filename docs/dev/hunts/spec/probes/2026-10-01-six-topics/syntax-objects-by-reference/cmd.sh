#!/bin/sh
# С3 | placement | syntax-objects-by-reference
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Objects (record, sum-type, arrays) are passed **by reference**' spec/syntax.md
grep -nF -- '**Сумма размещается ПО ЗНАЧЕНИЮ**' spec/decisions/02-types.md
