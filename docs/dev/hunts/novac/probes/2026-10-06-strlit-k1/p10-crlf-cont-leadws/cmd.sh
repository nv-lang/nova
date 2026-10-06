#!/bin/sh
# Probe p10-crlf-cont-leadws (hunt 2026-10-06 strlit x K1, run probe). Run from anywhere inside the
# repository: sh <this file>. NOVAC=/NOVA= override the binaries.
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
