#!/bin/sh
# Probe priv-leak-moduleless: out of cell: a module-less file reads Vec's private field; the oracle refuses E_FIELD_MODULE_PRIVATE
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='len'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
