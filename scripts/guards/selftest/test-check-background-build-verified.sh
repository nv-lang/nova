#!/usr/bin/env bash
# Самотест check-background-build-verified.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-background-build-verified.sh"
TMP="${TMPDIR:-/tmp}/selftest_bbv_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() { rm -rf "$TMP"; mkdir -p "$TMP/scripts/tools"; }
trap 'rm -rf "$TMP"' EXIT

# 1. Синхронная сборка — норма: её код возврата ЕСТЬ код возврата работы.
setup
printf 'cargo build --release || fail "build"\n' > "$TMP/scripts/x.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "синхронная сборка проходит"; else bad "ложный отказ на синхронной сборке: $out"; fi

# 2. Фоновая через `&` — отказ. Это и есть №597: обёртка вернёт ноль, даже
#    если сборка умерла.
setup
printf 'cargo build --release > /tmp/log 2>&1 &\n' > "$TMP/scripts/x.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "x.sh"; then ok "ловит фоновую сборку через &"; else bad "не поймал фоновую сборку (rc=$rc): $out"; fi

# 3. Через `nohup` — тоже отказ.
setup
printf 'nohup cargo build --release > /tmp/log 2>&1\n' > "$TMP/scripts/x.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "ловит nohup-сборку"; else bad "не поймал nohup (rc=$rc): $out"; fi

# 4. Закомментированная строка не считается: страж судит код, а не пример
#    в комментарии — иначе он покраснеет на собственной документации.
setup
printf '# nohup cargo build --release &\necho ok\n' > "$TMP/scripts/x.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "комментарий не считается нарушением"; else bad "ложный отказ на комментарии: $out"; fi

# 5. Сам build-compiler.sh исключён — он и есть проверяющая обёртка.
setup
printf 'nohup cargo build --release &\n' > "$TMP/scripts/tools/build-compiler.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "build-compiler.sh исключён из проверки"; else bad "ложный отказ на самой обёртке: $out"; fi

# 6. Страж назван на странице правил.
RULES="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/docs/dev/rules-for-agents.md"
if grep -q "check-background-build-verified.sh" "$RULES" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-background-build-verified: 6/6 ok"; exit 0; fi
echo "селфтест check-background-build-verified: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
