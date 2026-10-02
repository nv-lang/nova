#!/bin/sh
# С2 | patterns | d432-if-let-example
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`if let Ok(consume r) = e { … }' spec/decisions/02-types.md
grep -nF -- '**`if let` (Rust-style outer `let`)** — Plan 114 retracted' spec/decisions/03-syntax.md
