#!/bin/sh
# Probe shadow-same-block-capture: Two bindings named `x` in one block; the literal captures the SECOND.
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
