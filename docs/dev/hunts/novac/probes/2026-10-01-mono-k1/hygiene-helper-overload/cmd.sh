#!/bin/sh
# Probe hygiene-helper-overload: same with an argument: the caller's weight(x) is called, 3000 instead of 6
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='weight'
export PROBE GREP
. "$PROBE/../run-probe.sh"
