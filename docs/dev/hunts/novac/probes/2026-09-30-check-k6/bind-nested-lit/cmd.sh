#!/bin/sh
# Probe bind-nested-lit: D486 s4 irrefutable binding: bind-nested-lit
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
