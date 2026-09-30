#!/bin/sh
# Probe match-tuple-rest-middle: D486 s2/s3 tuple rest: match-tuple-rest-middle
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
