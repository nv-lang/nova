#!/bin/sh
# Probe generic-lit-capture-T-held: A handler literal inside a GENERIC function capturing `x T`: the context
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
