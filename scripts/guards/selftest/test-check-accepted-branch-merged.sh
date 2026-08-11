#!/usr/bin/env bash
# Самотест check-accepted-branch-merged.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-accepted-branch-merged.sh"
TMP="${TMPDIR:-/tmp}/selftest_abm_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {  # содержимое строки реестра
    rm -rf "$TMP"
    mkdir -p "$TMP/docs/plans"
    git -C "$TMP" init -q 2>/dev/null
    git -C "$TMP" -c user.name=t -c user.email=t@t commit -q --allow-empty -m base 2>/dev/null
    git -C "$TMP" branch -M main 2>/dev/null
    printf '%s\n' "$1" > "$TMP/docs/plans/221.1-bug-sweep.md"
}
trap 'rm -rf "$TMP"' EXIT

# 1. Ветки, названной в приёмке, не существует — норма: её удалили после слияния.
setup '| 1 | К1 | ЗАКРЫТО 2026-08-11 (окно p-ghost42). |'
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "удалённая ветка не считается долгом"; else bad "ложный отказ на удалённой ветке: $out"; fi

# 2. Ветка существует и ВЛИТА — норма.
setup '| 1 | К1 | ЗАКРЫТО 2026-08-11 (окно p-merged7). |'
git -C "$TMP" branch p-merged7 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "влитая ветка проходит"; else bad "ложный отказ на влитой ветке: $out"; fi

# 3. Ветка существует и НЕ влита — отказ. Это и есть случай 2026-08-11:
#    отчёт принят, класс записи переписан, ветка осталась в стороне.
setup '| 1 | К1 | ЗАКРЫТО 2026-08-11 (окно p-forgot9). |'
git -C "$TMP" checkout -q -b p-forgot9 2>/dev/null
printf 'x\n' > "$TMP/f.txt"
git -C "$TMP" add f.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m work 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "p-forgot9"; then
    ok "ловит принятую, но не влитую ветку"
else
    bad "не поймал не влитую ветку (rc=$rc): $out"
fi

# 4. Строка БЕЗ признака приёмки ветку не требует: страж судит по тому, что
#    автор записи назвал сам, и не угадывает связи.
setup '| 1 | К1 | Статус: ОТКРЫТ. Работа идёт в p-inprogress3. |'
git -C "$TMP" checkout -q -b p-inprogress3 2>/dev/null
printf 'y\n' > "$TMP/g.txt"
git -C "$TMP" add g.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m wip 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "открытая работа не требует слияния"; else bad "ложный отказ на открытой работе: $out"; fi

# 5. Страж назван на странице правил.
RULES="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/docs/dev/rules-for-agents.md"
if grep -q "check-accepted-branch-merged.sh" "$RULES" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-accepted-branch-merged: 5/5 ok"; exit 0; fi
echo "селфтест check-accepted-branch-merged: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
