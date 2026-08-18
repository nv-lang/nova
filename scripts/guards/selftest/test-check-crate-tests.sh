#!/usr/bin/env bash
# Самотест check-crate-tests.sh — обе стороны, на подставной команде.
#
# Страж, который не умеет краснеть, зелен ни о чём (класс F1, реестр №645).
# Здесь это особенно легко проглядеть: настоящий прогон идёт минуты и почти
# всегда зелёный, так что «страж работает» проверить нечем — кроме подставы.
#
# NOVA_CRATE_TESTS_CMD подменяет команду прогона; сам страж от этого не зависит.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-crate-tests.sh"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

run() { NOVA_CRATE_TESTS_CMD="$1" bash "$G" "$ROOT" >/dev/null 2>&1; echo $?; }

echo "== propuskaet =="
check "zelenyi nabor" \
  "$(run "echo 'test result: ok. 433 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'")" "0"
check "neskol'ko celey summiruyutsya" \
  "$(run "printf 'test result: ok. 200 passed; 0 failed\ntest result: ok. 233 passed; 0 failed\n'")" "0"
check "zelenyi s ignored" \
  "$(run "echo 'test result: ok. 433 passed; 0 failed; 2 ignored'")" "0"

echo "== krasneet =="
check "odin upavshiy sredi zelenyh" \
  "$(run "printf 'test result: ok. 400 passed; 0 failed\ntest result: FAILED. 33 passed; 1 failed\n'")" "1"
check "mnogo upavshih" \
  "$(run "echo 'test result: FAILED. 400 passed; 33 failed'")" "1"
check "molchanie -- ne 'proshlo'" \
  "$(run "echo 'compiling...'")" "1"
check "pustoy vyvod" \
  "$(run "true")" "1"
check "nabor sjalsya do gorstki" \
  "$(run "echo 'test result: ok. 3 passed; 0 failed'")" "1"
check "padenie komandy bez stroki verdikta" \
  "$(run "echo 'error: could not compile' ; exit 101")" "1"

echo "== granica minimuma =="
# Порог задан per-crate; 300 для nova-lsp — первый крейт в списке.
check "rovno na poroge -- propuskaetsya" \
  "$(run "echo 'test result: ok. 300 passed; 0 failed'")" "0"
check "na odin nizhe poroga -- krasneet" \
  "$(run "echo 'test result: ok. 299 passed; 0 failed'")" "1"

echo "== realnost =="
# Страж обязан судить НАСТОЯЩИЕ наборы, а не только подставу: без этого
# самотест проверяет лишь себя. Прогон долгий — потому и один.
if command -v cargo >/dev/null 2>&1; then
    bash "$G" "$ROOT" >/dev/null 2>&1
    check "nastoyashchie nabory prohodyat" "$?" "0"
else
    ok "cargo nedostupen -- realnyy progon propushchen"
fi

echo
echo "selftest check-crate-tests: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
