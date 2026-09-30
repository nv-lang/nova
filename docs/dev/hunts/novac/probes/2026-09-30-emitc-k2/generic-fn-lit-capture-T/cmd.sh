#!/bin/sh
# Probe generic-fn-lit-capture-T: handler literal inside a generic fn instantiated at int and float
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
