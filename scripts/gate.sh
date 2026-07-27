#!/bin/sh
# scripts/gate.sh — ЕДИНЫЙ авторитетный гейт (план 231 трек Д п.1).
# Запуск из корня целевого дерева (main-репа или worktree):  bash scripts/gate.sh
# Ассерты делает СКРИПТ, не интегратор: любой провал = exit 1 с внятной строкой.
#
# Состав (CLAUDE.md/dev-workflow):
#   1) cargo build --release (nova-cli)
#   2) мега-CU spec_tests/conformance ОДНИМ CU: exit=0 И строка "PASS: N  FAIL: 0" присутствует
#   3) nova check std/src (БЕЗ NOVA_STD_PATH): канон "PASS: 142  FAIL: 27  WARN: 1040"
#   4) флагман examples/flagship/aggregator --strict-effects: строка "built:"
set -u
ROOT="$(pwd)"
MAIN_REPO="d:/Sources/nv-lang/nova"
export NOVA_GC_LIB_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\lib"
export NOVA_INCLUDE_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\include"
unset NOVA_STD_PATH 2>/dev/null || true

fail() { echo "GATE FAIL: $1" >&2; exit 1; }

echo "== gate: arch-ratchet =="
bash "$ROOT/scripts/guards/arch-ratchet.sh" || fail "arch-ratchet (emit_c growth)"

# Реестр 221.1 №138 (урок 2026-07-27): копия рантайма внутри пакетной репы/
# worktree не под git → её протухание невидимо, и она ШАДОВИТ настоящий
# рантайм. Реальная цена промаха: полтора часа диагностики «регрессии
# компилятора» в nova-http, которой не было (устаревшие заголовки копии не
# объявляли символ из фикса №108), плюс >1 ГБ мусора по репам. Копия НЕ нужна —
# есть штатные NOVA_RT_DIR/NOVA_CG_INCLUDE (см. шапку самого стража).
echo "== gate: no-runtime-copy =="
bash "$ROOT/scripts/guards/check-no-runtime-copy.sh" || fail "копия рантайма в пакетной репе/worktree (№138)"

# Трек Ж (231): страж без самотеста — доверие на слово. Самотесты дешёвые
# (секунды) и проверяют ОБА свойства: ловит нарушение / не даёт ложняка.
echo "== gate: selftests стражей =="
for st in "$ROOT"/scripts/guards/selftest/test-*.sh; do
    [ -e "$st" ] || continue
    bash "$st" || fail "самотест стража: $(basename "$st")"
done

echo "== gate: cargo build --release =="
( cd "$ROOT/nova-cli" && cargo build --release ) || fail "cargo build"
NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || fail "nova.exe not found: $NOVA"

echo "== gate: mega-CU (spec_tests/conformance, one CU) =="
MEGA_LOG="${TMPDIR:-/tmp}/gate_mega_$$.log"
"$NOVA" test --positive --compile-error "$ROOT/spec_tests/conformance" >"$MEGA_LOG" 2>&1
MEGA_EXIT=$?
MEGA_LINE=$(sed -e "s/\[[0-9;]*m//g" "$MEGA_LOG" | grep -E "PASS: [0-9]+ +FAIL: [0-9]+" | tail -1)
echo "mega-CU exit=$MEGA_EXIT :: $MEGA_LINE"
[ "$MEGA_EXIT" -eq 0 ] || { grep -E "FAIL|TIMEOUT" "$MEGA_LOG" | grep -v "FAIL: 0" | head -10 >&2; fail "mega-CU exit=$MEGA_EXIT"; }
echo "$MEGA_LINE" | grep -qE "PASS: [0-9]+ +FAIL: 0\b" || fail "mega-CU: PASS/FAIL:0 line missing (crash prints none — see $MEGA_LOG)"

echo "== gate: check std/src (byte-canon) =="
STD_LINE=$("$NOVA" check "$ROOT/std/src" 2>&1 | sed -e "s/\[[0-9;]*m//g" | grep -E "^PASS" | tail -1)
echo "std :: $STD_LINE"
echo "$STD_LINE" | grep -q "PASS: 142  FAIL: 27  WARN: 1040" || fail "check std drifted from canon 142/27/1040: '$STD_LINE'"

echo "== gate: flagship aggregator --strict-effects =="
FLAG_LINE=$("$NOVA" build "$ROOT/examples/flagship/aggregator/src/main.nv" --strict-effects 2>&1 | sed -e "s/\[[0-9;]*m//g" | tail -1)
echo "flagship :: $FLAG_LINE"
echo "$FLAG_LINE" | grep -q "built:" || fail "flagship not built: '$FLAG_LINE'"

echo "== gate: D-number uniqueness =="
DUPES=$(grep -rhoE "^## D[0-9]+\." spec/decisions/*.md | sort | uniq -d)
if [ -n "$DUPES" ]; then
  echo "GATE FAIL: duplicate D-block numbers: $DUPES" >&2
  # известная коллизия D431 (реестр №123) — допускаем ДО перенумерации, прочие = красный
  EXTRA=$(echo "$DUPES" | grep -v "^## D431\.")
  [ -n "$EXTRA" ] && exit 1
fi
echo "GATE OK (final)"
