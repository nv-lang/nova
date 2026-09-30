#!/bin/sh
# Probe const-init-local-other-type: The constant's initializer `step + 10` is typed at EACH READ, in the
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
