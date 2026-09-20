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
# Baza novoy proverki (verdikt bez statusa) zhivet v proveryaemom dereve:
# bez neyo strazh chestno otkazyvaetsya, i samotest lovil by ne to.
NSB="$TMP/scripts/guards/verdict-no-status.baseline"
printf 'rows=0
scanned_rows=0
' > "$NSB"
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

echo "== verdikt bez statusa =="
# Stroka, obyavivshaya verdikt, obyazana obyavit i status: inache strazh
# reliza schitaet eyo otkrytoy po umolchaniyu, a napisano nichego.
# Perevody strok sobirayutsya iz chr(10): literalnyy escape uezzhaet
# cherez obolochku, i piton vnutri heredoc stanovitsya nevernym molcha.
python - "$REG" <<'PY2'
import io, sys
p = sys.argv[1]
BL = u"\u0411\u041b\u041e\u041a\u0418\u0420\u0423\u0415\u0422 \u0422\u0415\u0413:"
ST = u"\u0421\u0442\u0430\u0442\u0443\u0441:"
LF = chr(10)
rows = (u'| 1 | K1 | text. ' + BL + u' DA. |' + LF
        + u'| 2 | K1 | text. ' + BL + u' NET. ' + ST + u' OTKRYT |' + LF)
io.open(p, 'w', encoding='utf-8', newline=LF).write(rows)
PY2
printf 'rows=0
scanned_rows=0
' > "$NSB"
check "verdikt bez statusa -- krasneet" "$(run)" "1"
printf 'rows=1
scanned_rows=0
no_status_row=1
' > "$NSB"
check "ta zhe stroka v baze -- zeleno" "$(run)" "0"


echo "== imennaya baza: zamena, suzhenie razbora, samoprotivorechie =="
# Реестр из двух строк: 1 — вердикт БЕЗ статуса, 2 — полная.
python - "$REG" <<'PY3'
import io, sys
p = sys.argv[1]
BL = u"БЛОКИРУЕТ ТЕГ:"
ST = u"Статус:"
LF = chr(10)
rows = (u'| 1 | K1 | text. ' + BL + u' DA. |' + LF
        + u'| 2 | K1 | text. ' + BL + u' NET. ' + ST + u' OTKRYT |' + LF)
io.open(p, 'w', encoding='utf-8', newline=LF).write(rows)
PY3

# ЗАМЕНА: счёт тот же (1), номер в базе ДРУГОЙ. Счётный храповик молчал бы.
# Это не гипотеза: 2026-09-20 счёт 19 дважды пережил смену состава.
printf 'rows=1\nscanned_rows=0\nno_status_row=777\n' > "$NSB"
check "zamena nomera pri tom zhe schete -- krasneet" "$(run)" "1"

# ПОГАШЕНИЕ: номер в базе есть, нарушения уже нет — зелёный С СОВЕТОМ.
printf 'rows=2\nscanned_rows=0\nno_status_row=1\nno_status_row=777\n' > "$NSB"
check "pogashennyy nomer -- zelyonyy" "$(run)" "0"
sh "$G" "$TMP" 2>&1 | grep -q "POGASHENY\|ПОГАШЕНЫ" \
    && ok "pogashenie nazvano vsluh" \
    || bad "pogashenie proshlo molcha"

# БАЗА ПРИЗНАЁТ ДОЛГ ЧИСЛОМ, НО НЕ НАЗЫВАЕТ НОМЕРОВ.
printf 'rows=1\nscanned_rows=0\n' > "$NSB"
check "schet bez nomerov -- krasneet" "$(run)" "1"

# БАЗА ПРОТИВОРЕЧИТ СЕБЕ: счёт против числа номеров.
printf 'rows=5\nscanned_rows=0\nno_status_row=1\n' > "$NSB"
check "schet ne raven chislu nomerov -- krasneet" "$(run)" "1"

# СУЖЕНИЕ РАЗБОРА — мутируем ЯДРО, а не базу: копия сканера перестаёт узнавать
# строки реестра. Долг падает до нуля, и счётный храповик прочёл бы это как
# погашение. Знаменатель разобранных строк не даёт.
printf 'rows=1\nscanned_rows=2\nno_status_row=1\n' > "$NSB"
check "kontrol: pri shirokom razbore baza shoditsya" "$(run)" "0"
MUT="$TMP/mut"; mkdir -p "$MUT"
cp "$G" "$MUT/check-registry-single-verdict.sh"
sed 's/^ROW = re\.compile(.*/ROW = re.compile(r"^NIKOGDA-NE-SOVPADET")/' \
    "$HERE/../registry-verdict-scan.py" > "$MUT/registry-verdict-scan.py"
if sh "$MUT/check-registry-single-verdict.sh" "$TMP" > "$TMP/mutout" 2>&1; then
    bad "suzhennyy razbor proshel kak zelyonyy"
else
    grep -q "razbor suzilsya\|разбор сузился" "$TMP/mutout" \
        && ok "suzhenie razbora -- krasneet imenno pro suzhenie" \
        || bad "krasnyy, no ne pro suzhenie razbora"
fi

# Вернуть базу в состояние, ожидаемое остальными случаями ниже.
printf 'rows=0\nscanned_rows=0\n' > "$NSB"

echo "== ne vret o srede =="
rm -f "$REG"
check "propavshiy reestr -- FAIL, a ne 'ok'" "$(run)" "1"

echo "== realnost =="
check "nastoyashchiy reestr prohodit" "$(sh "$G" "$HERE/../../.." >/dev/null 2>&1; echo $?)" "0"

echo
echo "selftest check-registry-single-verdict: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
