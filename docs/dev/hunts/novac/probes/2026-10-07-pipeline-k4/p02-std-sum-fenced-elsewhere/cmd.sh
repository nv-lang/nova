#!/bin/sh
# Probe p02-std-sum-fenced-elsewhere (hunt 2026-10-07 pipeline x K4, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p02_std_sum_fenced_elsewhere
SUMS="PathStyle"
export PROBE NAME SUMS
. "$PROBE/../run-probe.sh"
