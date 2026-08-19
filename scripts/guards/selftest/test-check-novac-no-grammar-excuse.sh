#!/bin/sh
# Самотест check-novac-no-grammar-excuse.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-no-grammar-excuse.py"
T="${TMPDIR:-/tmp}/novac-excuse-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s\n" "$@" > "$d/m/m.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай: отговорка в тексте отказа ----------------------------
D=$(mk excuse "module a" "fn f() -> () {" \
    '    @refuse(t, "outside the E1 subset: construct not in the MVP grammar")' "}")
if run "$D"; then
    bad "отговорка прошла — главный случай не ловится"
else
    grep -q "ссылается на незнание грамматики" "$T/err" && ok "отговорка поймана" \
        || bad "покраснел не тем текстом"
fi

# --- вариант формулировки ------------------------------------------------
D=$(mk excuse2 "module a" "fn f() -> () {" \
    '    @refuse(t, "unknown construct here")' "}")
run "$D" && bad "«unknown construct» прошло" || ok "«unknown construct» поймано"

# --- КОММЕНТАРИЙ с той же строкой законен --------------------------------
D=$(mk comment "module a" \
    '// The old text said "construct not in the MVP grammar" and that is why' \
    "// every form is parsed now." "fn f() -> () { }")
run "$D" && ok "комментарий с историей класса проходит" \
    || bad "комментарий покраснел — страж стирает причину вместе с симптомом"

# --- отказ, называющий форму, проходит -----------------------------------
D=$(mk named "module a" "fn f() -> () {" \
    '    @refuse(t, "outside the subset: a variadic parameter arrives with generics (E2-b)")' "}")
run "$D" && ok "отказ по имени формы проходит" || bad "именной отказ покраснел"

# --- потерянная мишень ---------------------------------------------------
mkdir -p "$T/empty"
run "$T/empty" && bad "пустая директория прошла — страж не заметил потери мишени" \
    || ok "пустая директория красная (мишень потеряна)"

[ "$fails" -eq 0 ] && echo "test-check-novac-no-grammar-excuse: ok" || exit 1
