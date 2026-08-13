#!/usr/bin/env bash
# Самотест check-tree-matches-push.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-tree-matches-push.sh"
TMP="${TMPDIR:-/tmp}/selftest_dirtypush_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# Игрушечная репа с одним коммитом. Авторство задаём ЛОКАЛЬНО для фикстуры —
# `git config` настоящей репы не трогаем ни при каких условиях.
setup() {
    rm -rf "$TMP"; mkdir -p "$TMP"
    git -C "$TMP" init -q 2>/dev/null
    git -C "$TMP" config user.email "selftest@example.invalid"
    git -C "$TMP" config user.name "selftest"
    printf 'one\n' > "$TMP/a.txt"
    git -C "$TMP" add a.txt
    git -C "$TMP" commit -qm "init" 2>/dev/null
}
trap 'rm -rf "$TMP"' EXIT

# 1. Чистое дерево — норма.
setup
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистое дерево проходит"; else bad "ложный отказ: $out"; fi

# 2. Изменённый отслеживаемый файл — отказ. Ровно случай №635.
setup
printf 'two\n' >> "$TMP/a.txt"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "a.txt"; then ok "ловит изменённый файл и НАЗЫВАЕТ его"; else bad "не поймал (rc=$rc): $out"; fi

# 3. Изменение В ИНДЕКСЕ, но не закоммиченное — тоже отказ: `git add` без
#    `commit` даёт ровно ту же ложь.
setup
printf 'two\n' >> "$TMP/a.txt"
git -C "$TMP" add a.txt
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "ловит застрявшее в индексе"; else bad "индекс прошёл: $out"; fi

# 4. Удалённый отслеживаемый файл — отказ.
setup
rm "$TMP/a.txt"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "ловит удалённый файл"; else bad "удаление прошло: $out"; fi

# 5. Неотслеживаемый мусор — НЕ отказ. Черновики в дереве обычное дело, и
#    запрет на них учил бы обходить стража целиком.
setup
printf 'scratch\n' > "$TMP/notes.tmp"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "неотслеживаемое не мешает"; else bad "ложняк на мусоре: $out"; fi

# 6. Осознанный обход работает и говорит о себе вслух.
setup
printf 'two\n' >> "$TMP/a.txt"
out=$(NOVA_ALLOW_DIRTY_PUSH=1 bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "NOVA_ALLOW_DIRTY_PUSH"; then ok "обход работает и называет себя"; else bad "обход не сработал (rc=$rc): $out"; fi

# 7. Не-git каталог не роняет хук.
rm -rf "$TMP"; mkdir -p "$TMP"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "не-git каталог проходит"; else bad "упал на не-git: $out"; fi

# 8. Страж назван на странице правил.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
if grep -q "check-tree-matches-push.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

# 9. Хук pre-push его зовёт — иначе страж существует, но не срабатывает.
if grep -q "check-tree-matches-push.sh" "$REAL/scripts/githooks/pre-push" 2>/dev/null; then
    ok "pre-push зовёт стража"
else
    bad "pre-push не зовёт check-tree-matches-push.sh"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-tree-matches-push: все проверки ok"; exit 0; fi
echo "селфтест check-tree-matches-push: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
