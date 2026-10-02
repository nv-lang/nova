#!/bin/sh
# С5 | variants | d55-option-wrap-vs-generic-payload
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'ro opt Option[str] = "alice"' spec/decisions/02-types.md
grep -nF -- 'payload = ГОЛЫЙ generic-параметр' spec/decisions/02-types.md
