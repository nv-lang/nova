#!/usr/bin/env bash
# scripts/tools/integrator-push.sh — гейт и пуш main ОДНИМ механизмом, чтобы
# пуш физически не мог уйти при красном гейте.
#
# ЗАЧЕМ, по двум замерам одного дня (2026-09-01, интегратор, оба — мои
# собственные промахи):
#   1. `bash gate.sh | tail -2 && git push` — конвейер вернул код `tail`,
#      и пуш ушёл при КРАСНОМ гейте (doc-conventions), краснота была
#      замечена по тексту, пролетевшему над выводом пуша.
#   2. `... && gate.sh; echo rc; git push` — точка с запятой: пуш снова
#      не зависел от rc гейта и ушёл при красном (бюджет яруса).
# Один и тот же класс дважды за день — это не случайность, а дыра в
# дисциплине, и дыры дисциплины здесь закрываются механизмом, а не
# памятью (CLAUDE.md: «внимание заменяем механизмом»).
#
# ПОЧЕМУ НЕ `push-after-gate.sh`: тот НАМЕРЕННО отказывает на main
# («main принадлежит интегратору») — он для окон с ветками. Этот —
# ровно для интегратора на main, и НИЧЕГО не решает сам: ярус фиксирован
# (loop), пуш только origin+зеркала, никаких скипов. Нужен скип CI-проверки
# (разобранная краснота) — переменная прокидывается явно, и причина уже
# обязана лежать в реестре по правилу самого pre-push хука.
#
# ИСПОЛЬЗОВАНИЕ (из корня):
#   bash scripts/tools/integrator-push.sh
#   NOVA_SKIP_CI_CHECK=1 bash scripts/tools/integrator-push.sh   # причина — в реестре
# Код возврата: 0 — гейт зелёный И все три пуша прошли; иначе не-ноль,
# и с какого шага — видно по последней строке.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

BRANCH=$(git -C "$ROOT" rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" != "main" ]; then
    echo "integrator-push: ветка '$BRANCH' — это инструмент интегратора для main;" >&2
    echo "  для веток есть scripts/tools/push-after-gate.sh (он выбирает ярус сам)." >&2
    exit 1
fi

echo "integrator-push: ярус loop, $(date +%H:%M:%S)"
if ! NOVA_GATE_TIER=loop bash "$ROOT/scripts/gate.sh"; then
    echo "integrator-push: ГЕЙТ КРАСНЫЙ — пуша не будет (в этом весь смысл скрипта)." >&2
    exit 1
fi

fail=0
for r in origin gitverse sourcecraft; do
    if git -C "$ROOT" push "$r" main; then
        echo "integrator-push: $r ok"
    else
        echo "integrator-push: $r НЕ ПРОШЁЛ" >&2
        fail=1
    fi
done
[ "$fail" -eq 0 ] || exit 1

echo "integrator-push: все три зеркала:"
for r in origin gitverse sourcecraft; do
    printf '  %-12s %s\n' "$r" "$(git -C "$ROOT" ls-remote "$r" refs/heads/main | cut -c1-12)"
done
