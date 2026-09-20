#!/bin/sh
# Корень ВЫВОДИТСЯ, а не пишется литералом (реестр 221.1 #698): идём вверх
# от каталога пробы до дерева с AGENTS.md. $1 остаётся override'ом.
if [ -n "$1" ]; then R="$1"; else
    R="$(cd "$(dirname "$0")" && pwd)"
    while [ "$R" != "/" ] && [ ! -f "$R/AGENTS.md" ]; do R="$(dirname "$R")"; done
fi
D="$(cd "$(dirname "$0")" && pwd)"
cd "$R" || exit 2
echo '== forma =='
./nova-cli/target/release/nova.exe check "$D/p.nv"; echo "rc=$?"
echo '== KONTROL =='
./nova-cli/target/release/nova.exe check "$D/control.nv"; echo "rc=$?"
