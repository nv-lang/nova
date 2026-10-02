#!/bin/sh
# С6 | as | conversions-normative-self-link
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'Russian original (normative): [conversions.md](conversions.md)' spec/conversions.md
grep -nF -- '# Nova — конверсии типов' spec/conversions.ru.md
grep -n 'Russian original (normative)' spec/*.md   # every English page links to itself
