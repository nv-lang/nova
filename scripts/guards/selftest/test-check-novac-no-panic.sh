#!/usr/bin/env bash
# Самотест check-novac-no-panic.sh — обе стороны, через поддельный novac.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-panic.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac/fixtures"
echo "x" > "$FIX/novac/fixtures/pos_probe.nv"

mkbin() { printf '#!/bin/sh\n%s\n' "$1" > "$TMP/bin.sh"; chmod +x "$TMP/bin.sh"; }
run() { sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1; echo $?; }

echo "== проходит =="
sh "$G" "$FIX" "$TMP/absent" >/dev/null 2>&1
check "без бинаря — зелёный" "$?" "0"
mkbin 'exit 0'
check "чистый выход 0" "$(run)" "0"
mkbin 'echo "error: something" >&2; exit 1'
check "обычный отказ (код 1) — не паника" "$(run)" "0"

echo "== ловит =="
mkbin 'exit 139'
check "код >=128 (сигнал) — красный" "$(run)" "1"
mkbin 'echo "thread panicked at ..." >&2; exit 1'
check "слово panic в stderr — красный" "$(run)" "1"

echo "== пул (№1717): итог у каждой фикстуры =="
# Семь фикстур на три потока (3/2/2 строки): паника одной в середине списка
# обязана быть названа поимённо, а чистый корпус — сосчитан целиком.
for k in 1 2 3 4 5 6; do echo "x" > "$FIX/novac/fixtures/pos_n$k.nv"; done
mkbin 'exit 0'
NOVAC_POOL_JOBS=3 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "семь фикстур на три потока — зелёный" "$?" "0"
grep -q 'фикстур 7,.*потоков 3' "$TMP/pool.out"
check "сосчитаны все семь, потоков 3" "$?" "0"
mkbin 'case "$2" in *pos_n5.nv) exit 134;; esac; exit 0'
NOVAC_POOL_JOBS=3 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "паника одной фикстуры пула — красный" "$?" "1"
grep -q 'pos_n5.nv: код возврата 134' "$TMP/pool.out"
check "красный называет ИМЕННО её" "$?" "0"
# Поток пула умирает на pos_n2 (поддельный novac убивает своего родителя — поток):
# его строки остаются без итога. Без ветки «итога нет» `read` взял бы код прошлой
# строки, и страж был бы зелёным (охота guards 2026-10-05, находка 7).
mkbin 'case "$2" in *pos_n2.nv) kill -9 $PPID;; esac; exit 0'
NOVAC_POOL_JOBS=3 sh "$G" "$FIX" "$TMP/bin.sh" > "$TMP/pool.out" 2>&1
check "поток пула умер — красный" "$?" "1"
grep -q 'фикстур без итога: .*пул потерял' "$TMP/pool.out" && grep -q 'pos_n2.nv: итога нет' "$TMP/pool.out"
check "красный назван «пул потерял», с адресом строки, а не паникой novac" "$?" "0"
rm -f "$FIX"/novac/fixtures/pos_n*.nv

echo "== кэш прогона гейта (№1717): итог того же бинаря и того же пути — и только он =="
# Кэш снят бинарём, который отвечал 0; потом тот же путь бинаря начинает паниковать.
# Страж, читающий кэш, зелёный — это доказывает, что кэш читается. Чужой путь в
# записи или другой бинарь — кэш не годится, страж зовёт novac сам и краснеет.
mkbin 'exit 0'
CC="$TMP/cc"
sh "$ROOT/scripts/tools/novac-check-cache.sh" "$FIX" "$CC" "$TMP/bin.sh" > "$TMP/cc.out" 2>&1
grep -q 'фикстур 1, записей 1' "$TMP/cc.out"
check "инструмент кэша записал итог на каждую фикстуру" "$?" "0"
mkbin 'exit 139'
NOVAC_CHECK_CACHE="$CC" sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1
check "итог из кэша того же бинаря читается (новый ответ бинаря не виден)" "$?" "0"
cp "$TMP/bin.sh" "$TMP/bin2.sh"
NOVAC_CHECK_CACHE="$CC" sh "$G" "$FIX" "$TMP/bin2.sh" >/dev/null 2>&1
check "кэш другого бинаря не годится — страж судит сам, красный" "$?" "1"
for p in "$CC"/*.path; do echo "/elsewhere/pos_probe.nv" > "$p"; done
NOVAC_CHECK_CACHE="$CC" sh "$G" "$FIX" "$TMP/bin.sh" >/dev/null 2>&1
check "запись с чужим путём (совпал только ключ) — промах, страж судит сам, красный" "$?" "1"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
