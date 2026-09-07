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

FIX="$TMP/root"; mkdir -p "$FIX/compiler-codegen/src" "$FIX/nova-cli/src" "$FIX/nova-cli/src/bench"
TR="$FIX/compiler-codegen/src/test_runner.rs"; CLI="$FIX/nova-cli/src/main.rs"; SA="$FIX/compiler-codegen/src/main.rs"
BENCH="$FIX/nova-cli/src/bench/run.rs"

echo "== проходит =="
printf 'emitter.set_resolved_types(&module_env.resolved_types);\nemitter.set_resolved_callees(&module_env.resolved_callees);\nemitter.set_bench_mode(true);\n' > "$TR"
printf 'emitter.set_resolved_types(&build_env.resolved_types);\nemitter.set_resolved_callees(&build_env.resolved_callees);\n' > "$CLI"
printf 'emitter.set_resolved_types(&module_env.resolved_types);\nemitter.set_resolved_callees(&module_env.resolved_callees);\n' > "$SA"
printf 'emitter.set_resolved_types(&bench_env.resolved_types);\nemitter.set_resolved_callees(&bench_env.resolved_callees);\nemitter.set_bench_mode(true);\n' > "$BENCH"
sh "$G" "$FIX" >/dev/null 2>&1
check "паритет ЧЕТЫРЁХ драйверов — зелёный (конфиг-сеттеры не считаются)" "$?" "0"

echo "== ловит =="
printf 'emitter.set_resolved_types(&build_env.resolved_types);\n' > "$CLI"
sh "$G" "$FIX" >/dev/null 2>&1
check "канал есть в test_runner, нет в nova build — красный" "$?" "1"

printf 'emitter.set_resolved_types(&build_env.resolved_types);\nemitter.set_resolved_callees(&build_env.resolved_callees);\n' > "$CLI"
printf 'emitter.set_resolved_types(&module_env.resolved_types);\n' > "$SA"
sh "$G" "$FIX" >/dev/null 2>&1
check "канал есть в test_runner, нет в standalone — красный" "$?" "1"

# Красный случай РОВНО на четвёртом драйвере: без него расширение стража ничего не
# сторожит, а именно этот драйвер и был сломан (№1012).
printf 'emitter.set_resolved_types(&module_env.resolved_types);\n' > "$SA"
printf 'emitter.set_resolved_types(&bench_env.resolved_types);\n' > "$BENCH"
sh "$G" "$FIX" >/dev/null 2>&1
check "канал есть в test_runner, нет в bench run — красный" "$?" "1"

printf 'emitter.set_resolved_types(&module_env.resolved_types);\nemitter.set_resolved_callees(&module_env.resolved_callees);\n' > "$SA"
printf 'emitter.set_resolved_types(&bench_env.resolved_types);\nemitter.set_resolved_callees(&bench_env.resolved_callees);\n' > "$BENCH"

# --- ПРОХОДЫ (реестр №1023) ---
# Зелёный: один и тот же проход во всех четырёх. `desugar` выбран
# НАРОЧНО: его НЕТ в ALLOW_PASS, значит его отсутствие обязано краснеть.
printf 'emitter.set_resolved_types(&module_env.resolved_types);
emitter.set_resolved_callees(&module_env.resolved_callees);
' > "$SA"
printf 'emitter.set_resolved_types(&bench_env.resolved_types);
emitter.set_resolved_callees(&bench_env.resolved_callees);
' > "$BENCH"
printf 'crate::desugar::desugar_module(&mut module);
' >> "$TR"
printf 'nova_codegen::desugar::desugar_module(&mut module);
' >> "$CLI"
printf 'crate::desugar::desugar_module(&mut module);
' >> "$SA"
printf 'nova_codegen::desugar::desugar_module(&mut module);
' >> "$BENCH"
sh "$G" "$FIX" >/dev/null 2>&1
check "проход есть во всех четырёх — зелёный" "$?" "0"

# Красный: проход есть в эталоне, нет в bench run.
printf 'emitter.set_resolved_types(&bench_env.resolved_types);
emitter.set_resolved_callees(&bench_env.resolved_callees);
' > "$BENCH"
sh "$G" "$FIX" >/dev/null 2>&1
check "проход есть в test_runner, нет в bench run — красный" "$?" "1"

# ALLOW живой: `field_cache` назван в ALLOW_PASS для bench run, значит его отсутствие
# обязано оставлять зелёным — иначе нельзя отличить «разрешено» от «не заметил».
printf 'nova_codegen::desugar::desugar_module(&mut module);
' >> "$BENCH"
printf 'crate::field_cache::cache_module(&mut module);
' >> "$TR"
printf 'nova_codegen::field_cache::cache_module(&mut module);
' >> "$CLI"
printf 'crate::field_cache::cache_module(&mut module);
' >> "$SA"
sh "$G" "$FIX" >/dev/null 2>&1
check "проход из ALLOW_PASS — зелёный (ALLOW живой)" "$?" "0"

rm "$SA"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет файла драйвера — красный" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "четыре драйвера проекта в паритете" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
