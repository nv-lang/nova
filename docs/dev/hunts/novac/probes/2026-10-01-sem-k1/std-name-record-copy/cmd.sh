#!/bin/sh
# Probe std-name-record-copy: a program record named like std Permissions takes std value layout: a copy instead of an alias
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='Permissions'
export PROBE GREP EMIT_ALSO
. "$PROBE/../run-probe.sh"
