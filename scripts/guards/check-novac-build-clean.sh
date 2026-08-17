#!/bin/sh
# scripts/guards/check-novac-build-clean.sh — сборка novac идёт БЕЗ
# предупреждений компилятора (конвенция П30).
#
# ПОЧЕМУ ЭТО СТРАЖ, А НЕ ПОЖЕЛАНИЕ. 2026-08-17 сборка novac печатала
# двадцать пять предупреждений «doc-comment (`///`) before bare module-level
# `ro` is ignored» — то есть двадцать пять мест, где я писал документацию,
# которую компилятор ВЫБРАСЫВАЛ. Ни одно из них не было замечено месяц:
# предупреждение, на котором никто не краснеет, — это правило, которого нет.
# Тот же класс, что О-2 в реестре расхождений («правило объявлено, механизма
# нет»), только с обратной стороны: механизм говорил, а слушателя не было.
#
# ЧТО СУДИТ. Лог сборки novac (`target/novac-build.log`, который пишет шаг
# `novac-build` гейта). Если лога нет — страж СОБИРАЕТ сам, а не отвечает
# «судить нечего»: молчание, выходящее нулём, — ровно та дыра, из-за которой
# сутки блокера прошли для гейта зелёными (274.3/F1).
#
# ЧЕГО НЕ СУДИТ. Предупреждения оракула о ЧУЖОМ коде (std, рантайм): в логе
# они отличаются путём — строка обязана указывать в novac/src, иначе это не
# наша земля и не наш красный.
#
# $1 — корень репозитория; $2 — override пути лога (шов самотеста).
# Проверялся: Windows (Git Bash), 2026-08-17.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
LOG="${2:-$ROOT/target/novac-build.log}"
NAME=check-novac-build-clean

if [ ! -f "$LOG" ]; then
    NOVA_BIN="$ROOT/nova-cli/target/release/nova.exe"
    [ -f "$NOVA_BIN" ] || NOVA_BIN="$ROOT/nova-cli/target/release/nova"
    if [ ! -f "$NOVA_BIN" ] || [ ! -f "$ROOT/novac/src/main.nv" ]; then
        echo "$NAME: FAIL — нет ни лога сборки ($LOG), ни оракула, чтобы собрать: судить нечем, а нечем != зелено (274.3/F1)" >&2
        exit 1
    fi
    mkdir -p "$ROOT/target" "$ROOT/novac/target"
    "$NOVA_BIN" build "$ROOT/novac/src/main.nv" -o "$ROOT/novac/target/novac.exe" > "$LOG" 2>&1
fi

# предупреждения, указывающие в НАШИ исходники
W="${TMPDIR:-/tmp}/novac-build-clean.$$"
grep -i "warning" "$LOG" | grep -v "novac/target\|nova-cli/target" > "$W" 2>/dev/null
n=$(grep -c . "$W" 2>/dev/null)
[ -n "$n" ] || n=0

if [ "$n" -gt 0 ]; then
    echo "$NAME: FAIL — сборка novac печатает $n предупреждени(й) (П30):" >&2
    sort "$W" | uniq -c | sort -rn | head -n 8 | sed 's/^/  /' >&2
    echo "  Предупреждение, на котором никто не краснеет, — правило, которого нет." >&2
    echo "  Почини причину; если предупреждение чужое и неустранимое — оно" >&2
    echo "  подавляется маркером [LEGACY-#NNN] со сроком, а не привычкой." >&2
    rm -f "$W"
    exit 1
fi
rm -f "$W"
echo "$NAME ok: предупреждений сборки novac: 0 (лог $(basename "$LOG"))"
exit 0
