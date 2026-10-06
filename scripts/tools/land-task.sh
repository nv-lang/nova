#!/usr/bin/env bash
# scripts/tools/land-task.sh — механика вливания принятой задачи в main ОДНИМ
# скриптом: кандидат -> integrate/t<N> -> CI -> перемотка main -> три зеркала ->
# уборка integrate/t<N>.
#
# ЗАЧЕМ (слово владельца 2026-10-06): «механику вливания вернуть приёмщикам
# законным путём». До этого шаги 3–6 пути приёмщика (`/integrator`, «Путь
# приёмщика») делались отдельными командами, и классификатор прав Claude Code в
# сессиях приёмщиков отклонял их по одной («Modify Shared Resources», «CI
# Bypass»), поэтому механику с 2026-10-06 00:43 делал интегратор. Один скрипт с
# узким разрешением в `.claude/settings.json` судится один раз и целиком, а его
# шаги — те же, что делал интегратор руками, без единого обхода.
#
# ЧЕГО ОН НЕ ДЕЛАЕТ — и это намеренно:
#   * не строит коммит слияния. Кандидат = вершина ветки задачи, в которую уже
#     влит свежий origin/main (`git -C <дерево> merge --no-ff origin/main` в дереве
#     задачи, под хуками коммита и pre-merge-commit). Слияние без хуков
#     (`merge-tree` + `commit-tree`) обошло бы стражей коммита — поэтому его нет:
#     main не предок вершины -> отказ с подсказкой, код 3.
#   * не обходит CI: никаких NOVA_SKIP_CI_CHECK / NOVA_PUSH_UNPROVEN; main
#     двигается только на хеш, который check-push-proven-by-ci признал, а pre-push
#     судит его ещё раз.
#   * не берёт замок вливания: его держит вызывающий (`crew_task merge {n}`) —
#     у скрипта нет доступа к плагину, и проверить замок он не может.
#
# ИСПОЛЬЗОВАНИЕ (из любого дерева этой репы; приёмщик — через crew_watch под
# замком `crew_task merge`):
#   bash scripts/tools/land-task.sh <N> <хеш вершины ветки задачи>
#   bash scripts/tools/land-task.sh --dry-run <N> <хеш>   # только проверки, без пушей
#
# КОДЫ ВОЗВРАТА (последняя строка вывода — LANDED / LAND-FAIL с шагом):
#   0 — влито: origin/main == хеш, зеркала догнаны, integrate/t<N> снята;
#   2 — неверные аргументы; 3 — хеш не содержит свежий origin/main (влей main в
#   ветку задачи и запусти снова); 4 — CI красный или не дождались; 5 — main
#   сдвинулся, пока шёл CI (влей свежий main, запусти снова); 6 — пуш origin
#   отказан; 7 — зеркало не догнано (origin уже влит — догонит интегратор);
#   8 — вливает не приёмщик этой задачи и не интегратор (шаг 0).
#
# Ожидания печатают строку раз в минуту: сторож снимает окно за 10 минут тишины.

set -u
export LC_ALL=C

DRY=0
if [ "${1:-}" = "--dry-run" ]; then DRY=1; shift; fi
N=${1:-}
SHA_IN=${2:-}
say() { echo "land-task: $*"; }
die() { local rc=$1; shift; echo "land-task: $*" >&2; echo "LAND-FAIL rc=$rc task=#${N:-?}"; exit "$rc"; }

case "$N" in ''|*[!0-9]*) die 2 "первый аргумент — номер задачи (число), получено '$N'";; esac
[ -n "$SHA_IN" ] || die 2 "второй аргумент — хеш вершины ветки задачи"

# Шаг 0. КТО ВЛИВАЕТ (роль приёмщика, слово владельца 2026-10-06: «новая роль со своими
# правами приёмщика, и пусть вливает»). crew_watch запускает команду в сервере OpenCode
# мимо прав окна и с crew-harness плана 013 кладёт в окружение, кто её поставил:
# CREW_ROLE, CREW_REVIEW_N. Вливает приёмщик ЭТОЙ задачи (CREW_REVIEW_N == N) или
# интегратор; прочим — отказ, код 8. Вызов из вкладки OpenCode мимо crew_watch
# (OPENCODE_SESSION_ID есть, CREW_ROLE нет) — только пробный: роль там не видна.
# Ни того ни другого — запуск человеком из своего терминала, роль не судится.
if [ -n "${CREW_ROLE:-}" ]; then
    if [ "$CREW_ROLE" = integrator ]; then
        say "роль: интегратор (${CREW_SESSION_ID:-?})"
    elif [ "${CREW_REVIEW_N:-}" = "$N" ]; then
        say "роль: приёмщик задачи #$N ($CREW_ROLE, ${CREW_SESSION_ID:-?})"
    else
        die 8 "вливает приёмщик задачи #$N или интегратор; поставивший команду — роль $CREW_ROLE, приёмка #${CREW_REVIEW_N:-нет} (${CREW_SESSION_ID:-?})"
    fi
