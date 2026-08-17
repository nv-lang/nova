#!/bin/sh
# Самотест check-novac-table-is-match.sh (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-table-is-match.sh"
T="${TMPDIR:-/tmp}/novac-table-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s
" "$@" > "$d/m/m.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай: три подряд -----------------------------------------
D=$(mk table "module a" "fn f(k Kind) -> int {"     "    if k == Kind.A { return 1 }"     "    if k == Kind.B { return 2 }"     "    if k == Kind.C { return 3 }"     "    0" "}")
if run "$D"; then
    bad "цепочка из трёх прошла — главный случай не ловится"
else
    grep -q "это таблица" "$T/err" && ok "таблица цепочкой поймана" || bad "красный, но не про таблицу"
fi

# --- две ветки ещё читаются как условие ----------------------------------
D=$(mk two "module a" "fn f(k Kind) -> int {"     "    if k == Kind.A { return 1 }"     "    if k == Kind.B { return 2 }"     "    0" "}")
run "$D" && ok "две ветки не считаются таблицей" || bad "две ветки покраснели: $(cat "$T/err")"

# --- разные переменные — не цепочка --------------------------------------
D=$(mk mixed "module a" "fn f(k Kind, m Kind) -> int {"     "    if k == Kind.A { return 1 }"     "    if m == Kind.B { return 2 }"     "    if k == Kind.C { return 3 }"     "    0" "}")
run "$D" && ok "разные переменные не сливаются в одну таблицу" || bad "смешанные покраснели: $(cat "$T/err")"

# --- match — законная форма ----------------------------------------------
D=$(mk asmatch "module a" "fn f(k Kind) -> int {" "    match k {"     "        A => 1" "        B => 2" "        C => 3" "        _ => 0" "    }" "}")
run "$D" && ok "match не судится" || bad "match покраснел: $(cat "$T/err")"

D="$T/nofiles"; mkdir -p "$D/m"
if run "$D"; then bad "дерево без .nv прошло"; else grep -q "мишень" "$T/err" && ok "нет .nv — красный (класс №519)" || bad "красный, но не про мишень"; fi
run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-table-is-match ok: все случаи, включая порог в три ветки и разные переменные"
    exit 0
fi
exit 1
