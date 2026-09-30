#!/bin/sh
# Probe op-param-shadows-capture: An op PARAMETER carries the name of a binding of the enclosing scope. In
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
