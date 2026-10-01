#!/bin/sh
# Probe hygiene-helper-fn: a handed generic body calls the CALLER's private helper of the same name
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='helper'
export PROBE GREP
. "$PROBE/../run-probe.sh"
