#!/bin/sh
# С2 | as | d54-float-checked-none-vs-d430
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Для float → целое проверяемой формы **нет**;' spec/decisions/03-syntax.md
grep -nF -- 'Заводится **вторая бланкет-семья того же имени**' spec/decisions/04-effects.md
