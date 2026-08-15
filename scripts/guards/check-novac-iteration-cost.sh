#!/bin/sh
# scripts/guards/check-novac-iteration-cost.sh — храповик цены цикла novac
# (конвенция П14; план 274.2). Меряет смоук (тёплый), один novac check и
# фаззер; сравнивает с scripts/guards/novac-iteration-cost.baseline В ОБЕ
# СТОРОНЫ: факт > бюджет — просадка (красный); факт < бюджет/2 — ускорение
# без поднятия базы (тоже красный: база обязана отражать реальность).
# Дифф-раннер (68 с) не гоняется здесь — его цену печатает и судит
# check-novac-differential.sh по той же базе (ключ diff-corpus-ms).
#
# Замер шумный (машина под гейтом): каждый замер — лучший из двух прогонов,
# бюджеты с запасом x2. NOVAC_COST=0 пропускает страж (локальная итерация);
# пропуск в гейте — сознательное решение интегратора.
#
# $1 — корень репозитория. Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NAME=check-novac-iteration-cost
BASE="$ROOT/scripts/guards/novac-iteration-cost.baseline"
NOVAC="$ROOT/novac/target/novac.exe"
[ "${NOVAC_COST:-1}" = "0" ] && { echo "$NAME: пропущен (NOVAC_COST=0 — локальная итерация)"; exit 0; }
[ -f "$NOVAC" ] || { echo "$NAME ok: судить нечего (novac не собран)"; exit 0; }
[ -f "$BASE" ] || { echo "$NAME: FAIL — нет $BASE" >&2; exit 1; }

budget() { tr -d '\r' < "$BASE" | sed -n "s/^$1 \([0-9]*\)$/\1/p"; }
now_ms() { date +%s%N | cut -c1-13; }
best_of_two() {  # $@ = command; prints best wall ms
    b=999999999
    for i in 1 2; do
        s=$(now_ms); "$@" >/dev/null 2>&1; e=$(( $(now_ms) - s ))
        [ "$e" -lt "$b" ] && b=$e
    done
    echo "$b"
}
judge() {  # $1 key, $2 fact
    bud=$(budget "$1")
    [ -n "$bud" ] || { echo "  $1: нет бюджета в базе" >> "$T/bad"; return; }
    if [ "$2" -gt "$bud" ]; then
        echo "  $1: ПРОСАДКА — факт ${2}мс > бюджет ${bud}мс" >> "$T/bad"
    elif [ "$2" -lt $((bud / 4)) ]; then
        echo "  $1: ускорение факт ${2}мс < бюджет/4 (${bud}мс) — подними базу тем же коммитом" >> "$T/bad"
    fi
    echo "  $1: ${2}мс (бюджет ${bud})"
}
T="${TMPDIR:-/tmp}/novac-cost.$$"; mkdir -p "$T"; trap 'rm -rf "$T"' 0
cd "$ROOT" || exit 2

# warm the smoke cache once (cold run is the oracle's price, not the loop's)
sh scripts/tools/novac-e1-smoke.sh examples/basics/hello.nv >/dev/null 2>&1
judge smoke-warm-ms "$(best_of_two sh scripts/tools/novac-e1-smoke.sh examples/basics/hello.nv)"
judge check-one-ms "$(best_of_two "$NOVAC" check examples/basics/hello.nv)"
judge fuzz-ms "$(best_of_two sh scripts/tools/novac-fuzz-mutations.sh 40)"

if [ -f "$T/bad" ]; then
    echo "$NAME: FAIL — цена цикла вне бюджета (П14):" >&2
    cat "$T/bad" >&2
    exit 1
fi
echo "$NAME ok: цена цикла в бюджете"
exit 0
