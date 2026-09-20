#!/usr/bin/env bash
# scripts/tools/merge-precheck.sh — точечная проверка ПОСЛЕ слияния, ДО полного
# гейта (5+ минут).
#
# ЗАЧЕМ (замер /release-speed, 2026-09-21, интегратор). За одну ночь дважды
# полный прогон ловил отказы, которые дают вердикт за секунды сами по себе:
# слияние `nova-kim` принесло новый каталог верхнего уровня, машинный путь в
# файле-адаптере и раздутие session-start слоя контекста — check-repo-root-clean,
# check-no-machine-paths и check-context-layer-budget поймали бы все три
# СРАЗУ, без пятиминутного ожидания остального гейта. Цикл «слияние → гейт →
# отказ → фикс → снова гейт» отнял лишний полный прогон.
#
# ЧТО ЭТО НЕ ЗАМЕНЯЕТ. Это НЕ авторитетный гейт и не повод его не гонять:
# у полного гейта сотни других стражей, этот скрипт держит только три самых
# дешёвых и самых частых носителя вечерних отказов. Зелёный здесь — не
# основание для пуша; это ускоряет ОБНАРУЖЕНИЕ, а не заменяет проверку.
#
# ИСПОЛЬЗОВАНИЕ: после `git merge`, до `gate-bg.sh`, из корня репозитория:
#   bash scripts/tools/merge-precheck.sh .
set -u
export LC_ALL=C
ROOT="${1:-.}"

fail=0
run() {
    local name="$1"; shift
    echo "== precheck: $name =="
    if "$@"; then
        :
    else
        fail=1
    fi
}

run "repo-root-clean (новый каталог верхнего уровня?)" \
    bash "$ROOT/scripts/guards/check-repo-root-clean.sh" "$ROOT"
run "no-machine-paths (раскладка машины литералом?)" \
    bash "$ROOT/scripts/guards/check-no-machine-paths.sh" "$ROOT"
run "context-layer-budget (слой @-импортов вырос?)" \
    python "$ROOT/scripts/guards/check-context-layer-budget.py" "$ROOT"

if [ "$fail" -ne 0 ]; then
    echo "merge-precheck: ЕСТЬ ОТКАЗЫ — чини их ДО полного гейта, дешевле сейчас." >&2
    exit 1
fi
echo "merge-precheck ok: три дешёвых стража чисты; можно гнать полный гейт."
