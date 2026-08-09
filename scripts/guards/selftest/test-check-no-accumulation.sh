#!/usr/bin/env bash
# Селфтест scripts/guards/check-no-accumulation.sh.
#
# Страж без селфтеста не работает — это в проекте проверено дважды
# (LC_ALL=C у грепа с не-ASCII, детекция по подстроке пути в measure.sh).
# Проверяем ОБА направления: ловит рост накопления И не краснеет на живых
# ветках, иначе страж станет шумом и его отключат.
#
# ВАЖНО: работаем в ВРЕМЕННОМ репозитории. Ни одна команда не трогает
# `git config` пользователя — авторство задаётся флагами `-c` и переменными
# окружения на конкретный вызов (общий .git репозитория Nova делится между
# worktree, правка конфига травит авторство всей репы).
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-no-accumulation.sh"
FAILED=0

ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

GC="git -C $TMP -c user.name=selftest -c user.email=selftest@example.com -c commit.gpgsign=false"

git init -q -b main "$TMP" 2>/dev/null || { echo "не удалось создать врем. репозиторий" >&2; exit 1; }
echo base > "$TMP/f.txt"
$GC add f.txt >/dev/null 2>&1
$GC commit -q -m base >/dev/null 2>&1

# Ветка «замершая»: коммит датирован далеко в прошлом.
$GC branch stale-branch >/dev/null 2>&1
echo old > "$TMP/old.txt"
$GC add old.txt >/dev/null 2>&1
GIT_AUTHOR_DATE='2020-01-01T00:00:00' GIT_COMMITTER_DATE='2020-01-01T00:00:00' \
  git -C "$TMP" -c user.name=selftest -c user.email=selftest@example.com \
      commit -q -m "старый коммит" >/dev/null 2>&1
# перенос коммита на ветку без checkout рабочего дерева main
$GC branch -f stale-branch HEAD >/dev/null 2>&1
$GC reset -q --hard HEAD~1 >/dev/null 2>&1

BASE_FILE="$TMP/acc.baseline"

# 1. База 0, одна замершая ветка → рост, обязан покраснеть.
echo 'stale_branches=0' > "$BASE_FILE"
out=$(NOVA_ACC_BASELINE="$BASE_FILE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'НАКОПЛЕНИЕ ВЫРОСЛО'; then
    ok "ловит рост накопления (код 1)"
else
    bad "НЕ поймал рост (код $rc): $out"
fi

# 2. Та же ветка названа в базе → зелено (храповик держит долг, не запрещает его).
echo 'stale_branches=1' > "$BASE_FILE"
out=$(NOVA_ACC_BASELINE="$BASE_FILE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'роста накопления нет'; then
    ok "не краснеет на долге в пределах базы (код 0)"
else
    bad "ложное срабатывание на долге в базе (код $rc): $out"
fi

# 3. СВЕЖАЯ ветка (сегодняшний коммит) НЕ считается накоплением — иначе страж
#    краснел бы на каждом рабочем окне и его бы отключили.
$GC branch fresh-branch >/dev/null 2>&1
echo new > "$TMP/new.txt"
$GC add new.txt >/dev/null 2>&1
$GC commit -q -m "свежий коммит" >/dev/null 2>&1
$GC branch -f fresh-branch HEAD >/dev/null 2>&1
$GC reset -q --hard HEAD~1 >/dev/null 2>&1
echo 'stale_branches=1' > "$BASE_FILE"
out=$(NOVA_ACC_BASELINE="$BASE_FILE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then
    ok "живая ветка не считается накоплением (код 0)"
else
    bad "свежая ветка ошибочно сочтена накоплением (код $rc): $out"
fi

# 4. Порог настраивается. Проверяем ОТНОШЕНИЕ, а не абсолютное число: при
#    пороге 0 замерших обязано быть не меньше, чем при пороге 14. Абсолютное
#    сравнение оказалось хрупким — в прогоне ИЗНУТРИ гейта подготовка временного
#    репозитория дала другое число веток, и тест падал не по своей теме.
#    Селфтест обязан проверять СВОЙСТВО стража, а не совпадение с числом,
#    зависящим от среды (тот же урок, что у measure.sh и check-invariant-discipline).
n14=$(NOVA_ACC_BASELINE="$BASE_FILE" NOVA_ACC_STALE_DAYS=14 bash "$G" "$TMP" 2>&1 \
      | grep -oE 'замерших >14дн: [0-9]+' | grep -oE '[0-9]+$' | head -1)
n0=$(NOVA_ACC_BASELINE="$BASE_FILE" NOVA_ACC_STALE_DAYS=0 bash "$G" "$TMP" 2>&1 \
     | grep -oE 'замерших >0дн: [0-9]+' | grep -oE '[0-9]+$' | head -1)
if [ -n "${n0:-}" ] && [ -n "${n14:-}" ] && [ "$n0" -ge "$n14" ]; then
    ok "порог NOVA_ACC_STALE_DAYS действует (0дн: $n0 >= 14дн: $n14)"
else
    bad "порог не действует (0дн: '${n0:-нет}', 14дн: '${n14:-нет}')"
fi

# 5. Влитая ветка не считается вовсе (нет коммитов вне main).
$GC branch merged-branch main >/dev/null 2>&1
echo 'stale_branches=1' > "$BASE_FILE"
out=$(NOVA_ACC_BASELINE="$BASE_FILE" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then
    ok "влитая ветка не считается (код 0)"
else
    bad "влитая ветка сочтена накоплением (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-no-accumulation: 5/5 ok"
    exit 0
fi
echo "селфтест check-no-accumulation: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
