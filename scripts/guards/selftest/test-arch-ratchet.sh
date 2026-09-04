#!/usr/bin/env bash
# Селфтест scripts/guards/arch-ratchet.sh (аудит стражей 2026-08-08: из 15
# стражей arch-ratchet был ЕДИНСТВЕННЫМ без селфтеста, и при этом он
# pattern-based — считает по тексту, а не по структуре).
#
# Это не теория: метрика `infer` реально СЧИТАЛА КОММЕНТАРИИ вместо вызовов
# (база завышена 348 vs фактических 237, реестр 221.1 №440-смежное, летопись
# arch-ratchet.baseline 2026-08-07) — храповик молча НЕ СТОРОЖИЛ. arch-ratchet
# меряет долг миграции плана 196 (`infer` -> 0), и на убывании этого долга
# держится законность временного инварианта (docs/dev/conventions-governance.md,
# «Четвёртое основание: МИГРАЦИЯ»).
#
# Страж без селфтеста не работает — правило владельца 2026-07-27, энфорсится
# check-guard-wiring.sh. Доказываем ПЯТЬ свойств:
#   1. На реальной репе — зелено (страж пригоден к подключению).
#   2. Ловит РОСТ метрики (синтетическая база ниже фактического счёта) -> код 1.
#   3. Метрика считает КОНСТРУКЦИИ, а не текст: вызов в комментарии не считается.
#   4. Снижение метрики (факт < база) — принимается, код 0.
#   5. Равенство базе — НЕ ложное срабатывание, код 0.
#
# Работаем на ВРЕМЕННЫХ копиях (страж + своя baseline + фикстура emit_c.rs),
# реальный scripts/guards/arch-ratchet.baseline не трогаем.
#
# Запуск: scripts/guards/selftest/test-arch-ratchet.sh
# Выход: 0 — страж исправен, 1 — страж сломан.

set -uo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/selftest/ — корень репы на три уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD_SRC="$REPO_ROOT/scripts/guards/arch-ratchet.sh"

FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест arch-ratchet =="

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# arch-ratchet.sh резолвит EMIT-путь ОТНОСИТЕЛЬНО cwd (не относительно $0),
# а baseline — относительно $0. Поэтому каждая фикстура: копия стража рядом
# со своей baseline (тот же относительный layout scripts/guards/), плюс
# compiler-codegen/src/codegen/emit_c.rs под фикстурным корнем; запускаем
# С cwd = фикстурный корень.
mk_fixture() { # $1 = имя фикстуры
    local dir="$tmp/$1"
    mkdir -p "$dir/scripts/guards" "$dir/compiler-codegen/src/codegen"
    cp "$GUARD_SRC" "$dir/scripts/guards/arch-ratchet.sh"
    echo "$dir"
}

# ── Случай 1: на реальной репе — зелено ────────────────────────────────────
if ( cd "$REPO_ROOT" && bash scripts/guards/arch-ratchet.sh >/dev/null 2>&1 ); then
    ok "реальная репа nova: lines/infer в пределах baseline"
else
    bad "реальная репа краснит arch-ratchet — страж непригоден к подключению"
fi

# ── Случай 2: ловит РОСТ метрики (infer) ───────────────────────────────────
# Фикстура с ДВУМЯ настоящими вызовами infer_expr_c_type, база утверждает 1.
d2="$(mk_fixture case2_growth)"
cat > "$d2/compiler-codegen/src/codegen/emit_c.rs" <<'EOF'
fn a(x: i64) -> i64 {
    infer_expr_c_type(x)
}
fn b(y: i64) -> i64 {
    infer_expr_c_type(y)
}
EOF
lines2=$(wc -l < "$d2/compiler-codegen/src/codegen/emit_c.rs" | tr -d ' ')
printf 'lines=%s\ninfer=1\n' "$lines2" > "$d2/scripts/guards/arch-ratchet.baseline"
out2=$( (cd "$d2" && bash scripts/guards/arch-ratchet.sh) 2>&1 ); rc2=$?
if [ "$rc2" -eq 1 ] && echo "$out2" | grep -q "ARCH-RATCHET FAIL: infer=2 > baseline=1"; then
    ok "ловит рост infer (2 реальных вызова > базы 1, код 1)"
