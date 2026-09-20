#!/bin/sh
# E_COERCE_EFFECTFUL -- types/mod.rs:28028, checker path.
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(dirname "$0")"
echo "=== p.nv (expect E_COERCE_EFFECTFUL) ==="
"$N" check "$D/p.nv"; echo "rc=$?"
echo "=== control.nv (expect clean) ==="
"$N" check "$D/control.nv"; echo "rc=$?"
