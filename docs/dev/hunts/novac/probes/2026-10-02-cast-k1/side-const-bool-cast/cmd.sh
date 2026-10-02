#!/bin/sh
# Side probe const-bool-cast: `const D = true as u8` -- the oracle prints 1,
# Carina refuses E_CONST_NOT_CONSTEXPR while it accepts `300 as u8`,
# `'A' as u8` and `3.9 as int` as constant initializers.
# Run from anywhere inside the repository: sh <this file>
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
