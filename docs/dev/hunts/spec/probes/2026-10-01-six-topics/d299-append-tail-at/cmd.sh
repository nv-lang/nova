#!/bin/sh
# С2 | fluent | d299-append-tail-at
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'mut @append[S AsSlice[T]](other S) -> @ {' spec/decisions/02-types.md
grep -nF -- 'явный `@` в хвосте, `return @`, `=> @`' spec/decisions/03-syntax.md
