#!/bin/sh
# Probe std-name-record-overflow: same record: u8 field typed as std int field -- oracle panics on overflow, Carina prints 400
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='Permissions'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
