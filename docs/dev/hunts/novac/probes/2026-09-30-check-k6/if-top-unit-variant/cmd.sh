#!/bin/sh
# Probe if-top-unit-variant: D486 s4/s5 if-condition: if-top-unit-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
