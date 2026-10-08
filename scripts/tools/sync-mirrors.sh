#!/usr/bin/env bash
# scripts/tools/sync-mirrors.sh — догон зеркал gitverse и sourcecraft до последнего
# ЗЕЛЁНОГО origin/main. Без замка вливания.
#
# ЗАЧЕМ (задача #51, 2026-10-08): до неё `land-task.sh` пушил зеркала сам, под замком
# `crew_task merge`. Пуш зеркала после пуша main отказывает почти всегда: pre-push
# зовёт check-push-proven-by-ci, а тот судит НОВЕЙШИЙ прогон каждого workflow на хеше —
# пуш в main только что поставил на тот же хеш свежие прогоны, и они «still queued».
# Скрипт ждал CI на main ~15–20 мин, и всё это время замок держался зря: следующему
# вливанию нужен только сдвинутый origin/main, зеркала ему не нужны (#40, #45, #47,
# #48 за один день). Теперь `land-task.sh` печатает LANDED сразу после пуша origin/main,
# а зеркала догоняет этот скрипт — после отдачи замка.
#
# ЧТО ПУШИТСЯ — последний зелёный main, а не хеш одной задачи. Пока шёл CI, main мог
# уйти дальше (следующий приёмщик уже влил своё): пушить старый хеш — значит догонять
# прошлое. Кандидаты — origin/main и прежние вершины main (хеши прогонов CI ветки main),
# которые новее зеркала и лежат в истории origin/main. Из них берётся НОВЕЙШИЙ, кого
# признал check-push-proven-by-ci; судит не этот скрипт, а тот же страж, что и pre-push,
# и pre-push судит хеш ещё раз. Ничего не ослаблено: ни одного ключа обхода.
#
# ИДЕМПОТЕНТЕН: зеркало уже на цели — не трогается; повтор после любого кода безопасен
# (никакого --force: зеркало принимает только перемотку).
#
# ИСПОЛЬЗОВАНИЕ (из ЧИСТОГО дерева этой репы — дерева задачи до `cleaned`; главная
# копия с незакоммиченными правками интегратора не годится: pre-push
# check-tree-matches-push откажет):
#   bash scripts/tools/sync-mirrors.sh             # ждать до зелёного main и пушить
#   bash scripts/tools/sync-mirrors.sh --dry-run   # сказать, что было бы запушено
# Приёмщик — через crew_watch сразу после LANDED и отдачи замка:
#   crew_watch {command: "cd <дерево задачи> && bash scripts/tools/sync-mirrors.sh", minutes: 120}
# Ожидание: SYNC_MIRRORS_TIMEOUT (секунды, 5400), опрос — SYNC_MIRRORS_POLL (60).
#
# КОДЫ ВОЗВРАТА (последняя строка — MIRRORS-SYNCED / MIRRORS-FAIL):
#   0 — оба зеркала на вершине origin/main (или уже были);
#   2 — неверные аргументы / не дерево репы;
#   4 — CI на вершине origin/main КРАСНЫЙ: её не пушу; зеркала подтянуты до последнего
#       зелёного предка, если такой новее зеркала (в выводе — что где стоит);
#   6 — пуш зеркала отказан (pre-push или не перемотка) — вывод выше, разбор интегратору;
#   7 — зеркала не догнаны до вершины: CI на main не завершился за SYNC_MIRRORS_TIMEOUT
#       (подтянуты до последнего зелёного, если он есть) — запусти снова позже.
#       Это прежний смысл rc=7 `land-task.sh` («зеркало не догнано»), переехавший сюда:
#       land-task его больше не выдаёт.
#
# Ожидания печатают строку раз в минуту: сторож снимает окно за 10 минут тишины.

set -u
export LC_ALL=C

DRY=0
case "${1:-}" in
    --dry-run) DRY=1; shift ;;
    '') ;;
    *) echo "sync-mirrors: неизвестный аргумент '$1' (есть только --dry-run)" >&2
       echo "MIRRORS-FAIL rc=2"; exit 2 ;;
esac
say() { echo "sync-mirrors: $*"; }
die() { local rc=$1; shift; echo "sync-mirrors: $*" >&2; report; echo "MIRRORS-FAIL rc=$rc"; exit "$rc"; }
report() {
    [ -n "${ROOT:-}" ] || return 0
    for r in origin $MIRRORS; do
        printf 'sync-mirrors:   %-12s %s\n' "$r" "$(git -C "$ROOT" ls-remote "$r" refs/heads/main 2>/dev/null | cut -c1-9)"
    done
}

MIRRORS="gitverse sourcecraft"
ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || { ROOT=; die 2 "запускать из дерева репозитория nova"; }
PROOF="$ROOT/scripts/guards/check-push-proven-by-ci.py"
[ -f "$PROOF" ] || die 2 "нет $PROOF"
TIMEOUT=${SYNC_MIRRORS_TIMEOUT:-5400}
POLL=${SYNC_MIRRORS_POLL:-60}
REQ=7   # число обязательных workflow (REQUIRED в check-push-proven-by-ci.py)

# Состояние CI хеша по прогонам ВСЕХ веток (страж судит любой event): pending — есть
# незавершённые или прогонов меньше REQ (только что запушен); done — все завершены.
# Зелёный ли done — судит только страж.
ci_state() {
    local out n pend
    out=$(printf '%s\n' "$RUNS" | grep "^$1 " || true)
    n=$(printf '%s\n' "$out" | grep -c ' completed ' || true)
    pend=$(printf '%s\n' "$out" | grep -cE ' (queued|in_progress|waiting|pending|requested) ' || true)
    if [ "$pend" = 0 ] && [ "$n" -ge "$REQ" ]; then echo done; else echo pending; fi
}

