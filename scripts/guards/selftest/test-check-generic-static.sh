#!/usr/bin/env bash
# Самотест check-generic-static: страж обязан УМЕТЬ КРАСНЕТЬ.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-generic-static.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/compiler-codegen/src"
cp "$HERE/../generic-static-scan.py" "$TMP/scripts/guards/"
SRC="$TMP/compiler-codegen/src/probe.rs"

run() { sh "$G" "$TMP" >/dev/null 2>&1; echo $?; }
say() { sh "$G" "$TMP" 2>&1; }

echo "== check-generic-static selftest =="

# 1. Статик ВНУТРИ generic-функции -> отказ (случай №736).
cat > "$SRC" <<'EOF'
fn with_env<F: FnOnce()>(body: F) {
    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = GUARD.lock().unwrap();
    body();
}
EOF
check "a static inside a generic fn is refused" "$(run)" "1"
if say | grep -q "with_env"; then ok "the function is named"; else
  bad "the function is not named in the output"; fi

# 2. Тот же статик НА УРОВНЕ МОДУЛЯ -> зелено (это и есть правка).
cat > "$SRC" <<'EOF'
static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_env<F: FnOnce()>(body: F) {
    let _g = GUARD.lock().unwrap();
    body();
}
EOF
check "the same static at module level is fine" "$(run)" "0"

# 3. Статик внутри НЕ-generic функции -> зелено: там он один на процесс.
cat > "$SRC" <<'EOF'
fn once_flag() -> bool {
    static SEEN: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    SEEN.load(std::sync::atomic::Ordering::Relaxed)
}
EOF
check "a static inside a NON-generic fn is fine" "$(run)" "0"

# 4. Generic-функция без статиков -> зелено.
cat > "$SRC" <<'EOF'
fn apply<F: FnOnce() -> i32>(f: F) -> i32 {
    let v = f();
    v + 1
}
EOF
check "a generic fn without statics is fine" "$(run)" "0"

# 5. Статик ПОСЛЕ generic-функции (вне её тела) -> зелено.
cat > "$SRC" <<'EOF'
fn apply<F: FnOnce() -> i32>(f: F) -> i32 {
    f()
}

static AFTER: i32 = 7;
EOF
check "a static after the generic fn body is fine" "$(run)" "0"

# 6. Нет исходников -> отказ, а не зелено.
rm -rf "$TMP/compiler-codegen"
check "a missing crate tree refuses instead of passing" "$(run)" "1"

echo "  passed: $PASS   failed: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
