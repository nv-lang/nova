#!/bin/sh
# Probe generic-fn-lit-no-capture: A handler literal with NO capture inside a generic function, two instances.
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
