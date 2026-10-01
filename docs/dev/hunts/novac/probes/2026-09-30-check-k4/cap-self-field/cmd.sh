#!/bin/sh
# Probe cap-self-field (hunt 2026-09-30 check x K4)
export EMIT='_novac_self|_novac_hv1->ctx'
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
