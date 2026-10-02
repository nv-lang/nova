#!/bin/sh
# Probe ctor-field-type-unchecked (hunt 2026-10-03 match-value x K1). Run from anywhere inside the
# repository: sh <this file>. NOVAC=/NOVA= override the binaries.
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE
. "$PROBE/../run-probe.sh"
