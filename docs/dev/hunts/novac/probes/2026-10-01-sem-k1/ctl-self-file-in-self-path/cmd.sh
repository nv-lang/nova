#!/bin/sh
# Probe ctl-self-file-in-self-path: control: the compiled file is itself under NOVAC_SELF_PATH (overloads, generic, method) -- same as oracle
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
