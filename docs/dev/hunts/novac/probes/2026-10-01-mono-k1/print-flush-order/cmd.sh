#!/bin/sh
# Probe print-flush-order (the END of a print statement flushes stdout (registry 1703; the oracle since 1655, `nova_print_end()`): `println`, `print` (the statement without the newline) and a panic after them -- the probe captures stdout and stderr in one stream, so the order of the lines is the question. Oracle `a|7b` `x` `piece|panic: ...`. Before: Carina printed the panic message first, and refused `print`)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='nova_print_end'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
