#!/usr/bin/env bash
# Самотест check-dblock-supersede-pointers.py -- клетки ПАРАМИ: у каждой
# «ждём красного» есть «ждём зелёного» на почти том же входе, плюс проба на
# настоящем дереве: ok как есть, FAIL после снятия указателя у D326.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-dblock-supersede-pointers.py"
FAILED=0
ok()  { echo "  ok   $1"; }
bad() { echo "  PROVAL $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
run() { python "$G" "$1" > "$TMP/.out" 2> "$TMP/.err"; }

mk() {   # $1 каталог, $2 содержимое блока D1 после заголовка, $3 номер Dold в разделе
    rm -rf "$1"; mkdir -p "$1/spec/decisions"
    printf '## D1. Old block\n\n%s\nold text\n\n## D2. New block\n\ntext\n\n### Supersedes\n\n* D%s: replaced.\n\n### Next\n\nD99 mentioned here is not judged.\n' \
        "$2" "$3" > "$1/spec/decisions/02-types.md"
}

# (a) указатель есть -> ok
mk "$TMP/a" '> **Updated [D2](#d2):** see there.' 1
run "$TMP/a" && grep -q "ok:" "$TMP/.out" && ok "a: pointer present -> ok" || bad "a: expected ok"
# (b) указателя нет -> FAIL
mk "$TMP/b" '' 1
run "$TMP/b"; rc=$?
[ $rc -ne 0 ] && grep -q "D1 .*не указывает на D2" "$TMP/.err" && ok "b: no pointer -> FAIL" || bad "b: expected FAIL"
# (c) несуществующий D -> FAIL
mk "$TMP/c" '> **Updated [D2](#d2):** see there.' 77
run "$TMP/c"; rc=$?
[ $rc -ne 0 ] && grep -q "D77: заголовок не найден" "$TMP/.err" && ok "c: unknown D -> FAIL" || bad "c: expected FAIL"
# (d) указатель не в цитате (строка без >) -> FAIL
mk "$TMP/d" 'See D2 for details.' 1
run "$TMP/d"; rc=$?
[ $rc -ne 0 ] && ok "d: pointer not a quote line -> FAIL" || bad "d: expected FAIL"

# (e) настоящее дерево -> ok
run "$ROOT" && grep -q "ok:" "$TMP/.out" && ok "e: real tree -> ok" || { bad "e: real tree not ok"; cat "$TMP/.err" >&2; }
# (f) копия настоящего дерева без указателя у D326 -> FAIL
mkdir -p "$TMP/f/spec"; cp -r "$ROOT/spec/decisions" "$TMP/f/spec/decisions"
python - "$TMP/f/spec/decisions/02-types.md" <<'PY'
import io, re, sys
p = sys.argv[1]
L = io.open(p, encoding="utf-8").read().split("\n")
for i, ln in enumerate(L):
    if re.match(r"^#{2,3} D326\b", ln):
        for j in range(i + 1, min(i + 12, len(L))):
            if L[j].startswith(">") and "D488" in L[j]:
                del L[j]
                break
        break
io.open(p, "w", encoding="utf-8", newline="").write("\n".join(L))
PY
run "$TMP/f"; rc=$?
[ $rc -ne 0 ] && grep -q "D326 " "$TMP/.err" && ok "f: real tree minus D326 pointer -> FAIL" || bad "f: expected FAIL naming D326"

[ $FAILED -eq 0 ] && echo "test-check-dblock-supersede-pointers ok" || { echo "test-check-dblock-supersede-pointers FAIL" >&2; exit 1; }
