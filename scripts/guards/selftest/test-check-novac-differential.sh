#!/usr/bin/env bash
# Самотест check-novac-differential.sh — обе стороны, через поддельные novac И
# оракул (страж читает оракула по фиксированному пути внутри $1-корня — значит
# фикстурный корень несёт своего поддельного оракула).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-differential.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/novac/fixtures" "$FIX/nova-cli/target/release"
echo "x" > "$FIX/novac/fixtures/pos_probe.nv"
ORACLE="$FIX/nova-cli/target/release/nova.exe"

mkoracle() { printf '#!/bin/sh\n%s\n' "$1" > "$ORACLE"; chmod +x "$ORACLE"; }
mkbin()    { printf '#!/bin/sh\n%s\n' "$1" > "$TMP/bin.sh"; chmod +x "$TMP/bin.sh"; }
run() { sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1; echo $?; }

echo "== честные «судить нечего» =="
sh "$G" "$FIX" "$TMP/absent" >/dev/null 2>&1
check "без novac — зелёный" "$?" "0"
mkbin 'exit 0'
check "без оракула — зелёный" "$(run)" "0"

echo "== исходы совпали — проходит =="
mkoracle 'exit 0'
check "оба приняли" "$(run)" "0"
mkoracle 'exit 1'; mkbin 'exit 1'
check "оба отвергли" "$(run)" "0"

echo "== расхождение — ловит =="
mkoracle 'exit 0'; mkbin 'exit 1'
check "novac отверг, оракул принял, allow пуст — красный" "$(run)" "1"

echo "== расхождение в allow — законно =="
printf 'novac/fixtures/pos_probe.nv\n' > "$FIX/novac/divergences.allow"
check "то же расхождение из allow — зелёный" "$(run)" "0"

echo "== ОТВЕТ, а не только вердикт (274.3/F18) =="
mkdir -p "$FIX/scripts/tools"
rm -f "$FIX/novac/divergences.allow"
mksmoke() { printf '#!/bin/sh\n%s\n' "$1" > "$FIX/scripts/tools/novac-e1-smoke.sh"; chmod +x "$FIX/scripts/tools/novac-e1-smoke.sh"; }

mkoracle 'exit 0'; mkbin 'exit 0'; mksmoke 'exit 0'
check "оба приняли и ответ совпал — зелёный" "$(run)" "0"

mksmoke 'echo "stdout: novac 9, oracle 7"; exit 1'
check "оба приняли, но ОТВЕТ разный — красный" "$(run)" "1"

sh "$G" "$FIX" "$TMP/bin.sh" 2>&1 | grep -q "ОТВЕТ разный" \
  && ok "красный назван правильно (про ответ, а не про вердикт)" \
  || bad "красный, но не про ответ"

mkoracle 'exit 1'; mkbin 'exit 1'; mksmoke 'exit 1'
check "оба отвергли — смоук не запускается, зелёный" "$(run)" "0"

mkoracle 'exit 0'; mkbin 'exit 0'; mksmoke 'exit 1'
NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1
check "NOVAC_CORPUS=0 — шаг поведения пропущен осознанно" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
