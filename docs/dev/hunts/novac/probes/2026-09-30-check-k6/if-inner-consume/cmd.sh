#!/bin/sh
# Probe if-inner-consume: D486 s4/s5 if-condition: if-inner-consume
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
