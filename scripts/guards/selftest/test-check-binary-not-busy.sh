#!/usr/bin/env bash
# scripts/guards/selftest/test-check-binary-not-busy.sh
#
# Самотест стража «общий бинарь не занят чужим прогоном» (реестр 221.1 №1098).
#
# ЗАНЯТОСТЬ СОЗДАЁТСЯ НАСТОЯЩИМ ПРОЦЕССОМ, а не подделанным выводом `ps`.
# Подделка проверяла бы разбор строки, а предмет стража — ФАКТ занятости файла;
# это разные вопросы, и первый уже однажды выдал себя за второй (реестр №1088).
#
# Свободная сторона обязательна не меньше занятой: страж, краснеющий всегда,
# снимут первым же прогоном, и тогда настоящая занятость снова пройдёт молча.
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
    # Оговорка ВСЛУХ: на не-Windows предмета нет, и самотест это говорит,
    # а не молчит. Молчаливый пропуск читается как пройденная проверка.
    out=$(bash "$GUARD" "$ROOT" 2>&1)
    if printf '%s' "$out" | grep -q "судить нечего"; then
        note "не-Windows: страж честно сказал «судить нечего», а не «чисто»"
        echo "$NAME ok: предмета на этой платформе нет, и это НАЗВАНО"
        exit 0
    fi
    bad "не-Windows: ожидалась оговорка «судить нечего», получено: $(printf '%s' "$out" | head -1)"
    exit 1
fi

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t cbnb)
trap 'rm -rf "$TMP"' EXIT

# ── свободная сторона: имя, которого в процессах заведомо нет ──────────────
out=$(bash "$GUARD" "$ROOT" "zz-no-such-binary-zz-$$" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q "держателей 0"; then
    note "free ok: несуществующий путь — держателей 0, страж молчит"
else
    bad "free: ожидался зелёный с «держателей 0»; rc=$rc, вывод: $(printf '%s' "$out" | head -1)"
fi

# ── занятая сторона: НАСТОЯЩИЙ живой процесс ──────────────────────────────
# Берём `sleep` как носитель: он есть везде, живёт предсказуемо и не трогает
# ни бинарь, ни дерево. Страж ищет подстроку в списке процессов, и `sleep`
# проверяет РОВНО этот механизм.
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

# ── отказ ОБЪЯСНЯЕТ, а не только сообщает ────────────────────────────────
# Весь смысл строки №1098 — что прежний отказ называл не свою причину.
# Страж, повторяющий эту ошибку в другом виде, не стоил бы заведения.
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
    echo "$NAME ok: краснеет на живом процессе, молчит на свободном, и отказ называет причину"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails" >&2
exit 1
