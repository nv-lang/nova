#!/bin/sh
# Probe mv-holds-handler-lit: A handler literal as a MODULE VALUE's initializer: the literal stands
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
