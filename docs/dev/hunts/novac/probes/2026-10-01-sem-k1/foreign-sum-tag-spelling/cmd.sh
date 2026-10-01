#!/bin/sh
# Probe foreign-sum-tag-spelling: a sum of hp.geo: consumer spells NOVAC_TAG_hp_Dir_*, nova_make_Dir_*; owner spells NOVAC_TAG_hp_geo_Dir_*, novac_make_hp_geo_Dir_*
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='NOVAC_TAG_|novac_make_|nova_make_Dir'
EMIT_ALSO='src/geo/geo.nv'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
