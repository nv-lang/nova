#!/bin/sh
# E_CAST_UNWRAP_REQUIRED -- types/mod.rs:12018, checker path (nova check).
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(dirname "$0")"
echo "=== p.nv (expect E_CAST_UNWRAP_REQUIRED) ==="
"$N" check "$D/p.nv"; echo "rc=$?"
echo "=== control.nv (expect clean) ==="
"$N" check "$D/control.nv"; echo "rc=$?"
