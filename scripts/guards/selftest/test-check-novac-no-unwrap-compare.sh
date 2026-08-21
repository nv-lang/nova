#!/usr/bin/env bash
# Самотест check-novac-no-unwrap-compare.py — обе стороны, на фикстурном корне.
#
# Центральный случай — РАЗНИЦА между индексированием и сравнением. Страж, который
# просто запрещает `raw_*`, покрасил бы весь компилятор: распаковка ради индекса
# это и есть её назначение. Красным обязано быть только сравнение ДВУХ
# распакованных, потому что оно теряет тип — и второй случай ниже это кодирует
# замером, а не допущением (Г10): `raw_ty(t) >= types.len()` остаётся зелёным.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-unwrap-compare.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

SRC="$TMP/src"; mkdir -p "$SRC/sem"
F="$SRC/sem/mangle.nv"

echo "== проходит =="
python "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет novac/src — зелёный (судить нечего)" "$?" "0"

printf 'fn same(a TyId, b TyId) -> bool => a == b\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "завёрнутое сравнение — зелёный" "$RC" "0"

printf 'fn at(t TyId, n int) -> bool => raw_ty(t) >= n\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "распаковка ради ИНДЕКСА (сравнение с числом) — зелёный" "$RC" "0"

printf 'fn at(t TyId, v Vec[int]) -> bool => raw_ty(t) >= v.len()\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "распаковка против ДЛИНЫ — зелёный" "$RC" "0"

printf 'fn ord(a TyId, b TyId) -> bool => raw_ty(a) < raw_ty(b)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "УПОРЯДОЧИВАНИЕ двух распакованных — зелёный (другое правило)" "$RC" "0"

printf '// raw_ty(a) == raw_ty(b) в комментарии — рассказ, не код\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "то же в КОММЕНТАРИИ — зелёный" "$RC" "0"

echo "== краснеет =="
printf 'fn same(a TyId, b TyId) -> bool => raw_ty(a) == raw_ty(b)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "две распаковки через == — красный" "$RC" "1"
has "назвал файл и строку" "$OUT" "sem/mangle.nv:1"

printf 'fn diff(a FnRow, b FnRow) -> bool => raw_row(a) != raw_row(b)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "две распаковки через != — красный" "$RC" "1"

printf 'fn cross(a TyId, b DeclId) -> bool => raw_ty(a) == raw_decl(b)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "сравнение РАЗНЫХ пространств — красный (это и есть худший случай)" "$RC" "1"

printf 'fn f(a TyId, t TyId) -> bool => raw_ty(representation_of(ctx, a)) == raw_ty(t)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "распаковка ВЫЗОВА через == — красный (вложенные скобки)" "$RC" "1"

mkdir -p "$TMP/empty/src"
OUT=$(python "$G" "$TMP/empty" "$TMP/empty/src" 2>&1); RC=$?
check "каталог без .nv — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-no-unwrap-compare: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
