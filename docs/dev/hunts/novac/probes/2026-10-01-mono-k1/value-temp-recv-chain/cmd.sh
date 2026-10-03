#!/bin/sh
# Probe value-temp-recv-chain (a `-> @` CHAIN over a value temporary and over a named value receiver (D32, D488; R6/R8: `-> @` of a `mut @` is `mut ref @`, a copy only at a binding, return or store). THE ORACLE DOES NOT COMPILE IT: its C takes `.` on `NovaValue_Pv *` ("member reference type is a pointer"). Carina prints 6 6 6)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_set'
export PROBE GREP
. "$PROBE/../run-probe.sh"
