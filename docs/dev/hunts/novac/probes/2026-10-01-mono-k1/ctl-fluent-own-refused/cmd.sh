#!/bin/sh
# Probe ctl-fluent-own-refused: control: the SAME body shape in the file itself is refused by novac (oracle: 0, 0 per D409)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
