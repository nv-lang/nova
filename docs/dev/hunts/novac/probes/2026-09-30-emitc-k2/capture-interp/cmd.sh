#!/bin/sh
# Probe capture-interp: A captured binding read inside an interpolation slot of an op body: is
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
