#!/usr/bin/env bash
# scripts/guards/check-branch-absorption-method.sh
# Поглощение ветки сверяется предком и patch-id, а НЕ трёхточечным диффом.
#
# ДОМ И ОСНОВАНИЕ: реестр 221.1 №629 (доработка 2026-08-13), план 231 трек Д
# (машинное принуждение норм).
#
# ЗАЧЕМ — случай 2026-08-13, дословно. Вопрос «вошла ли работа ветки в main»
# я решал трёхточечным диффом `main...ветка`. У ветки `p-fix-n38-workertls`
# несколько баз слияния; git берёт одну ПРОИЗВОЛЬНО и сообщает об этом строкой
# в stderr, которую никто не читает: `multiple merge bases, using db6dd4f71`.
# С древней базы давно влитый код выглядит ОТСУТСТВУЮЩИМ — и ветка ДВАЖДЫ
# пережила разбор с вердиктом «настоящая работа, удалять на догадке нельзя»,
# хотя её фикс лежал в main с июля. Осторожность была верной, инструмент — нет.
#
# ПОЧЕМУ ПРАВИЛА МАЛО. Ошибка не в невнимательности: `A...B` — общепринятая
# форма для «что нового в B», и рука тянется к ней сама. Пока в проекте нет
# ОДНОЙ ДВЕРИ с правильным ответом, каждый следующий соберёт свою команду
# заново и наступит туда же.
#
# ЧТО ПРОВЕРЯЕТСЯ:
#   1. Ни один скрипт/док не судит о ветках трёхточечной формой `git diff|log|
#      rev-list … A...B` без пометки `[3DOT-OK: причина]` В ТОЙ ЖЕ СТРОКЕ.
#      Именно в той же, а не в комментарии сверху: пометка обязана быть видна
#      тому, кто читает строку, а не тому, кто прочёл абзац над ней. Длинное
#      объяснение при этом никто не запрещает — оно и пишется выше, а в строке
#      остаётся короткая ссылка на него.
#   2. Дверь существует: `scripts/tools/branch-absorbed.sh` на месте.
#   3. Стражи, судящие о слиянии веток, опираются на ПРЕДКОВЫЙ тест
#      (`--merged` / `merge-base --is-ancestor`), а не на дифф. Это защита от
#      «упрощения» их задним числом до сравнения деревьев.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): трёхточечный дифф, набранный человеком в
# терминале. Ни один страж не видит того, что не записано в файл. Он держит
# ПИСЬМЕННЫЕ следы приёма и наличие правильной двери — на большее не
# претендует.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-branch-absorption-method.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-branch-absorption-method.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
FAILED=0
say_fail() { echo "check-branch-absorption-method: $1" >&2; FAILED=1; }

SELF_NAME="check-branch-absorption-method"

# ── 1. Трёхточечная форма в письменных следах ────────────────────────────────
# Ищем ровно ТРИ точки между ссылками, а не многоточие в прозе: две точки
# (`A..B`) — законная форма и под запрет не попадает.
SCAN_DIRS=""
[ -d "$ROOT/scripts" ]  && SCAN_DIRS="$SCAN_DIRS $ROOT/scripts"
[ -d "$ROOT/docs/dev" ] && SCAN_DIRS="$SCAN_DIRS $ROOT/docs/dev"

if [ -n "$SCAN_DIRS" ]; then
    # shellcheck disable=SC2086
    HITS=$(grep -rnE 'git +(diff|log|rev-list|shortlog)[^#]*[A-Za-z0-9_"$}/-]\.\.\.[A-Za-z0-9_"$/-]' \
             --include='*.sh' --include='*.py' --include='*.md' \
             $SCAN_DIRS 2>/dev/null \
           | grep -v "$SELF_NAME" \
           | grep -v '3DOT-OK' || true)
    if [ -n "$HITS" ]; then
        while IFS= read -r line; do
            [ -n "$line" ] || continue
            say_fail "трёхточечная форма без пометки: ${line%%:*}:$(echo "$line" | cut -d: -f2)"
        done <<EOF
$HITS
EOF
    fi
fi

# ── 2. Дверь на месте ────────────────────────────────────────────────────────
DOOR="$ROOT/scripts/tools/branch-absorbed.sh"
if [ ! -f "$DOOR" ]; then
    say_fail "нет scripts/tools/branch-absorbed.sh — единственной двери к вопросу «вошла ли работа ветки»"
elif ! grep -q 'merge-base --is-ancestor' "$DOOR" 2>/dev/null; then
    say_fail "branch-absorbed.sh не опирается на предковый тест — он и был бы тогда тем же диффом под другим именем"
fi

# ── 3. Стражи о слиянии судят предком ────────────────────────────────────────
for g in check-accepted-branch-merged.sh check-no-accumulation.sh; do
    f="$ROOT/scripts/guards/$g"
    [ -f "$f" ] || continue
    # `--no-merged` — тот же предковый тест, только с отрицанием (так спрашивает
    # check-no-accumulation). Первая редакция образца требовала строго
    # `--merged` и объявила его нарушителем — ложняк, пойманный первым же
    # прогоном на настоящем дереве.
    grep -qE 'merge-base --is-ancestor|--(no-)?merged' "$f" 2>/dev/null \
        || say_fail "$g судит о слиянии веток, но предкового теста (--merged / merge-base --is-ancestor) в нём нет"
done

if [ "$FAILED" -ne 0 ]; then
    echo "" >&2
    echo '    A...B — это «изменения B от ОБЩЕГО ПРЕДКА», и предок берётся ОДИН.' >&2
    echo "    При нескольких базах слияния git выбирает произвольную и говорит об" >&2
    echo "    этом строкой в stderr, которую никто не читает. Давно влитый код" >&2
    echo "    выглядит тогда отсутствующим (реестр 221.1 №629)." >&2
    echo "    Спрашивай так:  bash scripts/tools/branch-absorbed.sh <ветка> [база]" >&2
    echo "    Форма нужна намеренно — пометь её рядом: [3DOT-OK: причина]." >&2
    echo "check-branch-absorption-method: FAIL" >&2
    exit 1
fi

echo "check-branch-absorption-method ok: о поглощении ветки судят предком, не диффом"
exit 0
