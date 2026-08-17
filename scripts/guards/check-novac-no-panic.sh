#!/bin/sh
# scripts/guards/check-novac-no-panic.sh — ноль паник novac на всех фикстурах.
#
# ПРАВИЛО (план 274, инвариант 11: «Ноль паник — крахи не приемлемы, даже
# редкие»; §10.3а: «ноль паник на сломанном/недописанном вводе»): прогон
# novac по ВСЕМ фикстурам novac/fixtures/**/*.nv не смеет кончаться
# паникой/крэшем. Признак (274.3/F3, контракт плана §7 «на ЛЮБОМ входе novac
# завершается кодом 0 или 1»): любой иной код возврата — 2 (usage/IO), 3
# (abort рантайма на Windows), 101 (паника), 124 (таймаут), >=128 (сигнал) —
# либо слово
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
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
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
    "$BIN" check "$f" > "$T/out" 2> "$T/err" </dev/null
    rc=$?
    if novac_is_panic_rc "$rc"; then
        printf '  %s: код возврата %s (контракт §7: вердикт 0/1, дверь 2)\n' "$rel" "$rc" >> "$T/bad"
        bad=$((bad+1))
    elif novac_is_silent_door "$rc" "$T/out" "$T/err"; then
        printf '  %s: exit 2 БЕЗ вывода — отказ двери без причины (274.3/F3)\n' "$rel" >> "$T/bad"
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
echo "$NAME ok: фикстур $N, паник/крэшей нет (все коды 0/1, stderr без 'panic')"
exit 0
