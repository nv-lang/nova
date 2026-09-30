#!/bin/sh
# Probe if-outer-mut: D486 s4/s5 if-condition: if-outer-mut
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
