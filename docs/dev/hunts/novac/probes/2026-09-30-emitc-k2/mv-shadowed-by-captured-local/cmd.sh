#!/bin/sh
# Probe mv-shadowed-by-captured-local: Registry 1397 kin: a LOCAL carries the name of a module value; the op body
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
