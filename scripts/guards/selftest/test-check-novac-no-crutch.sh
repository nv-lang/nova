#!/usr/bin/env bash
# Самотест check-novac-no-crutch.py — обе стороны, на фикстурном корне.
#
# Центральный случай — «маркер выше по ЭТОМУ блоку комментария»: на первом же
# прогоне по живому дереву страж с окном в три строки покрасил два законных
# места (check/typing.nv:318 и sem/sem.nv:797) — хвосты шестистрочных блоков
# [LEGACY-#676]. Случай кодирует этот замер, а не допущение (Г10): окно —
# весь непрерывный блок `//`, и обрыв блока пустой строкой или кодом снимает
# защиту, потому что тогда это уже другой комментарий.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-crutch.py"
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

printf '// An external declaration: the signature without the body.\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "чистый файл — зелёный" "$RC" "0"
has   "зелёный печатает счёт файлов" "$OUT" 'файлов .nv: 1'

printf '// This is an honest model, not a workaround.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "отрицание «not a workaround» — зелёный" "$?" "0"

printf '// [LEGACY-#676] the oracle mis-parses this form, so the call is\n// routed through a binding hop; when 676 lands the hop goes away\n// and the direct form returns. The hop is the whole workaround.\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>/dev/null); RC=$?
check "workaround под маркером в ЭТОМ блоке (3-я строка) — зелёный" "$RC" "0"
has   "управляемый обход посчитан" "$OUT" 'под маркером: 1'

printf '// [LEGACY-#676] a six-line block, exactly the live shape that a\n// three-line window painted red on 2026-08-19:\n// line three\n// line four\n// line five\n// and the hop is the workaround.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "маркер в 6-строчном блоке (ЗАМЕР: окно в 3 строки красило зря) — зелёный" "$?" "0"

echo "== ловит =="
printf '// This branch is a bootstrap crutch with an end.\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1 >/dev/null); RC=$?
check "«crutch» — красный" "$RC" "1"
has   "красный называет место" "$OUT" 'sem/mangle.nv:1'
has   "красный объясняет, чем костыль отличается от обхода" "$OUT" 'LEGACY-#NNN'

printf '// Это костыль бутстрапа, а не обязательство.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "«костыль» по-русски — красный" "$?" "1"

printf '// A tiny lattice for now; the real one lands later.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "«for now» — красный (срока нет, значит вечно)" "$?" "1"

printf '// Last put wins here, works, ставим на время.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "«на время» — красный" "$?" "1"

printf '// A stopgap until the registry knows the answer.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "«stopgap» — красный" "$?" "1"

printf '// A quick hack to get the shape through.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "«hack» — красный" "$?" "1"

printf '// The call is routed through a binding hop as a workaround.\nfn f() -> int => 1\n' > "$F"
OUT=$(python "$G" "$TMP" "$SRC" 2>&1 >/dev/null); RC=$?
check "workaround БЕЗ маркера — красный" "$RC" "1"
has   "красный требует именно маркер" "$OUT" 'без маркера'

printf '// [LEGACY-#676] the marker is here, in its own block.\n\n// A different comment: the hop is the workaround.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "маркер в ДРУГОМ блоке (разрыв пустой строкой) — красный" "$?" "1"

printf '// [LEGACY-#676] marker.\nfn g() -> int => 2\n// the hop is the workaround.\nfn f() -> int => 1\n' > "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "маркер в блоке, разорванном КОДОМ — красный" "$?" "1"

rm -f "$F"
python "$G" "$TMP" "$SRC" >/dev/null 2>&1
check "директория есть, а .nv нет — красный (страж потерял мишень, класс №519)" "$?" "1"

echo "== настоящее дерево =="
OUT=$(python "$G" "$ROOT" 2>/dev/null); RC=$?
check "novac проекта чист" "$RC" "0"
has   "счёт управляемых обходов печатается" "$OUT" 'под маркером:'

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
