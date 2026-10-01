#!/bin/sh
# Probe ctl-own-record-std-name: control: the same record declared in the compiled file itself is refused by name (shell struct clash)
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
