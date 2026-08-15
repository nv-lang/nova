#!/usr/bin/env bash
# Самотест check-retracted-param-form.sh — обе стороны, на фикстурном корне.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-retracted-param-form.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/docs/guide" "$FIX/docs/plans" "$FIX/docs/dev" "$FIX/spec" "$FIX/scripts/guards"
mk_base() { printf 'plans=%s\ndev=%s\nspec=%s\n' "$1" "$2" "$3" > "$FIX/scripts/guards/retracted-param-form.baseline"; }

echo "== проходит =="
: > "$FIX/docs/guide/g.md"
: > "$FIX/docs/plans/p.md"
: > "$FIX/docs/dev/d.md"
: > "$FIX/spec/s.md"
mk_base 0 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "чистое дерево — зелёный" "$?" "0"

printf 'fn fill(mut buf []u8) -> int\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "канон 'mut buf T' в руководстве — зелёный" "$?" "0"

printf 'a for mut x loop and a `*mut mut T` chain\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "проза про 'for mut x' и '*mut mut T' — не ловится" "$?" "0"

echo "== ловит =="
printf 'fn fill(buf mut []u8) -> int\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "снятая форма в ПУБЛИКУЕМОМ руководстве — красный (база не спасает)" "$?" "1"

: > "$FIX/docs/guide/g.md"
printf 'type io.Read protocol { mut @read(buf mut []u8) -> int }\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "снятая форма внутри @-метода протокола — красный" "$?" "1"

: > "$FIX/docs/guide/g.md"
printf 'fn wait(m mut Mutex)\n' > "$FIX/docs/plans/p.md"
mk_base 0 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "рост осадка в docs/plans выше базы — красный" "$?" "1"

mk_base 1 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "тот же осадок в пределах базы — зелёный" "$?" "0"

printf 'fn f(p ro mut int)   // (X) retract: так нельзя\n' > "$FIX/spec/s.md"
mk_base 1 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "контрпример со словом retract — не ловится" "$?" "0"

rm -f "$FIX/scripts/guards/retracted-param-form.baseline"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет файла базы — красный" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "дерево проекта в норме" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
