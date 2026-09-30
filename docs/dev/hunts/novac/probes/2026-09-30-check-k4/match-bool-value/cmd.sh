#!/bin/sh
# Probe match-bool-value (hunt 2026-09-30 check x K4)
export RUN='1'
export EMIT='pick|bool true|bool false|= b;|T[0-9]+ = '
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
