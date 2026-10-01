#!/bin/sh
# Probe instance-skips-subset-walk (new): the instance of a handed body is typed (`check_instance` -> `type_fn`) but never walked by the subset/shape judges; `ro s str = @len()` in a program module's `Vec[T] @bad` is accepted and run (prints 0), where the oracle refuses E7301 at geo.nv:4 and Carina refuses the same line in a file of its own ("a declared local type is not compiled yet")
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_bad|nova_str s'
export PROBE GREP
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
