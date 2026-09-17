#!/usr/bin/env bash
# Самотест check-doc-guide-names.py.
#
# Случаи подобраны по ОСЯМ, а не по историям: (существует ли имя) × (есть ли оно
# в базе) плюс два края — пустой источник истины и форма без скобок.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-doc-guide-names.py"
TMP="${TMPDIR:-/tmp}/selftest_dgn_$$"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {   # $1 — текст страницы гайда, $2 — содержимое базы (может быть пустым)
    rm -rf "$TMP"
    mkdir -p "$TMP/docs/guide" "$TMP/std/src" "$TMP/scripts/guards" \
             "$TMP/compiler-codegen/src/lexer"
    # Источник истины: лексер (ключевые слова) и std/src (объявления fn).
    {
        echo 'match tok {'
        echo '    "assert" => TokenKind::KwAssert,'
        echo '    "panic" => TokenKind::KwPanic,'
        echo '    "requires" => TokenKind::KwRequires,'
        echo '}'
    } > "$TMP/compiler-codegen/src/lexer/mod.rs"
    # Двадцать имён — чтобы пройти порог «источник прочитан».
    { for i in $(seq 1 20); do echo "fn std_fn_$i() -> () {}"; done; } \
        > "$TMP/std/src/lib.nv"
    printf '%s\n' "$1" > "$TMP/docs/guide/page.md"
    printf '%s\n' "${2:-}" > "$TMP/scripts/guards/doc-guide-names.baseline"
}

run() { python "$G" "$TMP" > "$TMP/.out" 2> "$TMP/.err"; }

# 1. Названа существующая форма — норма.
setup 'Use `assert(cond)` before the call.' ''
run
if [ $? -eq 0 ]; then ok "существующее имя проходит"; else bad "1: ложняк на `assert(`: $(cat "$TMP/.err")"; fi

# 2. Названа НЕсуществующая форма — отказ. Это случай 2026-09-17 (`debug_assert`).
setup 'This applies to `assert`, `debug_assert(` and friends.' ''
run
if [ $? -eq 1 ] && grep -q "debug_assert" "$TMP/.err"; then
    ok "несуществующее имя ловится и НАЗЫВАЕТСЯ"
else
    bad "2: не поймал debug_assert: $(cat "$TMP/.err")"
fi

# 3. То же имя, внесённое в базу с причиной, — норма.
setup 'C side calls `debug_assert(` here.' 'debug_assert  # C-макрос в примере'
run
if [ $? -eq 0 ]; then ok "имя из базы не краснит"; else bad "3: база не сработала: $(cat "$TMP/.err")"; fi

# 4. КРАЙ: форма БЕЗ скобок не судится вовсе — иначе английские слова в
#    обратных кавычках дали бы ложняки (`ro`, `consume`, `old`).
setup 'The `readonly` spelling is gone; use `ro`.' ''
run
if [ $? -eq 0 ]; then ok "имя без скобок не предмет проверки"; else bad "4: ложняк на слове без скобок"; fi

# 5. КРАЙ: источник истины не прочитан (пустой лексер и std) — ОТКАЗ, а не
#    зелёное. Иначе страж молчал бы там, где неизвестно ВСЁ.
setup 'Use `assert(cond)`.' ''
: > "$TMP/compiler-codegen/src/lexer/mod.rs"
: > "$TMP/std/src/lib.nv"
run
if [ $? -eq 1 ] && grep -q "istochnik\|источник" "$TMP/.err"; then
    ok "пустой источник истины — отказ, а не тишина"
else
    bad "5: при пустом источнике страж не отказал: $(cat "$TMP/.err")"
fi

# 6. Имя в базе, которого в доке больше нет, — не отказ, но НАЗВАНО.
setup 'Nothing here.' 'obsolete_name  # больше не упоминается'
run
if [ $? -eq 0 ] && grep -q "obsolete_name" "$TMP/.out"; then
    ok "устаревшая строка базы названа, но не краснит"
else
    bad "6: не сообщил о неиспользуемой строке базы: $(cat "$TMP/.out")"
fi

rm -rf "$TMP"
if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-doc-guide-names: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-doc-guide-names: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
