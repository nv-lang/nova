#!/usr/bin/env bash
# Самотест check-novac-export-has-caller.py — обе стороны, на фикстурном корне.
#
# Центральный случай — ТЕСТ СЧИТАЕТСЯ ВЫЗЫВАЮЩИМ. Дверь, которую зовёт только
# модульный тест, спрошена: тест — это спрос, причём самый честный. Страж, не
# знающий этого, покрасил бы законные двери, проверяемые только тестом.
#
# Второй — УПОТРЕБЛЕНИЕ, а не вызов: тип, стоящий в поле чужой записи, спрошен,
# хотя его никто не «вызывает».
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-export-has-caller.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

SRC="$TMP/src"; mkdir -p "$SRC/sem" "$SRC/check"
L="$SRC/sem/reg.nv"
U="$SRC/check/use.nv"

echo "== проходит =="
python "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет novac/src — зелёный (судить нечего)" "$?" "0"

printf 'export fn door(n int) -> int => n + 1\n' > "$L"
printf 'fn caller() -> int => door(1)\n' > "$U"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "экспорт со вызывающим — зелёный" "$RC" "0"
has "назвал число экспортов" "$OUT" "экспортов"

printf 'export fn door(n int) -> int => n + 1\n' > "$L"
rm -f "$U"
printf 'test "door works" {\n    assert(door(1) == 2)\n}\n' > "$SRC/sem/reg_test.nv"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "зовёт только ТЕСТ — зелёный (тест это спрос)" "$RC" "0"
rm -f "$SRC/sem/reg_test.nv"

printf 'export type FnRow int\n' > "$L"
printf 'type Holder value {\n    row FnRow\n}\n' > "$U"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "тип стоит в ПОЛЕ чужой записи — зелёный (употребление, не вызов)" "$RC" "0"

printf 'export const ENTRY_FN = "main"\n' > "$L"
printf 'fn is_entry(n str) -> bool => n == ENTRY_FN\n' > "$U"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "export const со читателем — зелёный" "$RC" "0"

echo "== краснеет =="
printf 'export fn lonely(n int) -> int => n + 1\n' > "$L"
printf 'fn other() -> int => 1\n' > "$U"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "экспорт без вызывающего — красный" "$RC" "1"
has "назвал файл и строку" "$OUT" "sem/reg.nv:1"
has "назвал имя" "$OUT" "lonely"
has "назвал конвенцию" "$OUT" "П35"

printf 'export type Unused int\n' > "$L"
printf 'fn other() -> int => 1\n' > "$U"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "экспортированный ТИП без употребления — красный" "$RC" "1"

printf 'export fn shadow(n int) -> int => n\nexport fn shadowed(n int) -> int => n\n' > "$L"
printf 'fn c() -> int => shadowed(1)\n' > "$U"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "ПРЕФИКС чужого имени не считается спросом — красный" "$RC" "1"
has "покраснел именно на префиксе" "$OUT" "shadow\`"

mkdir -p "$TMP/empty/src"
OUT=$(python "$G" "$TMP/empty" "$TMP/empty/src" 2>&1); RC=$?
check "каталог без .nv — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-export-has-caller: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
