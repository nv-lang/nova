#!/bin/sh
# Probe const-narrow-shift-inlined: `const S = (200 as u8) << 4` prints 128 in
# the oracle and 3200 in Carina -- the const is emitted as its initializer
# expression at the use site, not as a value of its type.
# Run from anywhere inside the repository: sh <this file>
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
