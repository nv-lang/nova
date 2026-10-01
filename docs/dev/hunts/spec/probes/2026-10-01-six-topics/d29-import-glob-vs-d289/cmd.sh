#!/bin/sh
# С5 | visibility | d29-import-glob-vs-d289
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`E_REEXPORT_GLOB` (D288) + `E_IMPORT_GLOB` (D289)' spec/decisions/07-modules.md
grep -nF -- '`E_IMPORT_GLOB` **убран**' spec/decisions/07-modules.md
