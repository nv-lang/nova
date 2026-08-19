#!/usr/bin/env bash
# Самотест check-novac-time-ledger.py — обе стороны, на фикстурном корне.
# Покрывает оба правила стража (274 §1.4): покрытие дат коммитов строками
# леджера и арифметику «сумма долей за одну дату <= 1.0» (ревью 274.3/F4 —
# за 2026-08-14 сумма была 5.55 дня, метрика дня 30 от такого недействительна).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-time-ledger.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac" "$FIX/docs/dev"
L="$FIX/docs/dev/novac-time-ledger.md"

echo "== проходит =="
python "$G" "$TMP/empty-root" >/dev/null 2>&1
check "нет novac и нет леджера — зелёный" "$?" "0"

printf '| 2026-08-14 | 274 | 0.5 | demo |\n' > "$L"
NOVA_TL_DATES="2026-08-14" python "$G" "$FIX" >/dev/null 2>&1
check "дата коммита покрыта строкой — зелёный" "$?" "0"

NOVA_TL_DATES="2026-08-01" python "$G" "$FIX" >/dev/null 2>&1
check "дата ДО начала леджера — зелёный (не судится)" "$?" "0"

{ printf '| 2026-08-14 | 274 | ~0.5 | тильда — та же оценка |\n'
  printf '| 2026-08-14 | ревью | 0.3 | ещё строка |\n'
  printf '| 2026-08-14 | оракул-фикс | 0.2 | и ещё |\n'
  printf '| 2026-08-15 | 274 | 0.1 | другой день |\n'; } > "$L"
NOVA_TL_DATES="2026-08-14" python "$G" "$FIX" >/dev/null 2>&1
check "сумма ровно 1.0 (с тильдой) — зелёный, граница включительно" "$?" "0"

echo "== ловит =="
NOVA_TL_DATES="2026-08-16" python "$G" "$FIX" >/dev/null 2>&1
check "дата коммита без строки — красный" "$?" "1"

printf '| 2026-08-14 | 274 | ~1.0 | ядро |\n| 2026-08-14 | ревью | 0.5 | ревью |\n' > "$L"
NOVA_TL_DATES="2026-08-14" python "$G" "$FIX" >/dev/null 2>&1
check "две строки одной даты дают 1.5 — красный" "$?" "1"
OUT="$(NOVA_TL_DATES=2026-08-14 python "$G" "$FIX" 2>&1 >/dev/null)"
case "$OUT" in
  *"2026-08-14: сумма 1.50 при 2 строках"*) ok "красный называет дату, сумму и число строк" ;;
  *) bad "красный без даты/суммы/числа строк: $OUT" ;;
esac

printf '| 2026-08-14 | 274 | пол-дня | доля не число |\n' > "$L"
NOVA_TL_DATES="2026-08-14" python "$G" "$FIX" >/dev/null 2>&1
check "доля не число — красный" "$?" "1"

rm "$L"
NOVA_TL_DATES="2026-08-14" python "$G" "$FIX" >/dev/null 2>&1
check "novac есть, леджера нет — красный" "$?" "1"

printf 'нет строк с датами\n' > "$L"
NOVA_TL_DATES="2026-08-14" python "$G" "$FIX" >/dev/null 2>&1
check "леджер без единой даты — красный" "$?" "1"

# Ранний выход «судить нечего» не должен глушить арифметику: леджер есть —
# значит он судится, даже если каталога novac ещё нет.
NOV="$TMP/nonovac"; mkdir -p "$NOV/docs/dev"
printf '| 2026-08-14 | 274 | 0.8 | a |\n| 2026-08-14 | 274 | 0.8 | b |\n' \
    > "$NOV/docs/dev/novac-time-ledger.md"
python "$G" "$NOV" >/dev/null 2>&1
check "леджер без novac/ — сумма всё равно судится, красный" "$?" "1"

echo "== настоящее дерево =="
python "$G" "$ROOT" >/dev/null 2>&1
check "даты коммитов novac покрыты и суммы долей <= 1.0" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
