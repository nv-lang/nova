#!/bin/sh
# Probe local-c-keyword-control: Control: an ordinary local named like a C keyword (`long`), no handler.
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
