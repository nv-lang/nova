#!/bin/sh
# С2 | patterns | d221-six-sites-vs-d486-five
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- '### §2 Pattern sites — complete enumeration' spec/decisions/02-types.md
grep -nF -- '### §1. Пять сайтов деструктуризации' spec/decisions/03-syntax.md
