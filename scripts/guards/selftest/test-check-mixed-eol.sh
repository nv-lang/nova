#!/usr/bin/env bash
# Самотест check-mixed-eol.sh — обе стороны, на фикстурном корне.
#
# Страж, который не умеет краснеть, зелен ни о чём (класс F1, реестр №645).
# Отдельно проверяется то, ради чего страж и не просто «есть \r» : ОДНОРОДНЫЙ
# файл любой из двух школ — законен. Красным обязано быть только СМЕШЕНИЕ.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-mixed-eol.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/scripts/guards" "$FIX/src" "$FIX/target"
cp "$ROOT/scripts/guards/mixed-eol-scan.py" "$FIX/scripts/guards/"

lf()   { printf 'a\nb\nc\n'            > "$1"; }
crlf() { printf 'a\r\nb\r\nc\r\n'      > "$1"; }
mix()  { printf 'a\r\nb\nc\r\n'        > "$1"; }

echo "== propuskaet =="
lf "$FIX/src/a.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "odnorodnyi LF" "$?" "0"

crlf "$FIX/src/a.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "odnorodnyi CRLF" "$?" "0"

crlf "$FIX/src/a.nv"; lf "$FIX/src/b.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "raznye faily raznyh shkol -- ne narushenie" "$?" "0"

crlf "$FIX/src/a.nv"; rm -f "$FIX/src/b.nv"
mix "$FIX/target/generated.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "smeshannyi v target/ -- propuskaetsya" "$?" "0"
rm -f "$FIX/target/generated.nv"

printf 'no newline at all' > "$FIX/src/c.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "fail bez perevodov strok" "$?" "0"
rm -f "$FIX/src/c.nv"

echo "== lovit =="
mix "$FIX/src/a.nv"
sh "$G" "$FIX" >/dev/null 2>&1
check "smeshannye okonchaniya -- krasnyi" "$?" "1"

crlf "$FIX/src/a.nv"
mix "$FIX/src/deep/d.nv" 2>/dev/null || { mkdir -p "$FIX/src/deep"; mix "$FIX/src/deep/d.nv"; }
sh "$G" "$FIX" >/dev/null 2>&1
check "smeshannye v podkataloge -- krasnyi" "$?" "1"
rm -rf "$FIX/src/deep"

mv "$FIX/scripts/guards/mixed-eol-scan.py" "$FIX/scripts/guards/off.py"
sh "$G" "$FIX" >/dev/null 2>&1
check "net yadra -- krasnyi (a ne tihiy nol)" "$?" "1"
mv "$FIX/scripts/guards/off.py" "$FIX/scripts/guards/mixed-eol-scan.py"

echo "== soobshchenie nazyvaet vinovnika =="
mix "$FIX/src/named.nv"
OUT="$(sh "$G" "$FIX" 2>&1)"
case "$OUT" in
    *named.nv*) ok "vyvod nazyvaet fail poimenno" ;;
    *) bad "vyvod ne nazyvaet fail" ;;
esac
case "$OUT" in
    *checkout*) ok "vyvod nazyvaet lechenie (perevykladka)" ;;
    *) bad "vyvod ne govorit chto delat" ;;
esac

echo
echo "selftest check-mixed-eol: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
