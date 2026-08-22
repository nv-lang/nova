#!/usr/bin/env bash
# Самотест check-novac-import-exists.py — обе стороны, на фикстурном корне.
#
# Центральный случай — ПЛЕЧИ СУММЫ импортируются наравне с именами: `DefFn`,
# `TkNewtype` в живом novac стоят в списках импорта, и страж, знающий только
# `export type`/`export fn`, покрасил бы весь компилятор. Второй — МЕТОДЫ:
# `export fn FnTable @add` отдаёт имя `add`. Оба закодированы замером (Г10).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-import-exists.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

SRC="$TMP/src"; mkdir -p "$SRC/types" "$SRC/check"
L="$SRC/types/types.nv"
F="$SRC/check/casts.nv"

echo "== проходит =="
python "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет novac/src — зелёный (судить нечего)" "$?" "0"

printf 'export type TyId int\nexport fn raw_ty(t TyId) -> int => t as int\n' > "$L"
printf 'import ../types.{TyId, raw_ty}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "тип и свободная функция — зелёный" "$RC" "0"

printf 'export type Interner {\n    rows []int\n}\nexport fn Interner mut @intern(k int) -> int => k\n' > "$L"
printf 'import ../types.{Interner, intern}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "МЕТОД отдаёт своё имя — зелёный" "$RC" "0"

printf 'export type TypeKind enum\n    | TkPrim\n    | TkNewtype\n' > "$L"
printf 'import ../types.{TypeKind, TkNewtype}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "ПЛЕЧО суммы импортируется наравне — зелёный" "$RC" "0"

printf 'export const ENTRY_FN = "main"\n' > "$L"
printf 'import ../types.{ENTRY_FN}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "export const — зелёный" "$RC" "0"

printf 'export type TyId int\n' > "$L"
printf 'import ../nowhere.{Whatever}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "путь ВНЕ novac/src — зелёный (названная слепая зона)" "$RC" "0"

echo "== краснеет =="
printf 'export type TyId int\nexport fn raw_ty(t TyId) -> int => t as int\n' > "$L"
printf 'import ../types.{TyId, raw_ty, no_such_name_at_all}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "несуществующее имя — красный" "$RC" "1"
has "назвал файл и строку" "$OUT" "check/casts.nv:1"
has "назвал имя и модуль" "$OUT" "no_such_name_at_all"

printf 'export type TyId int\nfn raw_ty(t TyId) -> int => t as int\n' > "$L"
printf 'import ../types.{TyId, raw_ty}\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "функция есть, но НЕ export — красный" "$RC" "1"

mkdir -p "$TMP/empty/src"
OUT=$(python "$G" "$TMP/empty" "$TMP/empty/src" 2>&1); RC=$?
check "каталог без .nv — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-import-exists: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
