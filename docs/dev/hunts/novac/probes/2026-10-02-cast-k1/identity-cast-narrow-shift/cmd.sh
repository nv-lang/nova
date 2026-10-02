#!/bin/sh
# Probe identity-cast-narrow-shift: `e as T` with `e` already of type T is
# emitted as the bare operand (novac/src/emit_c/emit_cast.nv, `from == to`),
# so a u8/i8/u16/i16 shift keeps its C-promoted bits through the cast.
# Run from anywhere inside the repository: sh <this file>
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
