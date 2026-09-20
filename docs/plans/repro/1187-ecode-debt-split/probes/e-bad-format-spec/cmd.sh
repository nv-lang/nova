#!/bin/sh
R="${1:-/d/Sources/nv-lang/nova-wt-research}"
D="$(cd "$(dirname "$0")" && pwd)"
cd "$R" || exit 2
echo '== forma =='
./nova-cli/target/release/nova.exe check "$D/p.nv"; echo "rc=$?"
echo '== KONTROL =='
./nova-cli/target/release/nova.exe check "$D/control.nv"; echo "rc=$?"
