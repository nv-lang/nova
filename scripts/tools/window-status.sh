#!/usr/bin/env bash
# scripts/tools/window-status.sh — состояние окна по текущему плану, фактами и без
# правок (заведён 2026-08-26 под команду /status; первая редакция — окно 274,
# коммит c3d179ecf в ветке p274-novac, здесь адаптирована под main).
#
# ЗАЧЕМ. «Где мы?» после сжатия контекста или утром отвечалось по памяти — а память
# после сжатия пуста, и сводка отстаёт от дерева. Здесь всё берётся из дерева и
# файлов: ветка и её расхождение с origin, незакоммиченное, статус-строки плана,
# последние закрытые шаги, вердикты последних прогонов гейта, строки реестра без
# номера. Команда /status читает этот вывод и докладывает в фиксированной форме.
#
# ЧЕМ ЭТА РЕДАКЦИЯ ОТЛИЧАЕТСЯ ОТ 274-Й: добавлен раздел ВНЕШНИЙ ГЕЙТ. Причина
# измерена в тот же день (реестр 221.1 №770): три пуша подряд ушли в main после
# локального «GATE OK» яруса loop, а авторитетный гейт краснел на всех трёх —
# на шаге яруса push, которого loop не гоняет вовсе. Статус, показывающий только
# локальные логи, повторил бы эту ложь ещё раз; поэтому вердикт CI здесь стоит
# ПЕРЕД локальным и кричит, если последний завершённый прогон красен.
#
# Использование: bash scripts/tools/window-status.sh [НОМЕР-ПЛАНА]
#   Номер плана по умолчанию — из имени ветки (p274-novac -> 274); нет цифр в имени —
#   скажет об этом и покажет только дерево и гейт.
# Ничего не пишет и не запускает тяжёлого: grep/git по дереву плюс один вызов
# `gh run list` — доли секунды и одна сетевая ходка.
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
    # Не просто `head -1`: у плана 221 рядом лежит 221-history.md, и по алфавиту
    # он идёт ПЕРВЫМ — первый же прогон этого скрипта прочитал летопись вместо
    # плана и доложил «живых строк: 0» при двенадцати. Летописи отсеиваются, а
    # если после этого осталось несколько файлов, берётся тот, где больше живых
    # пунктов: план — это тот файл, в котором есть что закрывать.
    F=$(ls docs/plans/"$PLAN"-*.md 2>/dev/null | grep -v -e '-history\.md$' -e '-archive\.md$' \
        | while read -r C; do printf '%s %s\n' "$(grep -c -a '^- \[ \]' "$C")" "$C"; done \
        | sort -rn | head -1 | cut -d' ' -f2-)
    [ -z "$F" ] && F=$(ls docs/plans/"$PLAN"-*.md 2>/dev/null | head -1)
    if [ -z "$F" ]; then
        echo "план $PLAN: файла docs/plans/$PLAN-*.md нет"
    else
        echo "файл: $F ($(wc -l < "$F" | tr -d ' ') строк)"
        echo "статус-строки:"
        grep -n -a -P '^\*\*[^*]*(\xd0\xa1\xd1\x82\xd0\xb0\xd1\x82\xd1\x83\xd1\x81|Status)[^*]*\*\*' "$F" | head -3 | cut -c1-160 | sed 's/^/    /'
        echo "живые строки (незакрытые пункты):"
        grep -c -a '^- \[ \]' "$F" | sed 's/^/    всего: /'
        grep -n -a '^- \[ \]' "$F" | head -6 | cut -c1-160 | sed 's/^/    /'
        echo "из них помечены блокерами тега:"
        grep -n -a '^- \[ \]' "$F" | grep -a -F 'БЛОКЕР' | head -6 | cut -c1-120 | sed 's/^/    /'
        echo "последние закрытия (ЗАКРЫТ <дата>):"
        grep -n -a -P '\xd0\x97\xd0\x90\xd0\x9a\xd0\xa0\xd0\xab\xd0\xa2[^0-9]{0,4}20[0-9]{2}-[0-9]{2}-[0-9]{2}' "$F" | tail -4 | cut -c1-160 | sed 's/^/    /'
        echo "подпланы: $(ls docs/plans/"$PLAN".*-*.md 2>/dev/null | wc -l | tr -d ' ') файлов"
    fi
