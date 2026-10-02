#!/usr/bin/env bash
# Самотест check-spec-code-home.py -- клетки ПАРАМИ: у каждой «ждём красного» есть
# «ждём зелёного» на почти том же входе, плюс настоящее дерево: ok с базой, FAIL без неё
# и FAIL после снятия ссылки на дом у настоящего блока.
set -u
export LC_ALL=C.UTF-8 PYTHONIOENCODING=utf-8

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-spec-code-home.py"
FAILED=0
ok()  { echo "  ok   $1"; }
bad() { echo "  PROVAL $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
run() { NOVA_SPEC_CODE_HOME_BASELINE="$2" python "$G" "$1" > "$TMP/.out" 2> "$TMP/.err"; }
: > "$TMP/empty.base"

mk() {   # $1 каталог, $2 текст блока D10 (старого), $3 текст блока D20 (нового, дом)
    rm -rf "$1"; mkdir -p "$1/spec/decisions"
    printf '## D10. Old\n\n%s\n\n## D20. New\n\n%s\n' "$2" "$3" > "$1/spec/decisions/02-types.md"
}

# (a) старый блок ссылается на дом -> ok
mk "$TMP/a" 'Rule, error `E_FOO`. See D20.' 'Rule, error `E_FOO`.'
run "$TMP/a" "$TMP/empty.base" && grep -q "ok:" "$TMP/.out" && ok "a: old block points to home -> ok" || bad "a: expected ok"
# (b) не ссылается -> FAIL, назван код, блок и дом
mk "$TMP/b" 'Rule, error `E_FOO`.' 'Rule, error `E_FOO`.'
run "$TMP/b" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "E_FOO назван в D10, но дом правила -- D20" "$TMP/.err" && ok "b: no pointer -> FAIL naming code, block, home" || bad "b: expected FAIL"
# (c) то же, но в базе -> ok
printf 'E_FOO D10\n' > "$TMP/c.base"
run "$TMP/b" "$TMP/c.base" && ok "c: baselined violation -> ok" || bad "c: expected ok"
# (d) строка базы без нарушения -> FAIL (база только убывает)
run "$TMP/a" "$TMP/c.base"; rc=$?
[ $rc -ne 0 ] && grep -q "больше не нарушение" "$TMP/.err" && ok "d: stale baseline line -> FAIL" || bad "d: expected FAIL on stale line"
# (e) код только в одном блоке -> нарушения нет
mk "$TMP/e" 'Nothing here.' 'Rule, error `E_FOO`.'
run "$TMP/e" "$TMP/empty.base" && ok "e: code in one block -> ok" || bad "e: expected ok"
# (f) дом -- НОВЕЙШИЙ по номеру: ссылка нового на старый дом не спасает
mk "$TMP/f" 'Rule, error `E_FOO`.' 'Rule, error `E_FOO`. See D10.'
run "$TMP/f" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "дом правила -- D20" "$TMP/.err" && ok "f: home is the newest, reverse pointer does not help -> FAIL" || bad "f: expected FAIL"
# (g) маска `E_FOO_*` и слово внутри идентификатора кодом не считаются
mk "$TMP/g" 'Family `E_FOO_*` and `xE_FOO`.' 'Rule, error `E_FOO`.'
run "$TMP/g" "$TMP/empty.base" && ok "g: wildcard and embedded word are not codes -> ok" || bad "g: expected ok"
# (h) предупреждение W_ считается как E_
mk "$TMP/h" 'Warn `W_BAR`.' 'Warn `W_BAR`.'
run "$TMP/h" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "W_BAR" "$TMP/.err" && ok "h: W_ code judged like E_ -> FAIL" || bad "h: expected FAIL on W_BAR"
# (i) ноль блоков -> FAIL (мишень потеряна)
rm -rf "$TMP/i"; mkdir -p "$TMP/i/spec/decisions"; printf 'no blocks\n' > "$TMP/i/spec/decisions/x.md"
run "$TMP/i" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "мишень потеряна" "$TMP/.err" && ok "i: zero blocks -> FAIL" || bad "i: expected FAIL on empty target"

# --- настоящее дерево ---
B="$ROOT/scripts/guards/spec-code-home.baseline"
run "$ROOT" "$B" && grep -q "ok:" "$TMP/.out" && ok "j: real tree + real baseline -> ok" || { bad "j: real tree not ok"; cat "$TMP/.err" >&2; }
run "$ROOT" "$TMP/empty.base"; rc=$?
[ $rc -ne 0 ] && grep -q "FAIL" "$TMP/.err" && ok "k: real tree, empty baseline -> FAIL (baseline is load-bearing)" || bad "k: expected FAIL with empty baseline"
# (l) три кода: дом по правилу «новейший» -- как задумано
for pair in "E_LIT_OUT_OF_RANGE D489" "E_CONSUME_IN_CONDITION D486"; do
    set -- $pair
    python - "$ROOT" "$1" "$2" <<'PY' && ok "l: $1 -> home $2" || bad "l: $1 home is not $2"
import sys, importlib.util as u
s = u.spec_from_file_location("g", sys.argv[1] + "/scripts/guards/check-spec-code-home.py")
m = u.module_from_spec(s); s.loader.exec_module(m)
n, where, viol, homes = m.violations(sys.argv[1])
sys.exit(0 if "D%d" % homes[sys.argv[2]] == sys.argv[3] else 1)
PY
done
# (m) копия настоящего spec/: из блока D184 снята ссылка на D486 -> FAIL, назван E_CONSUME_IN_CONDITION D184
mkdir -p "$TMP/m"; cp -r "$ROOT/spec" "$TMP/m/spec"
python - "$TMP/m/spec/decisions" <<'PY'
import glob, io, re, sys
done = False
for p in glob.glob(sys.argv[1] + "/*.md"):
    s = io.open(p, encoding="utf-8").read()
    m = re.search(r"^## D184\b.*?(?=^## )", s, re.M | re.S)
    if m and "D486" in m.group(0):
        s = s[:m.start()] + re.sub(r"\bD486\b", "Dxxx", m.group(0)) + s[m.end():]
        io.open(p, "w", encoding="utf-8", newline="").write(s)
        done = True
sys.exit(0 if done else "D184 has no D486 to strip")
PY
run "$TMP/m" "$B"; rc=$?
[ $rc -ne 0 ] && grep -q "E_CONSUME_IN_CONDITION назван в D184" "$TMP/.err" && ok "m: real tree minus D486 pointer in D184 -> FAIL" || bad "m: expected FAIL naming D184"

[ $FAILED -eq 0 ] && echo "test-check-spec-code-home ok" || { echo "test-check-spec-code-home FAIL" >&2; exit 1; }
