#!/bin/sh
# Probe bind-nested-unit-variant: D486 s4 irrefutable binding: bind-nested-unit-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
