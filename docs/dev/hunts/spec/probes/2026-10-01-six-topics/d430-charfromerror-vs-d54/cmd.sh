#!/bin/sh
# С2 | as | d430-charfromerror-vs-d54
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`CharFromError` (`int @to_char()`)' spec/decisions/04-effects.md
grep -nF -- '`type CharError enum AboveMax | BelowMin | Invalid`' spec/decisions/03-syntax.md
