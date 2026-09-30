#!/bin/sh
# Probe ice-tuple-option-ret: side: ICE on fn returning (int, Option[int])
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
