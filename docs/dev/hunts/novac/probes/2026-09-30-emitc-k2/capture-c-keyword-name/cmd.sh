#!/bin/sh
# Probe capture-c-keyword-name: A captured binding named like a C keyword. The context field and its
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
