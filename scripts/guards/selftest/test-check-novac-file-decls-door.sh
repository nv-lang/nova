#!/bin/sh
# Самотест check-novac-file-decls-door.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-file-decls-door.py"
T="${TMPDIR:-/tmp}/novac-filedecls-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s\n" "$@" > "$d/m/m.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай 1: сырой обход по имени параметра ---------------------
D=$(mk raw "module a" "fn collect(file Node) -> () {" \
    "    for c in branch_children(file) { look(c) }" "}")
if run "$D"; then
    bad "сырой branch_children(file) прошёл — главный случай не ловится"
else
    grep -q "мимо двери" "$T/err" && ok "сырой обход по имени пойман" \
        || bad "покраснел не тем текстом"
fi

# --- ГЛАВНЫЙ случай 2: обход детей, полученных разбором корня ------------
D=$(mk matched "module a" "fn emit(file Node) -> () {" "    match file {" \
    "        Branch { kind, children, .. } => {" \
    "            assert(kind == NodeKind.File, \"root\")" \
    "            for c in children { emit_one(c) }" "        }" "    }" "}")
run "$D" && bad "обход children рядом с NodeKind.File прошёл" \
    || ok "обход детей корня после утверждения о File пойман"

# --- второе имя корня ----------------------------------------------------
D=$(mk unit "module a" "fn walk(unit Node) -> () {" \
    "    for c in branch_children(unit) { g(c) }" "}")
run "$D" && bad "branch_children(unit) прошёл" || ok "второе имя корня пойман"

# --- ЗАКОННО: обход детей ЛЮБОГО другого узла ----------------------------
D=$(mk other "module a" "fn walk(decl Node) -> () {" \
    "    for c in branch_children(decl) { g(c) }" "}")
run "$D" && ok "обход детей другого узла не судится: это не вопрос о файле" \
    || bad "обход чужого узла покраснел — страж шире правила"

# --- ЗАКОННО: children ДАЛЕКО от утверждения про File --------------------
D=$(mk far "module a" "fn walk(n Node) -> () {" "    assert(k == NodeKind.File, \"x\")" \
    "    one()" "    two()" "    three()" "    four()" "    five()" "    six()" \
    "    for c in children { g(c) }" "}")
run "$D" && ok "children в семи строках от утверждения не судится" \
    || bad "далёкий обход покраснел"

# --- ЗАКОННО: сама дверь -------------------------------------------------
D=$(mk door "module a" "fn use_door(file Node) -> () {" \
    "    for c in file_decls(file) { g(c) }" "}")
run "$D" && ok "вызов двери не судится" || bad "вызов двери покраснел"

# --- ЗАКОННО: дом двери вне суда -----------------------------------------
D="$T/home"; mkdir -p "$D/sem"
printf "%s\n" "module a" "export fn file_decls(file Node) -> []Node {" \
    "    for c in branch_children(file) { out.push(c) }" "    out" "}" > "$D/sem/slots.nv"
run "$D" && ok "sem/slots.nv вне суда: дом самой двери" || bad "дом двери покраснел"

# --- ЗАКОННО: комментарий ------------------------------------------------
D=$(mk comment "module a" "fn f() -> () {" "    // was: branch_children(file)" "    g()" "}")
run "$D" && ok "упоминание в комментарии не судится" || bad "комментарий покраснел"

# --- ЗАКОННО: тест -------------------------------------------------------
D="$T/intest"; mkdir -p "$D/m"
printf "%s\n" "module a" "test \"x\" {" "    for c in branch_children(file) { g(c) }" "}" \
    > "$D/m/m_test.nv"
run "$D" && ok "*_test.nv вне суда: тест строит деревья руками" || bad "тест покраснел"

# --- пустое дерево -------------------------------------------------------
D="$T/empty"; mkdir -p "$D"
run "$D" && ok "дерево без .nv зелёное" || bad "пустое дерево покраснело"

[ "$fails" -eq 0 ] && echo "test-check-novac-file-decls-door: 10/10" && exit 0
echo "test-check-novac-file-decls-door: провалов $fails" >&2
exit 1
