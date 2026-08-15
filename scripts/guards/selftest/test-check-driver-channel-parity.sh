#!/usr/bin/env bash
# Самотест check-driver-channel-parity.sh — обе стороны, на фикстурном корне.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-driver-channel-parity.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/compiler-codegen/src" "$FIX/nova-cli/src"
TR="$FIX/compiler-codegen/src/test_runner.rs"; CLI="$FIX/nova-cli/src/main.rs"; SA="$FIX/compiler-codegen/src/main.rs"

echo "== проходит =="
printf 'emitter.set_resolved_types(&module_env.resolved_types);\nemitter.set_resolved_callees(&module_env.resolved_callees);\nemitter.set_bench_mode(true);\n' > "$TR"
printf 'emitter.set_resolved_types(&build_env.resolved_types);\nemitter.set_resolved_callees(&build_env.resolved_callees);\n' > "$CLI"
printf 'emitter.set_resolved_types(&module_env.resolved_types);\nemitter.set_resolved_callees(&module_env.resolved_callees);\n' > "$SA"
sh "$G" "$FIX" >/dev/null 2>&1
check "паритет трёх драйверов — зелёный (конфиг-сеттеры не считаются)" "$?" "0"

echo "== ловит =="
printf 'emitter.set_resolved_types(&build_env.resolved_types);\n' > "$CLI"
sh "$G" "$FIX" >/dev/null 2>&1
check "канал есть в test_runner, нет в nova build — красный" "$?" "1"

printf 'emitter.set_resolved_types(&build_env.resolved_types);\nemitter.set_resolved_callees(&build_env.resolved_callees);\n' > "$CLI"
printf 'emitter.set_resolved_types(&module_env.resolved_types);\n' > "$SA"
sh "$G" "$FIX" >/dev/null 2>&1
check "канал есть в test_runner, нет в standalone — красный" "$?" "1"

rm "$SA"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет файла драйвера — красный" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "три драйвера проекта в паритете" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
