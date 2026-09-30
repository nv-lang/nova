#!/bin/sh
# Probe with-same-effect-twice: Two bindings of ONE effect in one `with`: the swaps are keyed by the
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
