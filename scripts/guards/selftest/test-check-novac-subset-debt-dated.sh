#!/bin/sh
# Самотест check-novac-subset-debt-dated.py (П16). Швы: $2 — директория, $3 — база.
# Форма строки ok: два пробела, слово ok, пробелы — по ней считают случаи
# (check-novac-registry-counts.sh); двоеточие после ok делает случай невидимым.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-subset-debt-dated.py"
T="${TMPDIR:-/tmp}/subset-debt-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s\n" "$@" > "$d/m/m.nv"; echo "$d"; }

printf '%s\n' 'undated=0' > "$T/zero.baseline"
printf '%s\n' 'undated=1' > "$T/one.baseline"

# --- ГЛАВНЫЙ случай: новый долг БЕЗ этапа при базе ноль -------------------
D=$(mk undated "module a" \
    'const M = "outside the subset: an `if` in value position is not compiled yet"')
if run "$D" "$T/zero.baseline"; then
    bad "бессрочный долг прошёл при базе ноль - главный случай не ловится"
else
    grep -q "без этапа" "$T/err" && ok "бессрочный долг пойман" || bad "покраснел не тем текстом"
fi

# --- долг С этапом законен -------------------------------------------------
D=$(mk dated "module a" \
    'const M = "outside the subset: a declared local type is not compiled yet (E2-b3)"')
run "$D" "$T/zero.baseline" && ok "долг с этапом проходит" \
    || bad "долг с этапом покраснел - правило шире класса"

# --- этап в ДРУГОЙ форме тоже считается -----------------------------------
D=$(mk dated2 "module a" \
    'const M = "outside the subset: generic parameters are not compiled yet (E2)"')
run "$D" "$T/zero.baseline" && ok "короткая форма этапа (E2) принимается" \
    || bad "короткая форма этапа отвергнута"

# --- срок в форме ВОЛНЫ ПЛАНА 274.7 законен (решение владельца 2026-09-05) ----
D=$(mk wave "module a" \
    'const M = "outside the subset: an interpolation slot of a record is not compiled yet (274.7 B1b)"')
run "$D" "$T/zero.baseline" && ok "срок в форме волны 274.7 принимается" \
    || bad "срок в форме волны 274.7 отвергнут"

# --- а голая ссылка на план без волны — НЕ срок --------------------------------
D=$(mk bareplan "module a" \
    'const M = "outside the subset: something is not compiled yet (274.7)"')
if run "$D" "$T/zero.baseline"; then
    bad "голая ссылка на план сошла за срок - форма расширилась до любой ссылки"
else
    ok "голая ссылка на план без волны отвергнута"
fi

# --- ровно по базе: не хуже, чем было --------------------------------------
D=$(mk atbase "module a" \
    'const M = "outside the subset: string interpolation is not compiled yet"')
run "$D" "$T/one.baseline" && ok "долг ровно по базе проходит" \
    || bad "число, равное базе, покраснело - храповик судит строго больше"

# --- КОММЕНТАРИЙ с той же фразой законен -----------------------------------
D=$(mk comment "module a" \
    '// Раньше здесь стоял отказ "... is not compiled yet" без этапа.' \
    'fn f() -> () { }')
run "$D" "$T/zero.baseline" && ok "комментарий с историей класса проходит" \
    || bad "комментарий покраснел - страж стирает причину вместе с симптомом"

# --- отсутствие базы --------------------------------------------------------
run "$D" "$T/nosuch.baseline" && bad "отсутствие базы прошло - храповик без базы пуст" \
    || ok "отсутствие базы красное"

# --- потерянная мишень ------------------------------------------------------
mkdir -p "$T/empty"
run "$T/empty" "$T/zero.baseline" \
    && bad "пустая директория прошла - страж, сканирующий ничто, слеп" \
    || ok "пустая директория красная (мишень потеряна)"

[ "$fails" -eq 0 ] && echo "test-check-novac-subset-debt-dated: ok" || exit 1
