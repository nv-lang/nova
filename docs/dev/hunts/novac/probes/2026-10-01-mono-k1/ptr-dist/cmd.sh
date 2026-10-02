#!/bin/sh
# Probe ptr-dist (the raw-pointer intrinsic `p.dist(q) -> int` (D216, the methods amendment) -- the form of source/source.nv `s.ptr().dist(@text.ptr())` -- refused "no such method in the declarations handed to novac" (one in the 0.2 measure))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_gap'
export PROBE GREP
. "$PROBE/../run-probe.sh"
