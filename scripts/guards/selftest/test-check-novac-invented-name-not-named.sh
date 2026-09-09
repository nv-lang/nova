#!/bin/sh
# Самотест check-novac-invented-name-not-named.py (П16). Шов $2 — сканируемая директория.
#
# Проба идёт В ОБЕ СТОРОНЫ и по КАЖДОМУ разделу стража отдельно: чистое дерево зелено,
# выдуманное имя в `named` краснеет, отсутствие мишени краснеет ОТДЕЛЬНО от нарушения (иначе
# «ноль нарушений» на пустой выборке читался бы как проверка — класс №519), а комментарий с
# примером нарушения зелёным остаётся.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-invented-name-not-named.py"
T="${TMPDIR:-/tmp}/novac-invented-name-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# Подложка: дверь `named` в lower/ir.nv плюс потребитель в emit_c/.
mk() {
    d="$T/$1"; mkdir -p "$d/lower" "$d/emit_c"
    printf 'module novac.lower\n\nexport fn FnBuilder mut @named(ty int, name str) -> int {\n    0\n}\n' > "$d/lower/ir.nv"
    printf 'module novac.emit_c\n\nfn Emitter mut @lower_x(t int) -> int {\n    @ir.named(t, leaf_text(kids[1]))\n}\n' > "$d/emit_c/emit_c.nv"
    echo "$d"
}

# --- зелёная сторона -------------------------------------------------------
D=$(mk clean)
run "$D" && ok "чистое дерево — зелёное" || bad "чистое дерево покраснело: $(cat "$T/err")"
if grep -q "вызовов \`named\` 1" "$T/out"; then
    ok "знаменатель напечатан (вызовов \`named\` 1)"
else
    bad "зелёный, но знаменатель не напечатан: $(cat "$T/out")"
fi

# --- ГЛАВНЫЙ случай: выдуманное имя отдано `named` -------------------------
D=$(mk invented)
printf 'module novac.emit_c\n\nfn Emitter mut @lower_x(t int) -> int {\n    ro nm = "_novac_tmp_t7"\n    @ir.named(t, nm)\n    @ir.named(t, "_novac_scr_t3")\n}\n' > "$D/emit_c/emit_c.nv"
if run "$D"; then
    bad "выдуманное имя в \`named\` НЕ покраснело"
else
    ok "выдуманное имя в \`named\` краснеет"
    grep -q "_novac_scr_t3" "$T/err" || bad "покраснел, но не назвал нарушившую строку"
fi

# --- интерполяция: имя собрано в строке ------------------------------------
D=$(mk interp)
printf 'module novac.emit_c\n\nfn Emitter mut @lower_x(t int) -> int {\n    @ir.named(t, "_novac_i_t${u}")\n}\n' > "$D/emit_c/emit_c.nv"
run "$D" && bad "имя, собранное интерполяцией, НЕ покраснело" || ok "интерполированное выдуманное имя краснеет"

# --- КОММЕНТАРИЙ с примером нарушения обязан остаться зелёным --------------
D=$(mk comment)
printf 'module novac.emit_c\n\n// prose: never write @ir.named(t, "_novac_tmp_t7") -- that is what this guard forbids\nfn Emitter mut @lower_x(t int) -> int {\n    @ir.named(t, leaf_text(kids[1]))\n}\n' > "$D/emit_c/emit_c.nv"
run "$D" && ok "пример нарушения В КОММЕНТАРИИ зелёный" || bad "проза с примером покраснела: $(cat "$T/err")"

# --- мишень отсутствует: ОТКАЗ, а не «ноль нарушений» ----------------------
D=$(mk nodoor)
printf 'module novac.lower\n\nexport fn FnBuilder mut @temp(ty int) -> int {\n    0\n}\n' > "$D/lower/ir.nv"
if run "$D"; then
    bad "мишени нет, а страж сказал ok — это пустая выборка под видом проверки"
else
    ok "отсутствие двери \`named\` — отдельный ОТКАЗ"
    grep -q "519" "$T/err" || bad "отказал, но не назвал класс пустой выборки"
fi

# --- каталога нет вовсе ----------------------------------------------------
run "$T/nosuchdir" && bad "несуществующий каталог принят" || ok "несуществующий каталог — отказ"

if [ "$fails" -ne 0 ]; then
    echo "test-check-novac-invented-name-not-named: FAIL $fails" >&2
    exit 1
fi
echo "test-check-novac-invented-name-not-named ok: 7 случаев (зелёный, знаменатель, литерал, интерполяция, комментарий, нет двери, нет каталога)"
