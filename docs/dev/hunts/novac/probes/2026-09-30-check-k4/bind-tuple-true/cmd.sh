#!/bin/sh
# Probe bind-tuple-true (hunt 2026-09-30 check x K4)
export EMIT='nova_bool true'
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
