#!/usr/bin/env bash
# Самотест check-novac-doc-language.sh — дока, комментарии и строковые
# литералы novac по-английски (конвенция П13, план 274; норма самотестов —
# план 231 §4в: ловит нарушение И не даёт ложняка).
#
# ПОДЛОЖКА. У стража есть шов $2 — override сканируемой директории, поэтому
# настоящий novac/src нужен ровно один раз, последним случаем; всё остальное
# — маленькие деревья .nv во временном каталоге.
#
# ЧТО ЗДЕСЬ ВАЖНО ПРОВЕРИТЬ ОСОБО. У этого стража две стороны, и слабая —
# вторая:
#   1) ловит ли он русский — в ///-доке, в обычном //, в строковом литерале,
#      и в *_test.nv (дока тестов — тоже дока, маска не должна их исключать);
#   2) НЕ ЛОЖНЯЧИТ ли он. Первая версия правила писалась классом [А-Яа-я] и
#      под LC_ALL=C дала 176 ложных срабатываний на § и тире. Поэтому ловушка
#      locale C («§ — «кавычки»», греческий, CJK, emoji, диакритика — без
#      единой кириллической буквы) проверяется здесь наравне с уловом, и
#      рядом — законные ссылки П/№/§ + цифра, ВКЛЮЧАЯ буквенный суффикс
#      номера раздела (§4а, §10.3а, §2б, №114б). Суффикс — не поблажка: так
#      разделы нумерованы во всём проекте, и страж без него принуждает
#      транслитерировать ссылку в «§4a» латиницей, разрывая её с адресатом.
# И отдельно: исключение обязано быть УЗКИМ. «П» перед цифрой законна, «П»
# перед буквой («Проверка») — обычная кириллица и красная; суффикс — ровно
# ОДНА буква вплотную к цифрам («П13.5аб» красная); иначе завтра под видом
# ссылки проедет любой русский абзац.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-doc-language.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

SRC="$TMP/src"
mkdir -p "$SRC/sem" "$SRC/check"
run() { sh "$G" "$ROOT" "$SRC" > "$TMP/out" 2> "$TMP/err"; echo $?; }

