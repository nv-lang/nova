#!/bin/sh
# Probe hygiene-const-oracle-fails: observation: a module const in the body -- novac reads the caller's SCALE; the oracle does not build
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='SCALE'
export PROBE GREP
. "$PROBE/../run-probe.sh"
