#!/bin/sh
# С5 | visibility | d47-export-method-warning-vs-error
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'visibility limited to module' spec/decisions/07-modules.md
grep -nF -- 'E_PRIVATE_TYPE_IN_PUBLIC' spec/decisions/07-modules.md