elif [ -n "${OPENCODE_SESSION_ID:-}" ] && [ "$DRY" != 1 ]; then
    die 8 "из вкладки OpenCode вливание идёт через crew_watch (там видна роль); здесь можно только --dry-run"
else
    [ "$DRY" = 1 ] || say "роль не судится: запуск вне crew_watch и вне вкладки OpenCode (человек)"
fi

ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || die 2 "запускать из дерева репозитория nova"
COMMON=$(cd "$ROOT" && cd "$(git rev-parse --git-common-dir)" && pwd)
MAIN_COPY=$(dirname "$COMMON")
# ПРИЁМЩИК ВЛИВАЕТ ИЗ ДЕРЕВА ЗАДАЧИ, а не из главной копии (2026-10-06, посадка #24):
# crew_watch запускает команду в каталоге вкладки, у приёмщика это главная копия, где
# живёт интегратор со своими незакоммиченными правками, — pre-push check-tree-matches-push
# честно отказал, и обойти его было нечем, кроме запрещённого приёмщику флага. Дерево
# задачи чистое и содержит кандидата; главную копию скрипт после пуша догоняет сам.
if [ -n "${CREW_REVIEW_N:-}" ] && [ "$(cd "$ROOT" && pwd -P)" = "$(cd "$MAIN_COPY" && pwd -P)" ]; then
    die 2 "приёмщик вливает из дерева задачи: в команде crew_watch — cd <дерево задачи #$N> && bash scripts/tools/land-task.sh $N <хеш>"
fi
SHA=$(git -C "$ROOT" rev-parse --verify -q "$SHA_IN^{commit}") || die 2 "'$SHA_IN' — не коммит"
SHA9=${SHA:0:9}
CAND="integrate/t$N"
PROOF="$ROOT/scripts/guards/check-push-proven-by-ci.py"
CI_TIMEOUT=${LAND_CI_TIMEOUT:-5400}

say "задача #$N, вершина $SHA9, кандидат $CAND, главная копия $MAIN_COPY$( [ $DRY = 1 ] && echo ', ПРОБНЫЙ ПРОГОН')"

# Шаг 1. Кандидат от свежего main: вершина обязана содержать origin/main.
git -C "$ROOT" fetch -q origin || die 4 "fetch origin не прошёл"
MAIN_NOW=$(git -C "$ROOT" rev-parse origin/main)
if [ "$MAIN_NOW" = "$SHA" ]; then
    die 3 "$SHA9 уже и есть origin/main — вливать нечего"
fi
if ! git -C "$ROOT" merge-base --is-ancestor "$MAIN_NOW" "$SHA"; then
    die 3 "вершина $SHA9 не содержит свежий origin/main ${MAIN_NOW:0:9}: в дереве задачи \`git merge --no-ff origin/main\` (гейты, пуш ветки) и запусти снова с новым хешем"
fi
say "шаг 1 ok: $SHA9 содержит origin/main ${MAIN_NOW:0:9}"

if [ $DRY = 1 ]; then
    say "пробный прогон: дальше были бы пуш $CAND, ожидание CI, перемотка main, три зеркала, снятие $CAND"
    echo "LAND-DRY-OK task=#$N sha=$SHA9"
    exit 0
fi

# Ожидание CI на ветке для хеша: все прогоны хеша завершены и их не меньше 7
# (число обязательных workflow). Судит не этот цикл, а check-push-proven-by-ci.
wait_ci() {
    local branch=$1 start now out n pend
    start=$(date +%s)
    while :; do
        out=$(timeout 60 gh run list --branch "$branch" --limit 40 \
              --json workflowName,status,conclusion,headSha \
              --jq '.[]|"\(.headSha) \(.workflowName) \(.status) \(.conclusion)"' 2>/dev/null \
              | grep "^$SHA " || true)
        n=$(printf '%s\n' "$out" | grep -c ' completed ' || true)
        pend=$(printf '%s\n' "$out" | grep -cE ' (queued|in_progress|waiting|pending|requested) ' || true)
        now=$(date +%s)
        say "CI $branch $SHA9: завершено $n, идёт $pend ($(( (now-start)/60 )) мин)"
        if [ -n "$out" ] && [ "$pend" = 0 ] && [ "$n" -ge 7 ]; then return 0; fi
        [ $((now-start)) -lt "$CI_TIMEOUT" ] || return 1
        sleep 60
    done
}

