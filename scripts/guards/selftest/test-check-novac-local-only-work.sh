#!/usr/bin/env bash
# Самотест check-novac-local-only-work.sh — обе стороны, на фикстурном дереве.
#
# ЦЕНТРАЛЬНЫЙ СЛУЧАЙ — ВЕТКА БЕЗ НЕВЛИТОГО НЕ СЧИТАЕТСЯ. Локальная ветка,
# указывающая внутрь main, ничего не хранит: её удаление не теряет ни строки, и
# требовать для неё пуша значило бы засорять origin мусором, а страж — терять
# доверие. Второй несущий: СУЖЕНИЕ тоже красное (запушили ветку — опусти базу),
# иначе следующий рост до прежней цифры пройдёт молча.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-local-only-work.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

R="$TMP/repo"; B="$TMP/base"
mkdir -p "$R"
git -C "$R" init -q 2>/dev/null || git init -q "$R"
git -C "$R" config user.email t@t
git -C "$R" config user.name t
git -C "$R" symbolic-ref HEAD refs/heads/main
printf 'x\n' > "$R/f.txt"
git -C "$R" add f.txt >/dev/null 2>&1
git -C "$R" commit -qm base >/dev/null 2>&1
printf '0\n' > "$B"

echo "== проходит =="
OUT=$(bash "$G" "$TMP/nowhere" "$B" 2>&1); RC=$?
check "не git-дерево — зелёный (судить нечего)" "$RC" "0"

OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "только main — зелёный" "$RC" "0"

# ветка БЕЗ невлитого: указывает внутрь main
git -C "$R" branch stale main >/dev/null 2>&1
OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "ветка без невлитой работы — зелёный (терять нечего)" "$RC" "0"
has "назвал число" "$OUT" "0"

echo "== краснеет =="
# ветка С невлитым и без двойника на origin
git -C "$R" checkout -q -b work main >/dev/null 2>&1
printf 'y\n' > "$R/g.txt"
git -C "$R" add g.txt >/dev/null 2>&1
git -C "$R" commit -qm work >/dev/null 2>&1
git -C "$R" checkout -q main >/dev/null 2>&1
OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "невлитая ветка без копии — красный" "$RC" "1"
has "назвал ветку" "$OUT" "work"
has "назвал оба лечения" "$OUT" "git push origin"

# двойник на origin появился — снова зелено
git -C "$R" update-ref refs/remotes/origin/work refs/heads/work
OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "копия на origin появилась — зелёный" "$RC" "0"

# база выше факта — тоже красный
printf '1\n' > "$B"
OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "сужение без опускания базы — красный" "$RC" "1"
has "сказал, что база выше факта" "$OUT" "МЕНЬШЕ"

echo "== №1750: копия под другим именем; дерево задачи судит свою ветку =="
printf '0\n' > "$B"
git -C "$R" update-ref -d refs/remotes/origin/work
# кандидат ушёл на origin как integrate/<задача> — копия есть, хоть имя другое
git -C "$R" update-ref refs/remotes/origin/integrate/t1 refs/heads/work
OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "копия под другим именем (integrate/*) — зелёный" "$RC" "0"
git -C "$R" update-ref -d refs/remotes/origin/integrate/t1

# связанное дерево: в главной копии ветка 'work' без копии (красный), а дерево
# задачи на своей ветке 'mine' судит только её
git -C "$R" branch mine main >/dev/null 2>&1
WT="$TMP/wt"
git -C "$R" worktree add -q "$WT" mine >/dev/null 2>&1
printf 'z\n' > "$WT/h.txt"
git -C "$WT" add h.txt >/dev/null 2>&1
git -C "$WT" commit -qm mine >/dev/null 2>&1
git -C "$R" update-ref refs/remotes/origin/mine refs/heads/mine
OUT=$(bash "$G" "$WT" "$B" 2>&1); RC=$?
check "дерево задачи: своя ветка скопирована, чужая 'work' не судится — зелёный" "$RC" "0"
has "назвал область суда" "$OUT" "ветка этого дерева"
OUT=$(bash "$G" "$R" "$B" 2>&1); RC=$?
check "главная копия по-прежнему судит все ветки ('work' без копии) — красный" "$RC" "1"
git -C "$R" update-ref -d refs/remotes/origin/mine
OUT=$(bash "$G" "$WT" "$B" 2>&1); RC=$?
check "дерево задачи: своя ветка без копии — красный" "$RC" "1"
git -C "$R" worktree remove --force "$WT" >/dev/null 2>&1

echo "самотест check-novac-local-only-work: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
