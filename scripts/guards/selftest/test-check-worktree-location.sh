#!/usr/bin/env bash
# Селфтест scripts/guards/check-worktree-location.sh.
#
# Обе стороны: ловит дерево вне дозволенной папки `worktrees/` и НЕ краснит на
# дереве внутри неё. Второе не менее важно — страж, краснящий на правильном,
# будет отключён в первый же день.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-worktree-location.sh"
FAILED=0
OKN=0
ok()  { OKN=$((OKN + 1)); echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
# Храповик числа деревьев здесь не предмет: база настоящего репозитория к
# временному отношения не имеет.
export NOVA_WORKTREE_BASELINE="$TMP/no-baseline"
# Окружение запускающего не должно решать за самотест (у владельца ярлык
# выставляет NOVA_WORKTREE_ROOT; случай 7 задаёт его сам).
unset NOVA_WORKTREE_DIR NOVA_WORKTREE_ROOT

# Настоящий маленький репозиторий с настоящими worktree — иначе проверяется не то.
# Раскладка как у nv-lang: $TMP/root — папка репозиториев, repo — главная копия.
REPO="$TMP/root/repo"
mkdir -p "$REPO"
(
  cd "$REPO" || exit 1
  git init -q .
  git -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
) >/dev/null 2>&1

# Папку берём в ТОЙ ЖЕ форме, в какой пути отдаёт сам git: под MSYS `$TMP`
# выглядит как `/tmp/…`, а `git worktree list` печатает `C:/Users/…/Temp/…`.
PARENT_GIT=$(git -C "$REPO" rev-parse --show-toplevel 2>/dev/null)
PARENT_GIT="${PARENT_GIT%/repo}"

run() { out=$(bash "$G" "$REPO" 2>&1); rc=$?; }
add() { git -C "$REPO" branch -q "$1" 2>/dev/null; git -C "$REPO" worktree add -q "$2" "$1" >/dev/null 2>&1; }
del() { git -C "$REPO" worktree remove --force "$1" >/dev/null 2>&1; }

# 1. Одна главная копия, папки `worktrees/` нет — зелено. Это случай раннера
#    CI (2026-08-23 страж краснел на его единственном чекауте).
run
if [ "$rc" -eq 0 ]; then ok "одна главная копия — судить нечего (случай CI)"; else bad "покраснел на одной главной копии (код $rc): $out"; fi

# 2. Дерево В КОРНЕ рядом с репозиторием (прежнее дозволенное место), папки
#    `worktrees/` ещё нет — красно. Отсутствие папки не выключает правило.
add wt2 "$TMP/root/wt2"
run
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt2"; then ok "дерево в корне рядом с репозиторием ловится и без папки worktrees/"; else bad "не поймал дерево в корне (код $rc): $out"; fi
del "$TMP/root/wt2"

# 3. Дерево в `worktrees/` (папка выведена от главной копии) — зелено.
add wt3 "$TMP/root/worktrees/wt3"
run
if [ "$rc" -eq 0 ]; then ok "дерево в worktrees/ проходит"; else bad "ложный отказ на worktrees/: $out"; fi

# 4. Рядом с законным — дерево в корне: красно, в выводе ровно нарушитель.
add wt4 "$TMP/root/wt4"
run
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt4" && ! echo "$out" | grep -q "wt3"; then
    ok "ловит дерево в корне рядом с законным и называет только его"
else
    bad "не так судит смесь (код $rc): $out"
fi

# 5. После снятия нарушителя — снова зелено (страж не «залипает»).
del "$TMP/root/wt4"
run
if [ "$rc" -eq 0 ]; then ok "после снятия нарушителя снова зелено"; else bad "остался красным (код $rc): $out"; fi

# 6. Дерево ВНУТРИ репозитория (так кладёт изоляция Agent-инструмента) — красно.
add wt6 "$REPO/.claude/worktrees/wt6"
run
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt6"; then ok "ловит дерево внутри репозитория"; else bad "не поймал дерево внутри репозитория (код $rc): $out"; fi
del "$REPO/.claude/worktrees/wt6"

# 7. NOVA_WORKTREE_ROOT больше НЕ ЧИТАЕТСЯ: выставлен в каталог над всем
#    (как в ярлыке владельца) — дерево в корне всё равно красное.
add wt7 "$TMP/root/wt7"
out=$(NOVA_WORKTREE_ROOT="$PARENT_GIT" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt7"; then ok "прежний NOVA_WORKTREE_ROOT ничего не разрешает"; else bad "NOVA_WORKTREE_ROOT открыл корень (код $rc): $out"; fi
del "$TMP/root/wt7"

# 8. NOVA_WORKTREE_DIR переопределяет папку — в обе стороны: чужая папка
#    делает нарушителями и wt8, и законное по умолчанию wt3; своя — пропускает.
add wt8 "$TMP/other/wt8"
out=$(NOVA_WORKTREE_DIR="$PARENT_GIT/other-unused" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt8" && echo "$out" | grep -q "wt3"; then ok "чужая папка в NOVA_WORKTREE_DIR судит оба дерева"; else bad "override не кусается (код $rc): $out"; fi
OTHER_GIT="${PARENT_GIT%/root}/other"
del "$TMP/root/worktrees/wt3"
out=$(NOVA_WORKTREE_DIR="$OTHER_GIT" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "дерево в папке NOVA_WORKTREE_DIR проходит"; else bad "ложный отказ на override (код $rc): $out"; fi
del "$TMP/other/wt8"

# 9. Не-git каталог — зелено, а не падение.
mkdir -p "$TMP/plain"
out=$(bash "$G" "$TMP/plain" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "не-git каталог не краснит"; else bad "упал на не-git каталоге (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-worktree-location: $OKN/$OKN ok"; exit 0; fi
echo "селфтест check-worktree-location: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
