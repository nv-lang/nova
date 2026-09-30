#!/bin/sh
# Probe if-ro-name: D486 s4: if-ro-name
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
