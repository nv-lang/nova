#!/usr/bin/env bash
# Самотест check-novac-fixture-expect.sh — обе стороны, через поддельный novac.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-fixture-expect.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

# Дерево-подделка: novac_require_bin требует novac/src/main.nv, иначе
# отсутствие бинаря законно зелёное — и «ловит» ничего не доказало бы.
FIX="$TMP/root/novac/fixtures/form"; mkdir -p "$FIX" "$TMP/root/novac/src"
: > "$TMP/root/novac/src/main.nv"

mkbin() { printf '#!/bin/sh\n%s\n' "$1" > "$TMP/bin.sh"; chmod +x "$TMP/bin.sh"; }
run() { sh "$G" "$TMP/root" "$TMP/bin.sh" >/dev/null 2>&1; echo $?; }

D_SLICE='{\"severity\":\"error\",\"message\":\"outside the subset: a slice is not compiled\"}'
D_RANGE='{\"severity\":\"error\",\"message\":\"outside the subset: a range is a for head\"}'
D_WARN='{\"severity\":\"warning\",\"message\":\"outside the subset: a slice is not compiled\"}'

echo "== проходит =="
sh "$G" "$TMP/root" "$TMP/absent" >/dev/null 2>&1
check "без бинаря — красный (исходник есть)" "$?" "1"

printf 'fn f() {}\n' > "$FIX/neg_1.nv"
mkbin "echo \"$D_RANGE\""
check "фикстура без маркера — не судится" "$(run)" "0"

printf '// NOVAC_EXPECT a slice is not compiled\nfn f() {}\n' > "$FIX/neg_1.nv"
mkbin "echo \"$D_SLICE\""
check "маркер совпал — зелёный" "$(run)" "0"

mkbin "echo \"$D_RANGE\"; echo \"$D_SLICE\""
check "совпал не первой строкой — зелёный" "$(run)" "0"

echo "== ловит =="
mkbin "echo \"$D_RANGE\""
check "пришёл другой текст — красный" "$(run)" "1"

mkbin "echo \"$D_WARN\""
check "совпало только в warning — красный" "$(run)" "1"

mkbin "true"
check "ни одной диагностики — красный" "$(run)" "1"

mkbin "echo 'broken'"
check "нечитаемый вывод — красный" "$(run)" "1"

# Позитивная фикстура маркера не несёт: правило про neg_*, и страж не должен
# расширять свой предмет молча.
rm -f "$FIX/neg_1.nv"
printf '// NOVAC_EXPECT a slice is not compiled\nfn f() {}\n' > "$FIX/pos_1.nv"
mkbin "echo \"$D_RANGE\""
check "pos_* с маркером — не судится" "$(run)" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
