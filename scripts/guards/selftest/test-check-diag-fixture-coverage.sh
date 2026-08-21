#!/usr/bin/env bash
# Самотест check-diag-fixture-coverage: страж обязан УМЕТЬ КРАСНЕТЬ.
#
# Подставное дерево: свои исходники, свои фикстуры, своя база — чтобы
# самотест не зависел от настоящего репозитория и краснел только по своей
# причине.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-diag-fixture-coverage.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/compiler-codegen/src" \
         "$TMP/spec_tests/conformance/neg"
cp "$HERE/../diag-fixture-coverage-scan.py" "$TMP/scripts/guards/"
BASE="$TMP/scripts/guards/diag-fixture-coverage.baseline"

SRC="$TMP/compiler-codegen/src/diag.rs"
FIX="$TMP/spec_tests/conformance/neg/probe_neg.nv"

run() { sh "$G" "$TMP" >/dev/null 2>&1; echo $?; }
say() { sh "$G" "$TMP" 2>&1; }

echo "== check-diag-fixture-coverage selftest =="

# Два кода: один литералом, другой в КАНОННОЙ форме. Фикстур нет.
cat > "$SRC" <<'EOF'
fn a() -> Result<(), String> { Err("E_LITERAL_FORM".to_string()) }
fn b() -> Result<(), String> { Err("[E_CANON_FORM] the message text".to_string()) }
EOF
rm -f "$FIX"

# 1. База 2 — ровно столько и есть, зелено.
printf 'missing=2\n' > "$BASE"
check "count matches the baseline" "$(run)" "0"

# 2. База 1 — кодов без фикстуры больше, отказ.
printf 'missing=1\n' > "$BASE"
check "growth over the baseline is refused" "$(run)" "1"

# 3. КАНОННАЯ форма считается: без неё кодов было бы 1, и база 1 прошла бы.
#    Здесь база всё ещё 1, то есть страж КРАСНЫЙ и печатает список — в нём
#    обязан стоять именно канонный код.
if say | grep -q "E_CANON_FORM"; then
  ok "the canonical \"[E_FOO] ...\" spelling is counted"
else
  bad "the canonical spelling is NOT counted — the scan sees only literals"
fi

# 4. Фикстура на канонный код уменьшает счёт до 1.
cat > "$FIX" <<'EOF'
// EXPECT_COMPILE_ERROR E_CANON_FORM

module neg.probe

fn probe() -> int => 1
EOF
printf 'missing=1\n' > "$BASE"
check "a fixture removes its code from the count" "$(run)" "0"

# 5. Убывание — зелено, но с заметкой опустить базу.
printf 'missing=5\n' > "$BASE"
check "shrinking below the baseline is not a failure" "$(run)" "0"
if say | grep -q "1 "; then ok "the note names the real number"; else
  bad "the note does not name the real number"; fi

# 6. Фикстура БЕЗ маркера EXPECT_COMPILE_* не считается покрытием.
cat > "$FIX" <<'EOF'
module neg.probe
// mentions E_CANON_FORM in prose only, with no marker
fn probe() -> int => 1
EOF
printf 'missing=1\n' > "$BASE"
check "a file without an EXPECT_COMPILE_* marker does not count" "$(run)" "1"

# 7. Нет базы -> отказ, а не зелено «ничего не нашли».
rm -f "$BASE"
check "a missing baseline refuses instead of passing" "$(run)" "1"

# 8. Нет исходников компилятора -> отказ.
printf 'missing=2\n' > "$BASE"
rm -rf "$TMP/compiler-codegen"
check "a missing compiler tree refuses instead of passing" "$(run)" "1"

echo "  passed: $PASS   failed: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
