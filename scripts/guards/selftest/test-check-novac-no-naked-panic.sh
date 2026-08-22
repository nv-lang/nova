#!/usr/bin/env bash
# Самотест check-novac-no-naked-panic.py — обе стороны, на фикстурном корне.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-naked-panic.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac/src/diag" "$FIX/novac/src/lex"

echo "== проходит =="
python "$G" "$TMP/empty-root" >/dev/null 2>&1
check "нет novac/src — зелёный" "$?" "0"

printf 'export fn ice(msg str) -> never {\n    panic("E_NOVAC_ICE")\n}\n' > "$FIX/novac/src/diag/diag.nv"
printf 'fn lex_it() -> int => 1\n' > "$FIX/novac/src/lex/lex.nv"
python "$G" "$FIX" >/dev/null 2>&1
check "panic только в двери diag.nv — зелёный" "$?" "0"

printf '// a comment mentioning panic( is not a call\nfn f() -> int => 2\n' > "$FIX/novac/src/lex/lex.nv"
python "$G" "$FIX" >/dev/null 2>&1
check "panic( в комментарии — не находка" "$?" "0"

echo "== ловит =="
printf 'fn f() -> int {\n    panic("boom")\n}\n' > "$FIX/novac/src/lex/lex.nv"
python "$G" "$FIX" >/dev/null 2>&1
check "голый panic вне diag — красный" "$?" "1"

echo "== настоящее дерево =="
python "$G" "$ROOT" >/dev/null 2>&1
check "novac проекта чист" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
