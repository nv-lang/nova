#!/bin/sh
# Probe ctl-hygiene-no-shadow: control: without a same-named helper in the caller the body names the module's own helper (link fails: step 2a)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='helper'
export PROBE GREP
. "$PROBE/../run-probe.sh"
