#!/bin/sh
# scripts/guards/check-oracle-nesting-depth.sh — реестр 221.1 №800.
#
# ЗАЧЕМ. Оракул умирал на глубокой вложенности НЕ диагностикой, а смертью
# треда: `thread 'nova-check-0' has overflowed its stack`, без файла, без
# строки, без `E_*`. Это первое, что увидит внешний человек, скормивший
# компилятору сгенерированный код, — и ровно тот стандарт, который у `novac`
# механизирован рангом CORE с плана 274 (`check-novac-fuzz-zero-panic.sh`),
# тогда как у оракула фаззинга не было вовсе (замер 2026-08-29: стражей с
# `fuzz` в имени и без `novac` — ноль). Решение владельца 2026-08-29: закрыть
# класс ДО тега, потому что после тега оракул замораживается и найденное
# чинить будет некому.
#
# ЧТО ИМЕННО СУДИТСЯ — ПОВЕДЕНИЕ, А НЕ ФИКСТУРА. Фикстуры корпуса
# (`neg/m800_nesting_depth_neg.nv`, `standalone/m800_nesting_depth_pos.nv`)
# закрепляют ОДНУ глубину каждая. Этот страж мерит ОСЬ: на растущей
# вложенности компилятор обязан отвечать диагностикой на любой глубине и
# никогда не умирать. Формы взяты разные, потому что рекурсия у них разная и
# ломались они на разных числах (замер 2026-08-29): скобки — выражения,
# `if` — блоки, `{ … 1 … }` — блок-со-значением. Пустые блоки схлопываются и
# в набор не входят: они НЕ падали и судить их нечего.
#
# КАЖДАЯ ПРОБА — В СВОЁМ КАТАЛОГЕ, И ЭТО НЕ ПЕДАНТИЗМ. `nova check <файл>`
# тянет папку как модуль: пробы, сложенные вместе, меряют друг друга. На этом
# ошиблась первая ревизия (получила уверенное и ложное «падает на глубине
# 10», потому что рядом лежал файл глубины 20000).
#
# ЧЕМ ДОКАЗЫВАЕТСЯ КРАСНОТА: самотест `selftest/test-check-oracle-nesting-depth.sh`
# подсовывает стражу заведомо негодный «компилятор» (скрипт, который молча
# выходит 0 либо падает без диагностики) и требует, чтобы страж покраснел.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NOVA="${NOVA_ORACLE_BIN:-$ROOT/nova-cli/target/release/nova.exe}"
[ -f "$NOVA" ] || NOVA="${NOVA_ORACLE_BIN:-$ROOT/nova-cli/target/release/nova}"

if [ ! -f "$NOVA" ]; then
    # Ярус loop бинарь не собирает — судить нечего, и молчать об этом нельзя.
    echo "check-oracle-nesting-depth: пропуск — нет бинаря оракула ($NOVA)"
    exit 0
fi

