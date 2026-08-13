#!/usr/bin/env bash
# scripts/guards/check-std-test-baseline.sh
# `nova test std/src` против БАЗЫ ИМЁН — один механизм для нашего гейта и для CI.
#
# ДОМ И ОСНОВАНИЕ: реестр 221.1 №591 (внешний гейт красен тремя дорожками),
# №402 (локальный и внешний гейт проверяют разное), план 231 трек Д.
#
# ЗАЧЕМ. Дорожка `nova-test-regression` красна на `main` постоянно — не своей
# поломкой, а известными дефектами: CI зовёт `nova test std` голым, и ЛЮБОЙ
# отказ валит прогон. У нашего гейта база имён есть, у CI её не было, потому
# что сверка жила ВНУТРИ `gate.sh` и позвать её со стороны было нечем.
#
# Последствие хуже самой красноты: постоянно красный гейт не отличает новое от
# старого, и каждый пуш идёт через ключ `NOVA_SKIP_CI_CHECK=1`. Механизм, в
# который перестали верить, охраняет ноль.
#
# СВЕРЯЕМ ИМЕНА, А НЕ СЧИТАЕМ ШТУКИ. Счётчик прячет ПОДМЕНУ: 2026-08-11 у нас
# было 6 локальных отказов против 8 на CI при неполном пересечении — счётчик
# сказал бы «7 <= 8, всё хорошо» и скрыл бы `reflect_test`, который до слияния
# группы M не падал вовсе.
#
# КАЖДОЕ ИМЯ В БАЗЕ НЕСЁТ НОМЕР ЗАПИСИ. Имя без `№NNN` — отложенный дефект без
# следа, а не фон: №559 был пронумерован сутками раньше и всё равно оказался
# невидим (№599).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-std-test-baseline.sh [КОРЕНЬ] [ПУТЬ-К-NOVA]
# Коды: 0 — новых отказов нет; 1 — отказ вне базы либо шаг ничего не доказал.
# Самотест — scripts/guards/selftest/test-check-std-test-baseline.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
NOVA="${2:-$ROOT/nova-cli/target/release/nova}"
BASE_FILE="$ROOT/scripts/guards/std-test-fail.baseline"

[ -x "$NOVA" ] || { echo "check-std-test-baseline: нет бинаря $NOVA" >&2; exit 1; }
[ -f "$BASE_FILE" ] || { echo "check-std-test-baseline: нет базы $BASE_FILE" >&2; exit 1; }

TMP="${TMPDIR:-/tmp}/stdtest_$$"
mkdir -p "$TMP"
trap 'rm -rf "$TMP"' EXIT

"$NOVA" test "$ROOT/std/src" > "$TMP/run.log" 2>&1

strip() { sed -e 's/\x1b\[[0-9;]*m//g' "$@"; }

SUMMARY=$(strip "$TMP/run.log" | grep -aE "^PASS: " | tail -1)
echo "std test :: ${SUMMARY:-<нет строки итога>}"

# Отсутствие строки итога — ОТКАЗ, а не «наверное всё хорошо»: шаг обязан
# ассертить свою собственную строку результата (№475).
[ -n "$SUMMARY" ] || {
    echo "check-std-test-baseline: нет строки итога — шаг ничего не доказал (№475)" >&2
    exit 1
}

strip "$TMP/run.log" | grep -aE "^(RUN-FAIL|CC-FAIL)" | awk '{print $2}' | sort -u > "$TMP/now.txt"

UNLINKED=$(grep -vE '^[[:space:]]*#' "$BASE_FILE" | grep -vE '^[[:space:]]*$' | grep -v '№' || true)
if [ -n "$UNLINKED" ]; then
    echo "$UNLINKED" | sed 's/^/    БЕЗ НОМЕРА ЗАПИСИ: /' >&2
    echo "check-std-test-baseline: имя в базе без ссылки на запись реестра" >&2
    exit 1
fi

grep -vE '^[[:space:]]*#' "$BASE_FILE" | grep -vE '^[[:space:]]*$' \
    | sed 's/[[:space:]]*#.*//' | sed 's/[[:space:]]*$//' | sort -u > "$TMP/base.txt"

NEWLY=$(comm -23 "$TMP/now.txt" "$TMP/base.txt")
GONE=$(comm -13 "$TMP/now.txt" "$TMP/base.txt")

if [ -n "$NEWLY" ]; then
    echo "$NEWLY" | sed 's/^/    НОВЫЙ ОТКАЗ: /' >&2
    echo "check-std-test-baseline: отказ, которого нет в базе (подмена или регресс)" >&2
    exit 1
fi

[ -n "$GONE" ] && echo "$GONE" | sed 's/^/    почищено (убери из базы): /'

echo "check-std-test-baseline ok: новых отказов нет (база — $(grep -cvE '^[[:space:]]*(#|$)' "$BASE_FILE") имён)"
exit 0
