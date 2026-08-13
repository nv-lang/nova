#!/usr/bin/env bash
# Самотест scripts/guards/check-gate-steps-assert.sh — обе стороны.
#
# Вторая половина здесь важнее первой: признак «литеральная пара слэш+n»
# в первой редакции дал пять ложных срабатываний на регулярках `[ \t]` внутри
# многострочных awk-программ. Ложняк на законной форме — то, из-за чего стража
# снимают, поэтому он проверяется отдельным случаем и остаётся навсегда.

set -u
export LC_ALL=C

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$SELF/scripts/guards/check-gate-steps-assert.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

# Собирает фикстуру дерева: $1 — тело gate.sh, $2 — тело стража check-probe.sh.
mkfix() {
    rm -rf "$TMP/fix"; mkdir -p "$TMP/fix/scripts/guards"
    printf '%s\n' "$1" > "$TMP/fix/scripts/gate.sh"
    printf '%s\n' "$2" > "$TMP/fix/scripts/guards/check-probe.sh"
}
run() { bash "$GUARD" "$TMP/fix" >/dev/null 2>&1; echo $?; }

GOOD_GUARD='#!/bin/sh
echo "check-probe ok: проверено"
exit 0'
MUTE_GUARD='#!/bin/sh
exit 0'

echo "== правило 2: вызов мимо обёртки =="
mkfix 'guard "$ROOT/scripts/guards/check-probe.sh" || fail "x"' "$GOOD_GUARD"
check "через guard — проходит" "$(run)" "0"

mkfix 'bash "$ROOT/scripts/guards/check-probe.sh" || fail "x"' "$GOOD_GUARD"
check "голым bash — падает" "$(run)" "1"

mkfix 'ENV_FLAG=0 bash "$ROOT/scripts/guards/check-probe.sh" || fail "x"' "$GOOD_GUARD"
check "голым bash с префиксом переменной — падает" "$(run)" "1"

echo "== правило 3: страж не способен предъявить строку =="
mkfix 'guard "$ROOT/scripts/guards/check-probe.sh" || fail "x"' "$MUTE_GUARD"
check "страж без 'ok:' в исходнике — падает" "$(run)" "1"

# Расширение периметра по указанию владельца 2026-08-14: правило накрывает ВЕСЬ
# набор стражей, а не только тех, кого зовёт гейт. Иначе страж, вызываемый
# хуком или CI, мог бы молча возвращать ноль.
mkfix 'echo "гейт этого стража не зовёт вовсе"' "$MUTE_GUARD"
check "немой страж вне гейта — тоже падает" "$(run)" "1"

echo "== правило 3: 'OK' без двоеточия за доказательство не считается =="
mkfix 'guard "$ROOT/scripts/guards/check-probe.sh" || fail "x"' '#!/bin/sh
echo "check-probe: OK"
exit 0'
check "'OK' без двоеточия — падает" "$(run)" "1"

echo "== исключение: check-ci-status законно ничего не проверяет =="
mkfix 'bash "$ROOT/scripts/guards/check-ci-status.sh" || true' "$GOOD_GUARD"
printf '%s\n' "$MUTE_GUARD" > "$TMP/fix/scripts/guards/check-ci-status.sh"
check "исключение проходит голым вызовом и без строки" "$(run)" "0"

echo "== правило 1: подмена переноса строки =="
mkfix 'guard "$ROOT/scripts/guards/check-probe.sh" "$ROOT" \n    || fail "x"' "$GOOD_GUARD"
check "литеральная пара вместо переноса — падает" "$(run)" "1"

echo "== правило 1: законные формы НЕ трогает (пять ложняков при заведении) =="
mkfix 'guard "$ROOT/scripts/guards/check-probe.sh" || fail "x"' '#!/bin/sh
printf "%s\n" "строка"
awk '"'"'
    /^[ \t]*x/ { sub(/[ \t]+$/, "", $0); print }
'"'"' /dev/null
echo "check-probe ok: проверено"'
check "printf %s\\n и awk [ \\t] — не находка" "$(run)" "0"

echo "== настоящее дерево =="
bash "$GUARD" "$SELF" >/dev/null 2>&1
check "дерево проекта зелёное" "$?" "0"

echo ""
echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
