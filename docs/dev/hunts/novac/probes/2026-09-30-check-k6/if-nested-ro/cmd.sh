#!/bin/sh
# Probe if-nested-ro: D486 s4/s5 if-condition: if-nested-ro
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
