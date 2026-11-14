#!/usr/bin/env bash
# control-d432-exits (emit_c x K3, hunt 2026-11-14-emit_c-k3).
#
# КОНТРОЛЬ (инвариант, «проверено и держится»): D432-временная выхода.
# Значение выхода ждёт в temp (низ @lower_return -> @ret_via), протокол
# (ScopeJoin/with-restore) печатается ДО `return <temp>`; break/continue,
# покидающие цикл с bare-скоупом, печатают join до терминатора. Носитель:
# with+return, bare supervised+return, break из цикла через bare-скоуп.
# Замер 2026-11-14: порядок в C -- temp, disarm, run, restore, return; clang
# чист; поведение байт-в-байт с оракулом (novac-e1-smoke из основного дерева).
#
# Запуск: cmd.sh <корень>; exit 0 = инвариант держится на этом носителе.
set -u
root="${1:-.}"
P="$(cd "$(dirname "$0")" && pwd)"
NOVAC="$root/novac/target/novac.exe"
[ -f "$NOVAC" ] || { echo "NO $NOVAC (собери novac)"; exit 2; }
export NOVA_STD_PATH="$root/std/src"
CLANG="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
RT="$root/compiler-codegen"
T="${TMPDIR:-/tmp}/hunt-ctrl-d432.$$"
mkdir -p "$T"; trap 'rm -rf "$T"' 0
fail=0

"$NOVAC" check "$P/probe.nv" >"$T/chk" 2>&1 && echo "check: rc 0" || { echo "ANCHOR LOST: check"; cat "$T/chk"; exit 1; }
"$NOVAC" emit "$P/probe.nv" >"$T/p.c" 2>"$T/p.err" && echo "emit: rc 0" || { echo "ANCHOR LOST: emit"; cat "$T/p.err"; exit 1; }

# вырезаем sup_ret -- единственный носитель «return поверх bare-скоупа»
awk '/_sup_ret__nova_int__to_nova_int\(nova_int n\) \{/,/^\}/' "$T/p.c" > "$T/sup.c"
[ -s "$T/sup.c" ] && echo "witness: функция sup_ret найдена" || { echo "ANCHOR LOST: sup_ret не найдена"; fail=1; }

l_temp=$(grep -n 'nova_int _novac_l[0-9]* = n;' "$T/sup.c" | head -1 | cut -d: -f1)
l_disarm=$(grep -n 'nova_scope_disarm_early_deadline(_novac_qdl' "$T/sup.c" | head -1 | cut -d: -f1)
l_run=$(grep -n 'nova_supervised_run(&_novac_q' "$T/sup.c" | head -1 | cut -d: -f1)
l_ret=$(grep -n 'return _novac_l[0-9]*;' "$T/sup.c" | head -1 | cut -d: -f1)
[ -n "$l_temp" ] && [ -n "$l_disarm" ] && [ -n "$l_run" ] && [ -n "$l_ret" ] \
    && echo "witness: строки temp=$l_temp disarm=$l_disarm run=$l_run return=$l_ret" \
    || { echo "ANCHOR LOST: не найдены все четыре строки (temp=$l_temp disarm=$l_disarm run=$l_run return=$l_ret)"; fail=1; }
if [ -n "$l_temp" ] && [ -n "$l_disarm" ] && [ -n "$l_run" ] && [ -n "$l_ret" ]; then
    if [ "$l_temp" -lt "$l_disarm" ] && [ "$l_disarm" -lt "$l_run" ] && [ "$l_run" -lt "$l_ret" ]; then
        echo "witness: значение в temp, join ДО return -- протокол живой, не мёртвый"
    else
        echo "ANCHOR BROKEN: протокол печатается не до return"; fail=1
    fi
fi

# return НЕ стоит прямо над своим значением в точке вычисления (старый дефект):
# между `= n;` и `return _novac_l` обязаны стоять строки протокола
sed -n "${l_temp},${l_ret}p" "$T/sup.c" | grep -q '_nova_active_scope = ' \
    && echo "witness: restore активного скоупа между temp и return" \
    || { echo "ANCHOR LOST: restore не между temp и return"; fail=1; }

if [ -f "$CLANG" ]; then
    "$CLANG" -fsyntax-only -ferror-limit=10000 -I "$RT" -I "$RT/nova_rt/libuv/include" \
        -DNOVA_USE_LIBUV=1 -DNOVA_MAX_EFFECT_STORAGES=10 -D_CRT_SECURE_NO_WARNINGS \
        "$T/p.c" 2>"$T/clang"
    bad=$(grep -cE "undeclared identifier|redefinition" "$T/clang")
    [ "$bad" -eq 0 ] && echo "witness: clang чист" \
        || { echo "ANCHOR BROKEN: clang назвал $bad"; grep -E "undeclared identifier|redefinition" "$T/clang" | head -4; fail=1; }
else
    echo "NO clang at $CLANG -- шаг CC не доказан"; fail=1
fi

[ "$fail" -eq 0 ] && echo "PROBE OK: D432-выход печатает протокол до return (контроль)" || echo "PROBE RED"
exit "$fail"
