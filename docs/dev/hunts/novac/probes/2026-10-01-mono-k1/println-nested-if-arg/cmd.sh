#!/bin/sh
# Probe println-nested-if-arg (a value `if` as an argument of a call that `println` prints, and one level deeper -- `println(idf(if c { 5 } else { 0 }))`, `println(take(idf(if ..)))`: the base checked them and its emitter died, "emit: expression kind outside the subset" (`@lower_println` hoisted the `??`/`?` forms of a call argument, not its value `if`); found in part 7 on the outer-argument cell)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_take'
export PROBE GREP
. "$PROBE/../run-probe.sh"
