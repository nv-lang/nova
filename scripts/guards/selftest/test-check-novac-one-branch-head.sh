#!/bin/sh
# Самотест стража «одна дверь печатает голову ветвления».
#
# ПРОБА В ОБЕ СТОРОНЫ У КАЖДОЙ ПОЛОВИНЫ, потому что половины независимы:
#   * страж, который только ЗАПРЕЩАЕТ новую копию, зеленеет и на дереве, где двери нет вовсе;
#   * страж, который только требует НАЛИЧИЯ двери, зеленеет при пяти копиях рядом с ней.
#
# МАТРИЦА ПОКРЫТИЯ ПО ОСЯМ (заведена сразу, а не после первого промаха — приём проверен
# 2026-09-08 на самотесте gate-bg, где девятый случай нашёл живой прогон):
#   A. голова вне двери:      нет | есть
#   B. дверь печатает голову: да  | нет (переехала/исчезла)
#   C. каталог эмиттера:      есть | нет
#   Клетки: (нет, да)      -> ПОКРЫТА: зелено, это здоровое дерево
#           (есть, да)     -> ПОКРЫТА: отказ на пятой копии
#           (нет, нет)     -> ПОКРЫТА: отказ на замолчавшей двери
#           (есть, нет)    -> НЕ НУЖНА ПО ПРИЧИНЕ: оба отказа сработали бы разом, клетка
#                             сильнее каждой покрытой, а не иная
#           C=нет          -> ПОКРЫТА: судить нечего, зелено (не выдаём отсутствие за здоровье)
#           C=есть, файлов ноль -> ПОКРЫТА: отказ, потому что «искать нечего» зелёным быть не должно
#
# Дерево не трогаем: страж принимает КОРЕНЬ аргументом, и все случаи собираются во временном.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
G="$GD/check-novac-one-branch-head.py"
T="${TMPDIR:-/tmp}/one-branch-head-selftest.$$"
trap 'rm -rf "$T"' 0

fails=0
cases=0
ok()  { echo "  ok: $1"; cases=$((cases+1)); }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

mk() {
    # mk <корень> — здоровое дерево: три двери, каждая печатает свою голову
    rm -rf "$1"; mkdir -p "$1/novac/src/emit_c"
    cat > "$1/novac/src/emit_c/emit_flow.nv" <<'EOF'
module novac.emit_c
fn Emitter mut @print_if_head(cond Cond, chained bool) -> () {
    if chained {
        @body.append("if (")
    } else {
        @body.append("    if (")
    }
}
EOF
    cat > "$1/novac/src/emit_c/emit_match.nv" <<'EOF'
module novac.emit_c
fn Emitter mut @print_arm_head(uniq int) -> () {
    @body.append("    if (!_novac_matched_t${uniq} && (")
}
EOF
    cat > "$1/novac/src/emit_c/emit_requires.nv" <<'EOF'
module novac.emit_c
fn Emitter mut @emit_requires_prologue(kids []Node, name str) -> () {
    @body.append("    if (!(")
}
EOF
}

# ── 1. здоровое дерево — зелено ───────────────────────────────────────────
mk "$T/healthy"
if python "$G" "$T/healthy" >/dev/null 2>&1; then
    ok "здоровое дерево: три двери, голов четыре — зелено"
else
    bad "здоровое дерево сочтено больным: [$(python "$G" "$T/healthy" 2>&1 | tail -1)]"
fi

# ── 2. пятая копия вне двери — отказ ──────────────────────────────────────
mk "$T/stray"
cat > "$T/stray/novac/src/emit_c/emit_c.nv" <<'EOF'
module novac.emit_c
fn Emitter mut @emit_if_value(n Node) -> () {
    @body.append("    if (")
}
EOF
out=$(python "$G" "$T/stray" 2>&1)
if [ $? -ne 0 ] && echo "$out" | grep -q "emit_c.nv"; then
    ok "голова вне двери — отказ, и место названо файлом"
else
    bad "копия формы вне двери пропущена: [$(echo "$out" | tail -1)]"
fi

# ── 3. дверь замолчала — отказ (вторая половина) ──────────────────────────
mk "$T/silent"
cat > "$T/silent/novac/src/emit_c/emit_flow.nv" <<'EOF'
module novac.emit_c
fn Emitter mut @print_if_head(cond Cond, chained bool) -> () {
    @body.append("nothing structural here")
}
EOF
out=$(python "$G" "$T/silent" 2>&1)
if [ $? -ne 0 ] && echo "$out" | grep -q "print_if_head"; then
    ok "дверь перестала печатать голову — отказ, и дверь названа"
else
    bad "замолчавшая дверь пропущена: [$(echo "$out" | tail -1)]"
fi

# ── 4. каталога эмиттера нет — зелено, но не выдаём это за здоровье ───────
rm -rf "$T/nodir"; mkdir -p "$T/nodir"
out=$(python "$G" "$T/nodir" 2>&1)
if [ $? -eq 0 ] && echo "$out" | grep -q "судить нечего"; then
    ok "нет каталога эмиттера — зелено СО СЛОВАМИ «судить нечего»"
else
    bad "отсутствие каталога выдано за здоровье: [$(echo "$out" | tail -1)]"
fi

# ── 5. каталог есть, файлов нет — отказ ───────────────────────────────────
rm -rf "$T/empty"; mkdir -p "$T/empty/novac/src/emit_c"
if python "$G" "$T/empty" >/dev/null 2>&1; then
    bad "пустой каталог эмиттера сочтён здоровым"
else
    ok "каталог есть, а файлов нет — отказ, а не тихое зелено"
fi

# ── 6. комментарий с головой не считается печатью ─────────────────────────
mk "$T/comment"
cat > "$T/comment/novac/src/emit_c/emit_place.nv" <<'EOF'
module novac.emit_c
fn Emitter mut @emit_assign(a AssignStmt) -> () {
    // once this spelled @body.append("    if (") by hand -- now the door does
    @body.append("    ")
}
EOF
if python "$G" "$T/comment" >/dev/null 2>&1; then
    ok "голова в КОММЕНТАРИИ не считается копией формы"
else
    bad "комментарий сочтён печатью: [$(python "$G" "$T/comment" 2>&1 | tail -1)]"
fi

echo "$(basename "$0" .sh): $cases случаев, провалов $fails"
[ "$fails" -eq 0 ] || exit 1
