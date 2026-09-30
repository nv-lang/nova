#!/bin/sh
# Probe bind-ro-binder: D486 s4: bind-ro-binder
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
