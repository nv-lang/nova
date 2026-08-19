#!/bin/sh
# Самотест check-novac-channel-one-writer.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-channel-one-writer.py"
T="${TMPDIR:-/tmp}/novac-channel-writer-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# подложка: канал + чекер-писатель + невинный потребитель
mk() {
    d="$T/$1"; mkdir -p "$d/sem" "$d/check" "$d/emit_c"
    printf 'module novac.sem\n\nexport type CheckOut {\n    types []int /// a\n}\n\nexport fn CheckOut mut @record_type(id int, t int) -> () { }\n' > "$d/sem/channel.nv"
    printf 'module novac.check\n\nfn Checker mut @record(n int, t int) -> () {\n    @chan.record_type(n, t)\n}\n' > "$d/check/check.nv"
    printf 'module novac.emit_c\n\nfn Emitter @emit(e int) -> int => @out.type_of(e)\n' > "$d/emit_c/emit_c.nv"
    echo "$d"
}

D=$(mk clean)
run "$D" && ok "чистое дерево — зелёное" || bad "чистое дерево покраснело: $(cat "$T/err")"
grep -q "все в check/" "$T/out" || bad "зелёный, но без счёта писателей"

# --- ГЛАВНЫЙ случай: второй писатель канала вне check ---------------------
D=$(mk w2)
printf 'module novac.emit_c\n\nfn Emitter mut @fix(id int) -> () {\n    @out.record_type(id, 3)\n}\n' > "$D/emit_c/emit_c.nv"
if run "$D"; then
    bad "второй писатель канала прошёл — главный случай не ловится"
else
    grep -q "record_type" "$T/err" && ok "второй писатель канала пойман и назван" || bad "красный, но не про писателя"
fi

# --- вывод типа уехал ниже чекера ----------------------------------------
D=$(mk t2)
printf 'module novac.emit_c\n\nfn Emitter @emit(e int) -> int => type_of(e, @ctx, @scope)\n' > "$D/emit_c/emit_c.nv"
if run "$D"; then
    bad "вызов решётки type_of вне check прошёл"
else
    grep -q "type_of" "$T/err" && ok "вывод типа вне чекера пойман" || bad "красный, но не про вывод типа"
fi

D=$(mk t3)
printf 'module novac.mono\n\nfn go() -> () {\n    ro v = fresh_var()\n}\n' > "$D/sem/other.nv"
if run "$D"; then
    bad "fresh_var вне check прошёл"
else
    grep -q "fresh_var" "$T/err" && ok "fresh_var вне чекера пойман" || ok "красный (текст иной, но пойман)"
fi

# --- чтение канала законно ------------------------------------------------
D=$(mk rd)
printf 'module novac.emit_c\n\nfn Emitter @emit(e int) -> int {\n    ro t = @out.type_of(e)\n    t\n}\n' > "$D/emit_c/emit_c.nv"
run "$D" && ok "чтение канала (.type_of) не считается выводом" || bad "чтение канала покраснело: $(cat "$T/err")"

# --- мишень потеряна ------------------------------------------------------
D="$T/nochan"; mkdir -p "$D/sem"; printf 'module novac.sem\n' > "$D/sem/sem.nv"
if run "$D"; then
    bad "дерево без канала прошло молча"
else
    grep -q "мишень" "$T/err" && ok "отсутствие канала — красный (класс №519)" || bad "красный, но не про мишень"
fi

# --- ПРЯМАЯ ЗАПИСЬ В ПОЛЕ, мимо всякой двери ---------------------------
# Инвариант "ONE writer" держался на вежливости: поля CheckOut публичны, и
# `out.types[id] = 0` компилируется из любого модуля. Адверсарная проверка
# 2026-08-17 это проделала -- проба собралась оракулом, а страж остался
# зелёным, потому что грепал только имена дверей record_*.
D=$(mk direct)
printf 'module novac.emit_c\n\nfn Emitter mut @fix(id int) -> () {\n    @out.types[id] = 0\n}\n' > "$D/emit_c/emit_c.nv"
run "$D"
if [ -s "$T/err" ] && grep -q "ПРЯМАЯ запись" "$T/err"; then
    ok "прямая запись в поле канала — красный, и причина названа"
else
    bad "прямая запись в таблицу канала прошла зелёной"
fi

D=$(mk direct2)
printf 'module novac.emit_c\n\nfn Emitter mut @fix() -> () {\n    @out.subst_args = []int.new()\n}\n' > "$D/emit_c/emit_c.nv"
run "$D"
grep -q "ПРЯМАЯ запись" "$T/err" && ok "подмена таблицы целиком — красный" || bad "подмена таблицы целиком прошла"

D=$(mk readok)
printf 'module novac.emit_c\n\nfn Emitter @look(id int) -> int => @out.types[id]\n' > "$D/emit_c/emit_c.nv"
run "$D" && ok "ЧТЕНИЕ поля канала — законно, зелёный" || bad "чтение поля покрасило: судья не различает чтение и запись"


run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-channel-one-writer ok: все случаи, включая второго писателя канала и вывод типа ниже чекера"
    exit 0
fi
exit 1
