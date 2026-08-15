#!/bin/sh
# scripts/guards/check-novac-no-panic.sh — ноль паник novac на всех фикстурах.
#
# ПРАВИЛО (план 274, инвариант 11: «Ноль паник — крахи не приемлемы, даже
# редкие»; §10.3а: «ноль паник на сломанном/недописанном вводе»): прогон
# novac по ВСЕМ фикстурам novac/fixtures/**/*.nv не смеет кончаться
# паникой/крэшем. Признаки: код возврата >= 128 (сигнал/крэш) или слово
# 'panic' в stderr — красный. Отвержение с диагностикой (обычный ненулевой
# код) — законно.
#
# НЕ проверяет: фаззинг мутациями корпуса (полная форма §10.3 дорастёт на Э1;
# этот страж — минимум «ожидающего бинарь»), правильность исходов (дифф-гейт),
# формат диагностики (diag-schema). Контракт вызова: '<bin> check <file>';
# если CLI novac окажется иным — страж правится тем же коммитом, что вводит
# бинарь.
#
# Страж «ожидает бинарь»: пока novac/target/novac.exe не существует — зелёный
# честной строкой: страж до кода легален, молчание нелегально (№645).
#
# $1 — корень репозитория (default: вычислить от себя);
# $2 — override бинаря novac (для самотеста).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
BIN="${2:-$ROOT/novac/target/novac.exe}"
NAME=check-novac-no-panic
. "$(dirname "$0")/lib/novac.sh"

novac_require_bin "$NAME" "$ROOT" "$BIN"

FIXDIR="$ROOT/novac/fixtures"
T="${TMPDIR:-/tmp}/novac-no-panic.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

if [ -d "$FIXDIR" ]; then
    find "$FIXDIR" -type f -name '*.nv' | sort > "$T/list"
else
    : > "$T/list"
fi
N=$(wc -l < "$T/list" | tr -d ' ')
if [ "$N" -eq 0 ]; then
    echo "$NAME ok: судить нечего (0 фикстур .nv в novac/fixtures)"
    exit 0
fi

bad=0
while IFS= read -r f; do
    rel=${f#"$ROOT"/}
    "$BIN" check "$f" >/dev/null 2> "$T/err" </dev/null
    rc=$?
    if [ "$rc" -ge 128 ]; then
        printf '  %s: код возврата %s (>=128 — крэш/сигнал)\n' "$rel" "$rc" >> "$T/bad"
        bad=$((bad+1))
    elif grep -qi 'panic' "$T/err"; then
        printf '  %s: слово panic в stderr (код %s)\n' "$rel" "$rc" >> "$T/bad"
        bad=$((bad+1))
    fi
done < "$T/list"

if [ "$bad" -gt 0 ]; then
    echo "$NAME: FAIL — паника/крэш novac на $bad фикстур(ах):" >&2
    cat "$T/bad" >&2
    echo "  Инвариант 11 плана 274: сломанный ввод пережёвывается узлами-" >&2
    echo "  ошибками и диагностикой, не паникой. Чинить причину в novac" >&2
    echo "  той же волной; catch_unwind-обёртка — не починка." >&2
    exit 1
fi
echo "$NAME ok: фикстур $N, паник/крэшей нет (коды < 128, stderr без 'panic')"
exit 0
