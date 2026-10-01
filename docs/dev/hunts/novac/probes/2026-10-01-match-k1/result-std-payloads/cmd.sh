#!/bin/sh
# Probe result-std-payloads: Result from std: the Err payload read, a guard, a whole binder
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='tag == NOVA_TAG_Result'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
