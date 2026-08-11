#!/usr/bin/env bash
# scripts/tools/push-docs.sh
# Отправить на зеркала ТОЛЬКО документные коммиты — те, что не трогают код.
#
# ОСНОВАНИЕ: решение владельца 2026-08-11. Правило «пушить после зелёного
# гейта» остаётся для КОДА и не смягчается. Но гейт идёт сорок минут, а
# коммиты в планы, спеку и реестр он по существу не проверяет — и держать их
# в столе значит терять при любой аварии дерева. За один день так набралось
# двадцать два коммита, из которых код трогали три.
#
# ЧТО ДЕЛАЕТ. Смотрит непушенные коммиты В ПОРЯДКЕ ОТ СТАРЫХ К НОВЫМ и находит
# самый длинный НАЧАЛЬНЫЙ отрезок, где ни один коммит не касается
# `compiler-codegen/`, `std/`, `nova-cli/`. Его и отправляет. Первый же
# кодовый коммит останавливает отрезок — дальше ждём зелёного гейта.
#
# ПОЧЕМУ ИМЕННО НАЧАЛЬНЫЙ ОТРЕЗОК, а не «все документные». Git отправляет
# историю до указанного коммита целиком: выборочно вытащить документный
# коммит из середины нельзя, не переписав историю. Значит «отправить всю доку»
# при кодовом коммите в середине означало бы протащить и код — молча и мимо
# правила. Скрипт этого не делает и прямо говорит, где остановился.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/push-docs.sh            # показать, что отправится
#   bash scripts/tools/push-docs.sh --do       # отправить
#
# Зеркала берутся из `git remote`; секреты не печатаются (в URL sourcecraft
# лежит токен — поэтому `git remote -v` здесь не зовётся никогда).

set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DO=0
[ "${1:-}" = "--do" ] && DO=1

cd "$ROOT" || exit 1

CODE_RE='^(compiler-codegen|std|nova-cli)/'

RANGE=$(git -C "$ROOT" rev-list --reverse origin/main..main 2>/dev/null)
if [ -z "$RANGE" ]; then
    echo "push-docs: непушенных коммитов нет"
    exit 0
fi

LAST_DOC=""
STOPPED_AT=""
for c in $RANGE; do
    if git -C "$ROOT" show --name-only --format="" "$c" | grep -qE "$CODE_RE"; then
        STOPPED_AT="$c"
        break
    fi
    LAST_DOC="$c"
done

TOTAL=$(printf '%s\n' "$RANGE" | wc -l | tr -d ' ')

if [ -z "$LAST_DOC" ]; then
    echo "push-docs: первый же непушенный коммит трогает код ($(git -C "$ROOT" log -1 --format=%h "$STOPPED_AT")) — отправлять нечего"
    echo "    Код уходит только после зелёного гейта. Это правило не смягчалось."
    exit 0
fi

N=$(git -C "$ROOT" rev-list --count "origin/main..$LAST_DOC")
echo "push-docs: документных коммитов в начальном отрезке — $N из $TOTAL непушенных"
git -C "$ROOT" log --oneline "origin/main..$LAST_DOC" | sed 's/^/    /'

if [ -n "$STOPPED_AT" ]; then
    echo "    ── остановка: $(git -C "$ROOT" log -1 --format='%h %s' "$STOPPED_AT") трогает код"
    echo "       дальше — только после зелёного гейта"
fi

if [ "$DO" -eq 0 ]; then
    echo "push-docs: это предпросмотр; отправить — 'bash scripts/tools/push-docs.sh --do'"
    exit 0
fi

RC=0
for r in $(git -C "$ROOT" remote); do
    if git -C "$ROOT" push "$r" "$LAST_DOC:main" >/dev/null 2>&1; then
        echo "push-docs ok: $r"
    else
        echo "push-docs FAIL: $r" >&2
        RC=1
    fi
done
exit "$RC"