# Шаг 2. Кандидат в integrate/t<N>. Стоит там другой хеш (прошлый круг) — снять:
# --force не нужен и не применяется.
OLD=$(git -C "$ROOT" ls-remote origin "refs/heads/$CAND" | cut -f1)
if [ -n "$OLD" ] && [ "$OLD" != "$SHA" ]; then
    git -C "$ROOT" push -q origin --delete "$CAND" || die 6 "не снялась прежняя $CAND (${OLD:0:9})"
    say "прежняя $CAND (${OLD:0:9}) снята"
fi
if [ "$OLD" != "$SHA" ]; then
    git -C "$ROOT" push -q origin "$SHA:refs/heads/$CAND" || die 6 "пуш $CAND не прошёл"
fi
say "шаг 2 ok: $CAND = $SHA9"

# Шаг 3. CI на кандидате и его доказательство полным хешем.
wait_ci "$CAND" || die 4 "CI на $CAND не завершился за $((CI_TIMEOUT/60)) мин"
if ! python "$PROOF" "$SHA"; then
    die 4 "CI на $SHA9 не доказан (вывод выше); $CAND оставлена для разбора"
fi
say "шаг 3 ok: CI доказал $SHA9"

# Шаг 4. main не сдвинулся, пока шёл CI.
git -C "$ROOT" fetch -q origin || die 5 "fetch origin не прошёл"
MAIN_NOW=$(git -C "$ROOT" rev-parse origin/main)
if ! git -C "$ROOT" merge-base --is-ancestor "$MAIN_NOW" "$SHA"; then
    die 5 "main сдвинулся на ${MAIN_NOW:0:9}, пока шёл CI: влей его в ветку задачи и запусти снова"
fi

# Шаг 5. Перемотка main на origin (сервер примет только перемотку; pre-push
# судит хеш ещё раз), затем — главной копии, если она чистая и стоит на main.
git -C "$ROOT" push origin "$SHA:refs/heads/main" || die 6 "пуш main на origin отказан (вывод выше)"
say "шаг 5 ok: origin/main = $SHA9"
if [ "$(git -C "$MAIN_COPY" rev-parse --abbrev-ref HEAD 2>/dev/null)" = "main" ] \
   && [ -z "$(git -C "$MAIN_COPY" status --porcelain --untracked-files=no 2>/dev/null)" ]; then
    if git -C "$MAIN_COPY" merge -q --ff-only "$SHA" 2>/dev/null; then
        say "главная копия перемотана на $SHA9"
    else
        say "главная копия не перематывается (там свои коммиты) — догонит интегратор"
    fi
else
    say "главная копия не на main или грязная — не трогаю, догонит интегратор"
fi

# Шаг 6. Зеркала. pre-push судит и их; пока на main стоят свежие прогоны того же
# хеша, он может отказать «run still queued» — тогда дождаться их и повторить.
mirror_fail=0
for r in gitverse sourcecraft; do
    if git -C "$ROOT" push -q "$r" "$SHA:refs/heads/main" 2>/dev/null; then
        say "зеркало $r ok"
        continue
    fi
    say "зеркало $r отказало — жду CI на main для $SHA9 и повторяю"
    if wait_ci main && git -C "$ROOT" push -q "$r" "$SHA:refs/heads/main"; then
        say "зеркало $r ok (после CI на main)"
    else
        echo "land-task: зеркало $r НЕ догнано" >&2
        mirror_fail=1
    fi
done

# Шаг 7. Уборка кандидата.
git -C "$ROOT" push -q origin --delete "$CAND" 2>/dev/null && say "шаг 7 ok: $CAND снята"

for r in origin gitverse sourcecraft; do
    printf 'land-task:   %-12s %s\n' "$r" "$(git -C "$ROOT" ls-remote "$r" refs/heads/main | cut -c1-9)"
done
[ $mirror_fail = 0 ] || die 7 "origin влит, но зеркало не догнано — сообщи интегратору"
echo "LANDED task=#$N main=$SHA9"
