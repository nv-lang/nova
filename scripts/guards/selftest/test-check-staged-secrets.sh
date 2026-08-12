#!/usr/bin/env bash
# Самотест check-staged-secrets.sh.
#
# Фикстуры здесь ПО ПРИРОДЕ содержат подложенные ключи и токены — это их
# работа. Сам страж исключает каталог самотестов из периметра именно поэтому:
# детектор не должен ловить сам себя.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-staged-secrets.sh"
TMP="${TMPDIR:-/tmp}/selftest_secrets_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/scripts/guards" "$TMP/src"
    git -C "$TMP" init -q 2>/dev/null
    git -C "$TMP" config user.email t@t 2>/dev/null
    git -C "$TMP" config user.name t 2>/dev/null
    printf 'echo ok\n' > "$TMP/src/clean.sh"
    git -C "$TMP" add src/clean.sh 2>/dev/null
    : > "$TMP/scripts/guards/secrets-allowlist.baseline"
}
trap 'rm -rf "$TMP"' EXIT
export NOVA_SECRETS_ALLOWLIST=""

run_tree() { NOVA_SECRETS_ALLOWLIST="$TMP/scripts/guards/secrets-allowlist.baseline" bash "$G" --tree "$TMP" 2>&1; }

# 1. Чистое дерево — норма.
setup
out=$(run_tree); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистое дерево проходит"; else bad "ложный отказ: $out"; fi

# 2. Приватный ключ — отказ, и СОДЕРЖИМОЕ НЕ ПЕЧАТАЕТСЯ (печать утечки — тоже утечка).
setup
printf -- '-----BEGIN RSA PRIVATE KEY-----\nAAAA\n' > "$TMP/src/id.pem"
git -C "$TMP" add src/id.pem 2>/dev/null
out=$(run_tree); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "приватный ключ" && ! echo "$out" | grep -q "AAAA"; then
    ok "ловит приватный ключ и не печатает содержимое"
else
    bad "ключ не пойман или содержимое напечатано (rc=$rc)"
fi

# 3. Учётные данные в адресе — ровно форма нашего remote sourcecraft.
setup
printf 'url = "https://user:s3cr3t@example.com/repo.git"\n' > "$TMP/src/cfg.toml"
git -C "$TMP" add src/cfg.toml 2>/dev/null
out=$(run_tree); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "внутри URL" && ! echo "$out" | grep -q "s3cr3t"; then
    ok "ловит пароль внутри URL и не печатает его"
else
    bad "URL-учётка не поймана или напечатана (rc=$rc)"
fi

# 4. Путь в списке исключений не считается.
setup
printf -- '-----BEGIN RSA PRIVATE KEY-----\n' > "$TMP/src/id.pem"
git -C "$TMP" add src/id.pem 2>/dev/null
printf 'src/id.pem   # тестовый ключ примера, службу не защищает\n' > "$TMP/scripts/guards/secrets-allowlist.baseline"
out=$(run_tree); rc=$?
if [ "$rc" -eq 0 ]; then ok "путь из списка исключений пропускается"; else bad "исключение не сработало: $out"; fi

# 5. Строка списка БЕЗ причины — отказ: пропуск, выписанный молча.
setup
printf 'src/whatever\n' > "$TMP/scripts/guards/secrets-allowlist.baseline"
out=$(run_tree); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "без причины"; then ok "требует причину у строки исключения"; else bad "строка без причины прошла (rc=$rc): $out"; fi

# 6. Staged-режим: токен в ДОБАВЛЯЕМОЙ строке.
setup
printf 'token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123"\n' > "$TMP/src/t.toml"
git -C "$TMP" add src/t.toml 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "токен"; then ok "staged: ловит токен известной формы"; else bad "staged-токен не пойман (rc=$rc): $out"; fi

# 7. На настоящем дереве зелёный — иначе страж въезжает в гейт красным.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(NOVA_SECRETS_ALLOWLIST="$REAL/scripts/guards/secrets-allowlist.baseline" bash "$G" --tree "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 8. Страж назван на странице правил.
if grep -q "check-staged-secrets.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-staged-secrets: 8/8 ok"; exit 0; fi
echo "селфтест check-staged-secrets: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
