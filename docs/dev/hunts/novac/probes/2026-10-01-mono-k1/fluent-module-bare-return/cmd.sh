#!/bin/sh
# Probe fluent-module-bare-return (#1520): a bare `return` in a '-> @' extension method of a program module, instantiated in the caller's unit, is `return @` (D409)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_mark_if|return _novac_self'
export PROBE GREP
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
