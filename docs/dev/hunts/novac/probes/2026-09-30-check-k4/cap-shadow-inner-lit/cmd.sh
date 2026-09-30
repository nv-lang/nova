#!/bin/sh
# Probe cap-shadow-inner-lit (hunt 2026-09-30 check x K4)
export RUN='1'
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
