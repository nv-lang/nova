#!/usr/bin/env bash
# Самотест check-spec-pair-dnums.py -- клетки ПАРАМИ: у каждой «ждём красного» есть
# «ждём зелёного» на почти том же входе, плюс проба на настоящем дереве: ok как есть,
# FAIL после снятия одного упоминания D-блока из нормативной русской страницы.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-spec-pair-dnums.py"
FAILED=0
ok()  { echo "  ok   $1"; }
bad() { echo "  PROVAL $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
run() { NOVA_SPEC_PAIR_BASELINE="$2" python "$G" "$1" > "$TMP/.out" 2> "$TMP/.err"; }

mk() {   # $1 каталог, $2 текст X.md, $3 текст X.ru.md
    rm -rf "$1"; mkdir -p "$1/spec"
    printf '%s\n' "$2" > "$1/spec/x.md"
    printf '%s\n' "$3" > "$1/spec/x.ru.md"
    printf 'only Russian, D9 here is not judged\n' > "$1/spec/solo.ru.md"
}
: > "$TMP/empty.base"

# (a) одинаковые наборы -> ok
mk "$TMP/a" 'see D1 and D22' 'см. D22 и D1'
run "$TMP/a" "$TMP/empty.base" && grep -q "ok:" "$TMP/.out" && ok "a: same sets -> ok" || bad "a: expected ok"
# (b) русская называет D3, английская нет -> FAIL с адресом
mk "$TMP/b" 'see D1' 'см. D1 и D3'
run "$TMP/b" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "spec/x.ru.md называет D3, а spec/x.md" "$TMP/.err" && ok "b: ru-only D3 -> FAIL" || bad "b: expected FAIL naming D3"
# (c) то же расхождение, но в базе -> ok
printf 'x.md only-ru D3\n' > "$TMP/c.base"
run "$TMP/b" "$TMP/c.base" && grep -q "ok:" "$TMP/.out" && ok "c: baselined difference -> ok" || bad "c: expected ok"
# (d) строка базы без расхождения -> FAIL (база только убывает)
run "$TMP/a" "$TMP/c.base"; rc=$?
[ $rc -ne 0 ] && grep -q "больше не расхождение" "$TMP/.err" && ok "d: stale baseline line -> FAIL" || bad "d: expected FAIL on stale line"
# (e) английская называет лишнее -> FAIL (сторона only-en)
mk "$TMP/e" 'see D1 and D40' 'см. D1'
run "$TMP/e" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "spec/x.md называет D40" "$TMP/.err" && ok "e: en-only D40 -> FAIL" || bad "e: expected FAIL naming D40"
# (f) D-номер внутри слова не считается (граница слова): D12abc и xD5 -> ok
mk "$TMP/f" 'see D1, ID5 and D12abc' 'см. D1'
run "$TMP/f" "$TMP/empty.base" && grep -q "ok:" "$TMP/.out" && ok "f: non-word D refs ignored -> ok" || bad "f: expected ok"
# (g) ни одной пары -> FAIL (мишень потеряна)
rm -rf "$TMP/g"; mkdir -p "$TMP/g/spec"; printf 'D1\n' > "$TMP/g/spec/solo.ru.md"
run "$TMP/g" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "мишень потеряна" "$TMP/.err" && ok "g: no pairs -> FAIL" || bad "g: expected FAIL on zero pairs"

# (h) настоящее дерево с настоящей базой -> ok
run "$ROOT" "$ROOT/scripts/guards/spec-pair-dnums.baseline" && grep -q "ok:" "$TMP/.out" && ok "h: real tree -> ok" || { bad "h: real tree not ok"; cat "$TMP/.err" >&2; }
# (i) копия настоящего spec/ без упоминаний D486 в syntax.ru.md -> FAIL, названа D486
mkdir -p "$TMP/i"; cp -r "$ROOT/spec" "$TMP/i/spec"
python - "$TMP/i/spec/syntax.ru.md" <<'PY'
import io, re, sys
p = sys.argv[1]
s = io.open(p, encoding="utf-8").read()
io.open(p, "w", encoding="utf-8", newline="").write(re.sub(r"\bD486\b", "Dxxx", s))
PY
run "$TMP/i" "$ROOT/scripts/guards/spec-pair-dnums.baseline"; rc=$?
[ $rc -ne 0 ] && grep -q "spec/syntax.md называет D486" "$TMP/.err" && ok "i: real tree minus D486 in syntax.ru.md -> FAIL" || bad "i: expected FAIL naming D486"

[ $FAILED -eq 0 ] && echo "test-check-spec-pair-dnums ok" || { echo "test-check-spec-pair-dnums FAIL" >&2; exit 1; }
