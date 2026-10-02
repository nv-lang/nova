#!/bin/sh
# Probe handed-module-value (another program module's type-scoped MODULE VALUE with a written type (`export ro Kind.ALL []Kind = [..]`, the form of builtins.nv `BuiltinType.ALL` read by sem/collect.nv) and its type-scoped constant (`Kind.MAX`) -- refused "a type name takes a variant after the dot, and this type is not a sum"; the single-unit C build then lacks the owner's cell (simplification of part 6, docs/dev/simplifications.md))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_mv_Kind'
export PROBE GREP
. "$PROBE/../run-probe.sh"
