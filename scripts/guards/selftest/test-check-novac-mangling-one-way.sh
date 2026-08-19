#!/bin/sh
# Самотест check-novac-mangling-one-way.py (П16: обязан доказать, что ловит).
# Подложка через шов $2.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-mangling-one-way.py"
T="${TMPDIR:-/tmp}/novac-mangling-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/sem"; cat > "$d/sem/a.nv"; }

# --- 1. чистое дерево: имена только СТРОЯТСЯ — зелёный --------------------
mk g1 <<'EOF'
module a
fn c_type(ctx Ctx, t int) -> str => "Nova_${ctx.types[t].name}*"
fn use(ctx Ctx) -> str => c_type(ctx, 0)
EOF
if run "$T/g1"; then
    grep -q "разборов C-имени: 0" "$T/out" && ok "построение имён — зелёный" || bad "зелёный, но без итога [$(cat "$T/out")]"
else
    bad "чистое дерево покраснело: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: алгоритм D285 §3 — красный -----------------------
mk g2 <<'EOF'
module a
fn base_of(obj_ty str) -> str {
    mut s = obj_ty
    if s.starts_with("Nova_") { s = s.slice(5) }
    s
}
EOF
if run "$T/g2"; then
    bad "срезание приставки Nova_ прошло — страж не ловит свой главный случай"
else
    grep -q "ABI-приставки" "$T/err" && ok "срезание приставки поймано" || bad "красный, но не про приставку [$(cat "$T/err")]"
fi

# --- 3. разбор по разделителю мономорфизации ----------------------------
mk g3 <<'EOF'
module a
fn head_of(name str) -> int => name.find("____")
EOF
run "$T/g3" && bad "разбор по ____ прошёл" || ok "разбор по ____ пойман"

# --- 4. сравнение с ABI-именем как со значением -------------------------
mk g4 <<'EOF'
module a
fn is_str(c str) -> bool => c == "Nova_str"
EOF
run "$T/g4" && bad "сравнение с ABI-именем прошло" || ok "сравнение с ABI-именем поймано"

# --- 5. строковая операция прямо на результате двери --------------------
mk g5 <<'EOF'
module a
fn f(ctx Ctx, t int) -> bool => c_type(ctx, t).starts_with("N")
EOF
run "$T/g5" && bad "операция на результате двери прошла" || ok "операция на результате двери поймана"

# --- 6. КОММЕНТАРИЙ, описывающий форму чужих имён, — зелёный ------------
# Так документирован образец эмиссии оракула; краснеть на доке нельзя.
mk g6 <<'EOF'
module a
// the oracle spells it Nova_Vec____nova_int*, and c_type mirrors that shape
fn c_type(ctx Ctx, t int) -> str => "Nova_${ctx.types[t].name}*"
EOF
run "$T/g6" && ok "комментарий с чужим именем не краснит" || bad "комментарий покраснел: $(cat "$T/err")"

# --- 7. обычная строковая работа, не про ABI, — зелёная -----------------
mk g7 <<'EOF'
module a
fn ends(s str) -> bool => s.ends_with(".nv")
EOF
run "$T/g7" && ok "обычная работа со строками не судится" || bad "ложняк: $(cat "$T/err")"

# --- 8. нет директории — судить нечего ----------------------------------
run "$T/absent"
grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

# --- 9. настоящее дерево -------------------------------------------------
python "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящий novac/src — зелёный" || bad "настоящее дерево покраснело: $(python "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-mangling-one-way ok: все случаи, включая алгоритм D285 §3"
    exit 0
fi
exit 1
