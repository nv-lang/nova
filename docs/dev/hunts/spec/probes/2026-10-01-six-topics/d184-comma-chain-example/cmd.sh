#!/bin/sh
# С2 | patterns | d184-comma-chain-example
# Run from the repository root: both places print, with their line numbers.
# A missing line is itself the news -- the place moved or was fixed.
grep -nF -- 'if Some(user) = lookup(id), user.is_active {' spec/decisions/03-syntax.md
grep -nF -- '**Запятая-chain (Swift-стиль)** — отвергнута' spec/decisions/03-syntax.md
