#!/usr/bin/env bash
# Самотест check-novac-time-ledger.sh — обе стороны, на фикстурном корне.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-time-ledger.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac" "$FIX/docs/dev"
L="$FIX/docs/dev/novac-time-ledger.md"

echo "== проходит =="
sh "$G" "$TMP/empty-root" >/dev/null 2>&1
check "нет novac — зелёный" "$?" "0"

printf '| 2026-08-14 | 274 | 0.5 | demo |\n' > "$L"
NOVA_TL_DATES="2026-08-14" sh "$G" "$FIX" >/dev/null 2>&1
check "дата коммита покрыта строкой — зелёный" "$?" "0"

NOVA_TL_DATES="2026-08-01" sh "$G" "$FIX" >/dev/null 2>&1
check "дата ДО начала леджера — зелёный (не судится)" "$?" "0"

echo "== ловит =="
NOVA_TL_DATES="2026-08-15" sh "$G" "$FIX" >/dev/null 2>&1
check "дата коммита без строки — красный" "$?" "1"

rm "$L"
NOVA_TL_DATES="2026-08-14" sh "$G" "$FIX" >/dev/null 2>&1
check "novac есть, леджера нет — красный" "$?" "1"

printf 'нет строк с датами\n' > "$L"
NOVA_TL_DATES="2026-08-14" sh "$G" "$FIX" >/dev/null 2>&1
check "леджер без единой даты — красный" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "даты коммитов novac проекта покрыты" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
