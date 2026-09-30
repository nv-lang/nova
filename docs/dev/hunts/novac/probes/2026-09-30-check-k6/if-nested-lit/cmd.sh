#!/bin/sh
# Probe if-nested-lit: D486 s4/s5 if-condition: if-nested-lit
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
