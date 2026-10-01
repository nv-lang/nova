#!/bin/sh
# Probe fluent-module-mark: a '-> @' extension method of a program module, instantiated in the caller's unit, returns garbage
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_mark|nova_int x = '
export PROBE GREP
. "$PROBE/../run-probe.sh"
