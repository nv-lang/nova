#!/bin/sh
# С1 | placement | d280-any-sum-heap
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'в указатель `Nova_X*`' spec/decisions/08-runtime.md
grep -nF -- '**Сумма размещается ПО ЗНАЧЕНИЮ**' spec/decisions/02-types.md
