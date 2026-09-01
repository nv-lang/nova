#!/usr/bin/env bash
# check-oracle-abi-spellings.sh — ABI-спеллинги прелюдии оракула заморожены:
# Карина (novac) линкует свой интероп ровно по этим именам, и молчаливый дрейф
# спеллинга при рефакторе emit_c.rs обнаружился бы только линкером Карины —
# на другом треке и через часы (запрос окна 274, 2026-09-02, принят
# интегратором; договор описан в docs/dev/hunts и mn-safety-переписке).
#
# ЧТО СУДИТ: каждый якорь-спеллинг обязан существовать в emit_c.rs (count>=1).
# Дрейф = переименование ВСЕХ вхождений разом (иначе оракул сам не соберётся),
# поэтому исчезновение якоря целиком — ровно сигнатура дрейфа. Точечные правки
# отдельных вхождений страж не судит и судить не должен.
#
# ЦЕНА: девять грепов одного файла, << 1с — ярус loop.
# Самотест: scripts/guards/selftest/test-check-oracle-abi-spellings.sh
set -u
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
NAME=check-oracle-abi-spellings
EMIT="$ROOT/compiler-codegen/src/codegen/emit_c.rs"

if [ ! -f "$EMIT" ]; then
    echo "$NAME: FAIL — нет файла $EMIT" >&2
    exit 1
fi

# Якорь | зачем Карине (печатается при провале).
ANCHORS=(
    '"____"|разделитель generic-инстансов в именах типов (Nova_Vec____nova_int)'
    '_method_|спеллинг методов инстансов (Vec____nova_int_method_push)'
    '_static_new|спеллинг статических конструкторов инстансов'
    'NovaOpt_|family Option by-value (NovaOpt_<argC>)'
    'NovaRes_|family Result by-value'
    '_NovaTuple_|кортежи, length-prefixed спеллинг (D123)'
    'nova_contract_violation|вызов контрактной паники из эмиссии'
    'NOVA_CONTRACT_PRE|первый аргумент contract_violation (вид контракта)'
    'nova_fn_main_impl|единственный вход, который диктует рантайм'
)

missing=0
for pair in "${ANCHORS[@]}"; do
    a="${pair%%|*}"; why="${pair#*|}"
    if ! grep -qF -- "$a" "$EMIT"; then
        echo "$NAME: FAIL — якорь '$a' исчез из emit_c.rs ($why)." >&2
        echo "  Карина линкует интероп по этому спеллингу: переименовал —" >&2
        echo "  скажи окну 274 и поправь якорь здесь ТЕМ ЖЕ слиянием." >&2
        missing=$((missing+1))
    fi
done

if [ "$missing" -gt 0 ]; then
    echo "$NAME: FAIL — дрейфнувших якорей: $missing из ${#ANCHORS[@]}" >&2
    exit 1
fi
echo "$NAME ok: все ${#ANCHORS[@]} ABI-якорей прелюдии на месте (emit_c.rs)"
