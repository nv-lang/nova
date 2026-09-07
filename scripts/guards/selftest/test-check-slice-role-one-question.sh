#!/bin/sh
# Самотест check-slice-role-one-question.py.
#
# Доказывает мутацией шесть случаев: живое дерево зелёное; четвёртое место,
# спрашивающее роль, — красное; место из списка, потерявшее литерал `index`
# (половина роли), — красное; место из списка, потерявшее `end_index` вовсе, —
# красное. Плюс контроль: `end_index` в комментарии не считается вопросом.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-slice-role-one-question.py"
T="${TMPDIR:-/tmp}/slice-role-one-question-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
cases=0
ok()  { echo "  ok: $1"; cases=$((cases+1)); }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Строит минимальное дерево src со всеми тремя ожидаемыми местами.
make_tree() {
    d="$1"
    mkdir -p "$d/codegen" "$d/types"
    cat > "$d/codegen/emit_c.rs" <<'RS'
fn satisfies_range_index_role(&self, obj_ty: &str) -> bool {
    self.all_methods.contains(&(base.to_string(), "index".to_string()))
        && self.all_methods.contains(&(base.to_string(), "end_index".to_string()))
}
RS
    cat > "$d/types/mod.rs" <<'RS'
let has_index = self.find_method_decl(tname, "index").is_some();
let has_end = self.find_method_decl(tname, "end_index").is_some();
if has_index && !has_end { errors.push(Diagnostic::new(msg, e.span)); }
RS
    cat > "$d/lints.rs" <<'RS'
out.insert("index".to_string());
out.insert("end_index".to_string());
RS
}

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-slice-role-one-question ok:" "$T/out"; then
        ok "живое дерево — зелёный со строкой ok:"
    else
        bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
    fi
else
    bad "живое дерево красное: [$(head -n 3 "$T/err")]"
fi

# ── 2. синтетическое дерево из трёх мест — зелёный ────────────────────────
make_tree "$T/green/src"
if python "$G" "$T/green" "$T/green/src" > "$T/out2" 2> "$T/err2"; then
    ok "три ожидаемых места — зелёный"
else
    bad "три ожидаемых места красные: [$(head -n 3 "$T/err2")]"
fi

# ── 3. ЧЕТВЁРТАЯ ДВЕРЬ — красный ──────────────────────────────────────────
make_tree "$T/red4/src"
mkdir -p "$T/red4/src/sem"
cat > "$T/red4/src/sem/other.rs" <<'RS'
fn also_asks(t: &str) -> bool {
    table.has(t, "index") && table.has(t, "end_index")
}
RS
if python "$G" "$T/red4" "$T/red4/src" > "$T/out3" 2> "$T/err3"; then
    bad "четвёртое место НЕ покраснело"
else
    if grep -q "sem/other.rs" "$T/err3"; then
        ok "четвёртое место — красный, и место названо"
    else
        bad "красный, но место не названо: [$(head -n 2 "$T/err3")]"
    fi
fi

# ── 4. ПОЛОВИНА РОЛИ — красный ────────────────────────────────────────────
make_tree "$T/redhalf/src"
cat > "$T/redhalf/src/codegen/emit_c.rs" <<'RS'
fn satisfies_range_index_role(&self, obj_ty: &str) -> bool {
    self.all_methods.contains(&(base.to_string(), "end_index".to_string()))
}
RS
if python "$G" "$T/redhalf" "$T/redhalf/src" > "$T/out4" 2> "$T/err4"; then
    bad "половина роли НЕ покраснела"
else
    grep -q "codegen/emit_c.rs" "$T/err4" && ok "половина роли — красный" \
        || bad "красный, но место не названо: [$(head -n 2 "$T/err4")]"
fi

# ── 5. ДВЕРЬ ИСЧЕЗЛА — красный ────────────────────────────────────────────
make_tree "$T/redgone/src"
cat > "$T/redgone/src/types/mod.rs" <<'RS'
let has_index = self.find_method_decl(tname, "index").is_some();
RS
if python "$G" "$T/redgone" "$T/redgone/src" > "$T/out5" 2> "$T/err5"; then
    bad "исчезнувшая дверь НЕ покраснела"
else
    grep -q "types/mod.rs" "$T/err5" && ok "исчезнувшая дверь — красный" \
        || bad "красный, но место не названо: [$(head -n 2 "$T/err5")]"
fi

# ── 6. КОНТРОЛЬ: `end_index` в комментарии не считается вопросом ──────────
make_tree "$T/comment/src"
mkdir -p "$T/comment/src/sem"
cat > "$T/comment/src/sem/prose.rs" <<'RS'
// The role needs "index" and "end_index"; this line is prose about it,
/// and so is this one -- neither asks the question.
fn unrelated() -> bool { true }
RS
if python "$G" "$T/comment" "$T/comment/src" > "$T/out6" 2> "$T/err6"; then
    ok "проза про роль не считается вопросом"
else
    bad "комментарий покраснел, а не должен: [$(head -n 2 "$T/err6")]"
fi

if [ "$fails" -eq 0 ]; then
    echo "test-check-slice-role-one-question ok: $cases случаев (счётчик, не литерал)"
    exit 0
fi
echo "test-check-slice-role-one-question FAIL: $fails" >&2
exit 1
