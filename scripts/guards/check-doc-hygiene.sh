#!/usr/bin/env bash
# check-doc-hygiene.sh — страж гигиены публичной доки .nv и линт-сообщений
# (правило владельца 2026-07-31, nv-coding-style §25-расширение;
# план: docs/plans/231.2-enforcement-infra.md — энфорс-инфраструктура, трек
# стражей; связанные волны понижения baseline: перевод /// (endocs-std),
# линт-английский (p-lints), чистка внутренних ссылок (comment-hygiene)).
#
# ЧТО ЛОВИТ (три счётчика, все — храповик «только вниз»):
#   1) cyrillic_doc    — кириллица в `///`/`//!`-строках .nv (std/src, examples,
#                        пакетные репы рядом: nova-bigint/nova-polaris/nova-http/
#                        nova-compress/nova-tls, каталог src/). Дока — английская.
#   2) internal_doc    — внутренние ссылки в `///`/`//!`: номера планов, маркеры
#                        [M-...], D-номера, №N, Ф.N, «реестр», CLOSED/FIXED.
#                        Пользователю не интересно, что и по каким дефектам мы
#                        закрывали — язык ещё не в релизе; дока = смысл API.
#   3) cyrillic_lint   — кириллица в user-facing строках compiler-codegen/src/
#                        lints.rs (summary/тексты правил). Линт говорит по-английски.
#
# Baseline: scripts/guards/doc-hygiene.baseline (три строки name=N).
# Текущий долг зафиксирован; волны перевода/чистки опускают числа; РОСТ = красный.
#
# ИСПОЛЬЗОВАНИЕ: check-doc-hygiene.sh [корень-репы]   (по умолчанию — родитель scripts/)
set -u
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
BASELINE="$SCRIPT_DIR/doc-hygiene.baseline"

doc_dirs=("$ROOT/std/src" "$ROOT/examples")
parent="$(cd "$ROOT/.." && pwd)"
for pkg in nova-bigint nova-polaris nova-http nova-compress nova-tls; do
    [ -d "$parent/$pkg/src" ] && doc_dirs+=("$parent/$pkg/src")
done

cyr=0
intr=0
for d in "${doc_dirs[@]}"; do
    [ -d "$d" ] || continue
    c=$(grep -rE '^[[:space:]]*//[/!].*[а-яА-ЯёЁ]' --include='*.nv' "$d" 2>/dev/null | wc -l)
    i=$(grep -rE '^[[:space:]]*//[/!].*(\[M-|Plan [0-9]|План [0-9]|D[0-9]{2,3}[^0-9]|№[0-9]|Ф\.[0-9]|реестр|CLOSED|FIXED)' --include='*.nv' "$d" 2>/dev/null | wc -l)
    cyr=$((cyr + c)); intr=$((intr + i))
done
lint_cyr=$(grep -c '[а-яА-ЯёЁ]' "$ROOT/compiler-codegen/src/lints.rs" 2>/dev/null)
lint_cyr=${lint_cyr:-0}

fail=0
check() { # name actual
    local base
    base=$(grep -E "^$1=" "$BASELINE" 2>/dev/null | cut -d= -f2)
    [ -n "$base" ] || { echo "doc-hygiene: нет $1= в baseline" >&2; fail=1; return; }
    if [ "$2" -gt "$base" ]; then
        echo "DOC-HYGIENE FAIL: $1=$2 > baseline=$base (рост запрещён; правило владельца 2026-07-31)" >&2
        fail=1
    else
        echo "doc-hygiene ok: $1=$2 <= $base"
    fi
}
check cyrillic_doc "$cyr"
check internal_doc "$intr"
check cyrillic_lint "$lint_cyr"
exit $fail
