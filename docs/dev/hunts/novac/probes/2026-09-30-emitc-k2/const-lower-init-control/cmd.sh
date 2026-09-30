#!/bin/sh
# Probe const-lower-init-control: control: constant initializer reading a constant, no shadowing local
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
