#!/bin/sh
# Binar beryotsya iz glavnogo dereva: v dereve pomoshchnika sborki net.
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(cd "$(dirname "$0")" && pwd)"
for f in "$D"/*.nv; do
    printf '%s: ' "$(basename "$f")"
    "$N" check "$f" 2>&1 | grep -oE '\[E_[A-Z_]+\]' | sort -u | tr '\n' ' '
    echo
done
