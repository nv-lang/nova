#!/bin/sh
# С2 | variants | prelude-option-without-enum
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'type Option[T] | Some(T) | None' spec/decisions/08-runtime.md
grep -nF -- 'type Option[T] enum Some(T) | None' spec/decisions/02-types.md
