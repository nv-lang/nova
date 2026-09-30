#!/bin/sh
# Probe bind-tuple-rest-trailing: D486 s2/s3 tuple rest: bind-tuple-rest-trailing
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
