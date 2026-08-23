#!/usr/bin/env bash
# scripts/guards/check-novac-local-only-work.sh — работа, существующая РОВНО В
# ОДНОМ МЕСТЕ (класс «ветка умрёт вместе с диском»; замер 2026-08-23).
#
# ЗАЧЕМ. Вопрос владельца 2026-08-23 был про одну ветку: «как сделать, чтобы
# pchan244 случайно не удалили как мёртвую?». Замер ответил шире: копии на
# origin не имели 23 локальные ветки, и ДЕВЯТЬ из них несли невлитые коммиты —
# крупнейшая `p274-mangle-subst` с 217 коммитами от 08-19, по этому же плану.
# Ветка без удалённой копии живёт ровно на одном диске: одно `git branch -D`,
# одна переустановка, один отказ диска — и её нет. Причём выглядит она при этом
# как «мёртвая»: неслитая, старая, никому не мешает.
#
# ПРОВЕРЯЕТ: число локальных веток, у которых ОДНОВРЕМЕННО (1) есть коммиты,
# которых нет в `main`, и (2) нет двойника `origin/<имя>`. Это ХРАПОВИК: число
# не растёт. Опустить его — значит запушить ветку (или влить её), и оба ответа
# правильные.
# НЕ ПРОВЕРЯЕТ: свежесть двойника (ветка могла уехать вперёд после пуша —
# названная слепая зона: это уже вопрос синхронизации, а не потери); зеркала
# gitverse/sourcecraft (о них судит отдельный обход реестра реп).
#
# ПОЧЕМУ БЕЗ СЕТИ: судим по `refs/remotes/origin/*`, то есть по тому, что знает
# локальный git. Страж в гейте, который ходит в сеть, краснеет от плохого
# Wi-Fi — и его отключают первым.
#
# $1 — корень; $2 — override базы (шов самотеста).
set -u
export LC_ALL=C

NAME="check-novac-local-only-work"
ROOT="${1:-.}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
BASE_FILE="${2:-$ROOT/scripts/guards/novac-local-only.baseline}"

git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1 || {
    echo "$NAME ok: судить нечего (не git-дерево)"
    exit 0
}

# Точка отсчёта «влито ли» — main. Если её нет (мелкий клон CI), судить нечем.
if ! git -C "$ROOT" rev-parse --verify -q main >/dev/null 2>&1; then
    echo "$NAME ok: судить нечего (в этом клоне нет main — так выглядит CI)"
    exit 0
fi

BASE=0
[ -f "$BASE_FILE" ] && BASE=$(grep -vE '^#|^$' "$BASE_FILE" | head -1 | tr -dc '0-9')
[ -n "$BASE" ] || BASE=0

RISKY=""
for b in $(git -C "$ROOT" for-each-ref --format='%(refname:short)' refs/heads); do
    git -C "$ROOT" rev-parse --verify -q "refs/remotes/origin/$b" >/dev/null 2>&1 && continue
    AHEAD=$(git -C "$ROOT" rev-list --count "main..$b" 2>/dev/null || printf '0')
    [ "$AHEAD" -gt 0 ] 2>/dev/null || continue
    RISKY="$RISKY$AHEAD $b
"
done

CNT=$(printf '%s' "$RISKY" | grep -c . || true)

if [ "$CNT" -gt "$BASE" ]; then
    echo "$NAME: FAIL — веток с невлитой работой и БЕЗ копии на origin: $CNT при базе $BASE" >&2
    printf '%s' "$RISKY" | sort -rn | head -8 | sed 's|^|  коммитов невлито: |' >&2
    echo "  Такая ветка живёт ровно на одном диске и выглядит мёртвой: неслитая," >&2
    echo "  старая, никому не мешает. Замер 2026-08-23: девять таких, крупнейшая" >&2
    echo "  217 коммитов по действующему плану." >&2
    echo "  Лечится одним из двух, и оба правильные: git push origin <ветка> либо" >&2
    echo "  слияние. После этого опусти число в $BASE_FILE ТЕМ ЖЕ диффом." >&2
    exit 1
fi

if [ "$CNT" -lt "$BASE" ]; then
    echo "$NAME: FAIL — таких веток стало МЕНЬШЕ ($CNT при базе $BASE): опусти базу" >&2
    echo "  тем же диффом, иначе следующий рост до прежней цифры пройдёт молча." >&2
    exit 1
fi

echo "$NAME ok: веток с невлитой работой без копии на origin: $CNT (== база)"
exit 0
