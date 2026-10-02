#!/bin/sh
# С2 | variants | d406-colon-annotation
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'ro x: enum Some(int) | None' spec/decisions/02-types.md
grep -nF -- '## Аннотации типа — форма «name type», без двоеточия' spec/syntax.ru.md
