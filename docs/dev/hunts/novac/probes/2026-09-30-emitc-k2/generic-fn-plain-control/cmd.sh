#!/bin/sh
# Probe generic-fn-plain-control: Control for generic-fn-lit-capture-T: the same generic body, NO handler
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
