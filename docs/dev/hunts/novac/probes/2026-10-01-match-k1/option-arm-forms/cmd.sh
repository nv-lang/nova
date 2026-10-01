#!/bin/sh
# Probe option-arm-forms: disjunction, guards, whole binder, field scrutinee, statement position over Option
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
