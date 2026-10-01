#!/usr/bin/env bash
# Самотест check-spec-amend-places.py -- клетки ПАРАМИ: у каждой «ждём красного» есть
# «ждём зелёного» на почти том же входе. Вход -- настоящие временные git-репозитории со
# staged-диффом (страж читает индекс), плюс проба на копии настоящего spec/decisions/.
set -u
export LC_ALL=C.UTF-8 PYTHONIOENCODING=utf-8

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-spec-amend-places.py"
FAILED=0
ok()  { echo "  ok   $1"; }
bad() { echo "  PROVAL $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
GI() { git -C "$1" -c user.name=t -c user.email=t@t.invalid -c core.hooksPath=/dev/null -c core.autocrlf=false "${@:2}"; }

# $1 каталог. Базовый репозиторий: D1 (старый, будет амендирован), D2 с указателем на D1,
# D3 без указателя.
base() {
    rm -rf "$1"; mkdir -p "$1/spec/decisions"; git init -q "$1"
    cat > "$1/spec/decisions/02-types.md" <<'EOF'
## D1. Old rule

Rule text.

## D2. Place with pointer

> **Уточнено [D1](#d1)**: see there.

Same rule here.

## D3. Place without pointer

Same rule here too.
EOF
    GI "$1" add spec/decisions/02-types.md && GI "$1" commit -q -m base
}
# $1 каталог, $2 строка-амендмент: дописать под D1 (после "Rule text.") и застейджить
amend() {
    python - "$1/spec/decisions/02-types.md" "$2" <<'PY'
import io, sys
p, line = sys.argv[1], sys.argv[2]
s = io.open(p, encoding="utf-8").read()
s = s.replace("Rule text.\n", "Rule text.\n\n" + line + "\n", 1)
io.open(p, "w", encoding="utf-8", newline="").write(s)
PY
    GI "$1" add spec/decisions/02-types.md
}
msg() { printf '%s\n' "$2" > "$1/.msg"; }
run() { python "$G" "$1/.msg" "$1" > "$TMP/.out" 2> "$TMP/.err"; }
AM='> **Амендмент 2026-10-02**: the rule changes.'

# (a) амендмент без трейлера -> FAIL с рецептом
base "$TMP/a"; amend "$TMP/a" "$AM"; msg "$TMP/a" "spec: amend D1"
run "$TMP/a"; rc=$?
[ $rc -ne 0 ] && grep -q "без трейлера" "$TMP/.err" && grep -q "spec-reader" "$TMP/.err" && ok "a: amendment, no trailer -> FAIL" || bad "a: expected FAIL with recipe"
# (b) only here с причиной -> ok
msg "$TMP/a" "spec: amend D1

Spec-places: only here -- no other block states this rule"
run "$TMP/a" && grep -q "only here" "$TMP/.out" && ok "b: only here + reason -> ok" || bad "b: expected ok"
# (c) only here без причины (3 слова) -> FAIL
msg "$TMP/a" "spec: amend D1

Spec-places: only here -- just because really"
run "$TMP/a"; rc=$?
[ $rc -ne 0 ] && grep -q "5+ слов" "$TMP/.err" && ok "c: only here, 3 words -> FAIL" || bad "c: expected FAIL on short reason"
# (d) место с указателем (D2) -> ok
msg "$TMP/a" "spec: amend D1

Spec-places: D2 (spec-reader)"
run "$TMP/a" && grep -q "проверено 1" "$TMP/.out" && ok "d: place with pointer -> ok" || bad "d: expected ok"
# (e) место без указателя (D3) -> FAIL, назван D3
msg "$TMP/a" "spec: amend D1

Spec-places: D2, D3 (spec-reader)"
run "$TMP/a"; rc=$?
[ $rc -ne 0 ] && grep -q "D3 (spec/decisions/02-types.md" "$TMP/.err" && ! grep -q "D2 (spec" "$TMP/.err" && ok "e: place without pointer -> FAIL naming D3 only" || bad "e: expected FAIL naming D3"
# (f) несуществующий блок -> FAIL
msg "$TMP/a" "spec: amend D1

Spec-places: D77"
run "$TMP/a"; rc=$?
[ $rc -ne 0 ] && grep -q "D77 -- заголовок не найден" "$TMP/.err" && ok "f: unknown block -> FAIL" || bad "f: expected FAIL on D77"
# (g) место = сам амендированный блок -> FAIL
msg "$TMP/a" "spec: amend D1

Spec-places: D1"
run "$TMP/a"; rc=$?
[ $rc -ne 0 ] && grep -q "сам амендированный блок" "$TMP/.err" && ok "g: place is the amended block -> FAIL" || bad "g: expected FAIL"
# (m) трейлер, перенесённый на вторую строку -> ok
msg "$TMP/a" "spec: amend D1

Spec-places: D2,
  D2 (spec-reader)"
run "$TMP/a" && ok "m: wrapped trailer -> ok" || bad "m: expected ok on wrapped trailer"
# (j) клапан -> ok и громко
msg "$TMP/a" "spec: amend D1"
NOVA_SPEC_PLACES_NA="typo fix in heading" run "$TMP/a" && grep -q "КЛАПАН" "$TMP/.out" && ok "j: valve -> ok, loud" || bad "j: expected ok via valve"
# (k) слияние -> ok
touch "$(git -C "$TMP/a" rev-parse --absolute-git-dir)/MERGE_HEAD"
run "$TMP/a" && grep -q "слияние" "$TMP/.out" && ok "k: MERGE_HEAD -> not judged" || bad "k: expected ok on merge"
# (l) пустое сообщение -> ok
: > "$TMP/a/.msg"
run "$TMP/a" && grep -q "судить нечего" "$TMP/.out" && ok "l: empty message -> nothing to judge" || bad "l: expected ok"

# (n) форма Amendment по-английски -> FAIL без трейлера
base "$TMP/n"; amend "$TMP/n" '> **Amendment 2026-10-02**: the rule changes.'; msg "$TMP/n" "spec: amend D1"
run "$TMP/n"; rc=$?
[ $rc -ne 0 ] && ok "n: English Amendment form -> FAIL" || bad "n: expected FAIL"
# (o) указатель `> **Уточнено` -- не амендмент -> ok без трейлера
base "$TMP/o"; amend "$TMP/o" '> **Уточнено [D9](#d9)**: pointer, not an amendment.'; msg "$TMP/o" "spec: pointer"
run "$TMP/o" && grep -q "нет" "$TMP/.out" && ok "o: pointer line is not an amendment -> ok" || bad "o: expected ok"
# (h) обычная правка текста -> ok без трейлера
base "$TMP/h"; amend "$TMP/h" 'Plain extra sentence.'; msg "$TMP/h" "spec: wording"
run "$TMP/h" && ok "h: plain edit -> ok" || bad "h: expected ok"
# (i) НОВЫЙ блок со своей строкой-амендментом внутри -> ok (его держит страж указателей)
base "$TMP/i"
printf '\n## D4. New block\n\n%s\n' "$AM" >> "$TMP/i/spec/decisions/02-types.md"
GI "$TMP/i" add spec/decisions/02-types.md; msg "$TMP/i" "spec: new block D4"
run "$TMP/i" && ok "i: whole new block with amendment-like line -> ok" || bad "i: expected ok"
# (q) амендмент не в spec/decisions -> не судится
base "$TMP/q"; mkdir -p "$TMP/q/docs"; printf '%s\n' "$AM" > "$TMP/q/docs/x.md"; GI "$TMP/q" add docs/x.md; msg "$TMP/q" "docs"
run "$TMP/q" && ok "q: amendment outside spec/decisions -> ok" || bad "q: expected ok"

# --- настоящее дерево: копия spec/decisions/, амендмент под D489 ---
R="$TMP/real"; rm -rf "$R"; mkdir -p "$R/spec"; cp -r "$ROOT/spec/decisions" "$R/spec/decisions"; git init -q "$R"
GI "$R" add spec/decisions >/dev/null 2>&1; GI "$R" commit -q -m base
python - "$R" <<'PY'
import glob, io, re, sys
for p in glob.glob(sys.argv[1] + "/spec/decisions/*.md"):
    s = io.open(p, encoding="utf-8").read()
    m = re.search(r"^## D489\b.*\n", s, re.M)
    if m:
        s = s[:m.end()] + "\n> **Амендмент 2026-10-02**: probe.\n" + s[m.end():]
        io.open(p, "w", encoding="utf-8", newline="").write(s)
        break
else:
    sys.exit("D489 not found")
PY
GI "$R" add spec/decisions >/dev/null 2>&1
msg "$R" "spec: amend D489

Spec-places: D44, D54, D227 (spec-reader)"
run "$R" && grep -q "проверено 3" "$TMP/.out" && ok "p1: real tree, D489 amended, places D44/D54/D227 carry pointers -> ok" || { bad "p1: expected ok"; cat "$TMP/.err" >&2; }
msg "$R" "spec: amend D489

Spec-places: D44, D1"
run "$R"; rc=$?
[ $rc -ne 0 ] && grep -q "D1 (spec/decisions" "$TMP/.err" && ! grep -q "D44 (spec" "$TMP/.err" && ok "p2: real tree, D1 has no pointer -> FAIL naming D1 only" || bad "p2: expected FAIL naming D1"

[ $FAILED -eq 0 ] && echo "test-check-spec-amend-places ok" || { echo "test-check-spec-amend-places FAIL" >&2; exit 1; }
