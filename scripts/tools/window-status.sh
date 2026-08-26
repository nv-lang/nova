#!/usr/bin/env bash
# scripts/tools/window-status.sh — состояние окна по текущему плану, фактами и без
# правок (заведён 2026-08-26 под команду /status).
#
# ЗАЧЕМ. «Где мы?» после сжатия контекста или утром отвечалось по памяти — а память
# после сжатия пуста, и сводка отстаёт от дерева. Здесь всё берётся из дерева и
# файлов: ветка и её расхождение с origin, незакоммиченное, статус-строки плана,
# последние закрытые шаги, вердикты последних прогонов гейта, строки реестра без
# номера. Команда /status читает этот вывод и докладывает в фиксированной форме.
#
# Использование: bash scripts/tools/window-status.sh [НОМЕР-ПЛАНА]
#   Номер плана по умолчанию — из имени ветки (p274-novac -> 274); нет цифр в имени —
#   скажет об этом и покажет только дерево и гейт.
# Ничего не пишет и не запускает тяжёлого: одни grep/git по дереву, доли секунды.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

BR=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "?")
PLAN="${1:-}"
if [ -z "$PLAN" ]; then
    PLAN=$(printf '%s' "$BR" | grep -oE '[0-9]{3}' | head -1)
fi

echo "== ДЕРЕВО =="
echo "worktree: $ROOT"
echo "ветка:    $BR"
echo "HEAD:     $(git log -1 --format='%h %ad %s' --date=short 2>/dev/null | cut -c1-110)"
UP=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || true)
if [ -n "$UP" ]; then
    echo "непушено в $UP: $(git rev-list --count "$UP"..HEAD 2>/dev/null || echo '?')"
else
    echo "непушено: ветка без upstream"
fi
if git rev-parse --verify -q origin/main >/dev/null; then
    # Две двухточечные формы, не A...B: трёхточечная берёт ОДНОГО предка из
    # нескольких и молчит об этом (реестр 221.1 №629, страж branch-absorption).
    BEHIND=$(git rev-list --count HEAD..origin/main 2>/dev/null || echo '?')
    AHEAD=$(git rev-list --count origin/main..HEAD 2>/dev/null || echo '?')
    echo "против origin/main: отстаю на $BEHIND, впереди на $AHEAD"
fi
DIRTY=$(git status --short 2>/dev/null | wc -l | tr -d ' ')
echo "незакоммичено: $DIRTY файлов"
[ "$DIRTY" -gt 0 ] && git status --short | head -10 | sed 's/^/    /'
echo "последние коммиты:"
git log -5 --format='    %h %ad %s' --date=short 2>/dev/null | cut -c1-110

echo
echo "== ПЛАН =="
if [ -z "$PLAN" ]; then
    echo "номер плана не выведен из имени ветки '$BR' — передай его аргументом"
else
    F=$(ls docs/plans/"$PLAN"-*.md 2>/dev/null | head -1)
    if [ -z "$F" ]; then
        echo "план $PLAN: файла docs/plans/$PLAN-*.md нет"
    else
        echo "файл: $F ($(wc -l < "$F" | tr -d ' ') строк)"
        echo "статус-строки:"
        grep -n -a -P '^\*\*[^*]*(\xd0\xa1\xd1\x82\xd0\xb0\xd1\x82\xd1\x83\xd1\x81|Status)[^*]*\*\*' "$F" | head -3 | cut -c1-160 | sed 's/^/    /'
        echo "живые строки (СДЕЛАНО / живая строка):"
        grep -n -a -P '\xd0\xb6\xd0\xb8\xd0\xb2\xd0\xb0\xd1\x8f \xd1\x81\xd1\x82\xd1\x80\xd0\xbe\xd0\xba' "$F" | head -4 | cut -c1-160 | sed 's/^/    /'
        echo "последние закрытия (ЗАКРЫТ <дата>):"
        grep -n -a -P '\xd0\x97\xd0\x90\xd0\x9a\xd0\xa0\xd0\xab\xd0\xa2[^0-9]{0,4}20[0-9]{2}-[0-9]{2}-[0-9]{2}' "$F" | tail -4 | cut -c1-160 | sed 's/^/    /'
        echo "подпланы: $(ls docs/plans/"$PLAN".*-*.md 2>/dev/null | wc -l | tr -d ' ') файлов"
    fi
fi

echo
echo "== ГЕЙТ (последние прогоны в target/) =="
if ls target/*.log >/dev/null 2>&1; then
    ls -t target/gate-*.log target/push-*.log target/loop-*.log target/modtests-*.log 2>/dev/null | head -4 | while read -r L; do
        V=$(grep -a -E 'GATE OK|GATE FAIL|NOVAC-GATE OK|NOVAC-GATE FAIL|PASS [0-9]+, FAIL [0-9]+' "$L" | tail -1 | cut -c1-90)
        printf '    %s  %s  %s\n' "$(date -r "$L" '+%Y-%m-%d %H:%M' 2>/dev/null)" "$L" "${V:-(вердикта нет)}"
    done
else
    echo "    логов в target/ нет — гейт в этом дереве не гоняли"
fi

echo
echo "== РЕЕСТР И МАРКЕРЫ =="
REG=docs/plans/221.1-bug-sweep.md
if [ -f "$REG" ]; then
    echo "строк без номера (TBD): $(grep -a -cP '^\| *(\xe2\x84\x96)?TBD *\|' "$REG" || true)"
    echo "маркеров LEGACY-#TBD в novac/: $(grep -rho 'LEGACY-#TBD[^]]*' novac/ 2>/dev/null | wc -l | tr -d ' ')"
fi
echo "маркеров [M-…] в незакоммиченных правках: $(git diff HEAD 2>/dev/null | grep -c '^+.*\[M-' || true)"
