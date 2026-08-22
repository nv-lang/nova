#!/bin/sh
# Самотест check-novac-no-silent-skip.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-no-silent-skip.py"
T="${TMPDIR:-/tmp}/novac-silent-skip-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d"; shift; printf '%s\n' "$@" > "$d/check.nv"; echo "$d"; }

# --- ГЛАВНЫЙ случай: ветка ушла молча -------------------------------------
D=$(mk silent 'module novac.check' '' 'fn Checker mut @type_expr(e Node) -> () {' '    ro kids = branch_children(e)' '    if kids.len() < 2 { return }' '    @record(e, 1)' '}')
if run "$D"; then
    bad "молчаливый выход прошёл — главный случай не ловится"
else
    grep -q "молчаливый выход" "$T/err" && ok "молчаливый выход пойман" || bad "красный, но не про молчаливый выход"
fi

# --- отказ рядом — законно -------------------------------------------------
D=$(mk refused 'module novac.check' '' 'fn Checker mut @type_expr(e Node) -> () {' '    ro kids = branch_children(e)' '    if kids.len() < 2 {' '        @report_first_leaf_of(kids, "outside the subset: incomplete")' '        return' '    }' '    @record(e, 1)' '}')
run "$D" && ok "выход после отказа — зелёный" || bad "отказ не зачтён: $(cat "$T/err")"

# --- ice рядом — законно ---------------------------------------------------
D=$(mk iced 'module novac.check' '' 'fn Checker mut @type_expr(e Node) -> () {' '    ro kids = branch_children(e)' '    if kids.len() < 2 { ice("check: broken shape") }' '    @record(e, 1)' '}')
run "$D" && ok "ice вместо выхода — зелёный" || bad "ice не зачтён: $(cat "$T/err")"

# --- выход, потому что отказ уже выдан -------------------------------------
D=$(mk after 'module novac.check' '' 'fn Checker mut @type_expr(e Node) -> () {' '    @type_expr(kids[0])' '    if @out.len() > 0 { return }' '    @record(e, 1)' '}')
run "$D" && ok "выход по уже выданному отказу — зелёный" || bad "не зачтён: $(cat "$T/err")"

# --- названная причина -----------------------------------------------------
D=$(mk marked 'module novac.check' '' 'fn Checker mut @type_call_stmt(c Node) -> () {' '    // SILENT-OK: refused already by the subset walk above' '    if name != PRINT_FN { return }' '    @record(c, 1)' '}')
run "$D" && ok "пометка SILENT-OK с причиной принимается" || bad "пометка не принята: $(cat "$T/err")"

# --- слово return в КОММЕНТАРИИ не судится ---------------------------------
D=$(mk comment 'module novac.check' '' 'fn Checker mut @type_expr(e Node) -> () {' '    // this return used to be silent, and that was the bug' '    @record(e, 1)' '}')
run "$D" && ok "return в прозе не считается веткой" || bad "комментарий принят за код: $(cat "$T/err")"

# --- функция ВНЕ прохода канала не судится ---------------------------------
D=$(mk outside 'module novac.check' '' 'fn Checker mut @type_expr(e Node) -> () {' '    @record(e, 1)' '}' '' 'fn helper(k []Node) -> bool {' '    if k.len() < 2 { return false }' '    true' '}')
run "$D" && ok "предикат вне прохода канала не судится" || bad "чужая функция попала под суд: $(cat "$T/err")"

# --- мишень потеряна -------------------------------------------------------
D=$(mk nowalk 'module novac.check' '' 'fn other() -> int => 1')
if run "$D"; then
    bad "дерево без прохода канала прошло молча"
else
    grep -q "мишень" "$T/err" && ok "нет функций прохода — красный (класс №519)" || bad "красный, но не про мишень"
fi

run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-silent-skip ok: все случаи, включая молчаливый выход и названную причину"
    exit 0
fi
exit 1
