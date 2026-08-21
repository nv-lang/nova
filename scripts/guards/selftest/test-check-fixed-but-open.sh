#!/usr/bin/env bash
# Самотест check-fixed-but-open: страж обязан УМЕТЬ КРАСНЕТЬ.
#
# Подставное дерево: свой корень, свой реестр, своя история — чтобы самотест
# не зависел ни от настоящего файла, ни от настоящих коммитов и краснел
# только по своей причине.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-fixed-but-open.sh"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/docs/plans"
cp "$HERE/../fixed-but-open-scan.py" "$TMP/scripts/guards/"

REG="$TMP/docs/plans/221.1-bug-sweep.md"
BASE="$TMP/scripts/guards/fixed-but-open.baseline"
HIST="$TMP/hist.txt"

# История подставная: два заголовка-правки и один не-правка.
printf '%s\n' \
  'aaaaaaa|fix(#901): a landed fix' \
  'bbbbbbb|registry(#902): only a registry note, not a fix' \
  'ccccccc|fix(#903): another landed fix' > "$HIST"

run() { sh "$G" "$TMP" "$HIST" >/dev/null 2>&1; echo $?; }

# Реестр пишется ПИТОНОМ, а не оболочкой: маркеры кириллические, и через
# оболочку они уже уезжали перекодированными (реестр №590).
mk() {
python - "$REG" "$@" <<'PY'
import io, sys
p, kinds = sys.argv[1], sys.argv[2:]
ST = u"Статус"          # Статус
OPEN_RU = u"ОТКРЫТ"      # ОТКРЫТ
CLOSED_RU = u"ЗАКРЫТ"    # ЗАКРЫТ
rows = []
for k in kinds:
    num, state = k.split(":")
    word = {"open": OPEN_RU, "openen": u"OPEN", "closed": CLOSED_RU}[state]
    rows.append(u"| %s | K1 | text. **%s:** %s 2026-08-19. |" % (num, ST, word))
io.open(p, "w", encoding="utf-8", newline="\n").write(
    u"# registry\n\n| N | P | text |\n| --- | --- | --- |\n"
    + u"\n".join(rows) + u"\n")
PY
}

: > "$BASE"

echo "== check-fixed-but-open selftest =="

# 1. Строка открыта, правка слита, номера в базе нет -> отказ.
mk "901:open"
check "open row with a landed fix is refused" "$(run)" "1"

# 2. То же, но статус записан по-английски (`OPEN`) -> тот же отказ.
mk "901:openen"
check "the English spelling of the status is seen too" "$(run)" "1"

# 3. Строка закрыта -> зелено.
mk "901:closed"
check "a closed row with a fix is fine" "$(run)" "0"

# 4. Строка открыта, но правки в истории нет -> зелено.
mk "902:open"
check "an open row without a fix commit is fine" "$(run)" "0"

# 5. Строка открыта, правка есть, номер внесён в базу -> зелено.
mk "901:open"
printf '%s\n' '901  carrier fixed, class open' > "$BASE"
check "a baselined row is accepted" "$(run)" "0"

# 6. База не молчит про ВТОРУЮ строку: одна внесена, другая нет -> отказ.
mk "901:open" "903:open"
check "a second unlisted row still refuses" "$(run)" "1"

# 7. Обе внесены -> зелено.
printf '%s\n' '901  carrier fixed, class open' '903  partial fix' > "$BASE"
check "both baselined is fine" "$(run)" "0"

# 8. Устаревшая запись базы -> зелено (заметка, не отказ): закрытие строки
#    правильное действие, ложным красным за него наказывать нельзя (№703).
mk "901:closed" "903:closed"
check "a stale baseline entry is a note, not a failure" "$(run)" "0"

# 9. Комментарии и пустые строки базы не считаются номерами.
mk "901:open"
printf '%s\n' '# 901 mentioned only in a comment' '' > "$BASE"
check "a number inside a comment does not count" "$(run)" "1"

# 10. Нет реестра -> отказ, а не зелено «ничего не нашли».
printf '%s\n' '901  carrier fixed, class open' > "$BASE"
rm -f "$REG"
check "a missing registry refuses instead of passing" "$(run)" "1"

echo "  passed: $PASS   failed: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
