#!/bin/sh
# Probe fluent-return-other-recv: return of a non-receiver value of the receiver type in a -> @ block body (D409 rule 2)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='return (o|_novac_self);'
export PROBE GREP NOSELF
. "$PROBE/../run-probe.sh"
