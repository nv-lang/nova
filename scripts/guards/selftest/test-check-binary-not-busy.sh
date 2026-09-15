#!/usr/bin/env bash
# scripts/guards/selftest/test-check-binary-not-busy.sh
#
# Самотест стража «бинарь ЭТОГО дерева не занят чужим прогоном» (реестр 221.1
# №1098).
#
# ЗАНЯТОСТЬ СОЗДАЁТСЯ НАСТОЯЩИМ ПРОЦЕССОМ, а не подделанным выводом `ps`:
# подделка проверяла бы разбор строки, а предмет стража — ФАКТ занятости файла.
#
# СЛУЧАЙ НА РАЗЛИЧЕНИЕ СВОЕГО ОТ СОСЕДСКОГО ОБЯЗАТЕЛЕН, и появился он не из
# аккуратности: первая редакция стража искала ОТНОСИТЕЛЬНЫЙ хвост
# `nova-cli/target/release/nova` и на первом же живом применении покраснела на
# соседнем worktree (`nova-p274/...`), чей бинарь этой сборке не мешает.
# Прежний самотест этого не поймал, потому что проверял только «нашёл живой
# процесс» и «молчит, когда некого искать» — обе стороны ВЕРНЫ и обе слепы к
# специфичности пути.
set -u
export LC_ALL=C
NAME="test-check-binary-not-busy"
ROOT="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
GUARD="$ROOT/scripts/guards/check-binary-not-busy.sh"
[ -f "$GUARD" ] || { echo "$NAME: FAIL — нет $GUARD" >&2; exit 1; }

fails=0
note() { echo "$NAME: $*"; }
bad()  { echo "$NAME: FAIL — $*" >&2; fails=$((fails + 1)); }

if ! command -v ps >/dev/null 2>&1 || ! ps -W >/dev/null 2>&1; then
    out=$(bash "$GUARD" "$ROOT" 2>&1)
    if printf '%s' "$out" | grep -q "судить нечего"; then
        note "не-Windows: страж честно сказал «судить нечего», а не «чисто»"
        echo "$NAME ok: предмета на этой платформе нет, и это НАЗВАНО"
        exit 0
    fi
    bad "не-Windows: ожидалась оговорка «судить нечего», получено: $(printf '%s' "$out" | head -1)"
    exit 1
fi

# ── свободная сторона ─────────────────────────────────────────────────────
out=$(bash "$GUARD" "$ROOT" "/zz-no-such-tree-$$/nova-cli/target/release/nova" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q "держателей 0"; then
    note "free ok: несуществующий путь — держателей 0, страж молчит"
else
    bad "free: ожидался зелёный с «держателей 0»; rc=$rc, вывод: $(printf '%s' "$out" | head -1)"
fi

# ── занятая сторона: НАСТОЯЩИЙ живой процесс ──────────────────────────────
sleep 30 &
SLEEPER=$!
sleep 1
out=$(bash "$GUARD" "$ROOT" "sleep" 2>&1); rc=$?
kill "$SLEEPER" 2>/dev/null || true
wait "$SLEEPER" 2>/dev/null || true
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q "занят чужим прогоном"; then
    note "busy ok: живой процесс найден, отказ по предмету"
else
    bad "busy: живой процесс не пойман; rc=$rc, вывод: $(printf '%s' "$out" | head -2 | tr '\n' ' ')"
fi

# ── РАЗЛИЧЕНИЕ: искомое обязано быть АБСОЛЮТНЫМ и нести корень ────────────
# Случай, которого не было и из-за которого страж покраснел на соседе.
# Проверяем НЕ поведение на выдуманном процессе (его не построить), а то, ЧТО
# страж ищет: относительный хвост совпал бы у всех деревьев сразу.
out=$(bash "$GUARD" "$ROOT" 2>&1)
needle=$(printf '%s' "$out" | sed -n 's/.*искал: \([^ )]*\).*/\1/p' | head -1)
if [ -z "$needle" ]; then
    bad "specificity: страж не печатает, что именно искал — проверить нечем"
elif printf '%s' "$needle" | grep -q "^/"; then
    note "specificity ok: искомое АБСОЛЮТНО ($needle) — соседний worktree не совпадёт"
else
    bad "specificity: искомое НЕ абсолютно ($needle) — совпадёт с любым деревом, включая чужое"
fi

# ── отказ ОБЪЯСНЯЕТ, а не только сообщает ────────────────────────────────
sleep 30 &
SLEEPER2=$!
sleep 1
out2=$(bash "$GUARD" "$ROOT" "sleep" 2>&1)
kill "$SLEEPER2" 2>/dev/null || true
wait "$SLEEPER2" 2>/dev/null || true
if printf '%s' "$out2" | grep -q "НЕ права доступа"; then
    note "message ok: отказ прямо отводит ложную причину (права доступа)"
else
    bad "message: отказ не объясняет причину — ровно та беда, ради которой страж заведён"
fi

if [ "$fails" -eq 0 ]; then
    echo "$NAME ok: краснеет на живом процессе, молчит на свободном, ищет АБСОЛЮТНЫЙ путь, и отказ называет причину"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails" >&2
exit 1
