#!/bin/sh
# Probe op-local-after-capture-read: In ONE op body the name `base` is first a CAPTURE, then a LOCAL of the op
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
