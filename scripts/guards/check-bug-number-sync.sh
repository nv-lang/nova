#!/usr/bin/env bash
# check-bug-number-sync.sh — правило владельца 2026-08-01 (№217 в 221.1):
# каждый НОВЫЙ [M-...]-маркер в backlog-followups.md обязан иметь № в
# 221.1-bug-sweep.md (нулевая толерантность: к релизу все баги нумерованы,
# информация не теряется при потере сессии). Исторические ненумерованные
# (до введения правила) заморожены в bug-number-sync.baseline — храповик:
# список может только УМЕНЬШАТЬСЯ (маркер получил № → строку можно убрать).
# НОВЫЙ маркер без № и вне baseline = КРАСНЫЙ гейт с именами.
# ВРЕМЕННОСТЬ (владелец 2026-08-01): правило действует МИНИМУМ до релиза
# Nova v0.1; после тега — интегратор напоминает владельцу и предлагает
# пострелизную схему записи; менять/снимать правило БЕЗ согласования
# владельца ЗАПРЕЩЕНО (при молчании — напоминать повторно).
set -u
export LC_ALL=C
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
BASELINE="$SCRIPT_DIR/bug-number-sync.baseline"
BACKLOG="$ROOT/docs/plans/backlog-followups.md"
SWEEP="$ROOT/docs/plans/221.1-bug-sweep.md"
# Один проход (не греп-на-маркер): все имена backlog минус имена, встречающиеся
# в 221.1, минус baseline. comm требует сортировки.
tmpb="$(mktemp)"; tmps="$(mktemp)"
grep -oE '\[M-[a-z0-9_.-]+\]' "$BACKLOG" | tr -d '[]' | sort -u > "$tmpb"
grep -oE 'M-[a-z0-9_.-]+' "$SWEEP" | sort -u > "$tmps"
# tr -d '\r': git autocrlf может пересоздать baseline с CRLF на Windows-чекауте
# (прецедент 2026-08-01: merge p-eff-hygiene → comm перестал матчить ВСЁ).
missing=$(comm -23 "$tmpb" "$tmps" | comm -23 - <(tr -d '\r' < "$BASELINE" | sort -u))
rm -f "$tmpb" "$tmps"
fail=0
[ -n "$missing" ] && fail=1
if [ "$fail" = "1" ]; then
    echo "BUG-NUMBER-SYNC FAIL: новые маркеры БЕЗ № в 221.1-bug-sweep.md (правило владельца 2026-08-01, №217):" >&2
    for m in $missing; do echo "  - $m" >&2; done
    echo "Заведи № в 221.1 (краткая строка + статус + ссылка на backlog) — или, если это НЕ дефект (решение/фича), добавь имя в bug-number-sync.baseline с обоснованием в коммите." >&2
    exit 1
fi
echo "bug-number-sync ok: все новые маркеры нумерованы в 221.1"
