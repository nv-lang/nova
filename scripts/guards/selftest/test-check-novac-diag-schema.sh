#!/usr/bin/env bash
# Самотест check-novac-diag-schema.sh — обе стороны, через поддельный novac
# ($2-шов стража): настоящего бинаря нет и не требуется.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-diag-schema.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac/fixtures"
echo "x" > "$FIX/novac/fixtures/neg_probe.nv"

mkbin() { printf '#!/bin/sh\n%s\n' "$1" > "$TMP/bin.sh"; chmod +x "$TMP/bin.sh"; }
run() { sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1; echo $?; }

echo "== бинаря нет — честное «судить нечего» =="
sh "$G" "$FIX" "$TMP/absent" >/dev/null 2>&1
check "без бинаря — зелёный" "$?" "0"

echo "== валидная диагностика — проходит =="
mkbin 'echo "[{\"id\":1,\"code\":\"E_X\",\"severity\":\"error\",\"primary\":true,\"message\":\"m\"}]"'
check "полные поля" "$(run)" "0"

echo "== ловит =="
mkbin 'echo "this is not json"'
check "не-JSON — красный" "$(run)" "1"
mkbin 'echo "[{\"id\":1,\"severity\":\"error\",\"primary\":true,\"message\":\"m\"}]"'
check "нет поля code — красный" "$(run)" "1"

echo "== ноль фикстур — честное «судить нечего» =="
rm "$FIX/novac/fixtures/neg_probe.nv"
mkbin 'echo "[]"'
check "пустой набор — зелёный" "$(run)" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
