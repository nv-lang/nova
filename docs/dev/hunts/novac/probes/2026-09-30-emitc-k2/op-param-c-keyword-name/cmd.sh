#!/bin/sh
# Probe op-param-c-keyword-name: An op PARAMETER named like a C keyword: the op function's C parameter is
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
