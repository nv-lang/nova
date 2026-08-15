#!/bin/sh
# Самотест check-novac-no-default-branch.sh (П16: обязан доказать, что ловит).
# Подложка через шов $2.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-no-default-branch.sh"
T="${TMPDIR:-/tmp}/novac-default-branch-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/sem"; cat > "$d/sem/a.nv"; }

# --- 1. исчерпывающий match — зелёный -------------------------------------
mk g1 <<'EOF'
module a
fn f(td TypeDef) -> () {
    match td.kind {
        TkPrim => {}
        TkRecord => emit_record(td)
        TkSum => emit_sum(td)
    }
}
EOF
if run "$T/g1"; then
    grep -q "веток «всё остальное»: 0" "$T/out" && ok "исчерпывающий match — зелёный" || bad "зелёный, но без итога [$(cat "$T/out")]"
else
    bad "match покраснел: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: живой дефект emit_type_decls ----------------------
mk g2 <<'EOF'
module a
fn f(td TypeDef, t int) -> () {
    if td.kind == TypeKind.TkRecord {
        emit_record(t)
    } else {
        emit_sum(t)
    }
}
EOF
if run "$T/g2"; then
    bad "'else' за разбором по виду прошёл — страж не ловит свой главный случай"
else
    grep -q "TkRecord" "$T/err" && ok "ветка «всё остальное» поймана, условие показано" || bad "красный, но без условия [$(cat "$T/err")]"
fi

# --- 3. else-ОТКАЗ законен ------------------------------------------------
mk g3 <<'EOF'
module a
fn f(td TypeDef, t int) -> () {
    if td.kind == TypeKind.TkRecord {
        emit_record(t)
    } else {
        ice("emit_c: type kind outside the subset")
    }
}
EOF
run "$T/g3" && ok "else с ice() законен — это отказ, а не работа" || bad "честный отказ покраснел: $(cat "$T/err")"

# --- 4. арм-заглушка `_ =>` — красная всегда ------------------------------
mk g4 <<'EOF'
module a
fn f(n Node) -> int {
    match n {
        Leaf(t) => 1
        _ => 0
    }
}
EOF
run "$T/g4" && bad "`_ =>` прошёл" || { grep -q "арм-заглушка" "$T/err" && ok "арм-заглушка `_ =>` поймана" || bad "красный, но не про заглушку"; }

# --- 5. подстановочник ВНУТРИ образца законен -----------------------------
mk g5 <<'EOF'
module a
fn f(n Node) -> int {
    match n {
        Leaf(_) => 1
        Branch { .. } => 2
    }
}
EOF
run "$T/g5" && ok "Leaf(_) и Branch { .. } — не заглушки, законны" || bad "payload-подстановочник покраснел: $(cat "$T/err")"

# --- 6. peek() — вопрос да/нет, не диспетчер ------------------------------
mk g6 <<'EOF'
module a
fn f(p Cursor) -> Node {
    if p.peek() == TokenKind.LParen {
        return method_call(p)
    } else {
        return field_access(p)
    }
}
EOF
run "$T/g6" && ok "заглядывание peek() не судится (вопрос да/нет)" || bad "peek покраснел зря: $(cat "$T/err")"

# --- 7. обычное булево условие с else -------------------------------------
mk g7 <<'EOF'
module a
fn f(xs []int, i int) -> int {
    if xs.len() > i {
        return xs[i]
    } else {
        return -1
    }
}
EOF
run "$T/g7" && ok "булев else не судится" || bad "булев else покраснел: $(cat "$T/err")"

# --- 8. нет директории — судить нечего ------------------------------------
run "$T/absent"
grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

# --- 9. настоящее дерево --------------------------------------------------
sh "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящий novac/src — зелёный" || bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-default-branch ok: все случаи, включая живой дефект и оба вида не-диспетчера"
    exit 0
fi
exit 1
