#!/usr/bin/env bash
# Самотест check-novac-oracle-fresh.sh — обе стороны, на фикстурном дереве.
#
# ЦЕНТРАЛЬНЫЙ СЛУЧАЙ — `.rs` ВНУТРИ target/ НЕ СУДИТСЯ. Сборка сама кладёт в
# `target/` сгенерированные и вендоренные `.rs` НОВЕЕ бинаря (build.rs, кэши
# кратов) — страж без этого исключения краснел бы сразу после каждой успешной
# пересборки, то есть ровно тогда, когда всё правильно. Второй несущий: нет
# бинаря — зелёный, потому что об отсутствии говорит шаг novac-build, и два
# стража на один факт спорили бы вслух.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-oracle-fresh.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

R="$TMP/tree"
mkdir -p "$R/nova-cli/src" "$R/nova-cli/target/release" "$R/compiler-codegen/src"
BIN="$R/nova-cli/target/release/nova.exe"

echo "== проходит =="
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "нет бинаря — зелёный (об этом говорит novac-build)" "$RC" "0"

printf 'fn main() {}\n' > "$R/nova-cli/src/main.rs"
printf 'pub fn emit() {}\n' > "$R/compiler-codegen/src/emit.rs"
: > "$BIN"                      # бинарь новее обоих исходников
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "бинарь новее всех .rs — зелёный" "$RC" "0"
has "назвал, где искал" "$OUT" "compiler-codegen"

# сгенерированный .rs внутри target/ новее бинаря — НЕ повод краснеть
mkdir -p "$R/nova-cli/target/release/build/x"
printf 'pub const V: u8 = 1;\n' > "$R/nova-cli/target/release/build/x/out.rs"
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "свежий .rs ВНУТРИ target/ — зелёный (его кладёт сама сборка)" "$RC" "0"

echo "== краснеет =="
printf 'fn main() { }\n' > "$R/nova-cli/src/main.rs"     # исходник стал новее бинаря
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "исходник новее бинаря — красный" "$RC" "1"
has "назвал число отставших" "$OUT" "1"
has "назвал файл" "$OUT" "nova-cli/src/main.rs"
has "дал команду пересборки" "$OUT" "cargo build --release"

printf 'pub fn emit() { }\n' > "$R/compiler-codegen/src/emit.rs"
OUT=$(bash "$G" "$R" 2>&1); RC=$?
check "два отставших — красный" "$RC" "1"
has "посчитал оба" "$OUT" "2"

EMPTY="$TMP/empty"; mkdir -p "$EMPTY"
OUT=$(bash "$G" "$EMPTY" "$BIN" 2>&1); RC=$?
check "нет каталогов исходников — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "самотест check-novac-oracle-fresh: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
