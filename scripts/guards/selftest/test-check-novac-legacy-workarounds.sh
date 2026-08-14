#!/usr/bin/env bash
# Самотест check-novac-legacy-workarounds.sh — обе стороны, на фикстурном корне.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-legacy-workarounds.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac/src/lex" "$FIX/docs/plans"
REG="$FIX/docs/plans/221.1-bug-sweep.md"

echo "== проходит =="
sh "$G" "$TMP/empty-root" >/dev/null 2>&1
check "нет novac — зелёный" "$?" "0"

printf '| 900 | K1 | bug demo. Статус: ОТКРЫТ |\n' > "$REG"
printf '// [LEGACY-#900] workaround site\nfn f() -> int => 1\n' > "$FIX/novac/src/lex/lex.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "маркер открытого бага — зелёный" "$?" "0"

echo "== ловит =="
printf '// [LEGACY-#901] workaround site\nfn f() -> int => 1\n' > "$FIX/novac/src/lex/lex.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "маркер бага без строки в реестре — красный" "$?" "1"

printf '| 901 | K1 | bug demo. Статус: ЗАКРЫТ 2026-08-14 |\n' >> "$REG"
sh "$G" "$FIX" >/dev/null 2>&1
check "маркер ЗАКРЫТОГО бага — красный (фоссилизация)" "$?" "1"

printf '// EXPECT_CC_ERROR boom\nfn f() -> int => 1\n' > "$FIX/novac/src/lex/lex.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "EXPECT_CC_ERROR без [LEGACY-#NNN] — красный" "$?" "1"

printf '// EXPECT_CC_ERROR boom\n// [LEGACY-#900] attributed\nfn f() -> int => 1\n' > "$FIX/novac/src/lex/lex.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "EXPECT_CC_ERROR с атрибуцией — зелёный" "$?" "0"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "novac проекта чист" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
