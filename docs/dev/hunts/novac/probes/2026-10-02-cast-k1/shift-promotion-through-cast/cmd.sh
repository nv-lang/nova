#!/bin/sh
# Probe shift-promotion-through-cast: `(a << 4) as u16` with `a u8 = 200`
# prints 3200 in BOTH compilers (norm 128); bound first, the same value is 128.
# Run from anywhere inside the repository: sh <this file>
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
