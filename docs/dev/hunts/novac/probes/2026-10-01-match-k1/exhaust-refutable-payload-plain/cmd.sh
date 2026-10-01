#!/bin/sh
# Probe exhaust-refutable-payload-plain: A(3)/B(true)/C without catch-all: coverage judged by the head
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
