#!/bin/sh
# Probe ctl-index-contract: control: the requires of a handed body fires as in the oracle
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_index'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
