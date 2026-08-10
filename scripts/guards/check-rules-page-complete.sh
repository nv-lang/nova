#!/usr/bin/env bash
# scripts/guards/check-rules-page-complete.sh
# Каждый страж обязан быть назван на странице правил для агентов.
#
# ДОМ И ОСНОВАНИЕ: план 231 «Выход из цикла точечных фиксов», трек Д (машинное
# принуждение норм); запись реестра 221.1 №560.
#
# ЗАЧЕМ. 2026-08-10 замер показал: из 32 стражей **28 не упоминались нигде** в
# цепочке онбординга (`CLAUDE.md` → `read-project.md` → `dev-workflow.md` →
# `AGENTS.md`). То есть агент читал онбординг и не мог узнать, что именно
# покраснеет — правила существовали только в шапках самих стражей. Отсюда
# наблюдаемые нарушения: русские сообщения коммитов, `git add -u`, забирающий
# чужое, самовольный выбор номера D-блока.
#
# Страж без объяснения — это запрет без причины: его обходят, а не соблюдают.
# Поэтому новый страж обязан появиться на странице `docs/dev/rules-for-agents.md`
# ТЕМ ЖЕ слиянием, что и сам страж.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): что объяснение ВЕРНОЕ. Проверяется только
# наличие имени на странице — отличить точное описание от отписки машина не
# может, это суждение.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-rules-page-complete.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-rules-page-complete.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || { echo "check-rules-page-complete: нет каталога $ROOT" >&2; exit 1; }

PAGE="docs/dev/rules-for-agents.md"
[ -f "$PAGE" ] || { echo "check-rules-page-complete: нет страницы правил $PAGE" >&2; exit 1; }

MISSING=""
N=0
for g in scripts/guards/*.sh; do
    [ -f "$g" ] || continue
    name=$(basename "$g" .sh)
    # Сам этот страж и установщик — служебные, но названы там же ради полноты.
    N=$((N + 1))
    grep -q -- "$name" "$PAGE" || MISSING="$MISSING $name"
done

echo "check-rules-page-complete: стражей $N, страница $PAGE"

if [ -n "$MISSING" ]; then
    echo "check-rules-page-complete: НАРУШЕНИЕ — не названы на странице правил:" >&2
    for m in $MISSING; do echo "    $m" >&2; done
    echo "" >&2
    echo "    Страж, о котором агент не может прочитать, — запрет без причины." >&2
    echo "    Допиши строку в $PAGE ТЕМ ЖЕ слиянием: имя стража и одной" >&2
    echo "    фразой — что он не даёт сделать (реестр 221.1 №560)." >&2
    echo "check-rules-page-complete: FAIL" >&2
    exit 1
fi

echo "check-rules-page-complete ok: все стражи названы на странице правил"
exit 0
