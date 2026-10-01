#!/bin/sh
# Probe shell-overload-index-set: v.index(1, 9) on Vec[int] is sent to the shell's getter index(i)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='method_index'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
