#!/usr/bin/env bash
# Самотест check-retired-names.sh — обе стороны, на фикстурном корне.
#
# Страж, который не умеет краснеть, зелен ни о чём (класс F1, реестр №645).
# Отдельно проверяется то, ради чего страж и не является простым грепом:
# новое имя СОДЕРЖИТ старое (`SignedInts` содержит `SignedInt`) и ловиться не
# должно; строка-объяснение снятия — тоже.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-retired-names.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/scripts/guards" "$FIX/spec" "$FIX/std" "$FIX/docs/plans"
cp "$ROOT/scripts/guards/retired-names-scan.py" "$FIX/scripts/guards/"
printf 'OldName -> NewName\n' > "$FIX/scripts/guards/retired-names.list"
clean() { : > "$FIX/spec/s.md"; : > "$FIX/std/x.nv"; : > "$FIX/docs/plans/p.md"; }

echo "== propuskaet =="
clean
sh "$G" "$FIX" >/dev/null 2>&1
check "chistoe derevo" "$?" "0"

clean
printf 'we use NewName everywhere\n' > "$FIX/spec/s.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "tolko novoe imya" "$?" "0"

clean
printf 'type OldNameXs set i8\n' > "$FIX/spec/s.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "novoe imya SODERZHIT staroe kak prefiks" "$?" "0"

clean
printf 'AMENDMENT: zdes stoyalo OldName, teper NewName\n' > "$FIX/spec/s.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "stroka-obyasnenie snyatiya ne schitaetsya" "$?" "0"

clean
printf 'plan text still mentions OldName\n' > "$FIX/docs/plans/p.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "docs/plans -- istoriya, ne proveryaetsya" "$?" "0"

echo "== lovit =="
clean
printf 'bound is OldName here\n' > "$FIX/spec/s.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "snyatoe imya v spec -- krasnyi" "$?" "1"

clean
printf 'fn[T OldName] T @f()\n' > "$FIX/std/x.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "snyatoe imya v std -- krasnyi" "$?" "1"

clean
rm -f "$FIX/scripts/guards/retired-names.list"
sh "$G" "$FIX" >/dev/null 2>&1
check "net spiska par -- krasnyi (a ne 'nechego sudit)" "$?" "1"

printf 'OldName -> NewName\n' > "$FIX/scripts/guards/retired-names.list"
mv "$FIX/scripts/guards/retired-names-scan.py" "$FIX/scripts/guards/off.py"
sh "$G" "$FIX" >/dev/null 2>&1
check "net yadra -- krasnyi (a ne tihiy nol)" "$?" "1"
mv "$FIX/scripts/guards/off.py" "$FIX/scripts/guards/retired-names-scan.py"

echo
echo "selftest check-retired-names: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
