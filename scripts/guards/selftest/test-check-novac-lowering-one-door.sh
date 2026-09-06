#!/bin/sh
# Самотест check-novac-lowering-one-door.py.
#
# Доказывает мутацией то, ради чего страж заведён: число армов размещения
# значения в эмиттере равно базе, и отклонение В ЛЮБУЮ сторону красное —
# рост значит «форму понизили в эмиттере, а не в lower», убыль без сдвига базы
# значит «база перестала быть правдой».
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-lowering-one-door.py"
T="${TMPDIR:-/tmp}/novac-lowering-one-door-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-novac-lowering-one-door ok:" "$T/out"; then
        ok "живое дерево — зелёный со строкой ok:"
    else
        bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
    fi
else
    bad "живое дерево красное: [$(head -n 2 "$T/err")]"
fi

# Общая заготовка: эмиттер с ТРЕМЯ армами (две по виду узла, один общий);
# комментарий с NodeKind.MatchExpr считаться не должен.
mk_tree() {
    mkdir -p "$1/novac/src/emit_c" "$1/scripts/guards"
    cat > "$1/novac/src/emit_c/e.nv" <<'NV'
module novac.emit_c

fn Emitter mut @emit_bind(init Node) -> () {
    // NodeKind.MatchExpr in a comment is prose, not an arm
    if init.kind_of() == NodeKind.MatchExpr {
        @emit_match(init)
    } else if init.kind_of() == NodeKind.IfExpr {
        @emit_if_value(init)
    } else if is_expr_kind(init.kind_of()) {
        @emit_expr(init)
    }
}
NV
}

# ── 2. число равно базе — зелёный ────────────────────────────────────────
mk_tree "$T/eq"
printf '3\n' > "$T/eq/scripts/guards/novac-lowering-doors.baseline"
if python "$G" "$T/eq" > "$T/o2" 2>&1; then
    grep -q "армов размещения значения в эмиттере 3 (база 3)" "$T/o2" \
        && ok "три арма при базе 3 — зелёный, комментарий не посчитан" \
        || bad "зелёный, но счёт не тот: [$(head -n 1 "$T/o2")]"
else
    bad "равенство с базой покраснело: [$(head -n 1 "$T/o2")]"
fi

# ── 3. РОСТ — красный (главный случай) ───────────────────────────────────
mk_tree "$T/grow"
printf '2\n' > "$T/grow/scripts/guards/novac-lowering-doors.baseline"
if python "$G" "$T/grow" > "$T/o3" 2> "$T/e3"; then
    bad "рост армов прошёл: [$(head -n 1 "$T/o3")]"
else
    grep -q "РОСТ" "$T/e3" \
        && ok "рост армов — красный" \
        || bad "красный, но не про рост: [$(head -n 1 "$T/e3")]"
fi

# ── 4. УБЫЛЬ без сдвига базы — красный ───────────────────────────────────
mk_tree "$T/drop"
printf '5\n' > "$T/drop/scripts/guards/novac-lowering-doors.baseline"
if python "$G" "$T/drop" > "$T/o4" 2> "$T/e4"; then
    bad "убыль без сдвига базы прошла: [$(head -n 1 "$T/o4")]"
else
    grep -q "база отстала" "$T/e4" \
        && ok "убыль без сдвига базы — красный" \
        || bad "красный, но не про отставшую базу: [$(head -n 1 "$T/e4")]"
fi

# ── 5. базы нет — красный с командой ─────────────────────────────────────
mk_tree "$T/nobase"
if python "$G" "$T/nobase" > "$T/o5" 2> "$T/e5"; then
    bad "отсутствие базы прошло: [$(head -n 1 "$T/o5")]"
else
    grep -q "update-baseline" "$T/e5" \
        && ok "нет базы — красный, и названа команда" \
        || bad "красный, но без команды: [$(head -n 1 "$T/e5")]"
fi

# ── 6. --update-baseline пишет сегодняшнее число ─────────────────────────
mk_tree "$T/upd"
python "$G" "$T/upd" --update-baseline > "$T/o6" 2>&1
if [ "$(cat "$T/upd/scripts/guards/novac-lowering-doors.baseline")" = "3" ]; then
    ok "--update-baseline записал 3"
else
    bad "--update-baseline записал не то: [$(cat "$T/upd/scripts/guards/novac-lowering-doors.baseline" 2>/dev/null)]"
fi

# ── 7. нет novac/src/emit_c — честное «судить нечего» ────────────────────
mkdir -p "$T/bare"
if python "$G" "$T/bare" > "$T/o7" 2>&1; then
    grep -q "судить нечего" "$T/o7" \
        && ok "нет emit_c — судить нечего" \
        || bad "зелёный без честной формулировки: [$(head -n 1 "$T/o7")]"
else
    bad "отсутствие emit_c сделано красным: [$(head -n 1 "$T/o7")]"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
echo "test-check-novac-lowering-one-door ok: равенство, рост, убыль, нет базы, запись базы, нет мишени"
exit 0
