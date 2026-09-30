#!/bin/sh
# Probe capture-destructured-field: The captured binding comes from a record DESTRUCTURE (`ro { n, .. } = c`):
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
