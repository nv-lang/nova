#!/usr/bin/env bash
# Самотест check-novac-smoke-wrapper.sh — обе стороны, на фикстурном
# инструменте.
#
# ЦЕНТРАЛЬНЫЙ СЛУЧАЙ — ОБЁРТКА, ТЕРЯЮЩАЯ ПРОБЕЛ. `for a in $@` без кавычек
# рвёт `/tmp/a b.c` на две строки, и argv оракула прочитается как два файла;
# на Linux это выглядит как «перехват не сработал», а не как «кавычка
# потерялась». Второй несущий: обёртка БЕЗ `__END__` — разбор argv нечем
# остановить, и в линк-команду уезжает хвост следующей сборки.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-smoke-wrapper.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

F="$TMP/tool.sh"

echo "== проходит =="
OUT=$(bash "$G" "$TMP" "$TMP/nowhere.sh" 2>&1); RC=$?
check "нет инструмента — зелёный (судить нечего)" "$RC" "0"

# правильная обёртка — та же форма, что в инструменте
{
  echo '#!/usr/bin/env bash'
  printf '        printf %s%s%s "$LOG" "$LOG" "$REAL_CLANG" > "$T/clang-log.sh"\n' \
    "'" '#!/bin/sh\nfor a in "$@"; do printf "%%s\\n" "$a" >> "%s"; done\nprintf "__END__\\n" >> "%s"\nexec "%s" "$@"\n' "'"
} > "$F"
OUT=$(bash "$G" "$TMP" "$F" 2>&1); RC=$?
check "правильная обёртка — зелёный" "$RC" "0"
has "назвал место строки" "$OUT" "tool.sh:2"

echo "== краснеет =="
# без POSIX-ветки вовсе — ровно состояние до 2026-08-23
printf '#!/usr/bin/env bash\n# only a .cmd branch here\n' > "$F"
OUT=$(bash "$G" "$TMP" "$F" 2>&1); RC=$?
check "POSIX-ветки нет — красный" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

# обёртка без кавычек вокруг "$@" — теряет аргумент с пробелом
{
  echo '#!/usr/bin/env bash'
  printf '        printf %s%s%s "$LOG" "$LOG" "$REAL_CLANG" > "$T/clang-log.sh"\n' \
    "'" '#!/bin/sh\nfor a in $@; do printf "%%s\\n" "$a" >> "%s"; done\nprintf "__END__\\n" >> "%s"\nexec "%s" "$@"\n' "'"
} > "$F"
OUT=$(bash "$G" "$TMP" "$F" 2>&1); RC=$?
check "обёртка теряет ПРОБЕЛ — красный" "$RC" "1"
has "назвал именно пробел" "$OUT" "ПРОБЕЛОМ"

# обёртка без __END__
{
  echo '#!/usr/bin/env bash'
  printf '        printf %s%s%s "$LOG" "$LOG" "$REAL_CLANG" > "$T/clang-log.sh"\n' \
    "'" '#!/bin/sh\nfor a in "$@"; do printf "%%s\\n" "$a" >> "%s"; done\n: "%s"\nexec "%s" "$@"\n' "'"
} > "$F"
OUT=$(bash "$G" "$TMP" "$F" 2>&1); RC=$?
check "обёртка без __END__ — красный" "$RC" "1"
has "назвал маркер" "$OUT" "__END__"

# обёртка, не зовущая настоящий clang
{
  echo '#!/usr/bin/env bash'
  printf '        printf %s%s%s "$LOG" "$LOG" "$REAL_CLANG" > "$T/clang-log.sh"\n' \
    "'" '#!/bin/sh\nfor a in "$@"; do printf "%%s\\n" "$a" >> "%s"; done\nprintf "__END__\\n" >> "%s"\n: "%s"\n' "'"
} > "$F"
OUT=$(bash "$G" "$TMP" "$F" 2>&1); RC=$?
check "обёртка не зовёт настоящий clang — красный" "$RC" "1"
has "назвал, сколько дошло" "$OUT" "из 5"

echo "самотест check-novac-smoke-wrapper: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
