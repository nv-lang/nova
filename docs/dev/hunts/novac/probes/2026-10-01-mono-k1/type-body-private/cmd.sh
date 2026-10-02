#!/bin/sh
# Probe type-body-private (#1567: a PRIVATE type of a program module, constructed and read in its generic method; the caller has no type of that name)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_cellv|Cell'
export PROBE GREP
. "$PROBE/../run-probe.sh"
