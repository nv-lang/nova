#!/bin/sh
# Probe bind-top-unit-variant: D486 s3/s4: bind-top-unit-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
