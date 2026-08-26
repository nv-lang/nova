#!/usr/bin/env bash
# scripts/tools/git-readiness.sh — можно ли СЕЙЧАС коммитить, пушить, синхронизироваться
# с main: три вердикта по фактам дерева, без побочных действий (заведён 2026-08-27 под
# команду /commit-push).
#
# ЗАЧЕМ. Вопрос «а можно ли сейчас коммитить/пушить/синкать» решался по памяти окна и
# ошибался молча: коммит на грязном индексе чужого worktree, пуш поверх незакрытого
# слияния, синк при незакоммиченном. Здесь каждое из трёх действий получает вердикт с
# причиной, и команда действует по вердикту, а не по ощущению.
#
# ВЫВОД — три блока фиксированной формы:
#   COMMIT: ВОЗМОЖЕН | НЕВОЗМОЖЕН: <причина> | НЕЧЕГО
#   PUSH:   ВОЗМОЖЕН (после гейта: <ярус>) | НЕВОЗМОЖЕН: <причина> | НЕЧЕГО
#   SYNC:   НУЖЕН (отстаю на N) | НЕ НУЖЕН | НЕВОЗМОЖЕН: <причина>
# плюс строки фактов под каждым. Код возврата 0 всегда — это доклад, не страж.
#
# Что смотрит: ветка (main — интегратора), незавершённое слияние/rebase/cherry-pick,
# маркеры конфликта в изменённых файлах, изменённые/проиндексированные/неотслеживаемые,
# upstream и число непушенных, автор непушенных, отставание от origin/main (после
# `git fetch origin` с таймаутом; без сети — по последнему известному origin/main, и это
# сказано), сдвиг источников оракула в main (после синка нужна пересборка).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

BR=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "?")
GITDIR=$(git rev-parse --git-path . 2>/dev/null)
inprog=""
[ -e "$(git rev-parse --git-path MERGE_HEAD)" ] && inprog="слияние"
[ -e "$(git rev-parse --git-path rebase-merge)" ] || [ -e "$(git rev-parse --git-path rebase-apply)" ] && inprog="${inprog:+$inprog, }rebase"
[ -e "$(git rev-parse --git-path CHERRY_PICK_HEAD)" ] && inprog="${inprog:+$inprog, }cherry-pick"

STATUS=$(git status --short 2>/dev/null)
MOD=$(printf '%s\n' "$STATUS" | grep -c -v -E '^(\?\?|$)' || true)
UNTR=$(printf '%s\n' "$STATUS" | grep -c '^??' || true)
STAGED=$(git diff --cached --name-only 2>/dev/null | wc -l | tr -d ' ')
MARKERS=0
if [ "${MOD:-0}" -gt 0 ]; then
    MARKERS=$(git diff --name-only HEAD 2>/dev/null | xargs -r grep -l -E '^(<<<<<<<|=======|>>>>>>>)' 2>/dev/null | wc -l | tr -d ' ')
fi

echo "== ДЕРЕВО: ветка $BR, изменено $MOD, в индексе $STAGED, неотслеживаемых $UNTR${inprog:+, НЕЗАВЕРШЕНО: $inprog} =="

# ---------------------------------------------------------------- COMMIT
if [ -n "$inprog" ]; then
    echo "COMMIT: НЕВОЗМОЖЕН: незавершённое $inprog — сначала доведи его (разреши конфликты, грепни маркеры, заверши) или отмени"
elif [ "$BR" = "main" ]; then
    echo "COMMIT: НЕВОЗМОЖЕН: ветка main принадлежит интегратору — работа идёт в своей ветке своего worktree"
elif [ "${MARKERS:-0}" -gt 0 ]; then
    echo "COMMIT: НЕВОЗМОЖЕН: маркеры конфликта в $MARKERS изменённых файлах"
    git diff --name-only HEAD | xargs -r grep -l -E '^(<<<<<<<|=======|>>>>>>>)' 2>/dev/null | head -5 | sed 's/^/    /'
elif [ "${MOD:-0}" -eq 0 ] && [ "${UNTR:-0}" -eq 0 ]; then
    echo "COMMIT: НЕЧЕГО: изменённых и новых файлов нет"
