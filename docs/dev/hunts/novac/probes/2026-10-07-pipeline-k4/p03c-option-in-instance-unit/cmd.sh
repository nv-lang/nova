#!/bin/sh
# Probe p03c-option-in-instance-unit (hunt 2026-10-07 pipeline x K4, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p03c_option_in_instance_unit
SUMS="Color"
export PROBE NAME SUMS
. "$PROBE/../run-probe.sh"
