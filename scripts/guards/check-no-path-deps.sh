#!/usr/bin/env bash
# scripts/guards/check-no-path-deps.sh — энфорс D420: `path` ТОЛЬКО под [replace].
#
# ЗАЧЕМ (реестр 221.1 №444, 2026-08-08): D420 (`spec/decisions/09-tooling.md`)
# прямо требует: релизная форма зависимости — git+semver; `path` допустим ТОЛЬКО
# в `[replace]`, и притом в НЕкоммитящемся `nova.local.toml`/`nova.override.toml`
# («закоммиченный [replace] ломает чистый клон» — дофикс №2, вскрыт владельцем
# ещё 2026-07-13). Правило БЫЛО, машинной проверки НЕ БЫЛО — и `examples/nova.toml`
# месяцами держал `http = { path = "../../nova-http" }` и `polaris = { path = ... }`
# прямо в `[dependencies]`. На CI соседних реп нет → шаг «Flagship examples gate»
# краснел сообщением «резолюция зависимостей: зависимость polaris: path ...»,
# и этот красный маскировал ВСЁ остальное на nova-gate.
#
# ЧТО ПРОВЕРЯЕТ: ни один КОММИТЯЩИЙСЯ `nova.toml` не содержит `path =` вне
# секции `[replace]`. Не-коммитящиеся оверрайды (`nova.local.toml`,
# `nova.override.toml`) не проверяются вовсе — им path и положен.
#
# ИСПОЛЬЗОВАНИЕ: bash scripts/guards/check-no-path-deps.sh [КОРЕНЬ]

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || exit 1

VIOLATIONS=0

# Только отслеживаемые git'ом манифесты — локальные оверрайды не наши.
# ОДИН проход awk по всем файлам: построчный bash-цикл на этом же наборе не
# укладывался в 180 с (git-bash, ~сотни манифестов) — страж, который не успевает
# отработать, не страж.
# ИСКЛЮЧЕНИЕ: nova_tests/** и spec_tests/** — там манифесты с `path` ЗАКОННЫ,
# это фикстуры самого резолвера зависимостей (cycle_dep, dup_dep_neg,
# internal_dep) — они и обязаны проверять разрешение путей. Первая редакция
# стража дала на них 27 ложных срабатываний.
MANIFESTS=$(git ls-files '*nova.toml' 2>/dev/null | grep -vE '^(nova_tests|spec_tests|probes|scratch)')
if [ -n "$MANIFESTS" ]; then
    OUT=$(awk '
        FNR == 1 { in_replace = 0 }
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*\[replace[]. ]/ { in_replace = 1; next }
        /^[[:space:]]*\[/            { in_replace = 0; next }
        in_replace == 0 && /(^|[{, ])path[[:space:]]*=/ {
            printf "check-no-path-deps: НАРУШЕНИЕ D420 — %s:%d: path вне [replace]\n    %s\n",
                   FILENAME, FNR, $0
        }
    ' $MANIFESTS 2>/dev/null)
    if [ -n "$OUT" ]; then
        echo "$OUT" >&2
        VIOLATIONS=$(( VIOLATIONS + $(printf '%s\n' "$OUT" | grep -c 'НАРУШЕНИЕ') ))
    fi
fi

# Лок-файл: путевые источники в закоммиченном локе тоже нерезолвимы на CI.
for f in $(git ls-files '*nova.lock.toml' 2>/dev/null | grep -vE '^(nova_tests|spec_tests|probes|scratch)'); do
    [ -f "$f" ] || continue
    n=$(grep -c 'source = "path"' "$f" 2>/dev/null | tr -d '

 ')
    [ -n "$n" ] || n=0
    if [ "${n:-0}" -gt 0 ]; then
        echo "check-no-path-deps: НАРУШЕНИЕ — $f: $n путевых источник(ов) в КОММИТЯЩЕМСЯ локе" >&2
        echo "    лок обязан быть пересоздан БЕЗ активного override (nova update)" >&2
        VIOLATIONS=$((VIOLATIONS + n))
    fi
done

if [ "$VIOLATIONS" -gt 0 ]; then
    echo "check-no-path-deps: FAIL — $VIOLATIONS нарушени(й) D420" >&2
    exit 1
fi
echo "check-no-path-deps ok: path только под [replace]; в коммитящихся локах путей нет"
exit 0
