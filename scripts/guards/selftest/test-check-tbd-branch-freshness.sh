#!/usr/bin/env bash
# Самотест check-tbd-branch-freshness.sh — обе стороны, на фикстурном дереве.
#
# ЦЕНТРАЛЬНЫЙ СЛУЧАЙ — НОВЫЙ №TBD НА ОТСТАВШЕЙ ВЕТКЕ КРАСНЕЕТ, УЖЕ БЫВШИЙ В
# ФАЙЛЕ №TBD — НЕТ (страж судит ТОЛЬКО ДОБАВЛЕННЫЕ строки, иначе первый же
# коммит после заведения стража покраснел бы на унаследованном).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-tbd-branch-freshness.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

R="$TMP/repo"
mkdir -p "$R/docs/plans"
git -C "$R" init -q 2>/dev/null || git init -q "$R"
git -C "$R" config user.email t@t
git -C "$R" config user.name t
git -C "$R" symbolic-ref HEAD refs/heads/main

REG="docs/plans/221.1-bug-sweep.md"
printf '| 100 | K1 | old row |\n' > "$R/$REG"
printf 'x\n' > "$R/other.txt"
git -C "$R" add "$REG" other.txt >/dev/null 2>&1
git -C "$R" commit -qm base >/dev/null 2>&1

echo "== не git-дерево / ветка main / нет TBD =="
mkdir -p "$TMP/plain"
OUT=$(bash "$G" "$TMP/plain" 2>&1); RC=$?
check "не git-дерево — зелёный" "$RC" "0"

OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "на main, без нового TBD — зелёный" "$RC" "0"

git -C "$R" checkout -q -b work main >/dev/null 2>&1
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "на своей ветке, ничего не застейджено — зелёный" "$RC" "0"

echo "== существующий №TBD (не новый) не красит =="
printf '| 100 | K1 | old row |\n| №TBD | K2 | already there before this commit |\n' > "$R/$REG"
git -C "$R" add "$REG" >/dev/null 2>&1
git -C "$R" commit -qm "carries an old TBD, added on work before falling behind" >/dev/null 2>&1
# теперь эта строка уже В ИСТОРИИ этой ветки; следующий тест проверяет НОВУЮ.
# main продвигается ДРУГИМ файлом — регистр не трогает, слияние без конфликта.
git -C "$R" checkout -q main >/dev/null 2>&1
printf 'y\n' > "$R/other.txt"
git -C "$R" add other.txt >/dev/null 2>&1
git -C "$R" commit -qm "main advances, work is now behind" >/dev/null 2>&1
git -C "$R" checkout -q work >/dev/null 2>&1

echo "== новый №TBD на отставшей ветке — красный =="
printf '| 100 | K1 | old row |\n| №TBD | K2 | already there before this commit |\n| №TBD | K1 | brand new finding |\n' > "$R/$REG"
git -C "$R" add "$REG" >/dev/null 2>&1
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "новый TBD, ветка отстаёт от main — красный" "$RC" "1"
has "назвал причину" "$OUT" "отстаёт от main"
has "назвал команду" "$OUT" "git merge main"

echo "== обход NOVA_TBD_ALLOW_STALE =="
OUT=$(NOVA_TBD_ALLOW_STALE="known, filing anyway" bash "$G" "$R" 2>&1); RC=$?
check "обход осознан — зелёный" "$RC" "0"
has "назвал причину обхода" "$OUT" "known, filing anyway"

echo "== слияние main снимает красноту =="
git -C "$R" reset -q >/dev/null 2>&1
git -C "$R" merge -q --no-edit main >/dev/null 2>&1
MERGE_RC=$?
check "тестовое слияние прошло чисто (без конфликта)" "$MERGE_RC" "0"
printf '| 100 | K1 | old row |\n| №TBD | K2 | already there before this commit |\n| №TBD | K1 | brand new finding, after sync |\n' > "$R/$REG"
git -C "$R" add "$REG" >/dev/null 2>&1
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "после git merge main, новый TBD — зелёный" "$RC" "0"

echo
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
