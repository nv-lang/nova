#!/bin/sh
# Probe while-nested-ro: D486 s4: while-nested-ro
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