else
    bad "НЕ поймал рост infer (код $rc2): $out2"
fi

# ── Случай 3: метрика считает КОНСТРУКЦИИ, а не текст ──────────────────────
# Ровно дефект 2026-08-07: doc-comment/line-comment, упоминающий имя функции,
# НЕ должен увеличивать счётчик. 2 настоящих вызова + 3 упоминания в
# комментариях (///, // и отступ-`*` блочного комментария) — база = 2 должна
# пройти чисто; наивный `grep -c` дал бы 5 и покраснил бы ложно.
d3="$(mk_fixture case3_comments_not_counted)"
cat > "$d3/compiler-codegen/src/codegen/emit_c.rs" <<'EOF'
/// `infer_expr_c_type` dispatch chain — see also infer_expr_c_type below,
/// but this whole doc-comment must NOT be counted as a call.
// another line mentioning infer_expr_c_type as plain text, also not a call
/*
 * block comment continuation line calling infer_expr_c_type(z) — not a call
 */
fn a(x: i64) -> i64 {
    infer_expr_c_type(x)
}
fn b(y: i64) -> i64 {
    infer_expr_c_type(y)
}
EOF
lines3=$(wc -l < "$d3/compiler-codegen/src/codegen/emit_c.rs" | tr -d ' ')
printf 'lines=%s\ninfer=2\n' "$lines3" > "$d3/scripts/guards/arch-ratchet.baseline"
out3=$( (cd "$d3" && bash scripts/guards/arch-ratchet.sh) 2>&1 ); rc3=$?
if [ "$rc3" -eq 0 ] && echo "$out3" | grep -q "arch-ratchet ok: infer=2 <= 2"; then
    ok "считает 2 настоящих вызова, игнорирует 3 упоминания в комментариях (код 0)"
else
    bad "ложно посчитал комментарии как вызовы (код $rc3): $out3"
fi

# ── Случай 4: снижение метрики принимается ──────────────────────────────────
# 1 настоящий вызов, база утверждает 5 (долг сократили) -> код 0.
d4="$(mk_fixture case4_decrease)"
cat > "$d4/compiler-codegen/src/codegen/emit_c.rs" <<'EOF'
fn a(x: i64) -> i64 {
    infer_expr_c_type(x)
}
EOF
lines4=$(wc -l < "$d4/compiler-codegen/src/codegen/emit_c.rs" | tr -d ' ')
printf 'lines=%s\ninfer=5\n' "$lines4" > "$d4/scripts/guards/arch-ratchet.baseline"
out4=$( (cd "$d4" && bash scripts/guards/arch-ratchet.sh) 2>&1 ); rc4=$?
if [ "$rc4" -eq 0 ] && echo "$out4" | grep -q "arch-ratchet ok: infer=1 <= 5"; then
    ok "снижение долга (1 < база 5) принимается (код 0)"
else
    bad "снижение долга ложно забраковано (код $rc4): $out4"
fi

# ── Случай 5: равенство базе — НЕ ложное срабатывание ───────────────────────
d5="$(mk_fixture case5_equal)"
cat > "$d5/compiler-codegen/src/codegen/emit_c.rs" <<'EOF'
fn a(x: i64) -> i64 {
    infer_expr_c_type(x)
}
fn b(y: i64) -> i64 {
    infer_expr_c_type(y)
}
EOF
lines5=$(wc -l < "$d5/compiler-codegen/src/codegen/emit_c.rs" | tr -d ' ')
printf 'lines=%s\ninfer=2\n' "$lines5" > "$d5/scripts/guards/arch-ratchet.baseline"
out5=$( (cd "$d5" && bash scripts/guards/arch-ratchet.sh) 2>&1 ); rc5=$?
if [ "$rc5" -eq 0 ] && echo "$out5" | grep -q "arch-ratchet ok: infer=2 <= 2" && echo "$out5" | grep -q "arch-ratchet ok: lines=$lines5 <= $lines5"; then
    ok "равенство базе (2==2, lines==lines) — не ложное срабатывание (код 0)"
else
    bad "ложное срабатывание при равенстве базе (код $rc5): $out5"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест arch-ratchet: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест arch-ratchet: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
