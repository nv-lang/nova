#!/usr/bin/env bash
# check-workflow-apt-step.sh — шаг apt в workflow обязан иметь предел и повтор (задача #37).
#
# ПРАВИЛО. В `.github/workflows/*.yml`:
#   1. apt вызывается ТОЛЬКО через `scripts/tools/ci-apt-install.sh` (предел попытки, 3 повтора, apt-таймауты);
#      строка кода с `apt-get` / `apt install` напрямую — FAIL;
#   2. шаг, зовущий скрипт, несёт `timeout-minutes:` не больше 15 (три попытки по 270 с в него входят).
# ЗАЧЕМ. 2026-10-07 голый `sudo apt-get update && sudo apt-get install` зависал дважды: 84+ минуты без движения
# и отмена задания по пределу 120 минут; нормальное время шага — 1-2 минуты.
# Строки-комментарии (`#` первым непробельным) не судятся — там рецепт цитируют.
#
# $1 — корень репозитория. Самотест: scripts/guards/selftest/test-check-workflow-apt-step.sh
set -u
export LC_ALL=C
NAME=check-workflow-apt-step
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
WF="$ROOT/.github/workflows"
FILES="$(find "$WF" -type f \( -name '*.yml' -o -name '*.yaml' \) 2>/dev/null | sort)"
if [ -z "$FILES" ]; then
    echo "$NAME ok: судить нечего — в $ROOT нет .github/workflows/*.yml"
    exit 0
fi

BAD=""
STEPS=0
while IFS= read -r f; do
    [ -n "$f" ] || continue
    r=$(tr -d '\r' < "$f" | awk -v F="${f#$ROOT/}" '
        function flush() {
            if (uses) {
                steps++
                if (lim == "") printf "%s:%d: шаг с ci-apt-install.sh без timeout-minutes\n", F, start
                else if (lim + 0 > 15) printf "%s:%d: timeout-minutes %s > 15\n", F, start, lim
            }
            uses = 0; lim = ""
        }
        /^[ ]*#/ { next }
        /^[ ]*- / { flush(); start = NR }
        /(^|[^A-Za-z0-9_.-])apt(-get)?[ ]+(-[^ ]+[ ]+)*(install|update)/ {
            printf "%s:%d: apt напрямую, не через scripts/tools/ci-apt-install.sh\n", F, NR
        }
        /ci-apt-install\.sh/ { uses = 1 }
        /^[ -]*timeout-minutes:/ { v = $0; sub(/^[ -]*timeout-minutes:[ ]*/, "", v); sub(/[ ]*(#.*)?$/, "", v); lim = v }
        END { flush(); printf "STEPS=%d\n", steps }')
    n=$(printf '%s\n' "$r" | sed -n 's/^STEPS=//p')
    STEPS=$((STEPS + ${n:-0}))
    h=$(printf '%s\n' "$r" | grep -v '^STEPS=' || true)
    [ -n "$h" ] && BAD="$BAD$h
"
done <<EOF
$FILES
EOF

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — шаг apt без предела или повтора (задача #37):" >&2
    printf '%s' "$BAD" | sed 's/^/  /' >&2
    echo "  КАК НАДО: \`run: bash scripts/tools/ci-apt-install.sh пакеты...\` и \`timeout-minutes: 15\` на шаге." >&2
    exit 1
fi
echo "$NAME ok: шагов apt через ci-apt-install.sh с пределом ≤15 мин: $STEPS, прямых вызовов apt 0"
exit 0
