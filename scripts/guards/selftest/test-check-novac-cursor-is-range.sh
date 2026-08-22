#!/usr/bin/env bash
# Самотест check-novac-cursor-is-range.py — обе стороны, на фикстурном корне.
#
# Центральный случай — РАЗЛИЧИЕ между «шаг всегда один» и «шаг несёт информацию».
# Страж, запрещающий `while` вообще, покрасил бы обход цепочки и фикспойнт, то
# есть формы, у которых курсора нет. Красным обязан быть ровно один силуэт:
# `mut i = X` + `while i < Y` + РОВНО ОДИН безусловный `i += 1`. Живой случай
# `emit_c` (шаг на два при экранировании) закодирован ниже как зелёный — это
# замер, а не допущение (Г10).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-cursor-is-range.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

SRC="$TMP/src"; mkdir -p "$SRC/sem"
F="$SRC/sem/walk.nv"

echo "== проходит =="
python "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет novac/src — зелёный (судить нечего)" "$?" "0"

printf 'fn f(v Vec[int]) -> () {\n    for j in 2..v.len() {\n        use(v[j])\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "уже диапазон — зелёный" "$RC" "0"

printf 'fn f(b Vec[int]) -> () {\n    mut i = 0\n    while i < b.len() {\n        if b[i] == 1 {\n            i += 2\n        } else {\n            i += 1\n        }\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "шаг на ДВА в одной ветке — зелёный (шаг несёт информацию)" "$RC" "0"

printf 'fn f(b Vec[int]) -> () {\n    mut i = 0\n    while i < b.len() {\n        if b[i] == 1 {\n            i += 1\n        }\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "продвижение УСЛОВНОЕ (внутри if) — зелёный" "$RC" "0"

printf 'fn f(t Table) -> () {\n    mut r = t.head\n    while r >= 0 {\n        r = t.rows[r].next\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "обход ЦЕПОЧКИ (курсора-счётчика нет) — зелёный" "$RC" "0"

printf 'fn f() -> () {\n    mut grew = true\n    while grew {\n        grew = step()\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "фикспойнт while-по-флагу — зелёный" "$RC" "0"

echo "== краснеет =="
printf 'fn f(v Vec[int]) -> () {\n    mut j = 2\n    while j < v.len() {\n        use(v[j])\n        j += 1\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "ручной курсор с шагом один — красный" "$RC" "1"
has "назвал файл и строку" "$OUT" "sem/walk.nv:3"
has "предложил готовую форму" "$OUT" "for j in 2..v.len()"

printf 'fn f(v Vec[int], n int) -> () {\n    mut k = n + 1\n    while k < v.len() {\n        use(v[k])\n        k += 1\n    }\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "начало — ВЫРАЖЕНИЕ, а не литерал — тоже красный" "$RC" "1"
has "перенёс выражение в диапазон" "$OUT" "for k in n + 1..v.len()"

mkdir -p "$TMP/empty/src"
OUT=$(python "$G" "$TMP/empty" "$TMP/empty/src" 2>&1); RC=$?
check "каталог без .nv — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-cursor-is-range: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