fi

echo
echo "== ВНЕШНИЙ ГЕЙТ (CI, авторитетный — №770) =="
# Порядок намеренный: авторитетный вердикт ВЫШЕ локального. Локальный ярус loop
# не собирает компилятор и не судит корпус; принимать его «GATE OK» за состояние
# ветки — ровно та ошибка, ради которой этот раздел и заведён.
if command -v gh >/dev/null 2>&1; then
    CI=$(timeout 60 gh run list --limit 12 --branch "$BR" \
            --json name,status,conclusion,headSha,createdAt 2>/dev/null || echo '')
    if [ -z "$CI" ]; then
        echo "    вердикта нет: gh молчит (сеть, лимит или ветка не пушена) — это НЕ зелёный"
    else
        printf '%s' "$CI" | python -c "
import sys, json
try:
    rows = json.load(sys.stdin)
except Exception:
    rows = []
rows = [r for r in rows if r['name'] == 'nova-gate']
if not rows:
    print('    прогонов nova-gate на этой ветке нет')
else:
    for r in rows[:5]:
        print('    %-11s %-9s %s  %s' % (r['status'], r['conclusion'] or '-',
                                         r['headSha'][:9], r['createdAt'][:16]))
    done = [r for r in rows if r['status'] == 'completed']
    if done and done[0]['conclusion'] != 'success':
        print('    !! ПОСЛЕДНИЙ ЗАВЕРШЁННЫЙ ПРОГОН КРАСЕН (%s на %s) —'
              % (done[0]['conclusion'], done[0]['headSha'][:9]))
        print('       это состояние ветки, а не локальный ярус')
" 2>/dev/null || echo "    вердикт не разобран (python?)"
    fi
else
    echo "    gh не установлен — внешний вердикт неизвестен, и это НЕ зелёный"
fi

echo
echo "== ЛОКАЛЬНЫЙ ГЕЙТ (последние прогоны в target/) =="
if ls target/*.log >/dev/null 2>&1; then
    ls -t target/gate-*.log target/push-*.log target/loop-*.log target/modtests-*.log 2>/dev/null | head -4 | while read -r L; do
        V=$(grep -a -E 'GATE OK|GATE FAIL|NOVAC-GATE OK|NOVAC-GATE FAIL|PASS [0-9]+, FAIL [0-9]+' "$L" | tail -1 | cut -c1-90)
        printf '    %s  %s  %s\n' "$(date -r "$L" '+%Y-%m-%d %H:%M' 2>/dev/null)" "$L" "${V:-(вердикта нет)}"
    done
else
    echo "    логов в target/ нет — гейт в этом дереве не гоняли (или лог писался вне target/)"
fi

echo
echo "== РЕЕСТР И МАРКЕРЫ =="
REG=docs/plans/221.1-bug-sweep.md
if [ -f "$REG" ]; then
    echo "строк без номера (TBD): $(grep -a -cP '^\| *(\xe2\x84\x96)?TBD *\|' "$REG" || true)"
    if [ -f scripts/guards/registry-routes.baseline ]; then
        # Именно строку blockers=, а не первую не-комментарий: файл начинается
        # летописью, и `head -1` отдавал no_route= — число верное, но не то.
        echo "храповик блокеров (база): $(grep -a -oE '^blockers=[0-9]+' scripts/guards/registry-routes.baseline | head -1)"
    fi
    if [ -d novac ]; then
        echo "маркеров LEGACY-#TBD в novac/: $(grep -rho 'LEGACY-#TBD[^]]*' novac/ 2>/dev/null | wc -l | tr -d ' ')"
    fi
fi
echo "маркеров [M-…] в незакоммиченных правках: $(git diff HEAD 2>/dev/null | grep -c '^+.*\[M-' || true)"
