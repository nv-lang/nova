#!/bin/sh
# С2 | placement | d176-as-bytes-name
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'fn str @as_bytes() -> ro []u8' spec/decisions/02-types.md
grep -nF -- '`#coerce export fn str @bytes() -> ro []u8`' spec/decisions/02-types.md
