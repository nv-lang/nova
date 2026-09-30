#!/bin/sh
# Probe for-nested-variant: D486 s3/s4: for-nested-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
