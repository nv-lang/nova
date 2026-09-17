#!/usr/bin/env bash
# Самотест check-doc-guide-names.py.
#
# ДВА ПРЕДМЕТА: (1) имя со скобками существует в языке; (2) снятые формы без скобок
# судятся РОСТОМ. Случаи по ОСЯМ, а не по историям: (существует ли имя) × (есть ли оно
# в базе) плюс два края — пустой источник истины и форма без скобок.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-doc-guide-names.py"
TMP="${TMPDIR:-/tmp}/selftest_dgn_$$"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

add_codes() {
    {
        echo 'const C1: &str = "E_EXTERNAL_FN_RETRACTED";'
        echo 'const C2: &str = "E_KW_REMOVED_READONLY";'
        echo 'const C3: &str = "E_KW_REMOVED_LET";'
        echo 'const C4: &str = "E_ADDR_OF_REMOVED";'
        echo 'const C5: &str = "E_REDUNDANT_POINTER_RO";'
        echo 'const C6: &str = "E_UNSAFE_TYPE_MODIFIER_RENAMED";'
    } > "$TMP/compiler-codegen/src/codes.rs"
}

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
    add_codes
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

# 4. КРАЙ: слово без скобок, НЕ входящее в список снятых форм, не судится ни
#    одним предметом — иначе обычные английские слова в обратных кавычках дали
#    бы ложняки. (Слово без скобок, которое СНЯТО, судится вторым предметом —
#    случаи 7–9; до правки 2026-09-17 не судилось вовсе, и это была дыра.)
setup 'A `consume` value is moved, not copied.' ''
run
if [ $? -eq 0 ]; then ok "обычное слово без скобок не предмет проверки"; else bad "4: ложняк на слове без скобок: $(cat "$TMP/.err")"; fi

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

# --- ВТОРОЙ ПРЕДМЕТ: снятые формы без скобок, судятся РОСТОМ ---
# Чтобы случаи были честными, временное дерево должно нести КОДЫ диагностик:
# без них страж обязан отказать («список протух»), и это случай 10.

# 7. Снятая форма названа в прозе, базы на неё нет — ОТКАЗ с именем клетки.
setup 'Declare it with `external fn name()` for now.' ''
run
if [ $? -eq 1 ] && grep -q "page.md|external-fn" "$TMP/.err"; then
    ok "снятая форма сверх базы ловится, клетка названа"
else
    bad "7: не поймал рост снятых форм: $(cat "$TMP/.err")"
fi

# 8. То же упоминание, внесённое в базу с причиной, — норма (дока ОБЪЯСНЯЕТ).
setup 'Plan 91.12 removed `external fn` in favour of extern "C" fn.' 'page.md|external-fn=1  # объясняет ретракцию'
run
if [ $? -eq 0 ]; then ok "упоминание в пределах базы не краснит"; else bad "8: база второго предмета не сработала: $(cat "$TMP/.err")"; fi

# 9. Упоминаний стало БОЛЬШЕ, чем в базе, — отказ (храповик только вниз).
setup 'Both `external fn` here and `external fn` there.' 'page.md|external-fn=1  # было одно'
run
if [ $? -eq 1 ] && grep -q "было 1, стало 2" "$TMP/.err"; then
    ok "прибавление к существующей клетке ловится"
else
    bad "9: не поймал рост внутри клетки: $(cat "$TMP/.err")"
fi

# 10. КРАЙ, РАДИ КОТОРОГО СПИСОК И ПРОВЕРЯЕТ СЕБЯ: кода диагностики нет в
#     компиляторе — значит форма разснята или переименована, и список протух.
#     Отказ, а не тишина: иначе страж судил бы по вчерашнему дереву.
setup 'Nothing special here.' ''
: > "$TMP/compiler-codegen/src/codes.rs"
run
if [ $? -eq 1 ] && grep -q "E_EXTERNAL_FN_RETRACTED" "$TMP/.err"; then
    ok "протухший список снятых форм — отказ"
else
    bad "10: страж не заметил пропажу кода диагностики: $(cat "$TMP/.err")"
fi

# 11. Форма внутри ```-блока НЕ судится этим стражем: код — дом соседа
#     (check-doc-examples), и два дома на один предмет запрещены.
setup '```nova
external fn c_open(path str) -> int
```' ''
run
if [ $? -eq 0 ]; then ok "код в блоке — не предмет этого стража"; else bad "11: влез в чужой дом: $(cat "$TMP/.err")"; fi

rm -rf "$TMP"
if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-doc-guide-names: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-doc-guide-names: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
