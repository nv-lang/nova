#!/usr/bin/env bash
# КОНТРОЛЬ к p_check-worktree-location_inside-repo: тот же мини-репозиторий,
# но дерево вынесено ЗА дозволенный корень. Страж обязан покраснеть — и
# краснеет. Значит зелёный в парной пробе не «страж сломан», а именно
# зазор между объявленным правилом и заданным вопросом.
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
trap 'git -C "$TMP/nv-lang/maincopy" worktree remove --force "$TMP/elsewhere/outside" >/dev/null 2>&1; rm -rf "$TMP"' 0

mkdir -p "$TMP/nv-lang/maincopy" "$TMP/elsewhere"
REPO="$TMP/nv-lang/maincopy"
git -C "$REPO" init -q -b main
echo x > "$REPO/f.txt"
git -C "$REPO" add f.txt
git -C "$REPO" -c user.name=probe -c user.email=probe@example.invalid commit -q -m seed

git -C "$REPO" worktree add -q -b side "$TMP/elsewhere/outside" >/dev/null 2>&1

echo "--- git worktree list --porcelain ---"
git -C "$REPO" worktree list --porcelain | sed -n 's|^worktree ||p'

printf 'worktrees=9\n' > "$TMP/wt.baseline"

echo "--- guard ---"
NOVA_WORKTREE_BASELINE="$TMP/wt.baseline" bash "$GUARD" "$REPO"
rc=$?
echo "RC=$rc"
echo "--- artefact check ---"
if [ -d "$TMP/elsewhere/outside" ]; then
    echo "target created: <tmp>/elsewhere/outside"
else
    echo "TARGET MISSING - probe invalid"
    rc=99
fi
exit "$rc"
