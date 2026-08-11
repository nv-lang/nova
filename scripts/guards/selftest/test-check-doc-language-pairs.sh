#!/usr/bin/env bash
# Самотест check-doc-language-pairs.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-doc-language-pairs.sh"
TMP="${TMPDIR:-/tmp}/selftest_pairs_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# Пустая база: самотест проверяет саму механику, а не сегодняшние долги.
EMPTY_BASE="$TMP/empty.baseline"

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/spec" "$TMP/docs/guide"
    : > "$EMPTY_BASE"
    printf 'The effect boundary is a Nova API.\nIt is not a syscall table.\n' > "$TMP/spec/effects.md"
    printf 'Граница эффекта — это API языка.\nЭто не таблица системных вызовов.\n' > "$TMP/spec/effects.ru.md"
}
run() { NOVA_DOC_PAIR_BASELINE="$EMPTY_BASE" bash "$G" "$TMP" 2>&1; }
trap 'rm -rf "$TMP"' EXIT

# 1. Полная пара на своих языках — норма.
setup
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "полная пара проходит"; else bad "ложный отказ на полной паре: $out"; fi

# 2. Английская сторона без русской — отказ.
setup
printf 'A page with no Russian side at all.\n' > "$TMP/spec/lonely.md"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "lonely.ru.md"; then
    ok "ловит отсутствие русской стороны"
else
    bad "не поймал одиночку (rc=$rc): $out"
fi

# 3. Русская сторона без английской — тоже отказ (симметрия обязательна:
#    именно так и жил spec/paradigm).
setup
printf 'Страница, у которой нет английской стороны.\n' > "$TMP/spec/odinokaya.ru.md"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "odinokaya.md"; then
    ok "ловит отсутствие английской стороны"
else
    bad "не поймал русскую одиночку (rc=$rc): $out"
fi

# 4. Имя английское, текст русский — ровно случай GLOSSARY.en.md.
setup
printf 'Словарь переводчика, написанный по-русски целиком.\nВторая строка тоже русская.\n' > "$TMP/spec/slovar.md"
printf 'Пара, чтобы проверка пары не мешала проверке языка.\n' > "$TMP/spec/slovar.ru.md"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "имя обещает не то"; then
    ok "ловит русский текст под английским именем"
else
    bad "не поймал язык стороны (rc=$rc): $out"
fi

# 5. Русское имя, английский текст — непереведённая заглушка.
setup
printf 'This side is named Russian but never was translated.\n' > "$TMP/spec/stub.ru.md"
printf 'English side, as it should be.\n' > "$TMP/spec/stub.md"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "непереведённая заглушка"; then
    ok "ловит английский текст под русским именем"
else
    bad "не поймал заглушку (rc=$rc): $out"
fi

# 6. Старая форма .en.md не возвращается.
setup
printf 'Old-style English side.\n' > "$TMP/spec/legacy.en.md"
printf 'Русская сторона.\n' > "$TMP/spec/legacy.ru.md"
printf 'English side.\n' > "$TMP/spec/legacy.md"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "снята 2026-08-12"; then
    ok "ловит возврат формы .en.md"
else
    bad "не поймал .en.md (rc=$rc): $out"
fi

# 7. Строка базы без номера записи — отказ: имя без номера это долг без следа.
setup
printf 'Одиночка без пары.\n' > "$TMP/spec/dolg.ru.md"
printf 'spec/dolg.ru.md   # английской стороны нет\n' > "$EMPTY_BASE"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "без номера записи"; then
    ok "ловит строку базы без номера записи"
else
    bad "не поймал базу без номера (rc=$rc): $out"
fi

# 8. Та же строка С номером — база работает и гасит отказ пары.
setup
printf 'Одиночка без пары.\n' > "$TMP/spec/dolg.ru.md"
printf 'spec/dolg.ru.md   # №608 — английской стороны нет\n' > "$EMPTY_BASE"
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "база с номером гасит известный пробел"; else bad "база не сработала: $out"; fi

# 9. На настоящем дереве страж зелёный.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 10. Страж назван на странице правил.
if grep -q "check-doc-language-pairs.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-doc-language-pairs: 10/10 ok"; exit 0; fi
echo "селфтест check-doc-language-pairs: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
