#!/bin/sh
# scripts/guards/selftest/test-check-lint-rule-covered.sh — самотест стража
# check-lint-rule-covered.py (реестр 221.1 №1114: правило линта, ни разу не
# доказавшее, что умеет сработать).
#
# СЕМЬ СЛУЧАЕВ, каждый отвечает на свой вопрос:
#   1. правило названо фикстурой — ЗЕЛЁНЫЙ (ложняка нет);
#   2. правило без фикстуры сверх базы — КРАСНЫЙ, и в отказе назван АДРЕС (имя);
#   3. правило без фикстуры РОВНО на базе — ЗЕЛЁНЫЙ: храповик держит, а не
#      запрещает; иначе страж нельзя было бы завести на непустом долге;
#   4. правило без фикстуры, но ПОДАВЛЕННОЕ в std через `nova:allow`, — красный
#      с ДРУГИМ текстом: оно кем-то получено, значит срабатывать умеет. Два
#      состояния не должны слипаться в одно число;
#   5. имя правила, встречающееся ТОЛЬКО в утверждении юнит-теста (снятое
#      правило), в знаменатель НЕ входит — иначе страж требовал бы фикстуру на
#      то, чего компилятор не печатает. Ровно на этом мой первый счёт дал 43
#      вместо 42;
#   6. НОЛЬ правил — КРАСНЫЙ как потеря мишени, а не «непокрытых 0»;
#   7. база без ключей — КРАСНЫЙ: судить нечем != зелено.
#
# Фикстурное дерево — своё, во временном каталоге; настоящее дерево самотест не
# читает (кроме самого файла стража, который он и проверяет).
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-lint-rule-covered.py"
T="${TMPDIR:-/tmp}/lint-rule-covered-selftest.$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }
trap 'rm -rf "$T"' EXIT

if [ ! -f "$G" ]; then
    echo "test-check-lint-rule-covered: FAIL - нет самого стража $G" >&2
    exit 1
fi

# mk_root <каталог> <uncovered> <judged> — дерево с базой и пустым корпусом.
mk_root() {
    r="$1"
    rm -rf "$r"
    mkdir -p "$r/compiler-codegen/src" "$r/spec_tests" "$r/std/src" "$r/scripts/guards"
    printf 'uncovered=%s\njudged=%s\n' "$2" "$3" > "$r/scripts/guards/lint-rule-covered.baseline"
}

run() { python "$G" "$1" 2>&1; }

# --- 1. правило названо фикстурой -> зелёный -------------------------------------
mk_root "$T/c1" 0 1
cat > "$T/c1/compiler-codegen/src/lints.rs" <<'EOF'
out.push(LintWarning { rule: "W_COVERED", diag: d });
EOF
printf '// W_COVERED fires here\n' > "$T/c1/spec_tests/covered.nv"
OUT="$(run "$T/c1")"; RC=$?
if [ "$RC" -eq 0 ]; then ok "1 правило с фикстурой — зелёный"
else bad "1 покрытое правило дало отказ: $OUT"; fi

# --- 2. правило без фикстуры сверх базы -> красный, с именем ---------------------
mk_root "$T/c2" 0 1
cat > "$T/c2/compiler-codegen/src/lints.rs" <<'EOF'
out.push(LintWarning { rule: "W_ORPHAN", diag: d });
EOF
printf '// nothing here\n' > "$T/c2/spec_tests/other.nv"
OUT="$(run "$T/c2")"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q "W_ORPHAN"; then
    ok "2 правило без фикстуры — красный, имя названо"
else bad "2 непокрытое правило прошло или без имени: $OUT"; fi

# --- 3. ровно на базе -> зелёный --------------------------------------------------
mk_root "$T/c3" 1 1
cat > "$T/c3/compiler-codegen/src/lints.rs" <<'EOF'
out.push(LintWarning { rule: "W_ORPHAN", diag: d });
EOF
printf '// nothing here\n' > "$T/c3/spec_tests/other.nv"
OUT="$(run "$T/c3")"; RC=$?
if [ "$RC" -eq 0 ]; then ok "3 долг ровно на базе — зелёный (храповик держит)"
else bad "3 храповик запрещает вместо того, чтобы держать: $OUT"; fi

# --- 4. подавлено в std -> красный, но текстом про «срабатывать умеет» -----------
mk_root "$T/c4" 0 1
cat > "$T/c4/compiler-codegen/src/lints.rs" <<'EOF'
out.push(LintWarning { rule: "W_SUPPRESSED", diag: d });
EOF
printf '// nothing here\n' > "$T/c4/spec_tests/other.nv"
printf 'ro x = 1 // nova:allow W_SUPPRESSED\n' > "$T/c4/std/src/m.nv"
OUT="$(run "$T/c4")"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q "W_SUPPRESSED" \
   && echo "$OUT" | grep -q "умеет"; then
    ok "4 подавленное в std отличено от не показавшего ничего"
else bad "4 два состояния слиплись в одно: $OUT"; fi

# --- 5. снятое правило (только в утверждении теста) в знаменатель НЕ входит ------
mk_root "$T/c5" 0 1
cat > "$T/c5/compiler-codegen/src/lints.rs" <<'EOF'
out.push(LintWarning { rule: "W_COVERED", diag: d });
// снятое правило: живёт только в утверждении теста
assert!(!ws.iter().any(|w| w.rule == "W_RETIRED_ONE"));
EOF
printf '// W_COVERED fires here\n' > "$T/c5/spec_tests/covered.nv"
OUT="$(run "$T/c5")"; RC=$?
if [ "$RC" -eq 0 ] && ! echo "$OUT" | grep -q "W_RETIRED_ONE"; then
    ok "5 снятое правило не требует фикстуры — знаменатель по местам выдачи"
else bad "5 снятое правило попало под суд (счёт по литералу, а не по выдаче): $OUT"; fi

# --- 6. НОЛЬ правил -> красный как потеря мишени ---------------------------------
mk_root "$T/c6" 0 1
printf '// no rules at all\n' > "$T/c6/compiler-codegen/src/lints.rs"
OUT="$(run "$T/c6")"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -qi "мишень"; then
    ok "6 нулевая мишень — отказ, а не зелёный ноль"
else bad "6 пустая мишень прошла зелёной: $OUT"; fi

# --- 7. база без ключей -> красный ------------------------------------------------
mk_root "$T/c7" 0 1
cat > "$T/c7/compiler-codegen/src/lints.rs" <<'EOF'
out.push(LintWarning { rule: "W_COVERED", diag: d });
EOF
printf '// W_COVERED fires here\n' > "$T/c7/spec_tests/covered.nv"
printf '# letopis bez klyuchey\n' > "$T/c7/scripts/guards/lint-rule-covered.baseline"
OUT="$(run "$T/c7")"; RC=$?
if [ "$RC" -ne 0 ]; then ok "7 база без ключей — отказ"
else bad "7 база без ключей прошла зелёной: $OUT"; fi

# --- итог -------------------------------------------------------------------------
if [ "$FAILED" -ne 0 ]; then
    echo "test-check-lint-rule-covered: FAIL - провалено случаев: $FAILED из 7" >&2
    exit 1
fi
echo "test-check-lint-rule-covered ok: 7/7"
exit 0
