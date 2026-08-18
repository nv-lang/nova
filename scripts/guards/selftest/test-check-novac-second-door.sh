#!/bin/sh
# Самотест check-novac-second-door.py.
#
# Доказывает мутацией то, ради чего страж заведён: он видит вторую дверь по
# ФОРМЕ, а не по имени. Случай 2 — буквально сегодняшний живой дефект: две
# функции с РАЗНЫМИ именами и одинаковыми телами, которые прежний страж дверей
# (`check-novac-one-door-export.sh`, «одно имя из двух модулей») пропускал.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-second-door.py"
T="${TMPDIR:-/tmp}/novac-second-door-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-novac-second-door ok:" "$T/out"; then
        ok "живое дерево — зелёный со строкой ok:"
    else
        bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
    fi
else
    bad "живое дерево красное: [$(head -n 2 "$T/err")]"
fi

# ── 2. ДВЕ ДВЕРИ С РАЗНЫМИ ИМЕНАМИ — красный (главный случай) ────────────
mkdir -p "$T/dup/novac/src/a" "$T/dup/novac/src/b"
cat > "$T/dup/novac/src/a/a.nv" <<'NV'
module a

fn row_of(f Node, ctx Ctx) -> int {
    ro kids = branch_children(f)
    if kids.len() < 2 { return -1 } else { }
    ro nm = kids[1]
    if nm.kind_of() != NodeKind.LeafNode { return -1 } else { }
    ro d = ctx.defs.find(leaf_text(nm))
    if d < 0 { return -1 } else { }
    d
}
NV
cat > "$T/dup/novac/src/b/b.nv" <<'NV'
module b

fn lookup_row(f Node, ctx Ctx) -> int {
    ro kids = branch_children(f)
    if kids.len() < 2 { return -1 } else { }
    ro nm = kids[1]
    if nm.kind_of() != NodeKind.LeafNode { return -1 } else { }
    ro d = ctx.defs.find(leaf_text(nm))
    if d < 0 { return -1 } else { }
    d
}
NV
if python "$G" "$T/dup" > "$T/o2" 2> "$T/e2"; then
    bad "две двери с разными именами прошли: [$(head -n 1 "$T/o2")]"
else
    grep -q "написана дважды" "$T/e2" \
        && ok "две двери с разными именами — красный" \
        || bad "красный, но не про вторую дверь: [$(head -n 1 "$T/e2")]"
fi

# ── 3. РАЗНЫЕ тела — зелёный (правило про копию, не про похожесть) ───────
mkdir -p "$T/diff/novac/src/a" "$T/diff/novac/src/b"
cp "$T/dup/novac/src/a/a.nv" "$T/diff/novac/src/a/a.nv"
cat > "$T/diff/novac/src/b/b.nv" <<'NV'
module b

fn count_kids(f Node) -> int {
    mut n = 0
    for c in branch_children(f) {
        if c.kind_of() == NodeKind.LeafNode { continue }
        n += 1
    }
    n
}
NV
if python "$G" "$T/diff" > "$T/o3" 2>&1; then
    ok "разные тела законны — правило про копию, а не про сходство темы"
else
    bad "разные тела покраснели: [$(head -n 2 "$T/o3")]"
fi

# ── 4. РОСТ обращений мимо двери — красный (храповик) ────────────────────
mkdir -p "$T/ratchet/novac/src/emit_c" "$T/ratchet/scripts/guards"
cat > "$T/ratchet/novac/src/emit_c/e.nv" <<'NV'
module emit_c

fn ask(name str, ctx Ctx) -> int {
    ro d = ctx.defs.find(name)
    if d < 0 { return -1 } else { }
    ro { kind: k, payload: row } = ctx.defs.rows[d]
    if k != DefKind.DefFn { return -1 } else { }
    row
}
NV
printf '0\n' > "$T/ratchet/scripts/guards/novac-second-door.baseline"
if python "$G" "$T/ratchet" > "$T/o4" 2> "$T/e4"; then
    bad "рост обращений мимо двери прошёл: [$(head -n 1 "$T/o4")]"
else
    grep -q "мимо его двери" "$T/e4" \
        && ok "рост обращений мимо двери — красный" \
        || bad "красный, но не про храповик: [$(head -n 1 "$T/e4")]"
fi

# ── 5. нет novac/src — честное «судить нечего» ───────────────────────────
mkdir -p "$T/bare"
if python "$G" "$T/bare" > "$T/o5" 2>&1; then
    grep -q "судить нечего" "$T/o5" \
        && ok "нет novac/src — судить нечего" \
        || bad "зелёный без честной формулировки: [$(head -n 1 "$T/o5")]"
else
    bad "отсутствие novac/src сделано красным: [$(head -n 1 "$T/o5")]"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
echo "test-check-novac-second-door ok: все случаи, включая две двери с разными именами"
exit 0
