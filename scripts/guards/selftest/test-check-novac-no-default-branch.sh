#!/bin/sh
# Самотест check-novac-no-default-branch.py (П16: обязан доказать, что ловит).
# Подложка через шов $2.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-no-default-branch.py"
T="${TMPDIR:-/tmp}/novac-default-branch-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
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
run "$T/g4" && bad "`_ =>` прошёл" || { grep -q "без объяснения" "$T/err" && ok "арм-заглушка `_ =>` поймана" || bad "красный, но не про заглушку"; }

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

# --- 5а. пустое значение ХВОСТОМ за исчерпывающим match — красное ---------
mk g5a <<'EOF'
module a
fn c_method(td TypeDef, name str) -> str {
    match td.kind {
        TkPrim => { return "a" }
        TkRecord => { return "b" }
        TkSum => { return "c" }
    }
    ""
}
EOF
if run "$T/g5a"; then
    bad "мёртвое пустое значение хвостом прошло"
else
    grep -q "пустое значение хвостом" "$T/err" && ok "пустой хвост за match пойман" || bad "красный, но не про пустой хвост"
fi

# --- 5б. тот же хвост С маркером известного бага — зелёный ---------------
mk g5b <<'EOF'
module a
fn c_type(td TypeDef) -> str {
    match td.kind {
        TkPrim => { return "a" }
        TkRecord => { return "b" }
        TkSum => { return "c" }
    }
    // [LEGACY-#677] never-in-tail: unreachable pacifier.
    ""
}
EOF
run "$T/g5b" && ok "успокоитель с маркером законен" || bad "маркированный успокоитель покраснел: $(cat "$T/err")"

# --- 5в. "" как ОСМЫСЛЕННЫЙ сигнал после цепочки if — зелёный ------------
# Живой ложняк 2026-08-16: checked_op/cmp_op/raw_op возвращают "" в значении
# «не арифметика», и вызывающий это проверяет — хвост достижим.
mk g5v <<'EOF'
module a
fn checked_op(op TokenKind) -> str {
    if op == TokenKind.Plus { return "nova_int_checked_add" }
    if op == TokenKind.Minus { return "nova_int_checked_sub" }
    ""
}
EOF
run "$T/g5v" && ok "осмысленный сигнал \"\" после цепочки if не судится" || bad "сигнальный хвост покраснел зря: $(cat "$T/err")"

# --- 8. нет директории — судить нечего ------------------------------------
run "$T/absent"
grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

# --- 9. настоящее дерево --------------------------------------------------
python "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящий novac/src — зелёный" || bad "настоящее дерево покраснело: $(python "$G" "$ROOT" 2>&1 | head -3)"

# --- открытый домен: `_` обязателен, а не ленив (2026-08-16) --------------
mkdir -p "$T/open/sem"
{ echo 'module a'
  echo 'fn esc(c char) -> str => match c {'
  echo "    'q'  => "quote""
  echo '    _   => "other"'
  echo '}'
} > "$T/open/sem/a.nv"
run "$T/open" && ok "литеральные армы: подстановочник законен (открытый домен)" || bad "открытый домен покраснел: $(cat "$T/err")"

mkdir -p "$T/closed/sem"
{ echo 'module a'
  echo 'fn name(k Kind) -> str => match k {'
  echo '    Red => "r"'
  echo '    _   => "x"'
  echo '}'
} > "$T/closed/sem/a.nv"
if run "$T/closed"; then
    bad "заглушка на ИМЕНОВАННЫХ вариантах прошла — главный случай ослаб"
else
    grep -q "без объяснения" "$T/err" && ok "на закрытом множестве подстановочник по-прежнему красный" || bad "красный, но не про заглушку"
fi

# --- `_ => None` — частичное отображение, а не проглатывание (2026-08-17) --
mkdir -p "$T/partial/sem"
{ echo 'module a'
  echo 'fn op(k Kind) -> Option[Op] {'
  echo '    match k {'
  echo '        Plus => Some(Op.Add)'
  echo '        _ => None'
  echo '    }'
  echo '}'
} > "$T/partial/sem/a.nv"
run "$T/partial" && ok "подстановочник в None законен: тип объявил отображение частичным" || bad "частичное отображение покраснело: $(cat "$T/err")"

mkdir -p "$T/swallow/sem"
{ echo 'module a'
  echo 'fn name(k Kind) -> str {'
  echo '    match k {'
  echo '        Plus => "plus"'
  echo '        _ => "other"'
  echo '    }'
  echo '}'
} > "$T/swallow/sem/a.nv"
if run "$T/swallow"; then
    bad "заглушка с обычным значением прошла — запрет ослаб"
else
    grep -q "без объяснения" "$T/err" && ok "заглушка не-None по-прежнему красная" || bad "красный, но не про заглушку"
fi


# --- 12. ОБЪЯСНЁННЫЙ `_` законен (уточнение владельца 2026-08-18) --------
mkdir -p "$T/g12"
cat > "$T/g12/a.nv" <<'NV'
module m

fn dispatch(k TokenKind) -> int => match k {
    LParen => 1
    Ident => 2
    // Everything else does not begin an atom: the caller recovers with an
    // Err node and the token stays for the enclosing form to read.
    _ => 0
}
NV
if run "$T/g12"; then
    ok "объяснённый \`_\` законен — правило про молчание, не про подстановочник"
else
    bad "объяснённый \`_\` покраснел: [$(head -n 2 "$T/err")]"
fi

# --- 13. КОРОТКАЯ отписка объяснением не считается -----------------------
mkdir -p "$T/g13"
cat > "$T/g13/a.nv" <<'NV'
module m

fn dispatch(k TokenKind) -> int => match k {
    LParen => 1
    // прочее
    _ => 0
}
NV
if run "$T/g13"; then
    bad "отписка в два слова сошла за объяснение"
else
    ok "короткий комментарий объяснением не считается"
fi
echo "итог: FAIL $fails"

# --- пустой else с комментарием ПЕРЕД следующим армом: законно по П31, окно
# просмотра обязано закрыться на его скобке, а не читать соседний арм (2026-09-05)
mkdir -p "$T/empty-else-comment"
cat > "$T/empty-else-comment/a.nv" <<'NV'
module p
fn f(k NodeKind, rk []Node) -> int {
    if k == NodeKind.RecordCtor {
        if rk.len() > 0 && rk[0].kind_of() == NodeKind.Name {
            return 1
        } else {
            // Anonymous: nothing to judge here, by design.
        }
    } else if k == NodeKind.ArrayLit {
        ro x = rk.len()
        ro y = x + 1
        ro z = y + 1
        ro w = z + 1
        return w
    }
    0
}
NV
if python "$G" "$ROOT" "$T/empty-else-comment" >/dev/null 2>&1; then ok "пустой else с комментарием перед соседним армом — зелёный"; else bad "пустой else с комментарием покраснел от строк соседнего арма"; fi

if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-default-branch ok: все случаи, включая живой дефект и оба вида не-диспетчера"
    exit 0
fi
exit 1
