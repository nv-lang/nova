#!/bin/sh
# Самотест check-novac-no-alloc-in-lookup.sh (П16: обязан доказать, что ловит).
#
# ПОДЛОЖКА. Шов $2 — override сканируемой директории, поэтому настоящий
# novac/src не нужен: каждый случай — своё крошечное дерево .nv во временной
# папке. Красный случай главный: ИСТОРИЧЕСКАЯ форма дефекта (ключ собран
# интерполяцией строкой выше, а дверь зовётся ниже) — именно её прежний
# страж строковых ключей пропускал.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-no-alloc-in-lookup.sh"
T="${TMPDIR:-/tmp}/novac-alloc-lookup-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

mk() { d="$T/$1"; shift; mkdir -p "$d/sem"; cat > "$d/sem/a.nv"; }

# --- 1. законная дверь: цепочка + сравнение целых — зелёный ---------------
mk g1 <<'EOF'
module novac.sem

/// Row of this exact pair, or -1.
#realtime nogc
export fn FnTable @row_of(recv int, name str) -> int {
    mut r = @heads.find(name)
    while r >= 0 {
        ro fd = @rows[r]
        if fd.recv == recv { return r }
        r = fd.next
    }
    -1
}
EOF
if run "$T/g1"; then
    grep -q "аллокаций в дверях: 0" "$T/out" && ok "законная цепочка — зелёный [$(cat "$T/out")]" || bad "зелёный, но без итога числом"
else
    bad "законная цепочка покраснела: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: ключ собран строкой ВЫШЕ, дверь зовётся ниже ------
mk g2 <<'EOF'
module novac.sem

/// Register a signature.
export fn MethodTable mut @add(md MethodDef) -> () {
    ro key = "${md.recv}.${md.name}"
    ro row = @names.find(key)
    if row >= 0 { return }
    @names.put(key, @rows.len())
}
EOF
if run "$T/g2"; then
    bad "историческая форма (ключ строкой выше) ПРОШЛА — страж не ловит свой главный случай"
else
    if grep -q "интерполяц" "$T/err" && grep -q "add" "$T/err"; then
        ok "ключ, собранный выше вызова, пойман; дверь названа: $(sed -n '2p' "$T/err" | cut -c1-70)"
    else
        bad "красный, но без имени двери или причины [$(cat "$T/err")]"
    fi
fi

# --- 3. интерполяция прямо в аргументе двери — красный -------------------
mk g3 <<'EOF'
module novac.sem

/// Field type of owner.
export fn FieldTable @field_type(owner int, fname str) -> int {
    ro row = @names.find("${owner}.${fname}")
    @rows[row].type_id
}
EOF
run "$T/g3" && bad "интерполяция в аргументе двери прошла" || ok "интерполяция в аргументе двери поймана"

# --- 4. concat в двери — красный (не только интерполяция) ----------------
mk g4 <<'EOF'
module novac.sem

/// Lookup by a glued name.
export fn T @lookup(a str, b str) -> int {
    ro k = a.concat(b)
    @names.find(k)
}
EOF
run "$T/g4" && bad "concat в двери прошёл" || ok "concat в двери пойман"

# --- 5. StringBuilder в двери — красный ----------------------------------
mk g5 <<'EOF'
module novac.sem

/// Lookup building a key.
export fn T @lookup(a str) -> int {
    consume sb = StringBuilder.new()
    sb.append(a)
    @names.find(sb.into_str())
}
EOF
run "$T/g5" && bad "StringBuilder в двери прошёл" || ok "StringBuilder в двери пойман"

# --- 6. НЕ дверь: эмиттер строит текст — зелёный (иначе правило душит) ---
mk g6 <<'EOF'
module novac.emit

/// Emit a struct declaration.
fn emit_decl(name str, mut body StringBuilder) -> () {
    body.append("struct ${name} {\n")
    body.append("};\n")
}
EOF
run "$T/g6" && ok "функция без .find — не дверь, строить текст можно" || bad "эмиттер покраснел зря: $(cat "$T/err")"

# --- 7. ice-сообщение внутри двери — зелёный -----------------------------
mk g7 <<'EOF'
module novac.sem

/// Lookup that reports an invariant break.
#realtime nogc
export fn T @lookup(name str) -> int {
    ro r = @names.find(name)
    if r < 0 { ice("sem: unknown name ${name} past check") }
    r
}
EOF
run "$T/g7" && ok "сообщение ice в двери не судится (падаем один раз)" || bad "ice-сообщение покраснело зря: $(cat "$T/err")"

