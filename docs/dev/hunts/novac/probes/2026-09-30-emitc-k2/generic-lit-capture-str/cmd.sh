#!/bin/sh
# Probe generic-lit-capture-str: A handler literal inside a GENERIC function, capturing only a NON-generic
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
