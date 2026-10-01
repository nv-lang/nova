#!/bin/sh
# Probe fluent-handed-bare-return (#1520): a bare `return` in a handed std '-> @' body (Vec @dedup: the bare `return` of a short vector, and the fall-through of a long one) is `return @` (D409)
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_dedup|return _novac_self'
export PROBE GREP NOSELF
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
