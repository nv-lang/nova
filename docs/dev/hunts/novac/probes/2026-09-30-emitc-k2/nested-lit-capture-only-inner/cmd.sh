#!/bin/sh
# Probe nested-lit-capture-only-inner: rerun with the oracle copy
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
