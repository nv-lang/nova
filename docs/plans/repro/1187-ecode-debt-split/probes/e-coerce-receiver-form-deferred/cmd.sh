#!/bin/sh
# E_COERCE_RECEIVER_FORM_DEFERRED -- types/mod.rs:27890, static one-param ctor form.
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(dirname "$0")"
echo "=== p.nv (expect E_COERCE_RECEIVER_FORM_DEFERRED) ==="
"$N" check "$D/p.nv"; echo "rc=$?"
echo "=== control.nv (expect clean) ==="
"$N" check "$D/control.nv"; echo "rc=$?"
