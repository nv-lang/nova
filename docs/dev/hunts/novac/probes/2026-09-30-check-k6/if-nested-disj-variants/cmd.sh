#!/bin/sh
# Probe if-nested-disj-variants: D486 s4 cond: if-nested-disj-variants
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
