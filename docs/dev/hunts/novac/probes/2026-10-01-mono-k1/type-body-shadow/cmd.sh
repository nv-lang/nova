#!/bin/sh
# Probe type-body-shadow (#1567: the caller declares a type of the SAME name as the body module's private one, with another layout -- the body must build ITS type)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_geo_cellv|Cell'
export PROBE GREP
. "$PROBE/../run-probe.sh"
