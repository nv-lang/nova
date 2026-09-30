#!/bin/sh
# Probe if-top-wild: D486 s4 cond: if-top-wild
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
