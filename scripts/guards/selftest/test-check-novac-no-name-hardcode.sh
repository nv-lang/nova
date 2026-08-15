#!/usr/bin/env bash
# Самотест check-novac-no-name-hardcode.sh — запрет хардкода имён Nova/std в
# компиляторе (конвенция П5, план 274; норма самотестов — план 231 §4в:
# ловит нарушение И не даёт ложняка).
#
# ПОДЛОЖКА. У стража есть шов $2 — override сканируемой директории, поэтому
# настоящий novac/src не нужен: каждый случай — своё маленькое дерево .nv во
# временном каталоге.
#
# ЧТО ЗДЕСЬ ВАЖНО ПРОВЕРИТЬ ОСОБО. Список имён страж ВЫВОДИТ ИЗ ДАННЫХ
# (литералы builtins.nv + прелюдия языка), а не держит зашитым. Значит
# самотест обязан доказать не только «ловит println», но и саму выводимость:
# одно и то же слово в обычном файле — зелёное, пока его нет в builtins.nv, и
# красное сразу, как только оно там появилось. Иначе завтра список снова
# станет зашитым, и никто не заметит.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-name-hardcode.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

SRC="$TMP/src"
mkdir -p "$SRC/builtins" "$SRC/sem" "$SRC/lex"
run() { sh "$G" "$ROOT" "$SRC" > "$TMP/out" 2> "$TMP/err"; echo $?; }

# Единственный законный дом имён: реестр остатка П5.
cat > "$SRC/builtins/builtins.nv" <<'EOF'
/// The one legitimate home of std names (P5 remainder registry).
fn builtin_method_names() -> Vec[str] {
    return Vec[str].of("println", "byte_len", "[]int")
}
EOF
# Лексер: ключевые слова ГРАММАТИКИ — не сущности std, П5 их не судит.
cat > "$SRC/lex/lex.nv" <<'EOF'
/// Recognises a keyword of the grammar.
fn is_keyword(s: str) -> bool {
    return s == "fn" || s == "module" || s == "return"
}
EOF
# Тестовый файл: исключён по маске *_test.nv.
cat > "$SRC/sem/subset_test.nv" <<'EOF'
fn t() {
    assert_eq(name_of(d), "println")
}
EOF
# Чистый файл семантики: имя берётся из двери, не пишется буквой.
clean_subset() {
    cat > "$SRC/sem/subset.nv" <<'EOF'
/// Return type of a builtin method, looked up through the door.
// the word "println" here lives in a comment, not in a literal
fn subset_method_ret(m: MethodId) -> TypeId {
    return builtins.ret_of(m)
}
EOF
}

echo "== законное — проходит =="
sh "$G" "$ROOT" "$TMP/absent" > "$TMP/out" 2>&1
check "сканируемой директории нет — зелёный" "$?" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"

clean_subset
check "чистое дерево (имя лишь в комментарии и в builtins.nv) — зелёный" "$(run)" "0"
has "$TMP/out" 'имён в списке' "число имён списка напечатано"
has "$TMP/out" 'хардкод-имён вне builtins.nv: 0' "итог напечатан числом"

printf 'fn f() -> str {\n    return "printlnx"\n}\n' >> "$SRC/sem/subset.nv"
check "литерал ШИРЕ имени (\"printlnx\") — зелёный" "$(run)" "0"

echo "== выводимость списка из builtins.nv (дефект F14) =="
clean_subset
printf 'fn g() -> str {\n    return "frobnicate"\n}\n' >> "$SRC/sem/subset.nv"
check "слова нет в builtins.nv — зелёный" "$(run)" "0"

sed -i 's/"byte_len"/"byte_len", "frobnicate"/' "$SRC/builtins/builtins.nv"
check "то же слово появилось в builtins.nv — красный БЕЗ правки стража" "$(run)" "1"
has "$TMP/err" 'frobnicate' "новое имя названо поимённо"
sed -i 's/, "frobnicate"//' "$SRC/builtins/builtins.nv"

echo "== ловит =="
clean_subset
printf 'fn h(m: str) -> TypeId {\n    if m == "println" { return T_UNIT }\n    return T_INT\n}\n' >> "$SRC/sem/subset.nv"
check "имя из builtins.nv литералом вне builtins.nv — красный" "$(run)" "1"
has "$TMP/err" 'sem/subset.nv' "файл-нарушитель назван"
has "$TMP/err" 'println' "имя-нарушитель названо"
has "$TMP/err" 'builtins.nv' "подсказка «как чинить» указывает на реестр"

clean_subset
printf 'fn i() -> str {\n    return "byte_len"\n}\n' >> "$SRC/sem/subset.nv"
check "второе имя из реестра — тоже красный" "$(run)" "1"

echo "== прелюдия языка — без builtins.nv =="
SRC2="$TMP/src2"; mkdir -p "$SRC2/sem"
printf '/// Clean.\nfn f() -> int {\n    return 1\n}\n' > "$SRC2/sem/a.nv"
sh "$G" "$ROOT" "$SRC2" > "$TMP/out" 2> "$TMP/err"
check "дерево без builtins.nv и без имён — зелёный" "$?" "0"
printf 'fn g() -> str {\n    return "Option"\n}\n' >> "$SRC2/sem/a.nv"
sh "$G" "$ROOT" "$SRC2" > "$TMP/out" 2> "$TMP/err"
check "имя ПРЕЛЮДИИ («Option») без всякого builtins.nv — красный" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "novac/src проекта чист" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-no-name-hardcode ok: $PASS/$PASS"
    exit 0
fi
exit 1
