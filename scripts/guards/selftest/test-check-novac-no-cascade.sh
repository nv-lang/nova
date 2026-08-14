#!/usr/bin/env bash
# Самотест check-novac-no-cascade.sh — обе стороны, через поддельный novac.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-cascade.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac/fixtures"
echo "x" > "$FIX/novac/fixtures/neg_probe.nv"
D='{\"id\":1,\"code\":\"E_X\",\"severity\":\"error\",\"primary\":true,\"message\":\"m\"}'
W='{\"id\":2,\"code\":\"W_Y\",\"severity\":\"warning\",\"primary\":false,\"message\":\"w\"}'

mkbin() { printf '#!/bin/sh\n%s\n' "$1" > "$TMP/bin.sh"; chmod +x "$TMP/bin.sh"; }
run() { sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1; echo $?; }

echo "== проходит =="
sh "$G" "$FIX" "$TMP/absent" >/dev/null 2>&1
check "без бинаря — зелёный" "$?" "0"
mkbin "echo \"[$D]\""
check "ровно один error" "$(run)" "0"
mkbin "echo \"[$D, $W]\""
check "error + warning — warning не считается" "$(run)" "0"

echo "== ловит =="
mkbin "echo \"[$D, $D]\""
check "два error — каскад, красный" "$(run)" "1"
mkbin "echo \"[$W]\""
check "ноль error — красный" "$(run)" "1"
mkbin 'echo "broken"'
check "не-JSON — красный" "$(run)" "1"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
