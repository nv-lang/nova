#!/usr/bin/env bash
# Самотест check-panic-report-contract.sh — обе стороны.
#
# Страж проверяет ВЫВОД, а не текст исходника, поэтому самотест подсовывает
# ему поддельный «компилятор»: тот кладёт в -o скрипт, печатающий заранее
# заданный отказ. Так проверяется именно логика стража, без сборки Nova.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-panic-report-contract.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/spec_tests/conformance/neg"
echo "module x" > "$FIX/spec_tests/conformance/neg/f5_propagation_trace_full.nv"
# Фикстура проверки 4: страж берёт ожидаемую строку ИЗ ЕЁ EXPECT-шапки,
# поэтому здесь можно задать любой пин.
printf '// EXPECT_RUNTIME_PANIC probe.nv:24 (throw site)\nmodule y\n' \
    > "$FIX/spec_tests/conformance/neg/f2b_cleanup_does_not_steal_site.nv"

# $1 — человеческий stderr, $2 — json stderr. Пишет поддельный nova в $TMP/nova.
make_nova() {
    printf '%s' "$1" > "$TMP/human.txt"
    printf '%s' "$2" > "$TMP/json.txt"
    cat > "$TMP/nova" <<'EOS'
#!/usr/bin/env bash
# поддельный компилятор: -o <exe> получает скрипт-печатальщик
out=""
prev=""
for a in "$@"; do
    if [ "$prev" = "-o" ]; then out="$a"; fi
    prev="$a"
done
[ -n "$out" ] || exit 1
cat > "$out" <<'INNER'
#!/usr/bin/env bash
if [ "${NOVA_PANIC_FORMAT:-}" = "json" ]; then
    cat "@@TMP@@/json.txt" >&2
else
    cat "@@TMP@@/human.txt" >&2
fi
exit 101
INNER
sed -i "s|@@TMP@@|@@REAL@@|g" "$out"
chmod +x "$out"
exit 0
EOS
    sed -i "s|@@REAL@@|$TMP|g" "$TMP/nova"
    chmod +x "$TMP/nova"
}

GOOD_HUMAN='nova: unhandled Fail: leaf-error
  at probe.nv:24 (throw site)
  propagation trace (`?`-chain, oldest first):
    via app.nv:19 (?)
'
# то же, но точка броска уехала на строку defer’а — ровно дефект Ф.2(б)
STOLEN_HUMAN='nova: unhandled Fail: leaf-error
  at probe.nv:23 (throw site)
  propagation trace (`?`-chain, oldest first):
    via app.nv:19 (?)
'
GOOD_JSON='{"nova_failure":1,"kind":"fail","message":"leaf-error","site":{"file":"app.nv","line":29},"trace":[{"file":"app.nv","line":19}],"trace_dropped":0,"suppressed":[]}
'

echo "== проходит =="
make_nova "$GOOD_HUMAN" "$GOOD_JSON"
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "оба рендерера полны — зелёный" "$?" "0"

echo "== ловит =="
make_nova 'nova: unhandled Fail: leaf-error
' "$GOOD_JSON"
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "человеческий рендер без throw-site и трассы — красный (это и был №445)" "$?" "1"

make_nova 'nova: unhandled Fail: leaf-error
  at app.nv:29 (throw site)
' "$GOOD_JSON"
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "есть site, но нет propagation trace — красный" "$?" "1"

make_nova "$GOOD_HUMAN" '{"nova_failure":1,"kind":"fail","message":"leaf-error","trace":[],"suppressed":[]}
'
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "в JSON нет поля site — красный" "$?" "1"

make_nova "$GOOD_HUMAN" '{"nova_failure":1,"kind":"fail",
"message":"leaf-error","site":{"line":1},"trace":[{"file":"a","line":1}],"suppressed":[]}
'
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "JSON-запись не в одну строку — красный" "$?" "1"

make_nova "$GOOD_HUMAN" '{"nova_failure":1,"kind":"fail","message":"leaf-error","site":{"line":1},"trace":[{"file":"a","line":1}],"suppressed":[],}
'
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "JSON не разбирается парсером — красный" "$?" "1"

make_nova "$GOOD_JSON" "$GOOD_JSON"
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "умолчание печатает JSON вместо человеческого — красный" "$?" "1"

make_nova "$GOOD_HUMAN" "$GOOD_JSON"
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "точка броска на первопричине — зелёный" "$?" "0"

make_nova "$STOLEN_HUMAN" "$GOOD_JSON"
sh "$G" "$FIX" "$TMP/nova" >/dev/null 2>&1
check "cleanup украл точку броска (строка defer’а) — красный (Ф.2б)" "$?" "1"

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
RC=$?
if [ "$RC" = "0" ]; then
    ok "запись отказа проекта соответствует контракту D462"
else
    bad "настоящее дерево — красный (rc=$RC)"
fi

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
