#!/bin/sh
# Самотест меры переезда `novac-tree-edges.py --split` (П16). Шов — NOVAC_ROOT.
#
# ПОЧЕМУ У МЕРЫ, А НЕ У СТРАЖА, ЕСТЬ САМОТЕСТ. Эта мера не висит в гейте: её вердикт читает
# человек строкой `criterion of the move`. Именно поэтому она опаснее стража -- страж, который
# разучился ловить, роняет гейт кого-то ещё, а мера, которая разучилась, просто печатает
# «MET» и закрывает волну. За сутки 2026-09-10..13 это правило счёта переписывалось ЧЕТЫРЕЖДЫ,
# и каждая следующая редакция рождалась из ЗЕЛЁНОЙ ошибки предыдущей.
#
# ПРОБА ИДЁТ В ОБЕ СТОРОНЫ И ПО КАЖДОМУ СВОЙСТВУ ОТДЕЛЬНО:
#   * чистое дерево зелено и НАЗЫВАЕТ ЗНАМЕНАТЕЛЬ (число нарезанных областей);
#   * свободная функция, пишущая в билдер, краснеет -- и ДО первого метода в файле, и ПОСЛЕ
#     него: первое прежняя нарезка не видела вовсе, второе приписывала соседнему методу;
#   * приписывание доказывается ИМЕНЕМ в строке отказа, а не только его наличием: строка
#     обязана назвать свободную функцию, а не чистый метод над ней;
#   * ноль нарезанных областей -- ОТКАЗ (rc=2), а не «0 писателей»: ноль по пустому входу
#     читается как выполненный критерий;
#   * ноль изменяющих дверей -- ОТКАЗ, по той же причине.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
M="$ROOT/scripts/tools/novac-tree-edges.py"
T="${TMPDIR:-/tmp}/novac-tree-edges-split-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Подложка: модуль IR с изменяющей и читающей дверью.
mk_ir() {
    mkdir -p "$1/novac/src/lower" "$1/novac/src/emit_c"
    printf 'module novac.lower\n\nexport fn FnBuilder mut @place(e int, d int) -> () {\n    0\n}\n\nexport fn FnBuilder @decl_of(l int) -> int {\n    l\n}\n' \
        > "$1/novac/src/lower/ir.nv"
}
run() { NOVAC_ROOT="$1" python "$M" --split > "$T/out" 2> "$T/err"; echo $? > "$T/rc"; }
rc()  { cat "$T/rc"; }

# --- 1. ЧИСТОЕ ДЕРЕВО: только чтение -> критерий выполнен, знаменатель назван -------------
D="$T/clean"; mk_ir "$D"
printf 'module novac.emit_c\n\nfn plain_helper(x int) -> int {\n    x\n}\n\nfn Emitter mut @print_one(l int) -> () {\n    @lo.ir.decl_of(l)\n}\n' \
    > "$D/novac/src/emit_c/emit_c.nv"
run "$D"
if grep -q "criterion of the move: MET" "$T/out"; then ok "чистое дерево -- MET"
else bad "чистое дерево не дало MET: $(cat "$T/out" "$T/err")"; fi
if grep -q "regions sliced (methods AND free functions): 2" "$T/out"; then
    ok "знаменатель назван и равен 2 (метод + свободная функция)"
else bad "знаменатель не напечатан или не равен 2: $(grep regions "$T/out")"; fi

# --- 2. СВОБОДНАЯ ФУНКЦИЯ ДО ПЕРВОГО МЕТОДА пишет -> прежняя нарезка её НЕ ВИДЕЛА ---------
D="$T/before"; mk_ir "$D"
printf 'module novac.emit_c\n\nfn sneaky_first(e int) -> () {\n    @lo.ir.place(e, 0)\n}\n\nfn Emitter mut @print_one(l int) -> () {\n    @lo.ir.decl_of(l)\n}\n' \
    > "$D/novac/src/emit_c/emit_c.nv"
run "$D"
if grep -q "criterion of the move: not met" "$T/out"; then
    ok "пишущая свободная функция ДО первого метода -- критерий не выполнен"
else bad "писателя до первого метода не заметили: $(cat "$T/out")"; fi
if grep -q "sneaky_first (free fn)" "$T/out"; then
    ok "строка отказа называет свободную функцию по имени"
else bad "имя свободной функции в отчёте не названо: $(cat "$T/out")"; fi

# --- 3. СВОБОДНАЯ ФУНКЦИЯ ПОСЛЕ ЧИСТОГО МЕТОДА -> приписывания соседу быть не должно ------
D="$T/after"; mk_ir "$D"
printf 'module novac.emit_c\n\nfn Emitter mut @print_one(l int) -> () {\n    @lo.ir.decl_of(l)\n}\n\nfn sneaky_tail(e int) -> () {\n    @lo.ir.place(e, 0)\n}\n' \
    > "$D/novac/src/emit_c/emit_c.nv"
run "$D"
if grep -q "sneaky_tail (free fn)" "$T/out"; then
    ok "пишущая свободная функция ПОСЛЕ метода названа сама"
else bad "хвостовую свободную функцию не назвали: $(cat "$T/out")"; fi
if grep -q "Emitter.print_one .*doors" "$T/out"; then
    bad "чистый метод обвинён чужими дверями (приписывание соседу): $(cat "$T/out")"
else ok "чистый метод НЕ обвинён чужими дверями"; fi

# --- 4. НОЛЬ ОБЛАСТЕЙ -> отказ, а не «0 писателей» ---------------------------------------
D="$T/nodecl"; mk_ir "$D"
printf 'module novac.emit_c\n\n// only a comment, not one declaration\n' \
    > "$D/novac/src/emit_c/emit_c.nv"
run "$D"
if [ "$(rc)" = "2" ] && grep -q "REFUSED" "$T/err"; then
    ok "ноль нарезанных областей -- ОТКАЗ rc=2"
else bad "пустая нарезка не дала отказа: rc=$(rc) out=$(cat "$T/out")"; fi

# --- 5. НОЛЬ ИЗМЕНЯЮЩИХ ДВЕРЕЙ -> отказ (прежнее свойство, держим покрытым) --------------
D="$T/nodoors"; mkdir -p "$D/novac/src/lower" "$D/novac/src/emit_c"
printf 'module novac.lower\n\nexport fn FnBuilder @decl_of(l int) -> int {\n    l\n}\n' \
    > "$D/novac/src/lower/ir.nv"
printf 'module novac.emit_c\n\nfn Emitter mut @print_one(l int) -> () {\n    @lo.ir.decl_of(l)\n}\n' \
    > "$D/novac/src/emit_c/emit_c.nv"
run "$D"
if [ "$(rc)" = "2" ] && grep -q "REFUSED" "$T/err"; then
    ok "ноль изменяющих дверей -- ОТКАЗ rc=2"
else bad "отсутствие дверей не дало отказа: rc=$(rc)"; fi

if [ "$fails" -eq 0 ]; then
    echo "test-novac-tree-edges-split ok: 8 проверок, обе стороны"
    exit 0
fi
echo "test-novac-tree-edges-split: FAIL -- $fails" >&2
exit 1
