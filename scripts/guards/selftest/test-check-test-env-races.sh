#!/usr/bin/env bash
# Самотест check-test-env-races: страж обязан УМЕТЬ КРАСНЕТЬ.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-test-env-races.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/compiler-codegen/src"
cp "$HERE/../test-env-race-scan.py" "$TMP/scripts/guards/"
SRC="$TMP/compiler-codegen/src/probe.rs"

run() { sh "$G" "$TMP" >/dev/null 2>&1; echo $?; }
say() { sh "$G" "$TMP" 2>&1; }

echo "== check-test-env-races selftest =="

# 1. Две тестовые функции на ОДНУ переменную -> отказ (случай №733).
cat > "$SRC" <<'EOF'
#[test]
fn flag_default() {
    std::env::remove_var("NOVA_MARCH_NATIVE");
    assert_eq!(march_flag(), "x86-64-v3");
}

#[test]
fn flag_native() {
    std::env::set_var("NOVA_MARCH_NATIVE", "1");
    assert_eq!(march_flag(), "native");
}
EOF
check "two tests on one variable are refused" "$(run)" "1"
if say | grep -q "NOVA_MARCH_NATIVE"; then ok "the variable is named"; else
  bad "the variable is not named in the output"; fi

# 2. Обе стороны в ОДНОМ тесте -> зелено (правильная форма).
cat > "$SRC" <<'EOF'
#[test]
fn flag_reads_the_environment() {
    std::env::set_var("NOVA_MARCH_NATIVE", "1");
    assert_eq!(march_flag(), "native");
    std::env::remove_var("NOVA_MARCH_NATIVE");
    assert_eq!(march_flag(), "x86-64-v3");
}
EOF
check "both sides inside ONE test are fine" "$(run)" "0"

# 3. Разные переменные у разных тестов -> зелено.
cat > "$SRC" <<'EOF'
#[test]
fn a() { std::env::set_var("NOVA_ONE", "1"); }

#[test]
fn b() { std::env::set_var("NOVA_TWO", "1"); }
EOF
check "different variables do not race" "$(run)" "0"

# 4. Правка среды ВНЕ теста (рабочий код) не считается.
cat > "$SRC" <<'EOF'
fn setup_one() { std::env::set_var("NOVA_SHARED", "1"); }
fn setup_two() { std::env::remove_var("NOVA_SHARED"); }
EOF
check "mutations outside test fns do not count" "$(run)" "0"

# 5. Три теста на одну переменную -> отказ, и все три названы.
cat > "$SRC" <<'EOF'
#[test]
fn x() { std::env::set_var("NOVA_TRIPLE", "1"); }

#[test]
fn y() { std::env::set_var("NOVA_TRIPLE", "2"); }

#[test]
fn z() { std::env::remove_var("NOVA_TRIPLE"); }
EOF
check "three tests on one variable are refused" "$(run)" "1"
if [ "$(say | grep -c 'probe.rs ::')" = "3" ]; then ok "all three are listed"; else
  bad "not all three test names are listed"; fi

# 6. Нет исходников -> отказ, а не зелено.
rm -rf "$TMP/compiler-codegen"
check "a missing crate tree refuses instead of passing" "$(run)" "1"

echo "  passed: $PASS   failed: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
