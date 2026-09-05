#!/usr/bin/env bash
# Проба к находке: страж check-worktree-location объявляет в шапке
# «Рабочие деревья ... живут ТОЛЬКО РЯДОМ С РЕПОЗИТОРИЕМ — под тем же
# каталогом, что и главная рабочая копия, и никогда внутри неё»,
# а спрашивает у дерева ровно одно: «путь начинается с РОДИТЕЛЯ главной
# копии». Дерево ВНУТРИ репозитория этому префиксу удовлетворяет.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/check-worktree-location.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/check-worktree-location.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'git -C "$TMP/nv-lang/maincopy" worktree remove --force "$TMP/nv-lang/maincopy/.claude/worktrees/inside" >/dev/null 2>&1; rm -rf "$TMP"' 0

# «Рядом с репозиторием» = каталог-родитель главной рабочей копии.
mkdir -p "$TMP/nv-lang/maincopy"
REPO="$TMP/nv-lang/maincopy"
git -C "$REPO" init -q -b main
echo x > "$REPO/f.txt"
git -C "$REPO" add f.txt
git -C "$REPO" -c user.name=probe -c user.email=probe@example.invalid commit -q -m seed

# МИШЕНЬ: рабочее дерево ВНУТРИ репозитория — ровно .claude/worktrees/**,
# случай, названный во второй «измеренной причине» шапки стража.
git -C "$REPO" worktree add -q -b side "$REPO/.claude/worktrees/inside" >/dev/null 2>&1

echo "--- git worktree list --porcelain ---"
git -C "$REPO" worktree list --porcelain | sed -n 's|^worktree ||p'

# Своя база счёта деревьев: второй храповик стража не должен путаться под ногами.
printf 'worktrees=9\n' > "$TMP/wt.baseline"

echo "--- guard ---"
NOVA_WORKTREE_BASELINE="$TMP/wt.baseline" bash "$GUARD" "$REPO"
rc=$?
echo "RC=$rc"
echo "--- artefact check ---"
if [ -d "$REPO/.claude/worktrees/inside" ]; then
    echo "target created: <repo>/.claude/worktrees/inside"
else
    echo "TARGET MISSING - probe invalid"
    rc=99
fi
exit "$rc"
