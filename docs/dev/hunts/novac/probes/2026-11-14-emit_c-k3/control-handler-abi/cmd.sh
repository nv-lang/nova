#!/usr/bin/env bash
# control-handler-abi (emit_c x K3, hunt 2026-11-14-emit_c-k3).
#
# КОНТРОЛЬ (инвариант, «проверено и держится»): ABI-каст on_timeout-хендлера
# читает тип возврата ИЗ ЗАПИСАННОЙ СИГНАТУРЫ (@sup_handler_ret_c,
# emit_supervised.nv:170-175) во ВСЕХ позициях: value-форма с record-типом,
# блочное тело хендлера, statement-позиция с отброшенным не-unit результатом.
# Строка реестра 221.1 (handler-ABI, фикс r2) на этих НОВЫХ формах держится.
# Замер 2026-11-14: касты названы NovaValue_Pt/nova_str по собственному типу
# хендлера; поведение байт-в-байт с оракулом (novac-e1-smoke).
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
T="${TMPDIR:-/tmp}/hunt-ctrl-abi.$$"
mkdir -p "$T"; trap 'rm -rf "$T"' 0
fail=0

"$NOVAC" check "$P/probe.nv" >"$T/chk" 2>&1 && echo "check: rc 0" || { echo "ANCHOR LOST: check"; cat "$T/chk"; exit 1; }
"$NOVAC" emit "$P/probe.nv" >"$T/p.c" 2>"$T/p.err" && echo "emit: rc 0" || { echo "ANCHOR LOST: emit"; cat "$T/p.err"; exit 1; }

# 1) value-форма, хендлер с record-типом и БЛОЧНЫМ телом: каст = собственный тип хендлера
grep -q '((NovaValue_Pt(\*)(void\*, Nova_TimeoutError\*))' "$T/p.c" \
    && echo "witness: value-форма -- каст NovaValue_Pt(*)(void*, Nova_TimeoutError*)" \
    || { echo "ANCHOR LOST: каст NovaValue_Pt не найден"; fail=1; }

# 2) statement-позиция, результат отброшен, тип хендлера nova_str: каст = nova_str, не nova_unit
grep -q '(void)(((nova_str(\*)(void\*, Nova_TimeoutError\*))' "$T/p.c" \
    && echo "witness: statement с отброшенным значением -- каст nova_str(*)(...)" \
    || { echo "ANCHOR LOST: каст nova_str (отброшенное значение) не найден"; fail=1; }
grep -q '((nova_unit(\*)(void\*, Nova_TimeoutError\*))' "$T/p.c" \
    && { echo "ANCHOR BROKEN: нашёлся каст nova_unit(*) при str-хендлере -- регресс строки 221.1"; fail=1; } \
    || echo "witness: ложного nova_unit-каста нет"

# 3) у сигнатуры тел-функций хендлеров тот же второй параметр, что у каста
grep -q 'novac_lambda_[0-9]*_body(void\* _env_ptr, Nova_TimeoutError\* ' "$T/p.c" \
    && echo "witness: тело хендлера -- (void* _env_ptr, Nova_TimeoutError* ...)" \
    || { echo "ANCHOR LOST: сигнатура тела хендлера не найдена"; fail=1; }

# 4) clang чист по этим кастам
if [ -f "$CLANG" ]; then
    "$CLANG" -fsyntax-only -ferror-limit=10000 -I "$RT" -I "$RT/nova_rt/libuv/include" \
        -DNOVA_USE_LIBUV=1 -DNOVA_MAX_EFFECT_STORAGES=10 -D_CRT_SECURE_NO_WARNINGS \
        "$T/p.c" 2>"$T/clang"
    bad=$(grep -cE "undeclared identifier|incompatible function pointer" "$T/clang")
    [ "$bad" -eq 0 ] && echo "witness: clang -- касты без конфликтов" \
        || { echo "ANCHOR BROKEN: clang назвал $bad"; grep -E "undeclared identifier|function pointer" "$T/clang" | head -4; fail=1; }
else
    echo "NO clang at $CLANG -- шаг CC не доказан"; fail=1
fi

[ "$fail" -eq 0 ] && echo "PROBE OK: каст хендлера читает сигнатуру во всех позициях (контроль)" || echo "PROBE RED"
exit "$fail"
