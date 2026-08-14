#!/bin/sh
# scripts/guards/check-novac-time-ledger.sh — доля «274 против 221» дня 30
# считается из леджера, а не по памяти (план 274 §1.4, поправка 2026-08-14;
# №636: правило без механизма не действует).
#
# ПРАВИЛО: каждая дата коммита, коснувшегося novac/**, начиная с первой
# строки леджера, обязана иметь строку в docs/dev/novac-time-ledger.md.
# Сессии амнезийны (274 §1.2): день 30 без леджера — «решение по данным»
# по загрязнённому замеру.
#
# НЕ проверяет: честность долей (⚖ суждение) и сессии без коммитов в novac
# (фикс-окна оракула в Rust-дереве леджер просит, но машине их не видно).
#
# $1 — корень репозитория (default: вычислить от себя).
# env NOVA_TL_DATES — для самотеста: список дат вместо git log.
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NAME=check-novac-time-ledger

LEDGER="$ROOT/docs/dev/novac-time-ledger.md"
if [ ! -d "$ROOT/novac" ] && [ -z "${NOVA_TL_DATES:-}" ]; then
    echo "$NAME ok: судить нечего (novac ещё нет)"
    exit 0
fi
if [ ! -f "$LEDGER" ]; then
    echo "$NAME: FAIL — леджера $LEDGER нет, а novac есть (274 §1.4)" >&2
    exit 1
fi

START=$(grep -o '^| 20[0-9][0-9]-[0-9][0-9]-[0-9][0-9] |' "$LEDGER" | head -1 \
        | grep -o '20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]')
if [ -z "$START" ]; then
    echo "$NAME: FAIL — в леджере нет ни одной строки с датой" >&2
    exit 1
fi

if [ -n "${NOVA_TL_DATES:-}" ]; then
    DATES="$NOVA_TL_DATES"
else
    DATES=$(git -C "$ROOT" log --format=%as -- novac 2>/dev/null | sort -u)
fi

BAD=0
for d in $DATES; do
    [ "$d" \< "$START" ] && continue
    if ! grep -q "^| $d |" "$LEDGER"; then
        echo "$NAME: FAIL — коммит в novac/** от $d без строки в леджере" >&2
        echo "  Одна строка в конце сессии: дата · класс · доля · что (274 §1.4)." >&2
        BAD=1
    fi
done

[ "$BAD" -eq 0 ] || exit 1
N=$(grep -c '^| 20' "$LEDGER" || true)
echo "$NAME ok: все даты коммитов novac покрыты леджером (строк: $N)"
exit 0
