#!/bin/sh
# Probe array-literal-arg-ice (hunt 2026-09-30 check x K4)
export EMIT='internal compiler error'
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