# Чистая подложка: английская дока, английские литералы. Переписывается
# заново перед каждым случаем, чтобы случаи не заражали друг друга.
clean_tree() {
    rm -f "$SRC"/sem/*.nv "$SRC"/check/*.nv
    cat > "$SRC/sem/sem.nv" <<'EOF'
/// Resolves a name to a declaration.
// plain comment, english
fn resolve(n: str) -> DeclId {
    return decls.lookup(n)
}
EOF
    cat > "$SRC/check/check.nv" <<'EOF'
/// Type of an expression.
fn type_of(e: ExprId) -> TypeId {
    return error("expected int, found str")
}
EOF
}

echo "== законное — проходит =="
sh "$G" "$ROOT" "$TMP/absent" > "$TMP/out" 2>&1
check "сканируемой директории нет — зелёный" "$?" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано"

clean_tree
check "дерево с английскими комментариями — зелёный" "$(run)" "0"
has "$TMP/out" 'строк с кириллицей: 0' "итог напечатан числом"

echo "== исключение: ссылки на конвенции и реестр =="
clean_tree
cat > "$SRC/sem/refs.nv" <<'EOF'
/// Names of std are data, not knowledge of the compiler (П5).
// a silent int default is the №652 class; see also П13.5 and П14
fn f() -> int {
    return 1
}
EOF
check "строка со ссылками (П5), №652, П13.5, П14 — ЗЕЛЁНАЯ" "$(run)" "0"
has "$TMP/out" 'законных ссылок' "законные ссылки посчитаны отдельно"

# Номера разделов в этом проекте буквенные: §4а, §10.3а, §2б, №114б — 240
# употреблений в docs/**. Страж, который их краснит, заставляет
# транслитерировать ссылку и разрывать её с адресатом (ровно это уже
# случилось в novac/src/names/names.nv: «§4a» латиницей вместо «§4а»).
clean_tree
cat > "$SRC/sem/sect.nv" <<'EOF'
// novac/src/names — the ONE name door (architecture §2/§4а: identity is an id).
/// Contract: plan 274 §10.3а, machine-checked; see also 274.1 §2б and § 7в.
// a silent int default is the №114б class, ruled by П13.5а
fn s() -> int {
    return 6
}
EOF
check "буквенные номера разделов §4а, §10.3а, §2б, №114б, П13.5а — ЗЕЛЁНЫЕ" "$(run)" "0"
has "$TMP/out" 'строк с кириллицей: 0' "секционные ссылки не попали в улов"

clean_tree
printf '/// Doc.\n// § раздел без цифры\nfn u() -> int { return 7 }\n' > "$SRC/sem/sect_bad.nv"
check "«§ раздел» (знак перед буквой, без цифры) — красный" "$(run)" "1"
has "$TMP/err" 'sem/sect_bad.nv:2' "строка с «§ раздел» названа"

clean_tree
printf '/// Doc.\n// П13.5аб is a suffix plus one letter too many\nfn v() -> int { return 8 }\n' > "$SRC/sem/suf2.nv"
check "П13.5аб — красный (суффикс ровно одна буква, не хвост слова)" "$(run)" "1"
has "$TMP/err" 'sem/suf2.nv:2' "строка с двойным суффиксом названа"

echo "== кириллица шире А-я: Ё/ё и остальной блок U+0400..U+047F =="
clean_tree
printf '/// Doc.\n// Ёлка и ёжик\nfn w() -> int { return 9 }\n' > "$SRC/sem/yo.nv"
check "Ё и ё — красные (вне А-я, но кириллица)" "$(run)" "1"
has "$TMP/err" 'sem/yo.nv:2' "строка с Ё/ё названа"

clean_tree
printf '/// Doc.\n// Џ і ј ѐ Ѣ — cyrillic block outside А-я\nfn x() -> int { return 10 }\n' > "$SRC/sem/wide.nv"
check "Џ, і, ј, ѐ, Ѣ — красные (судится весь блок, а не только русский)" "$(run)" "1"
has "$TMP/err" 'sem/wide.nv:2' "строка с не-русской кириллицей названа"

echo "== ловушка locale C: § и тире — не кириллица =="
clean_tree
cat > "$SRC/sem/punct.nv" <<'EOF'
/// Section § 10.3 — an em dash, «angle quotes» and a ± sign, no cyrillic.
// §§ 1-2 – en dash – still english
// other scripts are NOT cyrillic: greek αβγΔ, CJK 中文, emoji 😀🧪,
// accents café naïve Müller, math → ≤ ≥ ∀ ∞, box ┌─┐│└┘, quotes “foo” ‘bar’ … ™ ©
fn g() -> str {
    return "\u0410 and \xd0\x9f spelled as ascii escapes are not cyrillic bytes"
}
EOF
check "§, тире, «кавычки», греческий/CJK/emoji/диакритика — ЗЕЛЁНАЯ (класс [А-Яа-я] здесь врал)" "$(run)" "0"

echo "== ловит =="
clean_tree
cat > "$SRC/sem/doc.nv" <<'EOF'
/// Resolves a name.
fn a() -> int { return 1 }
/// Возвращает тип выражения.
fn b() -> int { return 2 }
EOF
check "русский ///-док — красный" "$(run)" "1"
has "$TMP/err" 'sem/doc.nv:3' "файл и строка названы поимённо"
has "$TMP/err" 'FAIL' "строка FAIL в stderr"
has "$TMP/err" 'по-английски' "подсказка «как чинить» напечатана"

clean_tree
printf '/// Plain english doc.\nfn c() -> int {\n    // здесь считаем размер\n    return 3\n}\n' > "$SRC/sem/cmt.nv"
check "русский обычный // комментарий — красный" "$(run)" "1"
has "$TMP/err" 'sem/cmt.nv:3' "файл и строка названы поимённо"

clean_tree
printf '/// Reports a type mismatch.\nfn d() -> str {\n    return "ожидался int, получен str"\n}\n' > "$SRC/sem/lit.nv"
check "русский текст в СТРОКОВОМ ЛИТЕРАЛЕ — красный (это диагностика)" "$(run)" "1"
has "$TMP/err" 'sem/lit.nv:3' "файл и строка названы поимённо"

clean_tree
printf '/// Checks resolve.\nfn t() {\n    // проверяем, что имя нашлось\n    assert_eq(resolve("x"), 1)\n}\n' > "$SRC/sem/sem_test.nv"
check "русский в *_test.nv — красный (дока тестов тоже дока)" "$(run)" "1"
has "$TMP/err" 'sem/sem_test.nv:3' "тестовый файл назван поимённо"

echo "== исключение узкое: П перед БУКВОЙ — не ссылка =="
clean_tree
printf '/// Checks a decl.\n// Проверка имени\nfn e() -> int { return 4 }\n' > "$SRC/sem/pe.nv"
check "«Проверка» (П перед буквой) — красный" "$(run)" "1"
has "$TMP/err" 'sem/pe.nv:2' "строка с «Проверка» названа"

clean_tree
printf '/// Doc (П5).\n// П5 и ещё немного русского\nfn h() -> int { return 5 }\n' > "$SRC/sem/mix.nv"
check "законная ссылка + русский в той же строке — красный" "$(run)" "1"
has "$TMP/err" 'sem/mix.nv:2' "смешанная строка названа"

echo "== настоящее дерево =="
sh "$G" "$ROOT" > "$TMP/out" 2> "$TMP/err"
check "novac/src проекта — зелёный (он уже переведён)" "$?" "0"
has "$TMP/out" 'check-novac-doc-language ok' "зелёная строка формата <имя> ok:"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-doc-language ok: $PASS/$PASS"
    exit 0
fi
exit 1