# Зеркало r -> цель sha: уже там / перемотка / отказ.
push_mirror() {
    local r=$1 sha=$2 cur
    cur=$(git -C "$ROOT" ls-remote "$r" refs/heads/main | cut -f1)
    if [ "$cur" = "$sha" ]; then say "зеркало $r уже на ${sha:0:9}"; return 0; fi
    if [ -n "$cur" ] && git -C "$ROOT" cat-file -e "$cur^{commit}" 2>/dev/null \
       && git -C "$ROOT" merge-base --is-ancestor "$sha" "$cur"; then
        say "зеркало $r на ${cur:0:9} — уже не позади ${sha:0:9}, не трогаю"; return 0
    fi
    if [ $DRY = 1 ]; then say "пробный прогон: запушил бы $r ${cur:0:9} -> ${sha:0:9}"; return 0; fi
    if git -C "$ROOT" push -q "$r" "$sha:refs/heads/main"; then
        say "зеркало $r: ${cur:0:9} -> ${sha:0:9}"; return 0
    fi
    echo "sync-mirrors: пуш зеркала $r на ${sha:0:9} отказан (вывод выше)" >&2
    return 1
}

start=$(date +%s)
while :; do
    git -C "$ROOT" fetch -q origin || die 2 "fetch origin не прошёл"
    HEAD_SHA=$(git -C "$ROOT" rev-parse origin/main)
    RUNS=$(timeout 60 gh run list --limit 80 \
           --json workflowName,status,conclusion,headSha \
           --jq '.[]|"\(.headSha) \(.workflowName) \(.status) \(.conclusion)"' 2>/dev/null || true)
    # Прогоны ВСЕХ веток: страж судит любой event, так что прогоны integrate/t<N> —
    # свидетельство о том же хеше, а свежие прогоны main, пока идут, делают его pending.
    # Кандидаты: вершина origin/main и хеши прогонов в её истории, новее зеркал.
    CANDS=$( { echo "$HEAD_SHA"; printf '%s\n' "$RUNS" | cut -d' ' -f1; } | grep -E '^[0-9a-f]{40}$' | sort -u)
    BEHIND=""
    for r in $MIRRORS; do
        m=$(git -C "$ROOT" ls-remote "$r" refs/heads/main | cut -f1)
        BEHIND="$BEHIND ${m:-unknown}"   # зеркало не ответило — считаем его позади
    done
    ORDERED=""
    for c in $CANDS; do
        git -C "$ROOT" cat-file -e "$c^{commit}" 2>/dev/null || continue
        git -C "$ROOT" merge-base --is-ancestor "$c" "$HEAD_SHA" || continue
        newer=0
        for m in $BEHIND; do
            if ! git -C "$ROOT" cat-file -e "$m^{commit}" 2>/dev/null \
               || ! git -C "$ROOT" merge-base --is-ancestor "$c" "$m"; then newer=1; fi
        done
        [ $newer = 1 ] || continue
        ORDERED="$ORDERED$(git -C "$ROOT" rev-list --count "$c") $c
"
    done
    ORDERED=$(printf '%s' "$ORDERED" | sort -rn | cut -d' ' -f2)
    if [ -z "$ORDERED" ]; then
        say "зеркала не позади origin/main ${HEAD_SHA:0:9}"
        report; echo "MIRRORS-SYNCED main=${HEAD_SHA:0:9}"; exit 0
    fi

    # Новейший зелёный среди кандидатов; вершина красная — запоминаем.
    GREEN=""; HEAD_STATE=$(ci_state "$HEAD_SHA"); HEAD_RED=0
    for c in $ORDERED; do
        [ "$(ci_state "$c")" = done ] || continue
        if python "$PROOF" "$c" >/dev/null 2>&1; then GREEN=$c; break; fi
        [ "$c" = "$HEAD_SHA" ] && HEAD_RED=1
    done
    now=$(date +%s)
    say "origin/main ${HEAD_SHA:0:9}: CI $HEAD_STATE$( [ $HEAD_RED = 1 ] && echo ', КРАСНЫЙ'); последний зелёный новее зеркал: ${GREEN:0:9}${GREEN:+ }($(( (now-start)/60 )) мин)"

    if [ -n "$GREEN" ]; then
        fail=0
        for r in $MIRRORS; do push_mirror "$r" "$GREEN" || fail=1; done
        [ $fail = 0 ] || die 6 "зеркало не принимает ${GREEN:0:9} — сообщи интегратору"
        if [ "$GREEN" = "$HEAD_SHA" ]; then
            report; echo "MIRRORS-SYNCED main=${HEAD_SHA:0:9}$( [ $DRY = 1 ] && echo ' dry-run')"; exit 0
        fi
    fi
    if [ -n "$GREEN" ]; then WHERE="на последнем зелёном ${GREEN:0:9}"; else WHERE="не тронуты: зелёного новее них нет"; fi
    if [ $HEAD_RED = 1 ]; then
        python "$PROOF" "$HEAD_SHA" || true
        die 4 "CI на вершине origin/main ${HEAD_SHA:0:9} красный (вывод стража выше) — её не пушу; зеркала $WHERE"
    fi
    [ $((now-start)) -lt "$TIMEOUT" ] || die 7 "CI на main ${HEAD_SHA:0:9} не завершился за $((TIMEOUT/60)) мин; зеркала $WHERE — запусти снова позже"
    [ $DRY = 0 ] || { report; echo "MIRRORS-DRY main=${HEAD_SHA:0:9} green=${GREEN:0:9}"; exit 0; }
    sleep "$POLL"
done
