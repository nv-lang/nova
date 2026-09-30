#!/bin/sh
# Probe for-qualified-unit-variant: D486 s4: for-qualified-unit-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