elif [ "${MOD:-0}" -eq 0 ]; then
    echo "COMMIT: ВОЗМОЖЕН только новых файлов — $UNTR неотслеживаемых; каждый — git add <имя> по решению (черновики «по слову» не берём)"
    printf '%s\n' "$STATUS" | grep '^??' | head -8 | sed 's/^/    /'
else
    echo "COMMIT: ВОЗМОЖЕН: изменённых $MOD, новых $UNTR — раздели по природе работы, коммит с --only -- <файлы>"
    printf '%s\n' "$STATUS" | head -12 | sed 's/^/    /'
    if [ "${STAGED:-0}" -gt 0 ]; then
        echo "    в индексе уже $STAGED файлов — проверь git diff --cached --stat: чужое в индексе общего дерева --only не возьмёт, но знать надо"
    fi
fi
TBD=$(grep -a -cP '^\| *(\xe2\x84\x96)?TBD *\|' docs/plans/221.1-bug-sweep.md 2>/dev/null || true)
[ "${TBD:-0}" -gt 0 ] && echo "    внимание: в реестре $TBD строк без номера — гейт формы красен, номер даёт интегратор до гейта"

# ---------------------------------------------------------------- PUSH
UP="origin/$BR"
if [ "$BR" = "main" ]; then
    echo "PUSH: НЕВОЗМОЖЕН: main пушит интегратор"
elif ! git rev-parse --verify -q "$UP" >/dev/null 2>&1; then
    echo "PUSH: НЕВОЗМОЖЕН: у ветки нет $UP — первый пуш руками: git push -u origin $BR"
else
    N=$(git rev-list --count "$UP"..HEAD 2>/dev/null || echo 0)
    if [ -n "$inprog" ]; then
        echo "PUSH: НЕВОЗМОЖЕН: незавершённое $inprog"
    elif [ "${MOD:-0}" -gt 0 ]; then
        echo "PUSH: НЕВОЗМОЖЕН: $MOD изменённых файлов не закоммичены — сначала коммит (или решение, что они не едут)"
    elif [ "$N" -eq 0 ]; then
        echo "PUSH: НЕЧЕГО: непушенных коммитов 0"
    else
        WANT=$(git config user.name || true)
        BAD=$(git log --format='%an' "$UP"..HEAD | sort -u | grep -v -x -F "${WANT:-__none__}" || true)
        if [ -n "$BAD" ]; then
            echo "PUSH: НЕВОЗМОЖЕН: среди $N непушенных коммитов чужой автор: $BAD (ожидается '$WANT')"
        else
            CHANGED=$(git diff --name-only "$UP"..HEAD)
            TN=$(printf '%s\n' "$CHANGED" | grep -c -E '^(novac/|scripts/gate-novac\.sh|scripts/guards/check-novac-|scripts/guards/novac-)' || true)
            LANG=$(printf '%s\n' "$CHANGED" | grep -c -E '^(compiler-codegen/src/|std/src/)' || true)
            SPEC=$(printf '%s\n' "$CHANGED" | grep -c -E '^spec/decisions/' || true)
            TIER=main; [ "${TN:-0}" -gt 0 ] && TIER="novac push + main loop"
            echo "PUSH: ВОЗМОЖЕН (после гейта: $TIER): непушенных $N, файлов $(printf '%s\n' "$CHANGED" | grep -c .) — bash scripts/tools/push-after-gate.sh"
            git log --format='    %h %s' "$UP"..HEAD | head -8 | cut -c1-110
            # Уместность: вердикт гейта обязан быть моложе HEAD, иначе он про другое дерево.
            HEAD_T=$(git log -1 --format=%ct)
            LASTLOG=$(ls -t target/gate-*.log target/push-*.log 2>/dev/null | head -1)
            if [ -n "$LASTLOG" ]; then
                LOG_T=$(stat -c %Y "$LASTLOG" 2>/dev/null || echo 0)
                V=$(grep -a -E 'GATE OK|GATE FAIL|NOVAC-GATE OK|NOVAC-GATE FAIL' "$LASTLOG" | tail -1 | cut -c1-60)
                if [ "${LOG_T:-0}" -lt "${HEAD_T:-0}" ]; then
                    echo "    гейт: последний вердикт ($LASTLOG: ${V:-нет}) СТАРШЕ HEAD — гнать заново, push-after-gate это сделает"
                else
                    echo "    гейт: последний вердикт ($LASTLOG: ${V:-нет}) моложе HEAD"
                fi
            else
                echo "    гейт: в этом дереве ещё не гоняли"
            fi
            PLAN_T=$(printf '%s\n' "$CHANGED" | grep -c -E '^docs/plans/' || true)
            CODE_T=$(printf '%s\n' "$CHANGED" | grep -c -E '^(novac/src/|compiler-codegen/src/|std/src/)' || true)
            if [ "${CODE_T:-0}" -gt 0 ] && [ "${PLAN_T:-0}" -eq 0 ]; then
                echo "    уместность: код изменён, план не тронут — если это шаг плана, его статус едет тем же коммитом (страж живой строки судит отставание)"
            fi
            if [ "${LANG:-0}" -gt 0 ] && [ "${SPEC:-0}" -eq 0 ]; then
                echo "    внимание: тронут компилятор/std без spec/decisions — если это меняет ЯЗЫК, пуш без D-амендмента запрещён; если не меняет — скажи это в докладе"
            fi
        fi
    fi
