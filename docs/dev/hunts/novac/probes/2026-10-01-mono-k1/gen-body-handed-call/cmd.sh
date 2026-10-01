#!/bin/sh
# Probe gen-body-handed-call: a handed generic called inside the file's own generic: no instance is made, the linker refuses
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_is_empty|novac_fn_empty'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
