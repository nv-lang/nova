#!/bin/sh
# С5 | variants | runtimeerror-two-forms
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'type RuntimeError enum DivByZero | Overflow' spec/decisions/04-effects.md
grep -nF -- '    | DivByZero' spec/decisions/04-effects.md
