#!/usr/bin/env bash
# check-no-env-after-timeout.sh — реестр 221.1 №1310.
#
# ПРАВИЛО: переменная окружения для команды ставится ПЕРЕД `timeout`, а не между
# `timeout` и командой. `VAR=val cmd` — присваивание, которое делает ОБОЛОЧКА, и
# только для слов ПЕРЕД именем команды. `timeout` получает `VAR=val` как обычный
# аргумент и пытается запустить программу с таким именем:
#   timeout 5 NOVAC_SELF_PATH=x echo hi   ->  failed to run command, rc=127
#   NOVAC_SELF_PATH=x timeout 5 echo hi   ->  hi, rc=0
#
# ЧТО ЭТО СТОИЛО, замер 2026-09-23. `scripts/tools/novac-diff-corpus.sh` с
# 2026-09-01 писал `timeout 60 NOVAC_SELF_PATH=novac/src "$NOVAC" check ...`
# внутри `eval "..."`: пачка самосборки не запустилась НИ РАЗУ за три недели,
# каждый прогон уходил в пофайловый откат и печатал «ICE убил пачку» без ICE, а
# мера ступени 0.2 (`self-distance`) снималась не тем способом под батчевым
# именем. `scripts/tools/double-build.sh` дефект видел, обошёл у себя и строки
# не завёл. Правило было известно — механизма не было.
#
# ЧТО СУДИТ: строки кода `.sh` под `scripts/` и `.yml` под `.github/workflows/`,
# в том числе текст внутри кавычек (`eval "timeout 60 X=1 ..."` — ровно форма
# носителя). Форма: `timeout`, затем необязательные флаги (`-k 5`, `-s KILL`,
# `--foreground`), затем длительность, затем слово `ИМЯ=`.
# ЧЕГО НЕ СУДИТ: строки-комментарии (`#` первым непробельным) — там форму
# цитируют, объясняя её, и этот файл в том числе; `docs/` — проза.
# ХРАПОВИКА НЕТ: у формы нет законного применения, носителей на заведении ноль.
#
# Запуск: bash scripts/guards/check-no-env-after-timeout.sh [ROOT]
# Самотест: scripts/guards/selftest/test-check-no-env-after-timeout.sh
set -u
export LC_ALL=C
NAME=check-no-env-after-timeout
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

FILES=""
[ -d "$ROOT/scripts" ] && FILES="$(find "$ROOT/scripts" -type f -name '*.sh' 2>/dev/null | sort)"
if [ -d "$ROOT/.github/workflows" ]; then
    FILES="$FILES
$(find "$ROOT/.github/workflows" -type f \( -name '*.yml' -o -name '*.yaml' \) 2>/dev/null | sort)"
fi
N=$(printf '%s\n' "$FILES" | grep -c . || true)
if [ "$N" -eq 0 ]; then
    echo "$NAME ok: судить нечего — в $ROOT нет scripts/**/*.sh и .github/workflows/*.yml"
    exit 0
fi

# timeout [флаги] ДЛИТЕЛЬНОСТЬ ИМЯ=
RE='(^|[^A-Za-z0-9_-])timeout([[:space:]]+-[-A-Za-z]+([[:space:]]+[A-Za-z0-9.]+)?)*[[:space:]]+[0-9.]+[smhd]?[[:space:]]+[A-Za-z_][A-Za-z0-9_]*='
HITS=""
while IFS= read -r f; do
    [ -n "$f" ] || continue
    h=$(grep -nE "$RE" "$f" 2>/dev/null | grep -vE '^[0-9]+:[[:space:]]*#' || true)
    [ -n "$h" ] && HITS="$HITS$(printf '%s\n' "$h" | sed "s#^#  ${f#$ROOT/}:#")
"
done <<EOF
$FILES
EOF

if [ -n "$HITS" ]; then
    echo "$NAME: FAIL — переменная окружения стоит ПОСЛЕ \`timeout\` (реестр 221.1 №1310):" >&2
    printf '%s' "$HITS" >&2
    echo "  \`timeout\` запускает \`ИМЯ=значение\` как ПРОГРАММУ (rc=127), команда не стартует вовсе." >&2
    echo "  КАК НАДО: \`ИМЯ=значение timeout N команда\` — присваивание перед \`timeout\`." >&2
    exit 1
fi
echo "$NAME ok: файлов осмотрено $N (scripts/**/*.sh и .github/workflows), форм \`timeout N ИМЯ=\` 0"
exit 0