fi

# ---------------------------------------------------------------- SYNC
if timeout 60 git fetch --quiet origin 2>/dev/null; then
    FETCHED="свежий origin/main"
else
    FETCHED="БЕЗ СЕТИ — по последнему известному origin/main"
fi
if [ "$BR" = "main" ]; then
    echo "SYNC: НЕВОЗМОЖЕН: на main нечего синхронизировать, это ветка интегратора"
elif ! git rev-parse --verify -q origin/main >/dev/null 2>&1; then
    echo "SYNC: НЕВОЗМОЖЕН: origin/main не известен этому дереву"
else
    BEHIND=$(git rev-list --count HEAD..origin/main 2>/dev/null || echo '?')
    AHEAD=$(git rev-list --count origin/main..HEAD 2>/dev/null || echo '?')
    if [ -n "$inprog" ]; then
        echo "SYNC: НЕВОЗМОЖЕН: незавершённое $inprog ($FETCHED)"
    elif [ "${MOD:-0}" -gt 0 ]; then
        echo "SYNC: НЕВОЗМОЖЕН: $MOD изменённых файлов не закоммичены — синк только на чистом дереве ($FETCHED; отстаю на $BEHIND)"
    elif [ "$BEHIND" = "0" ]; then
        echo "SYNC: НЕ НУЖЕН: отставания от origin/main нет, впереди на $AHEAD ($FETCHED)"
    else
        ORACLE=$(git diff --name-only HEAD...origin/main 2>/dev/null | grep -c -E '^(compiler-codegen/|nova-cli/)' || true) # [3DOT-OK: files main added since the common base, not a branch judgement]
        echo "SYNC: НУЖЕН (отстаю на $BEHIND, впереди на $AHEAD; $FETCHED): git merge --no-ff origin/main, конфликты — вручную (реестр: объединение по номеру, main побеждает в общей строке), греп маркеров в одной команде с коммитом, коммит слияния с '# index-verified: merge'"
        git log --format='    %h %s' HEAD..origin/main | head -8 | cut -c1-110
        [ "${ORACLE:-0}" -gt 0 ] && echo "    после синка: main двигает источники оракула ($ORACLE файлов) — пересобрать nova-cli и оболочку novac перед гейтом"
        GUARDS=$(git diff --name-only HEAD...origin/main 2>/dev/null | grep -c -E '^(scripts/guards/|scripts/gate|spec/decisions/)' || true) # [3DOT-OK: files main added since the common base, not a branch judgement]
        if [ "${GUARDS:-0}" -gt 0 ]; then
            echo "    уместность: main двинул стражей/гейт/спеку ($GUARDS файлов) — локальный гейт судит не тем, чем CI; синк ДО пуша уместен"
        else
            echo "    уместность: стражи, гейт и спека в main не менялись — синк уместен перед новой волной, посреди волны не обязателен"
        fi
    fi
fi
exit 0
