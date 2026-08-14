#!/usr/bin/env bash
# Самотест check-rt-sigpipe-ign.sh — обе стороны, на фикстурном корне.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-rt-sigpipe-ign.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/compiler-codegen/nova_rt"
D="$FIX/compiler-codegen/nova_rt/driver.c"

echo "== проходит =="
printf 'void nova_driver_init(void) {\n    signal(SIGPIPE, SIG_IGN);\n}\n' > "$D"
sh "$G" "$FIX" >/dev/null 2>&1
check "живой SIG_IGN в теле — зелёный" "$?" "0"

echo "== ловит =="
printf 'void nova_driver_init(void) {\n    int x = 0;\n}\n' > "$D"
sh "$G" "$FIX" >/dev/null 2>&1
check "тело без SIG_IGN — красный" "$?" "1"

rm "$D"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет driver.c — красный" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "driver.c проекта несёт SIG_IGN" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
