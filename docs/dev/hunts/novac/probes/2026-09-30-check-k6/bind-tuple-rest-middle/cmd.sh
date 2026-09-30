#!/bin/sh
# Probe bind-tuple-rest-middle: D486 s2/s3 tuple rest: bind-tuple-rest-middle
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
