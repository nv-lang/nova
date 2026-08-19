#!/usr/bin/env bash
# Страж: число модулей `std/src/**` БЕЗ единого теста может только убывать.
#
# ЗАЧЕМ. `check-std-test-baseline` сторожит тесты, которые ПАДАЮТ. Модуль, у
# которого тестов нет ВОВСЕ, ей невидим по построению — и это ровно жалоба
# записи №471: «целый модуль выпадает из проверки, а гейт остаётся зелёным;
# непокрытость не отличается от исправности».
#
# ЗАМЕР 2026-08-19 (приёмка №471). Папок-модулей с `.nv` в `std/src` — 45, из
# них neg-каталоги не в счёт. Без единого теста ТРИ: `std/src/ffi`,
# `std/src/unicode` и `std/src/runtime/string` — последний это шесть файлов
# строкового рантайма (chars, core, parse, search, slice, transform), то есть
# самая ходовая часть библиотеки, у которой рядом нет ни одного теста.
#
# НОСИТЕЛЬ ЗАПИСИ №471 ПРИ ЭТОМ ИСЧЕЗ САМ. Она заведена 2026-08-08 словами
# «модуль `std/src/net` НЕ СОБИРАЕТСЯ, любой новый тест рядом падает на чужой
# ошибке `[P67-LEGACY]`». Замер 2026-08-19: `nova test std/src/net` даёт
# PASS 4 / FAIL 0, `[P67-LEGACY]` не встречается. Осталась только вторая
# половина приёмки — правило, и это оно.
#
# ХРАПОВИК, А НЕ НОЛЬ: три модуля покрываются волнами, а не одним слиянием.
# Убывание — заметка с просьбой опустить базу, не отказ (довод №703).
#
# $1 — корень. Самотест — selftest/test-check-std-module-coverage.sh
#
# План docs/plans/258-std-net-ownership.md (окно N1) и 231 (дисциплина
# механизмов принуждения).

set -u
export LC_ALL=C

NAME="check-std-module-coverage"
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
CORE="$(dirname "$0")/std-module-coverage-scan.py"
BASE_FILE="$ROOT/scripts/guards/std-module-coverage.baseline"

[ -f "$CORE" ] || { echo "$NAME: FAIL — нет ядра $CORE" >&2; exit 1; }

OUT=$(python "$CORE" "$ROOT" 2>&1)
if [ $? -ne 0 ]; then
    echo "$NAME: FAIL — ядро не отработало:" >&2
    printf '%s\n' "$OUT" | tail -5 >&2
    exit 1
fi

BARE=$(printf '%s\n' "$OUT" | sed -n 's/^bare=\(-\{0,1\}[0-9][0-9]*\)$/\1/p' | tail -1)
MODS=$(printf '%s\n' "$OUT" | sed -n 's/^modules=\(-\{0,1\}[0-9][0-9]*\)$/\1/p' | tail -1)
case "$BARE" in ''|*[!0-9-]*) echo "$NAME: FAIL — ядро не вернуло число" >&2; exit 1;; esac
if [ "$BARE" -lt 0 ]; then
    echo "$NAME: FAIL — ядро не нашло std/src (переехал?)" >&2
    exit 1
fi

BASE=$(grep -E '^bare=' "$BASE_FILE" 2>/dev/null | head -1 | cut -d= -f2)
case "$BASE" in ''|*[!0-9]*)
    echo "$NAME: FAIL — база не задана или некорректна ($BASE_FILE)" >&2
    exit 1;;
esac

if [ "$BARE" -gt "$BASE" ]; then
    echo "$NAME: FAIL — модулей std без единого теста стало больше: $BARE > $BASE" >&2
    printf '%s\n' "$OUT" | grep -v '^modules=\|^covered=\|^bare=' | head -12 >&2
    echo "    Модуль без теста РЯДОМ невидим базе известных отказов: она" >&2
    echo "    сторожит падающие тесты, а не отсутствующие. Непокрытость" >&2
    echo "    выглядит как исправность (№471)." >&2
    echo "    Клади тест рядом с модулем (конвенция std), а не в nova_tests." >&2
    exit 1
fi

if [ "$BARE" -lt "$BASE" ]; then
    echo "$NAME ok: без тестов $BARE из $MODS (база $BASE) — ЗАМЕТКА: опусти базу с летописью"
    exit 0
fi

echo "$NAME ok: модулей std без единого теста не прибавилось ($BARE из $MODS, база $BASE)"
exit 0
