#!/usr/bin/env bash
# Самотест scripts/tools/stale-processes.sh (поручение владельца 2026-09-23,
# пункт 5). Живой `ps` непредсказуем, поэтому вывод `ps -ef` подкладывается
# файлом через шов STALE_PS_FILE, «сейчас» — через STALE_NOW.
#
# Клетки, законное первым:
#   1. сегодняшний обычный процесс -> не назван, строка ok;
#   2. процесс, начатый не сегодня (STIME-дата) -> назван с PID;
#   3. `tail -f` старше часа -> назван, моложе часа -> нет;
#   4. строки-продолжения многострочной команды -> не процессы;
#   5. в инструменте нет ни kill, ни taskkill вне комментариев: перечень по построению.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TOOL="$ROOT/scripts/tools/stale-processes.sh"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
has()   { if printf '%s' "$1" | grep -q -- "$2"; then ok "$3"; else bad "$3 (нет '$2' в: $(printf '%s' "$1" | tr '\n' '|' | cut -c1-400))"; fi; }
hasnt() { if printf '%s' "$1" | grep -q -- "$2"; then bad "$3 (есть '$2')"; else ok "$3"; fi; }
HDR='     UID     PID    PPID  TTY        STIME COMMAND'
run() { OUT=$(STALE_PS_FILE="$T/ps" STALE_NOW="$1" bash "$TOOL" 2>&1); }

echo "== 1. сегодняшний обычный процесс =="
printf '%s\n' "$HDR" 'user    101       1 ?        12:00:00 /usr/bin/bash -c make' > "$T/ps"
run 15:00:00
has "$OUT" 'stale-processes ok:' "1: ничего не названо, строка ok"

echo "== 2. начат не сегодня =="
printf '%s\n' "$HDR" 'user   7838       1 ?          Sep 15 bash npx nuxt --port 1306' > "$T/ps"
run 15:00:00
has "$OUT" 'PID 7838 · с Sep 15 · начат не сегодня' "2: назван с PID и датой"
has "$OUT" 'перечислено 1, не снят НИ ОДИН' "2: счёт и «не снят» напечатаны"

echo "== 3. tail -f старше и моложе часа =="
printf '%s\n' "$HDR" \
    'user    201       1 ?        12:00:00 tail -f /tmp/gate_full.log' \
    'user    202       1 ?        14:50:00 tail -f /tmp/gate_novac.log' > "$T/ps"
run 15:00:00
has "$OUT" 'PID 201 · с 12:00:00 · слежение старше часа' "3: старше часа назван"
hasnt "$OUT" 'PID 202' "3: моложе часа не назван"

echo "== 4. строки-продолжения — не процессы =="
printf '%s\n' "$HDR" 'user    301       1 ?        14:59:00 /usr/bin/bash -c echo start' \
    '  and a continuation line of the same script' 'Sep 15 looks like a date but is text' \
    'echo at the moment 10:00:00 tail -f /tmp/leftover' > "$T/ps"
run 15:00:00
has "$OUT" 'stale-processes ok:' "4: продолжения не прочитаны процессами"
hasnt "$OUT" 'leftover' "4: продолжение со «временем» на месте STIME — не процесс"

echo "== 5. перечень по построению =="
if grep -vE '^[[:space:]]*#' "$TOOL" | grep -qE '(^|[^a-z])(task)?kill([^a-z]|$)'; then
    bad "5: в инструменте есть вызов kill/taskkill"
else
    ok "5: kill/taskkill вне комментариев нет"
fi

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-stale-processes ok: $PASS/$PASS"
    exit 0
fi
exit 1
