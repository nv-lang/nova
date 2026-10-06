#!/bin/sh
# scripts/tools/novac-check-cache.sh — ОДИН прогон `novac check` по корпусу
# novac/fixtures/**/*.nv в каталог-кэш на прогон гейта (реестр 221.1 №1717).
#
# Зачем: пять стражей корпуса звали `novac check` по одним и тем же фикстурам
# (~1200 запусков вместо ~390, замер CI 2026-10-05, run 37353522969), и трое из
# них снимались пределом 600с. Стражи читают итог дверью novac_check
# (scripts/guards/lib/novac.sh); записи нет — зовут novac сами, так что
# оборванный прогон этого инструмента не делает ни одного стража зелёным
# понарошку, он только не экономит.
#
# Запись на фикстуру: <ключ>.path (путь), .out, .err и последним .rc — читатель
# верит записи только при числовом .rc и совпавшем пути.
#
# Использование: sh scripts/tools/novac-check-cache.sh ROOT DIR [BIN]
export LC_ALL=C
ROOT="$(cd "${1:?ROOT}" && pwd)"
DIR="${2:?DIR}"
. "$(cd "$(dirname "$0")/../guards/lib" && pwd)/novac.sh"
BIN="${3:-$(novac_bin "$ROOT")}"
[ -f "$BIN" ] || { echo "novac-check-cache: нет бинаря $BIN — кэш не наполнен, стражи судят сами"; exit 0; }
mkdir -p "$DIR" || exit 1
printf '%s\n' "$BIN" > "$DIR/bin"
L="$DIR/list"
if [ -d "$ROOT/novac/fixtures" ]; then
    find "$ROOT/novac/fixtures" -type f -name '*.nv' | sort > "$L"
else
    : > "$L"
fi
one() {
    _e="$DIR/$(novac_check_key "$2")"
    printf '%s\n' "$2" > "$_e.path"
    # Через дверь вызова: вход программы (`// NOVAC_PROGRAM`) судится с корнем.
    novac_run_check "$BIN" "$2" "$_e.out" "$_e.err"
    echo "$?" > "$_e.rc"
}
J=$(novac_pool_jobs)
t0=$(date +%s)
novac_pool one "$L" "$J"
n=$(grep -c '' "$L")
m=$(find "$DIR" -name '*.rc' | wc -l | tr -d ' ')
echo "novac-check-cache: фикстур $n, записей $m, стена $(( $(date +%s) - t0 ))с, потоков $J"
