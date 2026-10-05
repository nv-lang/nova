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
NOVAC_SMOKE=0 NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/seam.out" 2>&1
check "NOVAC_SMOKE=0 — шаг поведения пропущен осознанно (красный смоук не виден)" "$?" "0"
grep -q 'NOVAC_SMOKE=0' "$TMP/seam.out"
check "NOVAC_SMOKE=0 — пропуск НАЗВАН в выводе стадией, а не молчит (№992)" "$?" "0"
NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1
check "NOVAC_CORPUS=0 один — смоук ВКЛЮЧЁН, красный смоук красит (швы разделены, №992)" "$?" "1"

echo "== близнец NOVAC_TWIN (2026-10-03): оракул собирает близнеца, novac — фикстуру =="
mkoracle 'exit 0'; mkbin 'exit 0'
printf '// NOVAC_TWIN pos_twin.nv\nx\n' > "$FIX/novac/fixtures/pos_probe.nv"
echo "x" > "$FIX/novac/fixtures/pos_twin.nv"
mksmoke 'case "$1" in */pos_twin.nv) [ "${2##*/}" = pos_probe.nv ] && exit 0;; esac; [ "$1" = "$2" ] && exit 0; exit 1'
check "близнец есть — смоук получает (близнец, фикстура), зелёный" "$(run)" "0"
mkoracle 'case "$2" in *pos_twin.nv) exit 0;; esac; exit 1'
printf 'novac/fixtures/pos_probe.nv\n' > "$FIX/novac/divergences.allow"
check "оракул отверг фикстуру (allow), близнеца принял — сверка идёт, зелёный" "$(run)" "0"
mksmoke 'exit 1'
check "то же, а ответ разошёлся с близнецом — красный" "$(run)" "1"
rm -f "$FIX/novac/divergences.allow"
mkoracle 'exit 0'
rm -f "$FIX/novac/fixtures/pos_twin.nv"
check "близнеца нет — красный" "$(run)" "1"
echo "x" > "$FIX/novac/fixtures/pos_probe.nv"

echo "== пул (№1717): фикстуры делятся между потоками, итог у КАЖДОЙ =="
# Семь фикстур на три потока: делёж по модулю оставляет потокам 3/2/2 строки,
# так что потерянный хвост любого потока виден числом. Поддельный novac
# отвергает pos_n4 (оба отвергли — исход совпал), смоук ругается на pos_n6.
for k in 1 2 3 4 5 6; do echo "x" > "$FIX/novac/fixtures/pos_n$k.nv"; done
mkoracle 'case "$2" in *pos_n4.nv) exit 1;; esac; exit 0'
mkbin 'case "$2" in *pos_n4.nv) exit 1;; esac; exit 0'
mksmoke 'exit 0'
NOVAC_POOL_JOBS=3 NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "семь фикстур на три потока — зелёный" "$?" "0"
grep -q 'ПОВЕДЕНИЕ: 6 из 7 байт-в-байт.*не судились 1.*потоков 3' "$TMP/pool.out"
check "сводка считает все семь: 6 совпали, 1 не судилась, потоков 3" "$?" "0"
grep -q 'pos_n4.nv: не судилась — novac отверг' "$TMP/pool.out"
check "несудимая фикстура названа поимённо с причиной" "$?" "0"
mksmoke 'case "$1" in *pos_n6.nv) echo "stdout: 1 vs 2"; exit 1;; esac; exit 0'
NOVAC_POOL_JOBS=3 NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "одна фикстура пула разошлась — красный" "$?" "1"
grep -q 'pos_n6.nv: поведение разошлось' "$TMP/pool.out"
check "красный называет ИМЕННО её" "$?" "0"
# Поток пула умирает (поддельный бинарь убивает родителя — поток): строки потока
# остаются без итога. Без ветки «итога нет» сводка взяла бы итог прошлой строки и
# была бы зелёной (охота guards 2026-10-05, находка 7). Этап 1 — через novac, этап 2 —
# через смоук.
mksmoke 'exit 0'
mkbin 'case "$2" in *pos_n2.nv) kill -9 $PPID;; esac; exit 0'
NOVAC_POOL_JOBS=3 NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "поток этапа 1 умер — красный" "$?" "1"
grep -q 'фикстур без исхода: .*пул потерял' "$TMP/pool.out" && grep -q 'pos_n2.nv: исхода нет' "$TMP/pool.out"
check "этап 1: красный назван «пул потерял», с адресом строки" "$?" "0"
mkbin 'case "$2" in *pos_n4.nv) exit 1;; esac; exit 0'
mksmoke 'case "$1" in *pos_n3.nv) kill -9 $PPID;; esac; exit 0'
NOVAC_POOL_JOBS=3 NOVAC_CORPUS=0 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "поток этапа 2 умер — красный" "$?" "1"
grep -q 'фикстур без итога поведения: .*пул потерял' "$TMP/pool.out" && grep -q 'pos_n3.nv: итога нет' "$TMP/pool.out"
check "этап 2: красный назван «пул потерял», а не «ОТВЕТ разный»" "$?" "0"
rm -f "$FIX"/novac/fixtures/pos_n*.nv

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
