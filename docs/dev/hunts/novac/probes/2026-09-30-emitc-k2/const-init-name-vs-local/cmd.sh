#!/bin/sh
# Probe const-init-name-vs-local: A CONSTANT is substituted at its read (emit_module_values.nv
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
