#!/usr/bin/env bash
# Проба к находке 4: check-worktree-location.sh обещает «никогда ВНУТРИ
# репозитория», а меряет соседний признак — «путь лежит под РОДИТЕЛЕМ главной
# копии». Всё, что внутри главной копии, лежит под её родителем тоже, поэтому
# ровно тот случай, ради которого страж заведён (worktree в
# `.claude/worktrees/**`, замер 2026-08-10), проходит ЗЕЛЁНЫМ.
#
# Запуск:  bash cmd.sh <КОРЕНЬ-РЕПОЗИТОРИЯ>
set -u
REPO="${1:?ukazhi koren repozitoriya nova pervym argumentom}"
GUARD="$REPO/scripts/guards/check-worktree-location.sh"
[ -f "$GUARD" ] || { echo "net $GUARD" >&2; exit 2; }

HERE="$(cd "$(dirname "$0")" && pwd)"
B="$HERE/base"; rm -rf "$B"; mkdir -p "$B/main"
git init -q "$B/main"
git -C "$B/main" config user.email probe@example.com
git -C "$B/main" config user.name probe
echo hi > "$B/main/readme"
git -C "$B/main" add readme
git -C "$B/main" commit -q -m init

echo "=== A. kontrol: derevo RYADOM s repozitoriem -> zelenyy"
git -C "$B/main" worktree add -q -b beside "$B/beside" >/dev/null 2>&1
bash "$GUARD" "$B/main"; echo "rc=$?"

echo
echo "=== B. PREDMET NARUSHEN: derevo VNUTRI repozitoriya"
echo "    (tot samyy .claude/worktrees/**, iz-za kotorogo strazh i zaveden)"
git -C "$B/main" worktree add -q -b inside "$B/main/.claude/worktrees/inside" >/dev/null 2>&1
echo "-- git worktree list:"
git -C "$B/main" worktree list --porcelain | sed -n 's|^worktree |   |p'
echo "-- verdikt strazha:"
bash "$GUARD" "$B/main"; echo "rc=$?"

echo
echo "=== C. i srazu sledstvie, radi kotorogo pravilo napisano:"
echo "   grep po derevu vidit chuzhoy snimok kak svoy fayl:"
find "$B/main" -name readme | sed "s|^$B/||" | sed 's/^/   /'
exit 0
