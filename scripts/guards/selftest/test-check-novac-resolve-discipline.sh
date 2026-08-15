#!/usr/bin/env bash
# Самотест check-novac-resolve-discipline.sh — два антипаттерна резолва
# (правила владельца 2026-08-14; класс №652 «тихий int-дефолт»). Норма
# самотестов — план 231 §4в: ловит нарушение И не даёт ложняка.
#
# ПОДЛОЖКА. У стража есть шов $2 — override сканируемой директории, поэтому
# настоящий novac/src не нужен: каждый случай — маленькое дерево .nv во
# временном каталоге. Судятся обе половины правила:
#   правило 1 — линейный скан имён (`== name`) вне names/;
#   правило 2 — промах резолва, молча ставший int (`< 0` → T_INT, голый
#     хвост T_INT/"nova_int", остаточная ветка `return T_X` отдельной строкой).
# И ровно так же важна вторая сторона: те же формы ВНУТРИ names/ и
# позитивная однострочная ветка `if <проверка> { return T_INT }` обязаны
# оставаться зелёными — иначе из стража начнут выкручиваться.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-resolve-discipline.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

SRC="$TMP/src"
mkdir -p "$SRC/names" "$SRC/sem"
run() { sh "$G" "$ROOT" "$SRC" > "$TMP/out" 2> "$TMP/err"; echo $?; }

# Дверь имён: линейный скан здесь — законное внутреннее устройство таблицы.
cat > "$SRC/names/table.nv" <<'EOF'
/// The one door of name -> id resolution (O(1) for callers).
fn slot_of(t: NameTable, name: str) -> int {
    for s in t.slots { if s.text == name { return s.id } }
    return -1
}
EOF
# Чистая семантика: резолв через дверь, промах — ice, каждый вид позитивно.
clean_sem() {
    cat > "$SRC/sem/check.nv" <<'EOF'
/// Resolve through the table door; a miss is a broken invariant, not an int.
fn type_of_name(t: NameTable, n: NameId) -> TypeId {
    let id = t.lookup(n)
    if id < 0 { return ice("unknown name after a clean check") }
    return decls.type_of(id)
}
/// Literal type: every kind is decided positively, the tail is ice.
fn lit_type(e: Expr) -> TypeId {
    if e.is_int_lit() { return T_INT }
    if e.is_str_lit() { return T_STR }
    return ice("unclassified literal")
}
EOF
}

echo "== законное — проходит =="
sh "$G" "$ROOT" "$TMP/absent" > "$TMP/out" 2>&1
check "сканируемой директории нет — зелёный" "$?" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"

clean_sem
check "дверь names/ со сканом + позитивные ветки — зелёный" "$(run)" "0"
has "$TMP/out" 'линейных сканов и тихих int-дефолтов: 0' "итог напечатан числом"

echo "== правило 1: линейный резолв по имени =="
clean_sem
printf 'fn find_decl(d: Decls, name: str) -> DeclId {\n    for x in d.items { if x.text == name { return x.id } }\n    return -1\n}\n' >> "$SRC/sem/check.nv"
check "тот же скан ВНЕ names/ — красный" "$(run)" "1"
has "$TMP/err" 'линейный резолв по имени' "правило 1 названо"
has "$TMP/err" 'check.nv' "файл-нарушитель назван"

clean_sem
printf 'fn skip(x: Sym, fname: str) -> bool {\n    return x.text != fname\n}\n' >> "$SRC/sem/check.nv"
check "сравнение != fname вне names/ — красный" "$(run)" "1"

echo "== правило 2: промах резолва не смеет стать int (№652) =="
clean_sem
printf 'fn ty(t: NameTable, n: NameId) -> TypeId {\n    let id = t.lookup(n)\n    if id < 0 { return T_INT }\n    return decls.type_of(id)\n}\n' >> "$SRC/sem/check.nv"
check "промах (< 0) → тихий T_INT — красный" "$(run)" "1"
has "$TMP/err" 'тихий int' "правило 2 названо"

clean_sem
printf 'fn ty2(e: Expr) -> TypeId {\n    if e.is_str_lit() { return T_STR }\n    T_INT\n}\n' >> "$SRC/sem/check.nv"
check "голый хвост T_INT — красный" "$(run)" "1"
has "$TMP/err" 'хвост-дефолт int' "хвост-дефолт назван"

clean_sem
printf 'fn cname(e: Expr) -> str {\n    if e.is_str_lit() { return "nova_str" }\n    "nova_int"\n}\n' >> "$SRC/sem/check.nv"
check "голый хвост \"nova_int\" — красный" "$(run)" "1"

clean_sem
printf 'fn ty3(e: Expr) -> TypeId {\n    if e.is_str_lit() { return T_STR }\n    return T_INT\n}\n' >> "$SRC/sem/check.nv"
check "остаточная ветка «return T_INT» отдельной строкой — красный" "$(run)" "1"
has "$TMP/err" 'остаточная ветка решает тип' "остаточная ветка названа"

echo "== вторая сторона: те же формы внутри names/ =="
clean_sem
printf 'fn find2(t: NameTable, name: str) -> int {\n    for s in t.slots { if s.text == name { return s.id } }\n    return -1\n}\n' >> "$SRC/names/table.nv"
check "второй скан ВНУТРИ names/ — зелёный (это и есть дверь)" "$(run)" "0"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "novac/src проекта чист" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-resolve-discipline ok: $PASS/$PASS"
    exit 0
fi
exit 1
