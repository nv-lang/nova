#!/bin/sh
# Самотест check-novac-tyid-door.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-tyid-door.py"
T="${TMPDIR:-/tmp}/novac-tyiddoor-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s\n" "$@" > "$d/m/m.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай: ровно та строка, что стоила сорока минут --------------
D=$(mk main "module a" "fn f() -> bool {" \
    "    if args[at].type_id >= 0 && p.type_id != args[at].type_id { return false }" \
    "    true" "}")
if run "$D"; then
    bad "голое сравнение type_id с нулём прошло — главный случай не ловится"
else
    grep -q "сравнивается с нулём вместо двери" "$T/err" && ok "главный случай пойман" \
        || bad "покраснел не тем текстом"
fi

# --- второй носитель того же класса: ret_id -------------------------------
D=$(mk retid "module a" "fn f() -> () {" "    if fd.ret_id >= 0 { emit() }" "}")
run "$D" && bad "ret_id >= 0 прошёл" || ok "ret_id пойман"

# --- третий: обратное сравнение ------------------------------------------
D=$(mk lt "module a" "fn f() -> () {" "    if fd.ret_id < 0 { return }" "}")
run "$D" && bad "ret_id < 0 прошёл" || ok "обратное сравнение пойман"

# --- ДВЕРЬ: развёрнутая обёртка законна ----------------------------------
D=$(mk unwrapped "module a" "fn f() -> bool {" "    raw_ty(t) >= 0" "}")
run "$D" && ok "raw_ty(...) >= 0 не судится: обёртка развёрнута" \
    || bad "развёрнутое сравнение покраснело — страж шире правила"

# --- ДВЕРЬ: is_ty законен ------------------------------------------------
D=$(mk door "module a" "fn f() -> bool {" "    is_ty(args[at].type_id)" "}")
run "$D" && ok "is_ty не судится" || bad "вызов двери покраснел"

# --- равенство С КОНКРЕТНЫМ id законно (prims.int_id бывает нулём) --------
D=$(mk eq "module a" "fn f() -> bool {" "    fd.ret_id == 0" "}")
run "$D" && ok "равенство с нулём не судится: это не вопрос о наличии" \
    || bad "== 0 покраснело — правило про порядковые операторы"

# --- КОММЕНТАРИЙ не судится ----------------------------------------------
D=$(mk comment "module a" "fn f() -> () {" "    // was: if fd.ret_id >= 0 { ... }" "    g()" "}")
run "$D" && ok "упоминание в комментарии не судится" || bad "комментарий покраснел"

# --- ТЕСТ вне суда -------------------------------------------------------
D="$T/intest"; mkdir -p "$D/m"
printf "%s\n" "module a" "test \"x\" {" "    assert(r.type_id >= 0)" "}" > "$D/m/m_test.nv"
run "$D" && ok "*_test.nv вне суда: тест строит значения, а не спрашивает их" \
    || bad "тест покраснел"

# --- дом двери вне суда --------------------------------------------------
D="$T/home"; mkdir -p "$D/types"
printf "%s\n" "module a" "export fn is_ty(t TyId) -> bool => raw_ty(t) >= 0" \
    "fn other(t TyId) -> bool => t.ty_id >= 0" > "$D/types/types.nv"
run "$D" && ok "types/types.nv вне суда: дом самой двери" || bad "дом двери покраснел"

# --- пустое дерево -------------------------------------------------------
D="$T/empty"; mkdir -p "$D"
run "$D" && ok "дерево без .nv зелёное" || bad "пустое дерево покраснело"

[ "$fails" -eq 0 ] && echo "test-check-novac-tyid-door: 10/10" && exit 0
echo "test-check-novac-tyid-door: провалов $fails" >&2
exit 1
