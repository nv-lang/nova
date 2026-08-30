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
# ЧАСЫ НЕ ПИШУТСЯ РУКАМИ И НЕ ВОСКРЕШАЮТСЯ. Часы трека = коммит, который
# добавил отчёт охоты, — но годится не всякое добавление (все три дыры найдены
# адверсариальной панелью 2026-08-30 ЗАПУСКОМ, каждая гасила долг за секунды):
#   - свёртка (удаление файла) часы не двигает — --diff-filter=A;
#   - `git rm` + возврат того же файла даёт ВТОРОЕ добавление того же пути:
#     путь, добавлявшийся больше одного раза, часами быть не может;
#   - копия прежнего отчёта под новым именем даёт новый путь, но ТОТ ЖЕ blob:
#     содержимое, уже добавлявшееся на этом треке, часами быть не может.
# Годных кандидатов ищем от новых к старым; не нашли — часы = anchor= из базы
# (засеян днём заведения механизма, ходит только с хроникой).
# Пустой отчёт барьером проб отсекает check-hunter-mark.sh — здесь про ЧАСЫ.
#
# ДОЛГ = сумма added-строк git diff --numstat <часы>..рабочее дерево по
# поверхности трека (тестовые файлы исключены — дописка тестов не добавляет
# территории для дефектов):
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
# Ключ читается СТРОГО один раз. Дыра, найденная панелью запуском: при `tail -1`
# дописанная в конец строка `budget_novac=999999` побеждала, а исходная
# оставалась в файле на виду — раскрутка бюджета без правки числа владельца.
key1() {
    _k="$1"
    _n=$(grep -cE "^$_k=" "$BASE" || true)
    if [ "${_n:-0}" -ne 1 ]; then
        echo "check-hunter-debt: FAIL — в базе $BASE ключ $_k= встречается $_n раз(а), должен ровно один: дописка второй строки — способ раскрутить число владельца молча" >&2
        return 1
    fi
    grep -E "^$_k=" "$BASE" | cut -d= -f2
}
ANCHOR=$(key1 anchor) || exit 1
if [ -z "$ANCHOR" ] || ! git -C "$ROOT" cat-file -e "$ANCHOR^{commit}" 2>/dev/null; then
    echo "check-hunter-debt: FAIL — якорь anchor= в базе пуст или не является коммитом этого дерева: «$ANCHOR»" >&2
    exit 1
fi

# Часы трека: новейшее ГОДНОЕ добавление отчёта. Печатает «<commit> <path>»
# или ничего. Годное = путь добавлялся ровно один раз И его содержимое на
# момент добавления не совпадает с содержимым отчёта, добавленного раньше.
find_clock() {
    _track="$1"
    # ВЕСЬ каталог трека, а фильтр имени — регэкспом ЗДЕСЬ, не глобом git:
    # в git-pathspec «*» ПЕРЕСЕКАЕТ «/», и панель обошла триггер одним мусорным
    # файлом hunts/novac/2026-06-06-junk/x.md — он попадал под «20*.md», а
    # mark-страж его не видел (его glob нерекурсивен).
    _all=$(git -C "$ROOT" log --diff-filter=A --format="C %H" --name-only \
           -- "docs/dev/hunts/$_track/" 2>/dev/null)
    [ -n "$_all" ] || return 0
    # список «commit<TAB>path» от новых к старым; путь — ровно отчёт трека
    _pairs=$(printf '%s\n' "$_all" | awk -v pfx="docs/dev/hunts/$_track/" '
        /^C /  { c = $2; next }
        index($0, pfx) == 1 {
            rest = substr($0, length(pfx) + 1)
            if (rest ~ /^20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]-[A-Za-z0-9._-]+\.md$/)
                print c "\t" $0
        }')
    [ -n "$_pairs" ] || return 0
    printf '%s\n' "$_pairs" > "$TMPP"
    while IFS='	' read -r _c _p; do
        [ -n "$_p" ] || continue
        # путь добавлялся больше одного раза — воскрешение, не охота
        _adds=$(awk -F'\t' -v p="$_p" '$2 == p' "$TMPP" | grep -c . || true)
        [ "${_adds:-0}" -eq 1 ] || continue
        # содержимое уже добавлялось на этом треке (копия) — не охота
        _blob=$(git -C "$ROOT" rev-parse "$_c:$_p" 2>/dev/null) || continue
        : > "$TMPDUP"
        awk -F'\t' -v p="$_p" '$2 != p' "$TMPP" | while IFS='	' read -r _oc _op; do
            [ -n "$_op" ] || continue
            _ob=$(git -C "$ROOT" rev-parse "$_oc:$_op" 2>/dev/null) || continue
            [ "$_ob" = "$_blob" ] && echo dup >> "$TMPDUP"
        done
        [ -s "$TMPDUP" ] && continue
        echo "$_c $_p"
        return 0
    done < "$TMPP"
}

rc=0
SUMMARY=""
TMPDUP="${TMPDIR:-/tmp}/hunter-debt-dup.$$"
TMPP="${TMPDIR:-/tmp}/hunter-debt-pairs.$$"
trap 'rm -f "$TMPDUP" "$TMPP"' 0 2 15
for TRACK in novac oracle; do
    BUDGET=$(key1 "budget_$TRACK") || { rc=1; continue; }
    case "$TRACK" in
        novac)  SURFACE="novac/src/*.nv";           EXCL='_test[.]nv$' ;;
        oracle) SURFACE="compiler-codegen/src/*.rs"; EXCL='/tests?/' ;;
    esac
    CLOCK=$(find_clock "$TRACK" | head -1 | cut -d' ' -f1)
    SRC="отчёт"
    if [ -z "$CLOCK" ]; then CLOCK="$ANCHOR"; SRC="якорь"; fi
    DEBT=$(git -C "$ROOT" diff --numstat "$CLOCK" -- "$SURFACE" 2>/dev/null \
        | awk -F'\t' -v excl="$EXCL" '$1 != "-" && $3 !~ excl {s += $1} END {print s+0}')
    if [ "$DEBT" -gt "$BUDGET" ]; then
        SHORT=$(git -C "$ROOT" rev-parse --short "$CLOCK" 2>/dev/null || echo "$CLOCK")
        echo "check-hunter-debt: FAIL — долг охоты трека $TRACK: добавлено $DEBT строк поверхности с последней охоты ($SRC $SHORT), бюджет $BUDGET." >&2
        echo "    Пусти охотника (.claude/agents/defect-hunter.md) по клетке этого трека и закоммить НОВЫЙ отчёт с непустыми пробами в docs/dev/hunts/$TRACK/ — копия прежнего отчёта и возврат удалённого часами не считаются." >&2
        rc=1
    fi
    SUMMARY="$SUMMARY $TRACK: долг $DEBT/$BUDGET ($SRC);"
done

if [ "$rc" -eq 0 ]; then
    echo "check-hunter-debt ok:$SUMMARY часы — из git, не из рук, и не воскрешаются"
fi
exit "$rc"
