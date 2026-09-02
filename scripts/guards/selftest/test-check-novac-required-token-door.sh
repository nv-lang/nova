#!/bin/sh
# Самотест check-novac-required-token-door.py (П16). Шов $2 — сканируемая директория.
# Форма строки НЕ украшение: check-novac-registry-counts.sh считает случаи по
# шаблону "^[[:space:]]+ok[[:space:]]", то есть двоеточие после ok делает случай
# невидимым для счёта, а число в строке реестра — ложью.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-required-token-door.py"
T="${TMPDIR:-/tmp}/novac-reqtoken-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
# Каждая подделка несёт вызов обязательной двери: без него страж краснеет по
# потерянной мишени, и это его отдельный случай ниже.
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; { printf "%s\n" "$@"; \
        printf "%s\n" "fn Cursor mut @other() -> () {" \
                       "    @push_expected(kids, TokenKind.Ident)" "}"; } \
        > "$d/m/m.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай: закрывающая фигурная необязательной дверью ------------
D=$(mk brace "module a" "fn Cursor mut @block() -> Node {" \
    "    @push_if(kids, TokenKind.RBrace)" "}")
if run "$D"; then
    bad "закрывающая фигурная через push_if прошла - главный случай не ловится"
else
    grep -q "RBrace" "$T/err" && ok "закрывающая фигурная поймана" || bad "покраснел не тем текстом"
fi

# --- имя, равенство, стрелка, скобка сигнатуры, in -------------------------
D=$(mk ident "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.Ident)" "}")
run "$D" && bad "имя через push_if прошло" || ok "имя поймано"

D=$(mk assign "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.Assign)" "}")
run "$D" && bad "равенство через push_if прошло" || ok "равенство поймано"

D=$(mk arrow "module a" "fn f() -> () {" "    @push_if(ak, TokenKind.FatArrow)" "}")
run "$D" && bad "стрелка плеча через push_if прошла" || ok "стрелка плеча поймана"

D=$(mk lparen "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.LParen)" "}")
run "$D" && bad "скобка сигнатуры через push_if прошла" || ok "скобка сигнатуры поймана"

D=$(mk kwin "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.KwIn)" "}")
run "$D" && bad "in цикла через push_if прошло" || ok "in цикла поймано"

# --- ЧЕСТНО необязательные остаются законными ------------------------------
D=$(mk comma "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.Comma)" "}")
run "$D" && ok "запятая через push_if проходит" || bad "страж съел запятую - правило шире класса"

D=$(mk unsafe "module a" "fn f() -> () {" "    @push_if(lead, TokenKind.KwUnsafe)" "}")
run "$D" && ok "unsafe у extern проходит" || bad "страж съел необязательный unsafe"

D=$(mk ellipsis "module a" "fn f() -> () {" "    @push_if(pk, TokenKind.Ellipsis)" "}")
run "$D" && ok "многоточие вариадика проходит" || bad "страж съел необязательное многоточие"

# --- КОММЕНТАРИЙ с той же строкой законен ----------------------------------
D=$(mk comment "module a" \
    "// До #809 здесь стояло @push_if(kids, TokenKind.RBrace) и файл без" \
    "// закрывающей скобки принимался молча." "fn f() -> () { }")
run "$D" && ok "комментарий с историей класса проходит" \
    || bad "комментарий покраснел - страж стирает причину вместе с симптомом"

# --- ПОТЕРЯННАЯ МИШЕНЬ: дверь переименовали, вызовов ноль ------------------
D2="$T/blind"; mkdir -p "$D2/m"
printf "%s\n" "module a" "fn f() -> () {" "    @push_if(kids, TokenKind.Comma)" "}" > "$D2/m/m.nv"
if run "$D2"; then
    bad "ноль вызовов обязательной двери прошёл - страж слеп и печатает ноль как замер"
else
    grep -q "потерянная мишень\|ослеп\|НИ РАЗУ" "$T/err" && ok "потерянная мишень поймана" \
        || bad "покраснел не тем текстом на потерянной мишени"
fi

# --- пустая директория -----------------------------------------------------
mkdir -p "$T/empty"
run "$T/empty" && bad "пустая директория прошла - страж не заметил потери мишени" \
    || ok "пустая директория красная (мишень потеряна)"

[ "$fails" -eq 0 ] && echo "test-check-novac-required-token-door: ok" || exit 1
