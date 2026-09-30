#!/bin/sh
# Probe nested-lit-capture-outer-param: Control: an inner literal reads the OUTER op's own parameter `label` -- a
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
