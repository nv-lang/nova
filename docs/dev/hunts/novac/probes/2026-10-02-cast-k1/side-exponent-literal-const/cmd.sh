#!/bin/sh
# Side probe exponent-literal-const: `const E = 1e20 as i32` -- the oracle
# prints 2147483647, Carina stops with E_NOVAC_ICE.
# Run from anywhere inside the repository: sh <this file>
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
