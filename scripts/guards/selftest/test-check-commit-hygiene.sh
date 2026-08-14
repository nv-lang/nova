#!/usr/bin/env bash
# Селфтест scripts/guards/check-commit-hygiene.sh.
#
# Оба направления. Второе важнее: хук, срывающий законный коммит, будет снесён в
# первый же день — и вместе с ним четыре правила, которые он держит.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-commit-hygiene.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
GC="git -C $TMP -c user.name=selftest -c user.email=selftest@example.com"
git init -q -b main "$TMP" 2>/dev/null || { echo "нет врем. репозитория" >&2; exit 1; }
git -C "$TMP" config user.name selftest
git -C "$TMP" config user.email selftest@example.com

MSG="$TMP/msg.txt"
# Фикстура «чистого» — АНГЛИЙСКАЯ: норма 2026-08-09 (сообщения по-английски)
# теперь проверяется этим же стражем, и прежняя русская фикстура сама была
# нарушением — самотест узнал об этом первым (реестр №650).
echo "ordinary message" > "$MSG"
echo base > "$TMP/f.txt"; $GC add f.txt >/dev/null 2>&1

E="NOVA_COMMIT_EMAIL=selftest@example.com"

# 1. Чистый случай — проходит. Это направление проверяем ПЕРВЫМ: страж, который
#    не пропускает законное, бесполезен независимо от того, что он ловит.
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистый коммит проходит"; else bad "ложный отказ на чистом (код $rc): $out"; fi

# 2. Маркер конфликта в ДОБАВЛЯЕМОЙ строке.
printf 'a\n<<<<<<< HEAD\nb\n' > "$TMP/c.txt"; $GC add c.txt >/dev/null 2>&1
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'МАРКЕРЫ КОНФЛИКТА'; then ok "ловит маркер конфликта"; else bad "не поймал маркер (код $rc): $out"; fi
$GC rm -q --cached c.txt >/dev/null 2>&1; rm -f "$TMP/c.txt"

# 3. Co-Authored-By в сообщении.
printf 'message\n\nCo-Authored-By: Someone <s@example.com>\n' > "$MSG"
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'Co-Authored-By'; then ok "ловит Co-Authored-By"; else bad "не поймал Co-Authored-By (код $rc): $out"; fi
echo "ordinary message" > "$MSG"

# 4. Чужое авторство.
out=$(NOVA_COMMIT_EMAIL=expected@nv-lang.org bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'user.email'; then ok "ловит чужое авторство"; else bad "не поймал авторство (код $rc): $out"; fi

# 5. Существующая строка-маркер, которая НЕ добавляется этим коммитом, не краснит.
#    Так живут селфтесты самих стражей — они держат такие строки как ДАННЫЕ.
printf 'x\n<<<<<<< HEAD\ny\n' > "$TMP/old.txt"; $GC add old.txt >/dev/null 2>&1
$GC -c core.hooksPath=/dev/null commit -q -m "старое" >/dev/null 2>&1
echo z >> "$TMP/f.txt"; $GC add f.txt >/dev/null 2>&1
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "существующий маркер вне индекса не краснит"; else bad "ложный отказ на старом маркере (код $rc): $out"; fi

# 6. Не-ASCII в сообщении — отказ (правило пришло в хук по №649: раньше язык
#    проверял только гейт — постфактум и дорого).
printf 'plans(274): %s.0 blocker list\n' "$(printf '\xd0\xa4')" > "$MSG"
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "ASCII"; then ok "ловит не-ASCII в сообщении"; else bad "не поймал не-ASCII (код $rc): $out"; fi

# 7. Кириллица в строке-цитате (>) — законна, не краснит.
printf 'quote kept\n\n> %s\n' "$(printf '\xd1\x86\xd0\xb8\xd1\x82\xd0\xb0\xd1\x82\xd0\xb0')" > "$MSG"
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$G" "$MSG" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "цитата строкой с > не краснит"; else bad "ложный отказ на цитате (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-commit-hygiene: 7/7 ok"; exit 0; fi
echo "селфтест check-commit-hygiene: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
