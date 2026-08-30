#!/bin/sh
# Самотест check-novac-closer-mandatory.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-closer-mandatory.py"
T="${TMPDIR:-/tmp}/novac-closer-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s\n" "$@" > "$d/m/m.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай: закрывающая фигурная взята необязательной дверью -----
D=$(mk brace "module a" "fn Cursor mut @block() -> Node {" \
    "    @push_if(kids, TokenKind.RBrace)" "}")
if run "$D"; then
    bad "необязательная дверь на закрывающей фигурной прошла - главный случай не ловится"
else
    grep -q "RBrace" "$T/err" && ok "закрывающая фигурная через push_if поймана" \
        || bad "покраснел не тем текстом"
fi

# --- те же грабли на круглой и квадратной ---------------------------------
D=$(mk paren "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.RParen)" "}")
run "$D" && bad "закрывающая круглая через push_if прошла" \
    || ok "закрывающая круглая через push_if поймана"

D=$(mk bracket "module a" "fn f() -> () {" "    @push_if(ik, TokenKind.RBracket)" "}")
run "$D" && bad "закрывающая квадратная через push_if прошла" \
    || ok "закрывающая квадратная через push_if поймана"

# --- ОБЯЗАТЕЛЬНАЯ дверь проходит ------------------------------------------
D=$(mk closer "module a" "fn Cursor mut @block() -> Node {" \
    "    @push_closer(kids, TokenKind.RBrace)" "}")
run "$D" && ok "push_closer проходит" || bad "обязательная дверь покраснела"

# --- запятая необязательна и остаётся законной ----------------------------
D=$(mk comma "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.Comma)" "}")
run "$D" && ok "запятая через push_if проходит (она правда необязательна)" \
    || bad "страж съел запятую - правило шире класса"

# --- КОММЕНТАРИЙ с той же строкой законен ---------------------------------
D=$(mk comment "module a" \
    "// До #809 здесь стояло @push_if(kids, TokenKind.RBrace) и файл без" \
    "// закрывающей скобки принимался молча." "fn f() -> () { }")
run "$D" && ok "комментарий с историей класса проходит" \
    || bad "комментарий покраснел - страж стирает причину вместе с симптомом"

# --- потерянная мишень ----------------------------------------------------
mkdir -p "$T/empty"
run "$T/empty" && bad "пустая директория прошла - страж не заметил потери мишени" \
    || ok "пустая директория красная (мишень потеряна)"

[ "$fails" -eq 0 ] && echo "test-check-novac-closer-mandatory: ok" || exit 1
