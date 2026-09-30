#!/bin/sh
# Probe bind-top-variant: D486 s4 irrefutable binding: bind-top-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
