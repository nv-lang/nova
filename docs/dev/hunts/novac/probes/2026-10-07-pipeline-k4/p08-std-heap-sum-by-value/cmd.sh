#!/bin/sh
# Probe p08-std-heap-sum-by-value (hunt 2026-10-07 pipeline x K4, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p08_std_heap_sum_by_value
SUMS="Align CharError RangeError"
export PROBE NAME SUMS
. "$PROBE/../run-probe.sh"
