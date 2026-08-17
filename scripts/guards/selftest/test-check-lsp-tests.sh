#!/usr/bin/env bash
# Самотест check-lsp-tests.sh — обе стороны, на подставной команде.
#
# Страж, который не умеет краснеть, зелен ни о чём (класс F1, реестр №645).
# Здесь это особенно легко проглядеть: настоящий прогон идёт полминуты и почти
# всегда зелёный, так что «страж работает» проверить нечем — кроме подставы.
#
# NOVA_LSP_TESTS_CMD подменяет команду прогона; сам страж от этого не зависит.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-lsp-tests.sh"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

run() { NOVA_LSP_TESTS_CMD="$1" bash "$G" "$ROOT" >/dev/null 2>&1; echo $?; }

echo "== propuskaet =="
check "zelenyi nabor" \
  "$(run "echo 'test result: ok. 433 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'")" "0"
check "zelenyi s ignored" \
  "$(run "echo 'test result: ok. 433 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out'")" "0"
check "poslednyaya stroka reshaet" \
  "$(run "printf 'test result: FAILED. 1 passed; 1 failed\ntest result: ok. 433 passed; 0 failed\n'")" "0"

echo "== krasneet =="
check "odin upavshiy" \
  "$(run "echo 'test result: FAILED. 432 passed; 1 failed; 0 ignored'")" "1"
check "mnogo upavshih" \
  "$(run "echo 'test result: FAILED. 400 passed; 33 failed; 0 ignored'")" "1"
check "molchanie -- ne 'proshlo'" \
  "$(run "echo 'compiling...'")" "1"
check "pustoy vyvod" \
  "$(run "true")" "1"
check "nabor sjalsya do gorstki" \
  "$(run "echo 'test result: ok. 3 passed; 0 failed'")" "1"
check "nerazbornaya stroka verdikta" \
  "$(run "echo 'test result: ok. many passed; none failed'")" "1"
check "padenie komandy bez stroki" \
  "$(run "echo 'error: could not compile' ; exit 101")" "1"

echo "== realnost =="
# Страж обязан судить НАСТОЯЩИЙ набор, а не только подставу: если cargo есть,
# один честный прогон подтверждает, что разбор строки совпадает с тем, что
# cargo печатает на самом деле. Без этого самотест проверяет лишь себя.
if command -v cargo >/dev/null 2>&1; then
    bash "$G" "$ROOT" >/dev/null 2>&1
    check "nastoyashchiy nabor prohodit" "$?" "0"
else
    ok "cargo nedostupen -- realnyy progon propushchen"
fi

echo
echo "selftest check-lsp-tests: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
