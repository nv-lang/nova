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

# ДО 2026-08-29 ЗДЕСЬ СТОЯЛО `scripts/guards/*.sh` — и это была ДЫРА РАЗМЕРОМ
# В СЕМЬДЕСЯТ СТРАЖЕЙ: питоновые стражи требование страницы обходили молча, и
# завести запрет без объяснения было достаточно, написав его на python.
# Найдено при исполнении критерия (в) шага 1 плана 276: новый python-страж был
# бы зелён здесь, не будучи названным нигде. Счёт по правилу проекта: стражи —
# это `check-*`, а `*-scan.py` и `run-guards.py` — сканеры-ядра, не запреты.
#
# ДОЛГ — ХРАПОВИК, А НЕ ОБВАЛ: на день расширения неназванными оказались 41, и
# все сорок один — семья `check-novac-*` окна 274. Описать чужие стражи за их
# автора значило бы написать отписку — ровно то, чего этот страж и не ловит
# (сказано в его же шапке). Поэтому долг зафиксирован числом: расти нельзя,
# снижать — с летописью.
BASE_FILE="${NOVA_RULES_PAGE_BASELINE:-$(dirname "${BASH_SOURCE[0]}")/rules-page.baseline}"

MISSING=""
N=0
for g in scripts/guards/check-*.sh scripts/guards/check-*.py; do
    [ -f "$g" ] || continue
    name=$(basename "$g"); name="${name%.sh}"; name="${name%.py}"
    N=$((N + 1))
    grep -q -- "$name" "$PAGE" || MISSING="$MISSING $name"
done

nmiss=0
for m in $MISSING; do nmiss=$((nmiss + 1)); done

if [ -f "$BASE_FILE" ]; then
    BASE=$(sed -n 's/^no_page_ref=\([0-9][0-9]*\).*/\1/p' "$BASE_FILE" | head -1)
    [ -n "$BASE" ] || BASE=0
else
    echo "check-rules-page-complete: базы нет ($BASE_FILE) — считаю базой 0" >&2
    BASE=0
fi

echo "check-rules-page-complete: стражей $N, не названо $nmiss (база $BASE), страница $PAGE"

if [ "$nmiss" -gt "$BASE" ]; then
    echo "check-rules-page-complete: НАРУШЕНИЕ — неназванных стало больше базы ($nmiss > $BASE):" >&2
    for m in $MISSING; do echo "    $m" >&2; done
    echo "" >&2
    echo "    Страж, о котором агент не может прочитать, — запрет без причины." >&2
    echo "    Допиши строку в $PAGE ТЕМ ЖЕ слиянием: имя стража и одной" >&2
    echo "    фразой — что он не даёт сделать (реестр 221.1 №560)." >&2
    echo "check-rules-page-complete: FAIL" >&2
    exit 1
fi

if [ "$nmiss" -lt "$BASE" ]; then
    echo "check-rules-page-complete: долг СНИЗИЛСЯ ($nmiss < базы $BASE) — опусти базу в $BASE_FILE с летописью" >&2
    echo "check-rules-page-complete: FAIL" >&2
    exit 1
fi

if [ "$nmiss" -eq 0 ]; then
    echo "check-rules-page-complete ok: все стражи названы на странице правил"
else
    echo "check-rules-page-complete ok: новых неназванных нет; старый долг $nmiss ровно по базе"
fi
exit 0
