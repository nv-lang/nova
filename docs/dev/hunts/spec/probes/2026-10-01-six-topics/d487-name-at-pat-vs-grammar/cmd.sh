#!/bin/sh
# С2 | patterns | d487-name-at-pat-vs-grammar
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '`name @ pat`, `..rest`, record-сокращение `{ x }`' spec/decisions/03-syntax.md
grep -nF -- 'Продолжение D19: перечень форм ЗАКРЫТ' spec/decisions/03-syntax.md
