#!/bin/sh
# С3 | visibility | paradigm-pub-and-prefix
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Два уровня видимости: либо `pub`, либо нет.' spec/paradigm.ru.md
grep -nF -- '**`pub` (Rust-стиль)**' spec/decisions/07-modules.md
