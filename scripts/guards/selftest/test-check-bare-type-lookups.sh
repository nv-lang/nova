#!/usr/bin/env bash
# Самотест check-bare-type-lookups: страж обязан УМЕТЬ КРАСНЕТЬ.
#
# Проверяется на подставном дереве: свой корень, свой файл, своя база — чтобы
# самотест не зависел от настоящего числа в репозитории и не краснел от
# честной работы окна W6.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-bare-type-lookups.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Подставное дерево: ядро берём настоящее, файл и базу — свои.
mkdir -p "$TMP/scripts/guards" "$TMP/compiler-codegen/src/types"
cp "$HERE/../bare-type-lookup-scan.py" "$TMP/scripts/guards/"

mk_src() { printf '%s\n' "$@" > "$TMP/compiler-codegen/src/types/mod.rs"; }
mk_base() { printf 'bare=%s\n' "$1" > "$TMP/scripts/guards/bare-type-lookups.baseline"; }
run() { sh "$G" "$TMP" >/dev/null 2>&1; echo $?; }

echo "== propuskaet =="
mk_src 'let a = self.types.get("X");' 'let b = self.types.get("Y");'
mk_base 2
check "rovno po baze" "$(run)" "0"
mk_base 5
check "nizhe bazy -- ubylo" "$(run)" "0"
mk_src 'let a = self.types_get_for_file("X", id);'
mk_base 0
check "razreshenie po faylu ne schitaetsya golym" "$(run)" "0"
mk_src '// self.types.get("X") -- eto tolko citata v kommentarii'
mk_base 0
check "citata v kommentarii ne schitaetsya" "$(run)" "0"

echo "== krasneet =="
mk_src 'let a = self.types.get("X");' 'let b = self.types.get("Y");' 'let c = self.types.get("Z");'
mk_base 2
check "vyroslo -- krasneet" "$(run)" "1"
mk_src 'if self.types.contains_key("X") { }' 'if self.types.contains_key("Y") { }'
mk_base 1
check "contains_key tozhe schitaetsya" "$(run)" "1"
mk_src 'for t in self.types.values() { }'
mk_base 0
check "obhod karty tozhe schitaetsya" "$(run)" "1"

echo "== ne vret o srede =="
rm -f "$TMP/compiler-codegen/src/types/mod.rs"
mk_base 10
check "propavshiy fayl -- FAIL, a ne 'ok'" "$(run)" "1"
mk_src 'let a = self.types.get("X");'
rm -f "$TMP/scripts/guards/bare-type-lookups.baseline"
check "propavshaya baza -- FAIL" "$(run)" "1"

echo "== realnost =="
# Настоящее дерево обязано проходить: база ставится по факту.
check "nastoyashchee derevo prohodit" "$(sh "$G" "$HERE/../../.." >/dev/null 2>&1; echo $?)" "0"

echo
echo "selftest check-bare-type-lookups: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
