#!/bin/sh
# Самотест check-novac-empty-str-door.py.
#
# Доказывает мутацией: пустота строки, спрошенная длиной представления
# (`.bytes().len() == 0`, `.byte_len() != 0`, с нулём слева и на неявном
# получателе `@byte_len()` внутри метода str), — красная;
# сравнение с `""`, длина ради длины (префикс D134) и `v.len() == 0` у вектора
# — зелёные; комментарий не считается.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-empty-str-door.py"
T="${TMPDIR:-/tmp}/novac-empty-str-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-novac-empty-str-door ok:" "$T/out"; then
        ok "живое дерево — зелёный со строкой ok:"
    else
        bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
    fi
else
    bad "живое дерево красное: [$(head -n 2 "$T/err")]"
fi

# ── 2. длина байтов против нуля — красный (главный случай, обе линзы, ноль слева) ──
mkdir -p "$T/red/novac/src/a"
cat > "$T/red/novac/src/a/a.nv" <<'NV'
module a

fn f(name str, piece str, text str) -> int {
    if name.bytes().len() == 0 { return 1 }
    if piece.byte_len() > 0 { return 2 }
    if 0 != text.bytes().len() { return 3 }
    // name.bytes().len() == 0 in a comment is prose
    0
}

fn str @h() -> int {
    if @byte_len() == 0 { return 4 }
    0
}
NV
if python "$G" "$T/red" > "$T/o2" 2> "$T/e2"; then
    bad "пустота через длину прошла: [$(head -n 1 "$T/o2")]"
else
    n=$(grep -c 'a/a.nv:' "$T/e2")
    [ "$n" = "4" ] && ok "четыре формы через длину (и неявный получатель) — красный, комментарий не посчитан" \
        || bad "красный, но посчитано $n вместо 4: [$(cat "$T/e2" | head -n 5)]"
fi

# ── 3. границы — зелёный: сравнение с "", длина ради длины, вектор ────────
mkdir -p "$T/green/novac/src/a"
cat > "$T/green/novac/src/a/a.nv" <<'NV'
module a

fn g(name str, v []int) -> str {
    if name == "" { return "-" }
    if name != "" { return "+" }
    if v.len() == 0 { return "v" }
    "${name.bytes().len()}${name}"
}
NV
if python "$G" "$T/green" > "$T/o3" 2>&1; then
    ok "сравнение с пустой строкой, длина ради длины, пустой вектор — законны"
else
    bad "границы покраснели: [$(head -n 3 "$T/o3")]"
fi

# ── 4. нет novac/src — честное «судить нечего» ───────────────────────────
mkdir -p "$T/bare"
if python "$G" "$T/bare" > "$T/o4" 2>&1; then
    grep -q "судить нечего" "$T/o4" && ok "нет novac/src — судить нечего" \
        || bad "зелёный без честной формулировки: [$(head -n 1 "$T/o4")]"
else
    bad "отсутствие novac/src сделано красным: [$(head -n 1 "$T/o4")]"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
echo "test-check-novac-empty-str-door ok: живое дерево, четыре красные формы, три границы, нет мишени"
exit 0
