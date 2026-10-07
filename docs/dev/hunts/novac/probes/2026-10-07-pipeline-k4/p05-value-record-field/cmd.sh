#!/bin/sh
# Probe p05-value-record-field (hunt 2026-10-07 pipeline x K4, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p05_value_record_field
SUMS="Color"
export PROBE NAME SUMS
. "$PROBE/../run-probe.sh"
