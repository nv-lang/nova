#!/bin/sh
# Probe for-record-partial: D486 s3/s4: for-record-partial
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
