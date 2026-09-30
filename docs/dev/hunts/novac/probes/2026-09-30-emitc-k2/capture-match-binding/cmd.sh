#!/bin/sh
# Probe capture-match-binding: The captured binding is a MATCH-ARM binding and a FOR binding: their C
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
