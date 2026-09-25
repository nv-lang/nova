#!/usr/bin/env bash
# Самотест check-novac-oracle-tax-link.py — обе стороны, на фикстурном корне.
# Найден на настоящем дереве 2026-09-25 (первый прогон стража): таблица
# 274.10 называла три маркера, которых в novac/src уже не было (два снятых
# обхода, один репоинт на другой номер) — вот почему проверка судится по
# ОБЕИМ сторонам сразу, а не по одной.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-oracle-tax-link.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

FIX="$TMP/root"; mkdir -p "$FIX/novac/src/demo" "$FIX/docs/plans"
PLAN="$FIX/docs/plans/274.10-oracle-tax-on-carina.md"
SITE="$FIX/novac/src/demo/demo.nv"

TABLE_HEAD='## Четырнадцать дефектов — по содержимому, а не по ссылке

| порядок | дефект | мест обхода | маркер в коде | оракул |
|---|---|---|---|---|'

echo "== проходит =="
python "$G" "$TMP/empty-root" >/dev/null 2>&1
check "нет novac — зелёный" "$?" "0"

printf '%s\n| 1 | №900 demo | **1** | `LEGACY-#900-demo` | открыт |\n\n---\n' "$TABLE_HEAD" > "$PLAN"
printf '// [LEGACY-#900-demo] workaround site\nfn f() -> int => 1\n' > "$SITE"
OUT=$(python "$G" "$FIX" 2>/dev/null); RC=$?
check "маркер в таблице и в коде совпали — зелёный" "$RC" "0"
has   "счётчик печатается на зелёном" "$OUT" 'дефектов в плане'
has   "счётчик считает маркированные строки" "$OUT" 'с маркером в коде 1'

printf '%s\n| 1 | №900 demo | обхода нет | нет | открыт |\n\n---\n' "$TABLE_HEAD" > "$PLAN"
printf 'fn f() -> int => 1\n' > "$SITE"
python "$G" "$FIX" >/dev/null 2>&1
check "маркера нет ни там, ни там — зелёный" "$?" "0"

echo "== ловит =="
printf '%s\n| 1 | №900 demo | **1** | `LEGACY-#900-demo` | открыт |\n\n---\n' "$TABLE_HEAD" > "$PLAN"
printf 'fn f() -> int => 1\n' > "$SITE"
OUT=$(python "$G" "$FIX" 2>&1); RC=$?
check "таблица называет маркер, которого в коде нет — красный (сторона A)" "$RC" "1"
has   "красный называет номер и маркер" "$OUT" 'LEGACY-#900-demo'

printf '%s\n| 1 | №900 demo | обхода нет | нет | открыт |\n\n---\n' "$TABLE_HEAD" > "$PLAN"
printf '// [LEGACY-#901-new] site\nfn f() -> int => 1\n' > "$SITE"
OUT=$(python "$G" "$FIX" 2>&1); RC=$?
check "маркер в коде без строки в таблице и вне ALLOWED_EXTRA — красный (сторона B)" "$RC" "1"
has   "красный называет место" "$OUT" 'demo.nv'

printf '%s\n| 1 | №700 demo (искл.) | **1** | нет | открыт |\n\n---\n' "$TABLE_HEAD" > "$PLAN"
printf '// [LEGACY-#700-user-error-as-ice] site\nfn f() -> int => 1\n' > "$SITE"
python "$G" "$FIX" >/dev/null 2>&1
check "маркер из ALLOWED_EXTRA (№700) вне таблицы — зелёный" "$?" "0"

printf '%s\n| 1 | №708 demo (искл.) | **1** | нет | открыт |\n\n---\n' "$TABLE_HEAD" > "$PLAN"
printf '// [LEGACY-#708-panic-payload-text] site\nfn f() -> int => 1\n' > "$SITE"
python "$G" "$FIX" >/dev/null 2>&1
check "маркер из ALLOWED_EXTRA (№708) вне таблицы — зелёный" "$?" "0"

rm -f "$PLAN"
printf 'fn f() -> int => 1\n' > "$SITE"
python "$G" "$FIX" >/dev/null 2>&1
check "нет плана 274.10 — красный" "$?" "1"

printf 'просто текст без заголовка раздела\n' > "$PLAN"
python "$G" "$FIX" >/dev/null 2>&1
check "план есть, заголовка раздела нет — красный" "$?" "1"

echo "== настоящее дерево =="
OUT=$(python "$G" "$ROOT" 2>&1); RC=$?
check "novac проекта чист" "$RC" "0"
has   "счётчик печатается" "$OUT" 'дефектов в плане'

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
