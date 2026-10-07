#!/bin/sh
# Probe p10-tuple-in-instance-unit (hunt 2026-10-07 pipeline x K4, a single-file unit). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p10_tuple_in_instance_unit
SUMS="Pt"
export PROBE NAME SUMS
. "$PROBE/../run-probe.sh"
