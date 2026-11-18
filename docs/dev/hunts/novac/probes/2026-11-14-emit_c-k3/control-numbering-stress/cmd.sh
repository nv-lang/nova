#!/usr/bin/env bash
# control-numbering-stress (emit_c x K3, hunt 2026-11-14-emit_c-k3).
#
# КОНТРОЛЬ (инвариант, «проверено и держится»): ОДНА нумерация C-имён скоупов
# при вложенности и чередовании форм. Носитель: value -> bare -> value ->
# statement-with-args, bare ВНУТРИ тела value, value ВНУТРИ цикла. Считал:
#   FnBuilder.seeded (ir.nv) + @body_builder/@body_builder_done/_resume
#   (emit_c.nv) + interlock @fresh_uniq с @frames_now открытого билдера.
# Замер 2026-11-14: все объявленные NovaFiberQueue _novac_q<N> различны,
# clang не находит ни необъявленных идентификаторов, ни переобъявлений;
# поведение байт-в-байт с оракулом (novac-e1-smoke из основного дерева).
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
T="${TMPDIR:-/tmp}/hunt-ctrl-num.$$"
mkdir -p "$T"; trap 'rm -rf "$T"' 0
fail=0

"$NOVAC" check "$P/probe.nv" >"$T/chk" 2>&1 && echo "check: rc 0" || { echo "ANCHOR LOST: check"; cat "$T/chk"; exit 1; }
"$NOVAC" emit "$P/probe.nv" >"$T/p.c" 2>"$T/p.err" && echo "emit: rc 0" || { echo "ANCHOR LOST: emit"; cat "$T/p.err"; exit 1; }

# 1) инъективность имён очередей в этой эмиссии
dup=$(grep -o 'NovaFiberQueue _novac_q[0-9]* = {0};' "$T/p.c" | sort | uniq -d)
[ -z "$dup" ] && echo "witness: все объявления _novac_q<N> различны ($(grep -c 'NovaFiberQueue _novac_q' "$T/p.c") шт.)" \
    || { echo "ANCHOR BROKEN: дубли объявлений: $dup"; fail=1; }

# 2) у каждой ссылки _novac_qdl/_novac_qprev (запись bare-формы) ЕСТЬ объявление:
#    в этом носителе все bare-скоупы печатаются парой ScopeOpen/ScopeJoin
for kind in qdl qprev; do
    for n in $(grep -o "_novac_${kind}[0-9]*" "$T/p.c" | sort -u | grep -o '[0-9]*$' | sort -u); do
        grep -q "_novac_${kind}${n} = " "$T/p.c" || { echo "ANCHOR BROKEN: _novac_${kind}${n} без объявления"; fail=1; }
    done
done
[ "$fail" -eq 0 ] && echo "witness: у всех _novac_qdl/_novac_qprev есть объявление" || true

# 3) clang: ни необъявленных, ни переобъявлений
if [ -f "$CLANG" ]; then
    "$CLANG" -fsyntax-only -ferror-limit=10000 -I "$RT" -I "$RT/nova_rt/libuv/include" \
        -DNOVA_USE_LIBUV=1 -DNOVA_MAX_EFFECT_STORAGES=10 -D_CRT_SECURE_NO_WARNINGS \
        "$T/p.c" 2>"$T/clang"
    bad=$(grep -cE "undeclared identifier|redefinition|redeclaration" "$T/clang")
    [ "$bad" -eq 0 ] && echo "witness: clang -- ни одного undeclared/redefinition" \
        || { echo "ANCHOR BROKEN: clang назвал $bad"; grep -E "undeclared identifier|redefinition|redeclaration" "$T/clang" | head -4; fail=1; }
else
    echo "NO clang at $CLANG -- шаг CC не доказан"; fail=1
fi

[ "$fail" -eq 0 ] && echo "PROBE OK: нумерация инъективна на этом носителе (контроль)" || echo "PROBE RED"
exit "$fail"
