#!/bin/sh
# Probe cond-some-3-control (hunt 2026-09-30 check x K4)
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
