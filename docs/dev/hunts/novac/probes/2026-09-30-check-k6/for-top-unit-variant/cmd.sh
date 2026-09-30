#!/bin/sh
# Probe for-top-unit-variant: D486 s4 for-header = binding rules: for-top-unit-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
