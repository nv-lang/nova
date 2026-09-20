#!/bin/sh
# Binar beryotsya iz glavnogo dereva: v dereve pomoshchnika sborki net.
# $1 libo $R zadayut koren glavnogo dereva; molcha pustym ne ostayotsya.
# Корень ВЫВОДИТСЯ, а не пишется литералом (реестр 221.1 #698): идём вверх
# от каталога пробы до дерева с AGENTS.md. $R и $1 остаются override'ами.
if [ -z "$R" ]; then
    R="$(cd "$(dirname "$0")" && pwd)"
    while [ "$R" != "/" ] && [ ! -f "$R/AGENTS.md" ]; do R="$(dirname "$R")"; done
fi
N="${1:-$R/nova-cli/target/release/nova.exe}"
[ -x "$N" ] || { echo "cmd.sh: nova ne nayden: $N" >&2; exit 2; }
D="$(cd "$(dirname "$0")" && pwd)"
for f in "$D"/*.nv; do
    printf '%s: ' "$(basename "$f")"
    "$N" check "$f" 2>&1 | grep -oE '\[E_[A-Z_]+\]' | sort -u | tr '\n' ' '
    echo
done
