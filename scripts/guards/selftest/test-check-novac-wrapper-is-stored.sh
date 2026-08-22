#!/usr/bin/env bash
# Самотест check-novac-wrapper-is-stored.py — обе стороны, на фикстурном корне.
#
# Центральный случай — ТРИ формы хранения, а не одна: поле записи, элемент
# вектора и полезная нагрузка плеча суммы. Страж, знающий только поле, покрасил
# бы `DeclId` (он хранится ровно плечом `DefTarget.DefType(DeclId)`), то есть
# живое пространство. Случай кодирует этот замер, а не допущение (Г10).
#
# И зеркальный случай: обёртка, встречающаяся ТОЛЬКО в сигнатурах, обязана
# краснеть — это и был отменённый `DefRow` волны В4.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-wrapper-is-stored.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

SRC="$TMP/src"; mkdir -p "$SRC/sem"
F="$SRC/sem/reg.nv"

echo "== проходит =="
python "$G" "$TMP/nowhere" >/dev/null 2>&1
check "нет novac/src — зелёный (судить нечего)" "$?" "0"

printf 'export type Row value {\n    v int\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "нет завёрнутых пространств — зелёный" "$RC" "0"

printf 'export type FnRow int\nexport type FnDef value {\n    next_row FnRow\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "хранится ПОЛЕМ записи — зелёный" "$RC" "0"
has "перечислил пространство в выводе" "$OUT" "FnRow"

printf 'export type TyId int\nexport type Chan {\n    types []TyId\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "хранится ЭЛЕМЕНТОМ вектора — зелёный" "$RC" "0"

printf 'export type DeclId int\nexport type DefTarget enum\n    | DefType(DeclId)\n    | DefFn(int)\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "хранится ПЛЕЧОМ суммы — зелёный (живой случай DeclId)" "$RC" "0"

mkdir -p "$SRC/check"
printf 'export type NodeId int\n' > "$F"
printf 'export type Rec value {\n    id NodeId\n}\n' > "$SRC/check/other.nv"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "хранение в ДРУГОМ файле — зелёный" "$RC" "0"
rm -f "$SRC/check/other.nv"

echo "== краснеет =="
printf 'export type DefRow int\nfn find(n str) -> DefRow => DefRow(0)\nfn use(d DefRow) -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "только в СИГНАТУРАХ — красный (это и был отменённый DefRow)" "$RC" "1"
has "назвал место объявления" "$OUT" "sem/reg.nv:1"
has "назвал само пространство" "$OUT" "DefRow"
has "объяснил разницу шага и личности" "$OUT" "ШАГ"

printf 'export type DefRow int\nexport type DefRow2 value {\n    x int\n}\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1); RC=$?
check "ПОХОЖЕЕ имя в чужом теле не считается хранением — красный" "$RC" "1"

mkdir -p "$TMP/empty/src"
OUT=$(python "$G" "$TMP/empty" "$TMP/empty/src" 2>&1); RC=$?
check "каталог без .nv — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-wrapper-is-stored: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
