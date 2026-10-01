#!/bin/sh
# Probe shell-overload-cap-n: v.cap(100) on Vec[int] is sent to the shell's cap() -- the shell is asked by NAME, not by overload
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='method_cap'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
