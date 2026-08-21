#!/usr/bin/env bash
# Самотест check-std-module-coverage: страж обязан УМЕТЬ КРАСНЕТЬ.
#
# Подставное дерево: свой `std/src`, своя база — чтобы самотест не зависел от
# настоящего числа и не краснел от честной работы окна.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-std-module-coverage.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/std/src"
cp "$HERE/../std-module-coverage-scan.py" "$TMP/scripts/guards/"
BASE="$TMP/scripts/guards/std-module-coverage.baseline"

run() { sh "$G" "$TMP" >/dev/null 2>&1; echo $?; }
say() { sh "$G" "$TMP" 2>&1; }

mk_mod() {
    mkdir -p "$TMP/std/src/$1"
    printf 'module std.%s\n\nfn helper() -> int => 1\n' "$1" > "$TMP/std/src/$1/core.nv"
}
add_test() {
    printf 'module std.%s\n\ntest "it works" {\n    assert(true)\n}\n' "$1" \
        >> "$TMP/std/src/$1/core.nv"
}

echo "== check-std-module-coverage selftest =="

# 1. Один модуль без теста, база 1 -> зелено.
mk_mod alpha
printf 'bare=1\n' > "$BASE"
check "count equal to the baseline is fine" "$(run)" "0"

# 2. Второй модуль без теста -> отказ, и оба названы.
mk_mod beta
check "a second bare module is refused" "$(run)" "1"
if say | grep -q "std/src/beta"; then ok "the bare module is named"; else
  bad "the bare module is not named"; fi

# 3. Тест внутри модуля снимает его со счёта.
add_test beta
check "a test inside the module removes it from the count" "$(run)" "0"

# 4. NEG-каталог не считается: у фикстур на compile-error нет блока `test`.
mkdir -p "$TMP/std/src/gamma/neg"
printf 'module std.gamma.neg\n\nfn bad() -> int => 1\n' > "$TMP/std/src/gamma/neg/x.nv"
check "a neg directory is not required to have tests" "$(run)" "0"

# 5. И суффиксный `*_neg` тоже.
mkdir -p "$TMP/std/src/delta_neg"
printf 'module std.delta_neg\n\nfn bad() -> int => 1\n' > "$TMP/std/src/delta_neg/x.nv"
check "a *_neg directory is not required either" "$(run)" "0"

# 6. Папка БЕЗ `.nv` модулем не считается.
mkdir -p "$TMP/std/src/notamodule"
printf 'x\n' > "$TMP/std/src/notamodule/readme.txt"
check "a directory without .nv is not a module" "$(run)" "0"

# 7. Убывание — зелено, с заметкой.
printf 'bare=5\n' > "$BASE"
check "shrinking below the baseline is not a failure" "$(run)" "0"

# 8. Нет базы -> отказ, а не зелено.
rm -f "$BASE"
check "a missing baseline refuses instead of passing" "$(run)" "1"

# 9. Нет std/src -> отказ.
printf 'bare=1\n' > "$BASE"
rm -rf "$TMP/std"
check "a missing std/src refuses instead of passing" "$(run)" "1"

echo "  passed: $PASS   failed: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
