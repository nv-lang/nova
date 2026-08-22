#!/bin/sh
# Самотест check-novac-precondition.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-precondition.py"
T="${TMPDIR:-/tmp}/novac-precond-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s
" "$@" > "$d/m/m.nv"; echo "$d"; }

D=$(mk good "module a" "" "fn f(i int) -> int" "    requires i >= 0, \"i must be non-negative\"" "{" "    i + 1" "}")
run "$D" && ok "предусловие в сигнатуре — зелёный" || bad "сигнатура покраснела: $(cat "$T/err")"

# --- ГЛАВНЫЙ случай: assert первой строкой тела --------------------------
D=$(mk hidden "module a" "" "fn f(i int) -> int {" "    assert(i >= 0, \"i must be non-negative\")" "    i + 1" "}")
if run "$D"; then
    bad "предусловие в теле прошло — главный случай не ловится"
else
    grep -q "первой строкой тела" "$T/err" && ok "спрятанное предусловие поймано" || bad "красный, но не про первую строку"
fi

# --- assert ГЛУБЖЕ в теле законен ----------------------------------------
D=$(mk deep "module a" "" "fn f(i int) -> int {" "    ro x = i * 2" "    assert(x >= 0, \"doubled stays non-negative\")" "    x" "}")
run "$D" && ok "assert над вычисленным (не первой строкой) законен" || bad "глубокий assert покраснел: $(cat "$T/err")"

# --- комментарий перед assert не прячет его ------------------------------
D=$(mk commented "module a" "" "fn f(i int) -> int {" "    // a note about the input" "    assert(i >= 0, \"i must be non-negative\")" "    i" "}")
if run "$D"; then
    bad "assert за комментарием прошёл — комментарий не делает его глубже"
else
    grep -q "первой строкой тела" "$T/err" && ok "комментарий не прячет предусловие" || bad "красный, но не про то"
fi

D="$T/nofiles"; mkdir -p "$D/m"
if run "$D"; then bad "дерево без .nv прошло"; else grep -q "мишень" "$T/err" && ok "нет .nv — красный (класс №519)" || bad "красный, но не про мишень"; fi
run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-precondition ok: все случаи, включая скрытое предусловие и законный глубокий assert"
    exit 0
fi
exit 1
