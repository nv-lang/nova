#!/bin/sh
# С5 | visibility | d29-cycles-forbidden-vs-d291
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '**Запрет циклов** — сильнее, чем в Rust' spec/decisions/07-modules.md
grep -nF -- '**ОТМЕНЕНО D291**' spec/decisions/07-modules.md
