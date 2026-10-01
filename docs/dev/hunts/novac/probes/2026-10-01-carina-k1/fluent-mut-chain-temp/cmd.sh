#!/bin/sh
# Probe fluent-mut-chain-temp (#1540): a fluent `mut` chain over the receiver a `-> @` call returned (`c.reset().set_pos(4)`, D181) is refused by Carina under P14 "needs a mutable place"; the oracle compiles it
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_reset|novac_fn_set_pos'
export PROBE GREP NOSELF
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
