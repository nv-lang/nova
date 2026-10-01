#!/bin/sh
# Probe instance-name-collision-module: in a file with a module line the std instance takes the FILE's module prefix and meets the file's own generic first
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_hp_first'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
