#!/bin/sh
# Probe const-upper-init-control: Control, upper-case: a constant whose initializer reads another constant,
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
