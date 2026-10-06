#!/bin/sh
# Probe p03-result-record (hunt 2026-10-06 progemit x K3, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p03_result_record
export PROBE NAME
. "$PROBE/../run-probe.sh"
