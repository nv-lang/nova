#!/bin/sh
# scripts/guards/check-novac-no-naked-panic.sh — голый `panic(` в novac/src
# вне закона; явный инвариант идёт через дверь `ice()`.
#
# План: docs/plans/274-novac-self-hosted-compiler.md §10.3; конвенция —
# docs/dev/novac-compiler-conventions.md П12 (правило родилось вместе с этим
# стражем, 2026-08-14, по вопросам владельца об ICE-практике эталонов).
#
# ПРАВИЛО (П12.1, зеркало внутреннего rustc-линта против прямого panic!):
# явный внутренний инвариант обязан идти через одну дверь `ice(...)` в
# novac/src/diag/diag.nv — она печатает диагностику E_NOVAC_ICE по схеме §7
# (машинный читатель получает структурную строку) и лишь затем умирает по
# правилу языка (panic = смерть файбера, наблюдаемая супервизором — D416).
# Голая строка `panic(` в любом другом файле novac/src — красный: не потому
# что крашит (assert'ы и контракты тоже умирают панкой, и это законно), а
# потому что явные инварианты обязаны идти через дверь со схемой и местом.
#
# НЕ проверяет: динамическую достижимость паник (это check-novac-no-panic.sh
# по фикстурам) и std-ассерты/контракты (всегда включены — позиция языка).
#
# $1 — корень репозитория (default: вычислить от себя).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NAME=check-novac-no-naked-panic

SRC="$ROOT/novac/src"
if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (novac/src ещё нет)"
    exit 0
fi

HITS=$(grep -rn 'panic(' "$SRC" --include='*.nv' 2>/dev/null \
       | grep -v '^[^:]*diag/diag\.nv:' \
       | grep -v '// ' || true)
N=$(printf '%s\n' "$HITS" | grep -c . || true)
N=${N:-0}

if [ "$N" -gt 0 ]; then
    echo "$NAME: FAIL — голый panic( вне двери ice() ($N):" >&2
    printf '%s\n' "$HITS" | head -10 | sed 's/^/    /' >&2
    echo "  Явный инвариант идёт через ice(...) из novac.diag (П12.1):" >&2
    echo "  она рендерит E_NOVAC_ICE по схеме §7 и лишь затем умирает" >&2
    echo "  по правилу языка. Голый panic — строка свободной формы без" >&2
    echo "  места и схемы; машинный читатель её не разберёт." >&2
    exit 1
fi
echo "$NAME ok: голых panic( в novac/src нет (дверь — ice() в diag)"
exit 0