# --- 8. КОММЕНТАРИЙ, цитирующий снятую форму, — зелёный ------------------
# Доки нарочно хранят историю дефекта; страж, который её краснит, заставляет
# врать в документации.
mk g8 <<'EOF'
module novac.sem

/// The first shape built the key as text ("${recv}.${name}") and allocated
/// on every lookup; the chain replaced it.
#realtime nogc
export fn FnTable @row_of(recv int, name str) -> int {
    // was: ro key = "${recv}.${name}" — snyato
    mut r = @heads.find(name)
    while r >= 0 { r = @rows[r].next }
    -1
}
EOF
run "$T/g8" && ok "история дефекта в комментарии не краснит" || bad "комментарий покраснел зря: $(cat "$T/err")"

# --- 8б. ЖИВОЙ ЛОЖНЯК (2026-08-16): эмиттер, который ЗОВЁТ дверь и при этом
# законно строит C-текст. Первая редакция стража считала дверью всякого, кто
# содержит .find(, и краснела на emit_fn/emit_arm/emit_expr. Различение:
# дверь только спрашивает, эмиттер пишет (.append/StringBuilder).
mk g8b <<'EOF'
module novac.emit_c

/// Emit one function.
fn Emitter mut @emit_fn(f Node) -> () {
    ro d = @ctx.defs.find(name)
    if d < 0 { ice("emit: fn missing") }
    @body.append("static nova_unit ${c_fn(name, true)}(void) {
")
}
EOF
run "$T/g8b" && ok "эмиттер, зовущий дверь, не судится (регресс живого ложняка)" || bad "ЛОЖНЯК ВЕРНУЛСЯ: $(sed -n '2p' "$T/err")"

# --- 8в. Метод ТАБЛИЦЫ судится, даже если пишет: такие типы живут ради
# поиска, и аллокация в них — та самая цена на каждый поиск.
mk g8v <<'EOF'
module novac.sem

/// Register into the table.
export fn FnTable mut @add(fd FnDef) -> () {
    ro key = "${fd.recv}.${fd.name}"
    @body.append(key)
    @heads.put(key, 0)
}
EOF
run "$T/g8v" && bad "метод таблицы с интерполяцией прошёл (спрятался за append)" || ok "метод таблицы судится всегда, даже если пишет"

# --- 8г. Дверь БЕЗ пометки `#realtime nogc` — красная: без неё компилятор
# не проверяет её на аллокацию, и вторая сеть исчезает молча.
mk g8g <<'EOF'
module novac.sem

/// Row of this exact pair, or -1.
export fn FnTable @row_of(recv int, name str) -> int {
    mut r = @heads.find(name)
    while r >= 0 { r = @rows[r].next }
    -1
}
EOF
if run "$T/g8g"; then
    bad "дверь без #realtime nogc прошла — вторую сеть можно снять молча"
else
    grep -q "без пометки" "$T/err" && ok "дверь без пометки поймана" || bad "красный, но не про пометку"
fi

# --- 9. тесты исключены ---------------------------------------------------
mkdir -p "$T/g9/sem"
cat > "$T/g9/sem/a_test.nv" <<'EOF'
module novac.sem
fn t() -> int {
    ro key = "${a}.${b}"
    @names.find(key)
}
EOF
run "$T/g9" && ok "файл *_test.nv исключён" || bad "тест покраснел: $(cat "$T/err")"

# --- 10. mangle.nv исключён поимённо -------------------------------------
mkdir -p "$T/g10/sem"
cat > "$T/g10/sem/mangle.nv" <<'EOF'
module novac.sem
/// Build a C name.
fn c_name(ctx Ctx, id int) -> str {
    ro t = ctx.types.find(id)
    "Nova_${t}_x"
}
EOF
run "$T/g10" && ok "mangle.nv исключён (его работа — делать имена)" || bad "mangle покраснел: $(cat "$T/err")"

# --- 11. нет директории — судить нечего ----------------------------------
run "$T/absent"
grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

# --- 12. настоящее дерево ------------------------------------------------
sh "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящее дерево novac/src — зелёное" || bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-alloc-in-lookup ok: все случаи, включая историческую форму дефекта"
    exit 0
fi
exit 1
