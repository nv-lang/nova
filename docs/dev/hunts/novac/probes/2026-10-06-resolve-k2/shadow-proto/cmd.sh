#!/bin/sh
# Probe (hunt 2026-10-06 resolve x K2). Run from anywhere inside the repository:
# sh <this file>. NOVAC=/NOVA= override the binaries.
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-check.sh"