# Путь обязан быть АБСОЛЮТНЫМ: каждая проба судится из своего каталога
# (`cd "$d"`), и относительный путь к бинарю там не разрешится. Ошибка была
# сделана и поймана этим же стражем при первом прогоне: он честно назвал
# `rc=127` падением — что формально верно, но диагноз был бы про компилятор,
# а виноват вызывающий. Отсюда же проверка ниже: если бинарь не запускается
# ВООБЩЕ, это отказ инструмента, а не вердикт о компиляторе.
case "$NOVA" in
    /*|[A-Za-z]:*) ;;
    *) NOVA="$(cd "$(dirname "$NOVA")" && pwd)/$(basename "$NOVA")" ;;
esac
if ! "$NOVA" --version >/dev/null 2>&1; then
    echo "check-oracle-nesting-depth: FAIL — бинарь оракула не запускается ($NOVA); это отказ инструмента, а не вердикт о вложенности" >&2
    exit 1
fi

T="${TMPDIR:-/tmp}/oracle-nesting.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0 2 15

# Глубины: одна заведомо законная (ниже предела) и три заведомо выше него,
# включая ту, на которой раньше умирал стек (~2500) и вдвое большую.
LEGAL_DEPTH=200
DEEP="600 2500 6000"
rc_all=0

make_case() {   # $1 = каталог, $2 = форма, $3 = глубина
    d="$T/$1"; mkdir -p "$d"
    awk -v form="$2" -v n="$3" -v mod="$1" '
    BEGIN {
        printf "module %s\n", mod
        if (form == "paren") {
            printf "fn probe() -> int {\n    "
            for (i = 0; i < n; i++) printf "("
            printf "1"
            for (i = 0; i < n; i++) printf ")"
            printf "\n}\n"
        } else if (form == "blockval") {
            printf "fn probe() -> int {\n    "
            for (i = 0; i < n; i++) printf "{"
            printf "1"
            for (i = 0; i < n; i++) printf "}"
            printf "\n}\n"
        } else {
            printf "fn probe() Io -> () {\n"
            for (i = 0; i < n; i++) printf "if true {\n"
            printf "println(\"x\")\n"
            for (i = 0; i < n; i++) printf "}\n"
            printf "}\n"
        }
    }' > "$d/$1.nv"
}

judge_deep() {  # $1 = имя случая
    d="$T/$1"
    ( cd "$d" && timeout 120 "$NOVA" check "$1.nv" ) > "$d/out" 2>&1
    rc=$?
    if grep -qi "overflowed its stack\|stack overflow" "$d/out"; then
        echo "check-oracle-nesting-depth: FAIL — $1: компилятор УМЕР на стеке вместо диагностики" >&2
        sed -n '1,3p' "$d/out" | sed 's/^/    /' >&2
        return 1
    fi
    # Паника = код НЕ 0/1/2 (тот же контракт, что у фаззера novac:
    # scripts/guards/lib/novac.sh, novac_is_panic_rc).
    if [ "$rc" -ne 0 ] && [ "$rc" -ne 1 ] && [ "$rc" -ne 2 ]; then
        echo "check-oracle-nesting-depth: FAIL — $1: код возврата $rc (не 0/1/2) — падение, а не вердикт" >&2
        sed -n '1,3p' "$d/out" | sed 's/^/    /' >&2
        return 1
    fi
    if ! grep -q "E_NESTING_TOO_DEEP" "$d/out"; then
        echo "check-oracle-nesting-depth: FAIL — $1: нет диагностики E_NESTING_TOO_DEEP (rc=$rc)" >&2
        sed -n '1,3p' "$d/out" | sed 's/^/    /' >&2
        return 1
    fi
    return 0
}

n_deep=0
for form in paren blockval nestedif; do
    for n in $DEEP; do
        case_name="${form}_${n}"
        make_case "$case_name" "$form" "$n"
        judge_deep "$case_name" || rc_all=1
        n_deep=$((n_deep + 1))
    done
done

# Вторая половина, без которой первая ничего не стоит: предел НЕ ТРОГАЕТ
# законный код. Без этой пробы «фикс», ставящий предел в 8, прошёл бы стража.
make_case "legal_paren" paren "$LEGAL_DEPTH"
( cd "$T/legal_paren" && timeout 120 "$NOVA" check "legal_paren.nv" ) > "$T/legal_paren/out" 2>&1
legal_rc=$?
if [ "$legal_rc" -ne 0 ]; then
    echo "check-oracle-nesting-depth: FAIL — законная вложенность $LEGAL_DEPTH отвергнута (rc=$legal_rc): предел слишком низок" >&2
    sed -n '1,3p' "$T/legal_paren/out" | sed 's/^/    /' >&2
    rc_all=1
fi

if [ "$rc_all" -eq 0 ]; then
    echo "check-oracle-nesting-depth ok: $n_deep глубоких форм отвечают E_NESTING_TOO_DEEP, законная глубина $LEGAL_DEPTH принята, паник 0"
fi
exit "$rc_all"
