#!/usr/bin/env bash
# docs/plans/repro/p848/run.sh — ловля зависания реестровых строк №848/№862
# (`presume_446_stress` идёт то 1-2 секунды, то не заканчивается вовсе).
#
# ЗАЧЕМ ЭТОТ КАТАЛОГ. До 2026-09-01 строка №848 говорила, что локально
# фикстура ЗЕЛЁНАЯ — носителя для отладки не существовало, и класс
# наблюдался только через чужой CI. Здесь он ловится на своей машине.
#
# ДВА ПУТИ ПОД ОДНИМ СИМПТОМОМ (разделены пробой фикса №868 на одном бинаре):
#   ШУМНЫЙ — фолт-петля GC-коллбека, десятки тысяч строк «access violation
#     in fiber arena» в stderr; ПОЧИНЕН c22a0f75f (2026-09-01).
#   ТИХИЙ — одна строка «Running 1 tests...» и вечное ожидание; это носитель
#     №862 (счётчик remote не декрементирован), ОТКРЫТ.
#
# РЕЖИМ ПО УМОЛЧАНИЮ — ПРЯМОЙ БИНАРЬ, и это исправление 2026-09-01 по
# замечанию окна 274: прежняя версия гоняла `nova test`, а тот (1) ГЛОТАЕТ
# stderr ребёнка — три дня класс был без улик именно поэтому; (2) читает
# `// ENV`-строки файла — прямой запуск их НЕ читает, окружение
# задаётся переменными здесь, в скрипте. Сборка — `nova build`
# прямо из фикстуры (test-блок собирается в exe, который гоняет тест).
#
# ЗАМЕРЫ ЧАСТОТЫ (для планирования серии):
#   тихий путь, фикс-бинарь, MAXPROCS=1, одиночно:  1/48 и 1/11, потом 0/60
#   окно 274, фикс-бинарь, MAXPROCS=2, ПОД нагрузкой яруса: 0/55
#   Считайте десятками прогонов на одну поимку; healthy = 1-2с.
#
# ИСПОЛЬЗОВАНИЕ (из корня репозитория):
#   bash docs/plans/repro/p848/run.sh [ПРОГОНОВ] [MAXPROCS] [ТАЙМАУТ_С]
#   NOVA_P848_VIA_RUNNER=1 bash ... — старый режим через `nova test`
#     (медленнее, stderr не виден; нужен только чтобы проверить сам раннер).
# Значения по умолчанию: 30 прогонов, MAXPROCS=1, таймаут 30с.
# Пойманный лог сохраняется рядом: caught_<N>.txt.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$ROOT/nova-cli/target/release/nova"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$HERE/oversub_stress.nv"

RUNS="${1:-30}"
MP="${2:-1}"
TMO="${3:-30}"

[ -x "$NOVA" ] || { echo "нет собранного nova: $NOVA" >&2; exit 1; }

if [ "${NOVA_P848_VIA_RUNNER:-0}" = "1" ]; then
    # Старый режим: раннер читает ENV-шапку файла, поэтому правим её и
    # ПЕЧАТАЕМ результат (первая попытка этого замера молча не меняла
    # значение, и девять прогонов померили одно и то же).
    tmp="$(mktemp)"
    sed "s|^// ENV NOVA_MAXPROCS=.*|// ENV NOVA_MAXPROCS=${MP}|" "$SRC" > "$tmp"
    grep -m1 '^// ENV NOVA_MAXPROCS=' "$tmp" || { echo "ENV-строка не найдена" >&2; exit 1; }
    mv "$tmp" "$SRC"
    hangs=0
    for i in $(seq 1 "$RUNS"); do
        s=$(date +%s)
        out="$("$NOVA" test "$SRC" --timeout "$TMO" 2>&1 | grep -aE 'PASS:|TIMEOUT' | tail -1)"
        e=$(date +%s)
        case "$out" in
            *"FAIL: 1"*|*TIMEOUT*) hangs=$((hangs + 1)); echo "  прогон ${i}: $((e-s))с — ЗАВИСЛО (stderr съеден раннером — переключитесь на прямой режим)";;
            *) echo "  прогон ${i}: $((e-s))с — ok";;
        esac
    done
    echo "ИТОГ (через раннер): зависаний ${hangs} из ${RUNS} при MAXPROCS=${MP}"
    exit 0
fi

# Прямой режим (по умолчанию): собрать один раз, гонять exe с env.
BIN="$HERE/oversub_stress_direct.exe"
echo "сборка прямого бинаря: nova build $SRC"
"$NOVA" build "$SRC" -o "$BIN" >/dev/null 2>&1 || { echo "сборка упала" >&2; exit 1; }

hangs=0
for i in $(seq 1 "$RUNS"); do
    s=$(date +%s)
    NOVA_MAXPROCS="$MP" NOVA_FIBERS_PER_WORKER=4000 \
        timeout "$TMO" "$BIN" > "$HERE/last.txt" 2>&1
    rc=$?
    e=$(date +%s)
    if [ "$rc" -ne 0 ]; then
        hangs=$((hangs + 1))
        cp "$HERE/last.txt" "$HERE/caught_${hangs}.txt"
        lines=$(wc -l < "$HERE/caught_${hangs}.txt")
        echo "  прогон ${i}: $((e-s))с rc=${rc} — ПОЙМАНО, лог caught_${hangs}.txt (${lines} строк; 1 строка = тихий путь №862, тысячи = фолт-петля №868)"
    else
        echo "  прогон ${i}: $((e-s))с — ok"
    fi
done
rm -f "$HERE/last.txt" "$BIN"
echo "ИТОГ (прямой бинарь): зависаний ${hangs} из ${RUNS} при MAXPROCS=${MP}"
