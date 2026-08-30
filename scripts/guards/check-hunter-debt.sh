#!/bin/sh
# scripts/guards/check-hunter-debt.sh — план 278 Ф.5 (регламент владельца
# 2026-08-30: «на автомате всё должно работать, а не по чуйке окна»).
#
# ЗАЧЕМ. Частота охоты не смеет держаться на памяти окна. Триггер — ДОЛГ ДЕРЕВА:
# сколько строк поверхности трека ДОБАВЛЕНО с последней охоты. Долг больше
# бюджета — гейт красный, пока охота не проведена и отчёт не закоммичен.
# Календарь отвергнут панелью 2026-08-30: часы не принадлежат дереву, а строки
# принадлежат; окно, не растящее поверхность, охотиться не обязано.
#
# ЧАСЫ НЕ ПИШУТСЯ РУКАМИ (дыра всех трёх критиков панели: рукописная дата
# гасит долг без охоты). Часы трека = коммит, который последним ДОБАВИЛ отчёт
# охоты (git log --diff-filter=A -- docs/dev/hunts/<трек>/20*.md):
#   - свёртка отчёта (удаление файла) часы НЕ двигает — diff-filter=A;
#   - незакоммиченный отчёт часы не двигает — сначала закоммить;
#   - бумажный отчёт не пройдёт check-hunter-mark.sh (пробы в дереве).
# Пока у трека нет ни одного отчёта, часы = якорь anchor=<коммит> из базы
# (засеян днём заведения механизма, ходит только с хроникой).
#
# ДОЛГ = сумма added-строк git diff --numstat <часы>..рабочее дерево по
# поверхности трека (тестовые файлы исключены — дыра критика №3: дописка
# тестов не добавляет территории для дефектов):
#   novac:  novac/src/*.nv без *_test.nv
#   oracle: compiler-codegen/src/*.rs
# Именно added, не net: рефакторный чурн рождает дефекты не хуже роста.
#
# БЮДЖЕТ — число ВЛАДЕЛЬЦА в scripts/guards/hunter-debt.baseline (не
# самоназначается; засев отмечен там как ожидающий слова владельца).
#
# Самотест: scripts/guards/selftest/test-check-hunter-debt.sh.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
BASE="${NOVA_HUNTER_DEBT_BASELINE:-$(cd "$(dirname "$0")" && pwd)/hunter-debt.baseline}"

if [ ! -f "$BASE" ]; then
    echo "check-hunter-debt: FAIL — нет базы $BASE (budget_novac=, budget_oracle=, anchor=)" >&2
    exit 1
fi
ANCHOR=$(grep -E '^anchor=' "$BASE" | tail -1 | cut -d= -f2)
if [ -z "$ANCHOR" ] || ! git -C "$ROOT" cat-file -e "$ANCHOR^{commit}" 2>/dev/null; then
    echo "check-hunter-debt: FAIL — якорь anchor= в базе пуст или не является коммитом этого дерева: «$ANCHOR»" >&2
    exit 1
fi

rc=0
SUMMARY=""
for TRACK in novac oracle; do
    BUDGET=$(grep -E "^budget_$TRACK=" "$BASE" | tail -1 | cut -d= -f2)
    if [ -z "$BUDGET" ]; then
        echo "check-hunter-debt: FAIL — в базе нет budget_$TRACK=" >&2
        rc=1
        continue
    fi
    case "$TRACK" in
        novac)  SURFACE="novac/src/*.nv";           EXCL='_test[.]nv$' ;;
        oracle) SURFACE="compiler-codegen/src/*.rs"; EXCL='/tests?/' ;;
    esac
    CLOCK=$(git -C "$ROOT" log --diff-filter=A -1 --format=%H -- "docs/dev/hunts/$TRACK/20*.md" 2>/dev/null)
    SRC="отчёт"
    if [ -z "$CLOCK" ]; then CLOCK="$ANCHOR"; SRC="якорь"; fi
    DEBT=$(git -C "$ROOT" diff --numstat "$CLOCK" -- "$SURFACE" 2>/dev/null \
        | awk -F'\t' -v excl="$EXCL" '$1 != "-" && $3 !~ excl {s += $1} END {print s+0}')
    if [ "$DEBT" -gt "$BUDGET" ]; then
        SHORT=$(git -C "$ROOT" rev-parse --short "$CLOCK" 2>/dev/null || echo "$CLOCK")
        echo "check-hunter-debt: FAIL — долг охоты трека $TRACK: добавлено $DEBT строк поверхности с последней охоты ($SRC $SHORT), бюджет $BUDGET." >&2
        echo "    Пусти охотника (.claude/agents/defect-hunter.md) по клетке этого трека и закоммить отчёт с пробами в docs/dev/hunts/$TRACK/ — часы двигает только ДОБАВЛЕННЫЙ отчёт." >&2
        rc=1
    fi
    SUMMARY="$SUMMARY $TRACK: долг $DEBT/$BUDGET ($SRC);"
done

if [ "$rc" -eq 0 ]; then
    echo "check-hunter-debt ok:$SUMMARY часы — из git, не из рук"
fi
exit "$rc"
