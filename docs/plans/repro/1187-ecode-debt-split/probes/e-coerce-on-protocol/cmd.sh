#!/bin/sh
# E_COERCE_ON_PROTOCOL -- parser/mod.rs:5878, parse-time (protocol body req).
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(dirname "$0")"
echo "=== p.nv (expect E_COERCE_ON_PROTOCOL) ==="
"$N" check "$D/p.nv"; echo "rc=$?"
echo "=== control.nv (expect clean) ==="
"$N" check "$D/control.nv"; echo "rc=$?"
