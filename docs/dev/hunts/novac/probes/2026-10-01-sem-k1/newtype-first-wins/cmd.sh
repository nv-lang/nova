#!/bin/sh
# Probe newtype-first-wins: two program modules declare newtype Id; the imported one (int) gets the other (u8) term: false refusal
PROBE="$(cd "$(dirname "$0")" && pwd)"
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
