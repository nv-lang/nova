#!/bin/sh
# E_BARE_TYPEVAR_NEEDS_PREFIX -- types/mod.rs:7017, reached by nova check
# (type-checker path, not emit_c.rs -- check is the right tool here).
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(dirname "$0")"
echo "=== p.nv (expect E_BARE_TYPEVAR_NEEDS_PREFIX) ==="
"$N" check "$D/p.nv"; echo "rc=$?"
echo "=== control.nv (expect clean) ==="
"$N" check "$D/control.nv"; echo "rc=$?"
