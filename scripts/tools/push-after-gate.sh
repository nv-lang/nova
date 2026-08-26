#!/usr/bin/env bash
# scripts/tools/push-after-gate.sh — пуш ветки окна ТОЛЬКО после зелёного гейта,
# ярус — по изменённым путям (заведён 2026-08-26 под команду /commit-push).
#
# ЗАЧЕМ. Правило «пушить сразу после зелёного гейта» держалось на памяти окна: какой
# ярус гонять (novac или основной), не забыть проверить авторство, повторить пуш при
# таймауте (2026-08-26 первый пуш упёрся в таймаут, коммит остался локальным и был
# замечен по «Everything up-to-date»). Здесь всё это — одна команда без выбора.
#
# Что делает, по порядку:
#   1. отказывает, если дерево не чистое (незакоммиченное не пушат «заодно») или
#      ветка — main (main принадлежит интегратору);
#   2. проверяет автора непушенных коммитов: %an обязан совпадать с git config user.name
#      этого дерева — авторство владельца, руками;
#   3. выбирает ярус: тронут novac/ или его стражи/скрипты — NOVAC_TIER=push
#      scripts/gate-novac.sh; иначе — NOVA_GATE_TIER=loop scripts/gate.sh (реестр,
#      маркеры, ссылки). Переменная GATE=novac|main|both|none переопределяет; none —
#      только с причиной в GATE_SKIP_REASON, и она печатается;
#   4. пушит с таймаутом и одним повтором; печатает unpushed после.
#
# Использование: bash scripts/tools/push-after-gate.sh
#   GATE=both — оба яруса; GATE=none GATE_SKIP_REASON="..." — без гейта (печатается).
# Логи гейта — target/gate-push-<ярус>.log; вердикт печатается одной строкой.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1
mkdir -p target

BR=$(git rev-parse --abbrev-ref HEAD)
if [ "$BR" = "main" ]; then
    echo "push-after-gate: ветка main принадлежит интегратору — окно пушит свою ветку" >&2
    exit 1
fi
# Неотслеживаемые файлы — не грязь: это ровно то, что НЕЛЬЗЯ подмести в коммит.
# Пуш блокируют только изменённые/проиндексированные пути.
DIRTY=$(git status --short | grep -v '^??' | wc -l | tr -d ' ')
UNTRACKED=$(git status --short | grep -c '^??' || true)
if [ "$DIRTY" -gt 0 ]; then
    echo "push-after-gate: дерево не чистое ($DIRTY файлов) — сначала коммит по именам или решение, что это не едет" >&2
    git status --short | head -10 | sed 's/^/    /' >&2
    exit 1
fi
UP="origin/$BR"
if ! git rev-parse --verify -q "$UP" >/dev/null; then
    echo "push-after-gate: у ветки $BR нет origin/$BR — первый пуш делается руками: git push -u origin $BR" >&2
    exit 1
fi
N=$(git rev-list --count "$UP"..HEAD)
if [ "$N" -eq 0 ]; then
    echo "push-after-gate: непушенных коммитов 0 — пушить нечего"
    exit 0
fi

# 2. авторство
WANT=$(git config user.name || true)
BAD=$(git log --format='%an' "$UP"..HEAD | sort -u | grep -v -x -F "${WANT:-__none__}" || true)
if [ -n "$BAD" ]; then
    echo "push-after-gate: среди $N непушенных коммитов чужой автор: $BAD (ожидается '$WANT') — не пушу" >&2
    exit 1
fi

# 3. ярус по путям
CHANGED=$(git diff --name-only "$UP"..HEAD)
TOUCH_NOVAC=$(printf '%s\n' "$CHANGED" | grep -c -E '^(novac/|scripts/gate-novac\.sh|scripts/guards/check-novac-|scripts/guards/novac-)' || true)
GATE="${GATE:-}"
if [ -z "$GATE" ]; then
    if [ "${TOUCH_NOVAC:-0}" -gt 0 ]; then GATE=both; else GATE=main; fi
fi
[ "${UNTRACKED:-0}" -gt 0 ] && echo "push-after-gate: неотслеживаемых файлов оставлено на месте: $UNTRACKED"
echo "push-after-gate: ветка $BR, непушенных $N, изменённых файлов $(printf '%s\n' "$CHANGED" | grep -c .), ярус: $GATE"

run_gate() { # имя команда...
    local name="$1"; shift
    local log="target/gate-push-$name.log"
    echo "push-after-gate: гоню $name -> $log"
    "$@" > "$log" 2>&1
    local rc=$?
    local verdict
    verdict=$(grep -a -E 'GATE OK|GATE FAIL|NOVAC-GATE OK|NOVAC-GATE FAIL' "$log" | tail -1 | cut -c1-120)
    echo "push-after-gate: $name: ${verdict:-(вердикта нет)} (rc=$rc)"
    if [ "$rc" -ne 0 ]; then
        grep -n -a 'FAIL' "$log" | head -8 | sed 's/^/    /' >&2
        return 1
    fi
    return 0
}

case "$GATE" in
    main)  run_gate main  env NOVA_GATE_TIER=loop bash scripts/gate.sh || exit 1 ;;
    novac) run_gate novac env NOVAC_TIER=push bash scripts/gate-novac.sh || exit 1 ;;
    both)  run_gate novac env NOVAC_TIER=push bash scripts/gate-novac.sh || exit 1
           run_gate main  env NOVA_GATE_TIER=loop bash scripts/gate.sh || exit 1 ;;
    none)  if [ -z "${GATE_SKIP_REASON:-}" ]; then
               echo "push-after-gate: GATE=none без GATE_SKIP_REASON — причина обязана быть названа" >&2; exit 1
           fi
           echo "push-after-gate: ГЕЙТ ПРОПУЩЕН по причине: $GATE_SKIP_REASON" ;;
    *)     echo "push-after-gate: неизвестный GATE=$GATE (main|novac|both|none)" >&2; exit 1 ;;
esac

# 4. пуш с одним повтором
push_once() { timeout 500 git push origin "$BR" 2>&1 | tail -1; return "${PIPESTATUS[0]}"; }
if ! push_once; then
    echo "push-after-gate: пуш не прошёл (таймаут или сеть) — повторяю один раз"
    push_once || true
fi
LEFT=$(git rev-list --count "$UP"..HEAD)
echo "push-after-gate: unpushed: $LEFT"
[ "$LEFT" -eq 0 ] || { echo "push-after-gate: пуш не завершён — повтори руками: git push origin $BR" >&2; exit 1; }
exit 0
