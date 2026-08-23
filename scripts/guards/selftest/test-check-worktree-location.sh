#!/usr/bin/env bash
# Селфтест scripts/guards/check-worktree-location.sh.
#
# Обе стороны: ловит дерево вне дозволенного корня и НЕ краснит на дереве
# внутри него. Второе не менее важно — страж, краснящий на правильном, будет
# отключён в первый же день.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-worktree-location.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Настоящий маленький репозиторий с настоящим worktree — иначе проверяется не то.
REPO="$TMP/allowed/repo"
mkdir -p "$REPO"
(
  cd "$REPO" || exit 1
  git init -q .
  git -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
  git branch -q wt1
) >/dev/null 2>&1

# Корень берём в ТОЙ ЖЕ форме, в какой пути отдаёт сам git: под MSYS `$TMP`
# выглядит как `/tmp/…`, а `git worktree list` печатает `C:/Users/…/Temp/…`.
# Сравнивать эти две записи одного каталога — значит проверять не то.
ALLOWED_GIT=$(git -C "$REPO" rev-parse --show-toplevel 2>/dev/null)
ALLOWED_GIT="${ALLOWED_GIT%/repo}"

# 1. Worktree ВНУТРИ дозволенного корня — зелено.
git -C "$REPO" worktree add -q "$TMP/allowed/wt1" wt1 >/dev/null 2>&1
out=$(NOVA_WORKTREE_ROOT="$ALLOWED_GIT" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "дерево в дозволенном корне проходит"; else bad "ложный отказ: $out"; fi

# 2. Worktree ВНЕ корня — красно, с его путём в выводе.
git -C "$REPO" branch -q wt2 2>/dev/null
git -C "$REPO" worktree add -q "$TMP/elsewhere/wt2" wt2 >/dev/null 2>&1
out=$(NOVA_WORKTREE_ROOT="$ALLOWED_GIT" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt2"; then ok "ловит дерево вне корня"; else bad "не поймал дерево вне корня (код $rc): $out"; fi

# 3. После снятия нарушителя — снова зелено (страж не «залипает»).
git -C "$REPO" worktree remove --force "$TMP/elsewhere/wt2" >/dev/null 2>&1
out=$(NOVA_WORKTREE_ROOT="$ALLOWED_GIT" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "после снятия нарушителя снова зелено"; else bad "остался красным (код $rc): $out"; fi

# 4. Не-git каталог — зелено, а не падение.
mkdir -p "$TMP/plain"
out=$(bash "$G" "$TMP/plain" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "не-git каталог не краснит"; else bad "упал на не-git каталоге (код $rc): $out"; fi

# 5. ДОЗВОЛЕННОГО КОРНЯ НА ЭТОЙ МАШИНЕ НЕТ — правило не про неё, зелено.
#    Именно этот случай у раннера CI, и именно на нём страж краснел
#    2026-08-23: единственный чекаут `/home/runner/work/nova/nova` был объявлен
#    деревом вне корня. Без этого случая новая ветка не судима — класс №519.
#    Проверяем обе стороны: при НЕСУЩЕСТВУЮЩЕМ корне зелено ДАЖЕ при
#    живом нарушителе — иначе ветка не та, что нужна; а при СУЩЕСТВУЮЩЕМ
#    тот же нарушитель обязан краснить (случай 2 выше это уже доказал).
git -C "$REPO" branch -q wt5 2>/dev/null
git -C "$REPO" worktree add -q "$TMP/elsewhere/wt5" wt5 >/dev/null 2>&1
out=$(NOVA_WORKTREE_ROOT="$TMP/no-such-root-here" bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "отсутствующий корень — правило не про эту машину (случай CI)"; else bad "покраснел там, где корня нет (код $rc): $out"; fi
git -C "$REPO" worktree remove --force "$TMP/elsewhere/wt5" >/dev/null 2>&1

# 6. БЕЗ NOVA_WORKTREE_ROOT корень ВЫВОДИТСЯ (родитель главной копии) — и
#    выведенный обязан кусаться так же, как заданный. Случаи 1–5 задают корень
#    переменной, то есть дефолт ими не проверен вовсе; с 2026-08-23 дефолт
#    больше не литерал, и без этого случая его подмена прошла бы незамеченной.
git -C "$REPO" branch -q wt6 2>/dev/null
git -C "$REPO" worktree add -q "$TMP/elsewhere/wt6" wt6 >/dev/null 2>&1
out=$(bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "wt6"; then
    ok "выведенный корень ловит дерево вне себя"
else
    bad "выведенный корень не поймал нарушителя (код $rc): $out"
fi
git -C "$REPO" worktree remove --force "$TMP/elsewhere/wt6" >/dev/null 2>&1
out=$(bash "$G" "$REPO" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "выведенный корень не ложнит на своём дереве"; else bad "ложняк на выведенном корне: $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-worktree-location: 7/7 ok"; exit 0; fi
echo "селфтест check-worktree-location: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
