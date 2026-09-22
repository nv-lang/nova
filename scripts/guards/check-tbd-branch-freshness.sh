#!/usr/bin/env bash
# scripts/guards/check-tbd-branch-freshness.sh — новая строка `№TBD` в реестре
# 221.1 не коммитится, пока ветка отстаёт от `main` (владелец, 2026-09-22).
#
# ЗАЧЕМ. Номер строки реестра даёт ТОЛЬКО интегратор, и делает это ПРИ слиянии
# ветки — окно пишет `№TBD` до этого момента. Если после слияния окно
# продолжает коммитить на СТАРОЙ базе, не подтянув `main` к себе, следующая
# пачка снова несёт СТАРУЮ находку с НОВЫМ `№TBD` — та же строка, уже
# пронумерованная у интегратора, заново безымянная у автора. За ночь
# 2026-09-21/22 это случилось трижды подряд (реестр 221.1, слияния
# ungrammar-audit-0921 ×2, p274-novac ×2) — интегратор вручную сверял
# байт-в-байт, дубликат это или нет, прежде чем отбросить чужую копию.
#
# ВОРКТРИ ДЕЛЯТ ОДИН `.git` — почему страж вообще может это судить без fetch:
# `main` виден любой ветке того же репозитория без пуша и без fetch, значит
# «ветка отстаёт от main» — факт, проверяемый ЛОКАЛЬНО в момент коммита, а не
# что-то, ждущее сети.
#
# ЧТО ПРОВЕРЯЕТ: если СТЕЙДЖЕННЫЙ дифф `docs/plans/221.1-bug-sweep.md`
# ДОБАВЛЯЕТ строку вида `| №TBD |` (новую, не переносит существующую), и
# текущая ветка НЕ содержит всех коммитов `main` (т.е. `main` не является
# предком HEAD) — отказ. Исключение: сама ветка `main` (её ведёт интегратор,
# он и назначает номера, дожидаться нечего).
#
# ЧЕГО НЕ ПРОВЕРЯЕТ: не судит СУЩЕСТВУЮЩИЕ `№TBD`-строки, уже лежавшие в
# файле до этого коммита (иначе первый же коммит после заведения стража
# покраснел бы на всём унаследованном), не судит другие реестры/файлы.
#
# ОБХОД: `NOVA_TBD_ALLOW_STALE=1` — осознанно, причина печатается в вывод и
# должна попасть в доклад (тот же класс, что `NOVA_MERGE_URGENT`).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-tbd-branch-freshness.sh [КОРЕНЬ] [ИМЯ-ВЕТКИ-ГЛАВНОЙ]
#   bash scripts/guards/check-tbd-branch-freshness.sh --selftest

set -u
export LC_ALL=C
NAME=check-tbd-branch-freshness

SELFTEST=0
ROOT="."
MAIN_BRANCH=""
for a in "$@"; do
    case "$a" in
        --selftest) SELFTEST=1 ;;
        *) if [ -z "$ROOT" ] || [ "$ROOT" = "." ]; then ROOT="$a"; else MAIN_BRANCH="$a"; fi ;;
    esac
done
MAIN_BRANCH="${MAIN_BRANCH:-main}"

if [ "$SELFTEST" -eq 1 ]; then
    exec sh "$(dirname "$0")/selftest/test-check-tbd-branch-freshness.sh"
fi

ROOT="$(cd "$ROOT" 2>/dev/null && pwd)" || { echo "$NAME: FAIL — корень не найден" >&2; exit 1; }
cd "$ROOT" || exit 1

REG="docs/plans/221.1-bug-sweep.md"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "$NAME ok: не git-дерево, судить нечего"
    exit 0
fi

BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")"
if [ "$BRANCH" = "$MAIN_BRANCH" ]; then
    echo "$NAME ok: ветка $MAIN_BRANCH — номера здесь назначает интегратор, ждать нечего"
    exit 0
fi

if ! git rev-parse --verify "$MAIN_BRANCH" >/dev/null 2>&1; then
    echo "$NAME ok: ветка $MAIN_BRANCH не найдена в этом дереве — судить нечего"
    exit 0
fi

# Есть ли новая строка №TBD в СТЕЙДЖЕННОМ дифф реестра? Считаем только
# ДОБАВЛЕННЫЕ строки (+), не убранные и не контекст.
NEW_TBD=0
if git diff --cached --name-only 2>/dev/null | grep -qx "$REG"; then
    NEW_TBD=$(git diff --cached -- "$REG" 2>/dev/null | grep -cE '^\+\| ?№TBD ?\|')
fi

if [ "${NEW_TBD:-0}" -eq 0 ]; then
    echo "$NAME ok: новых строк №TBD в индексе нет"
    exit 0
fi

if [ -n "${NOVA_TBD_ALLOW_STALE:-}" ]; then
    echo "$NAME: обход NOVA_TBD_ALLOW_STALE='$NOVA_TBD_ALLOW_STALE' — коммит с новым №TBD разрешён осознанно"
    exit 0
fi

# main — предок HEAD? Если нет, ветка отстала.
if git merge-base --is-ancestor "$MAIN_BRANCH" HEAD 2>/dev/null; then
    echo "$NAME ok: ветка не отстаёт от $MAIN_BRANCH, новых №TBD строк $NEW_TBD — коммит легален"
    exit 0
fi

BEHIND="$(git rev-list --count "HEAD..$MAIN_BRANCH" 2>/dev/null || echo "?")"
echo "$NAME: FAIL — коммит добавляет новую строку реестра с №TBD ($NEW_TBD шт.), а ветка отстаёт от $MAIN_BRANCH на $BEHIND коммит(ов)." >&2
echo "  Номер, который тебе сейчас кажется свободным, интегратор мог уже присвоить" >&2
echo "  ЭТОЙ ЖЕ находке на $MAIN_BRANCH -- слей его к себе ПЕРЕД коммитом:" >&2
echo "    git merge $MAIN_BRANCH" >&2
echo "  Осознанно коммитить как есть: NOVA_TBD_ALLOW_STALE=<причина>." >&2
exit 1
