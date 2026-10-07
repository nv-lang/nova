#!/bin/sh
# Probe p01-std-sum-fenced (hunt 2026-10-07 pipeline x K4, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p01_std_sum_fenced
SUMS="PathStyle"
export PROBE NAME SUMS
. "$PROBE/../run-probe.sh"
