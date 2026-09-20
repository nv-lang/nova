#!/bin/sh
# ВАЖНО: этот код живёт в emit_c.rs (фаза C-эмиссии) и НЕ поднимается
# командой `check` -- только `build`/`test`, которые доходят до кодогена.
# `check` на этом файле молча даёт PASS, что и стоило часа поиска.
N="${1:-/d/Sources/nv-lang/nova/nova-cli/target/release/nova.exe}"
D="$(cd "$(dirname "$0")" && pwd)"
T="${TMPDIR:-/tmp}/e-array-elem-narrow.$$"
for f in "$D"/*.nv; do
    printf '%s: ' "$(basename "$f")"
    "$N" build "$f" -o "$T" 2>&1 | grep -oE '\[E_[A-Z_]+\]' | sort -u | tr '\n' ' '
    echo
done
rm -f "$T" "$T.c"
