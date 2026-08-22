#!/usr/bin/env bash
# Самотест check-novac-import-exists.py — обе стороны, на фикстурном корне.
#
# ДВА ПРАВИЛА, и у каждого свои центральные случаи.
#
# Правило A (имя существует): несущее — ПЛЕЧИ СУММЫ и МЕТОДЫ импортируются
# наравне с именами. `DefFn` и `TkNewtype` стоят в живых списках novac, а
# `export fn FnTable @add` отдаёт имя `add`; страж, знающий только `export type`
# и `export fn`, покрасил бы весь компилятор.
#
# Правило B (имя используется): несущее — ГРАНУЛЯРНОСТЬ МОДУЛЯ. Импорт виден всем
# со-равным файлам папки, поэтому «сосед импортировал, а пользуюсь я» законно.
# Первый замер считал по ФАЙЛУ и дал 224 «неиспользуемых» из 568 — 188 ложных.
# Последний зелёный случай ниже кодирует именно это.
#
# И отдельно: добавление правила B ПОКРАСИЛО пять прежде зелёных случаев этого
# файла — фикстуры импортировали имена, которых сами не употребляли. Самотест
# поймал собственные фикстуры, и это правильный порядок: сначала фикстура
# честная, потом правило зелёное.
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

echo "== правило A: имя существует =="
python "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет novac/src — зелёный (судить нечего)" "$?" "0"

printf 'export type TyId int\nexport fn raw_ty(t TyId) -> int => t as int\n' > "$L"
printf 'import ../types.{TyId, raw_ty}\nfn f(t TyId) -> int => raw_ty(t)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "тип и свободная функция — зелёный" "$RC" "0"

printf 'export type Interner {\n    rows []int\n}\nexport fn Interner mut @intern(k int) -> int => k\n' > "$L"
printf 'import ../types.{Interner, intern}\nfn f(mut i Interner) -> int => i.intern(1)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "МЕТОД отдаёт своё имя — зелёный" "$RC" "0"

printf 'export type TypeKind enum\n    | TkPrim\n    | TkNewtype\n' > "$L"
printf 'import ../types.{TypeKind, TkNewtype}\nfn f(k TypeKind) -> bool => k == TypeKind.TkNewtype\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "ПЛЕЧО суммы импортируется наравне — зелёный" "$RC" "0"

printf 'export const ENTRY_FN = "main"\n' > "$L"
printf 'import ../types.{ENTRY_FN}\nfn f(n str) -> bool => n == ENTRY_FN\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "export const — зелёный" "$RC" "0"

printf 'export type TyId int\n' > "$L"
printf 'import ../nowhere.{Whatever}\nfn f(w Whatever) -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "путь ВНЕ novac/src — зелёный (названная слепая зона)" "$RC" "0"

echo "== правило A краснеет =="
printf 'export type TyId int\nexport fn raw_ty(t TyId) -> int => t as int\n' > "$L"
printf 'import ../types.{TyId, raw_ty, no_such_name_at_all}\nfn f(t TyId) -> int => raw_ty(t)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "несуществующее имя — красный" "$RC" "1"
has "назвал файл и строку" "$OUT" "check/casts.nv:1"
has "назвал имя и модуль" "$OUT" "no_such_name_at_all"

printf 'export type TyId int\nfn raw_ty(t TyId) -> int => t as int\n' > "$L"
printf 'import ../types.{TyId, raw_ty}\nfn f(t TyId) -> int => raw_ty(t)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "функция есть, но НЕ export — красный" "$RC" "1"

echo "== правило B: имя используется =="
printf 'export type TyId int\nexport type Interner {\n    rows []int\n}\n' > "$L"
printf 'import ../types.{TyId}\nfn f(t TyId) -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "используется — зелёный" "$RC" "0"

printf 'import ../types.{TyId, Interner}\nfn f(t TyId) -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "существует, но НЕ используется — красный" "$RC" "1"
has "назвал правило B" "$OUT" "B"

printf 'import ../types.{TyId, Interner}\nfn g() -> int => 1\n' > "$SRC/check/a.nv"
printf 'fn h(i Interner) -> int => 1\n' > "$SRC/check/b.nv"
printf 'import ../types.{TyId}\nfn f(t TyId) -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "имя использует СОСЕД по модулю — зелёный (гранулярность модуля)" "$RC" "0"
rm -f "$SRC/check/a.nv" "$SRC/check/b.nv"

mkdir -p "$TMP/empty/src"
OUT=$(python "$G" "$TMP/empty" "$TMP/empty/src" 2>&1); RC=$?
check "каталог без .nv — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-import-exists: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
