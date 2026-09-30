#!/bin/sh
# Probe generic-fn-lit-capture-one-inst: A handler literal CAPTURING `x T` inside a generic function, ONE instance.
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
