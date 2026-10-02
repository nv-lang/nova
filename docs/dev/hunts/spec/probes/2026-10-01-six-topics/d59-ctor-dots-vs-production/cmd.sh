#!/bin/sh
# С2 | patterns | d59-ctor-dots-vs-production
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Cons(h, ..) => "head: ${h}"' spec/decisions/03-syntax.md
grep -nF -- 'constructor = qual-head ['"'"'('"'"' pattern ('"'"','"'"' pattern)* '"'"')'"'"']' spec/decisions/03-syntax.md
