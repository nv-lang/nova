#!/bin/sh
# Probe priv-module-line: the SAME handed body (@len read) is refused once the file declares a module: privacy judged by the caller's module
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_is_empty'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
