#!/bin/sh
# Probe p01-leafgen (hunt 2026-10-06 progemit x K3, a multi-module program). Run from anywhere inside the
# repository: sh <this file>. NOVAC= overrides Carina's binary.
PROBE="$(cd "$(dirname "$0")" && pwd)"
NAME=p01_leafgen
export PROBE NAME
. "$PROBE/../run-probe.sh"
