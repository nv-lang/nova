#!/usr/bin/env bash
# Самотест check-crate-test-coverage.sh — обе стороны, на фикстурном корне.
#
# Страж, который не умеет краснеть, зелен ни о чём (класс F1, реестр №645).
# Здесь у «умеет краснеть» есть вторая половина, ради которой страж и писан:
# он обязан ОТЛИЧАТЬ цель, которую никто не гоняет, от цели, покрытой прогоном
# крейта ЦЕЛИКОМ — иначе он краснел бы на `nova-cli`, где `cargo test` идёт
# без флага и берёт всё, и был бы отключён первым же окном.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-crate-test-coverage.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/scripts/guards" "$FIX/compiler-codegen/tests" \
         "$FIX/nova-cli/tests" "$FIX/nova-lsp/tests"

# Страж читает SUITES из соседнего check-crate-tests.sh — фикстура подставляет
# свой, чтобы самотест не зависел от того, что гоняется в настоящем дереве.
write_suites() {
    printf '#!/bin/sh\nSUITES="%s"\n' "$1" > "$FIX/scripts/guards/check-crate-tests.sh"
}
base() { printf 'uncovered=%s\n' "$1" > "$FIX/scripts/guards/crate-test-coverage.baseline"; }
run()  { NOVA_CRATE_COVERAGE_BASELINE="$FIX/scripts/guards/crate-test-coverage.baseline" \
             sh "$G" "$FIX" >/dev/null 2>&1; }

touch "$FIX/compiler-codegen/tests/alpha.rs" "$FIX/compiler-codegen/tests/beta.rs"
touch "$FIX/nova-cli/tests/cli_one.rs"
touch "$FIX/nova-lsp/tests/lsp_one.rs"

echo "== ne lozhnit =="

# `nova-cli::150` — cargo без флага, значит крейт покрыт целиком.
# У compiler-codegen явно названа alpha; beta и lsp_one вне прогона = 2.
write_suites "nova-cli::150 compiler-codegen:--test=alpha:3 nova-lsp:--lib:300"
base 2; run
check "baza sovpadaet -- zeleno" "$?" "0"

# `--tests` покрывает все интеграционные цели крейта.
write_suites "nova-cli::150 compiler-codegen:--tests:900 nova-lsp:--tests:300"
base 0; run
check "--tests pokryvaet krait celikom" "$?" "0"

# Долг УМЕНЬШИЛСЯ — это не отказ, страж лишь просит опустить базу.
base 5; run
check "dolg upal -- ne otkaz" "$?" "0"

echo "== krasneet =="

write_suites "nova-cli::150 compiler-codegen:--test=alpha:3 nova-lsp:--lib:300"
base 2
touch "$FIX/compiler-codegen/tests/gamma.rs"
run
check "novaya cel vne progona -- otkaz" "$?" "1"
rm -f "$FIX/compiler-codegen/tests/gamma.rs"

run
check "cel ubrana -- snova zeleno" "$?" "0"

# Порча входных данных не должна выглядеть успехом.
printf '#!/bin/sh\necho no suites here\n' > "$FIX/scripts/guards/check-crate-tests.sh"
run
check "net stroki SUITES -- otkaz, a ne tihoe ok" "$?" "1"

rm -f "$FIX/scripts/guards/check-crate-tests.sh"
run
check "net storozha-progona -- otkaz" "$?" "1"

echo
if [ "$FAIL" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $FAIL свойств(а) не выполняются" >&2
    exit 1
fi
echo "самотест ok: страж ловит новую непокрытую цель, не ложнит на крейте, покрытом целиком, и не молчит на испорченном входе ($PASS свойств)"
exit 0
