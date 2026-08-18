#!/usr/bin/env bash
# Самотест check-registry-single-verdict: страж обязан УМЕТЬ КРАСНЕТЬ.
#
# Подставное дерево: свой корень, свой реестр — чтобы самотест не зависел от
# настоящего файла и краснел только по своей причине.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-registry-single-verdict.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/docs/plans"
cp "$HERE/../registry-verdict-scan.py" "$TMP/scripts/guards/"

REG="$TMP/docs/plans/221.1-bug-sweep.md"
mk() { printf '%s\n' "$@" > "$REG"; }
run() { sh "$G" "$TMP" >/dev/null 2>&1; echo $?; }

# Реестр пишется ПИТОНОМ, а не оболочкой: маркеры кириллические, и через
# оболочку они уже уезжали перекодированными (реестр №590).
python - "$REG" <<'PY'
import io, sys
# Заготовки строк реестра с НАСТОЯЩИМИ маркерами (кириллица) — пишем питоном,
# чтобы не зависеть от кодировки оболочки.
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"| 1 | K1 | text **%s NET** %s ZAKRYT |\n" % (BL, ST))
PY
echo "== propuskaet =="
check "odin verdikt i odin status" "$(run)" "0"

python - "$REG" <<'PY'
import io, sys
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
HB = u"ТЕГ БЛОКИРОВАЛ (историческая запись):"
HS = u"Статус был:"
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"| 1 | K1 | **%s DA** %s OTKRYT ... **%s NET** %s ZAKRYT |\n" % (HB, HS, BL, ST))
PY
check "letopis bez markera ne schitaetsya" "$(run)" "0"

python - "$REG" <<'PY'
import io, sys
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
# Строка, ОБЪЯСНЯЮЩАЯ правило, называет маркеры по имени в код-вставках.
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"| 1 | K1 | strok s dvumya `%s` bylo 37, a `%s` sem **%s NET** %s ZAKRYT |\n"
    % (ST, BL, BL, ST))
PY
check "citata markera v kod-vstavke ne schitaetsya" "$(run)" "0"

echo "== krasneet =="
python - "$REG" <<'PY'
import io, sys
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"| 1 | K1 | **%s DA** ... **%s NET** %s ZAKRYT |\n" % (BL, BL, ST))
PY
check "dva verdikta -- krasneet" "$(run)" "1"

python - "$REG" <<'PY'
import io, sys
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"| 1 | K1 | **%s NET** %s OTKRYT ... %s ZAKRYT |\n" % (BL, ST, ST))
PY
check "dva statusa -- krasneet" "$(run)" "1"

python - "$REG" <<'PY'
import io, sys
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
CH = u"ЧИНИТСЯ:"
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"| 1 | K1 | **%s** net plana ... **%s** plan 196 **%s NET** %s ZAKRYT |\n"
    % (CH, CH, BL, ST))
PY
check "dva marshruta -- krasneet" "$(run)" "1"

echo "== ne vret o srede =="
rm -f "$REG"
check "propavshiy reestr -- FAIL, a ne 'ok'" "$(run)" "1"

echo "== realnost =="
check "nastoyashchiy reestr prohodit" "$(sh "$G" "$HERE/../../.." >/dev/null 2>&1; echo $?)" "0"

echo
echo "selftest check-registry-single-verdict: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
