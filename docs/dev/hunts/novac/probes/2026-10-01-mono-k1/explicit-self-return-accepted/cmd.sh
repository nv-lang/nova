#!/bin/sh
# Probe explicit-self-return-accepted (new, K7): an explicit self-return in a '-> @' body -- a tail `@`, `return @`, `=> @` -- is E_EXPLICIT_SELF_RETURN (D409 rule 3); Carina accepts all three
PROBE="$(cd "$(dirname "$0")" && pwd)"
NOSELF=1
GREP='novac_fn_.*(tail|ret|arrow)'
export PROBE GREP NOSELF
. "$PROBE/../../2026-10-01-mono-k1/run-probe.sh"
